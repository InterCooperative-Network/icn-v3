use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
    response::{Html, IntoResponse},
};
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use axum::http::{Method, header::{AUTHORIZATION, CONTENT_TYPE}};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

// Import all necessary components from the current crate
use crate::error::ApiError;
use crate::handlers::{
    health_check_handler,
    cast_vote_handler,
    create_proposal_handler,
    create_thread_handler,
    get_proposal_detail_handler,
    get_proposal_votes_handler,
    get_proposals_handler,
    get_thread_detail_handler,
    get_threads_handler,
    Db, // InMemoryStore, // Removed unused InMemoryStore
    // Add authorized route handlers
    get_receipts_authorized, get_token_balances_authorized, get_token_transactions_authorized,
    get_receipt_stats_authorized, get_token_stats_authorized,
    // Add the entity transfer handler
    process_entity_transfer,
    process_batch_transfers,
    query_transfers,
    get_federation_ledger_stats,
};
use crate::auth_handlers::{
    issue_jwt_token_handler, revoke_token_handler, rotate_token_handler,
    start_revocation_cleanup
};
use crate::org_handlers::{
    process_token_transfer, process_community_governance_action
};
use crate::models::{
    GetProposalsQuery,
    GetThreadsQuery,
    Message,
    NewProposalRequest,
    NewThreadRequest,
    NewVoteRequest,
    ProposalDetail,
    ProposalStatus,
    ProposalSummary,
    ProposalVotesResponse,
    // Timestamp is implicitly handled by chrono in models
    ThreadDetail,
    // For OpenAPI schema generation
    ThreadSummary,
    Vote,
    VoteCounts,
    VoteType,
    EntityType,
    EntityRef,
    Transfer,
    TransferRequest,
    TransferResponse,
};
use crate::websocket::{websocket_routes, WebSocketState};
use crate::auth::{JwtConfig, revocation::{TokenRevocationStore, InMemoryRevocationStore}};

/// Type alias for the Axum application state
pub type AppState = (
    Db,
    Arc<WebSocketState>,
    Arc<JwtConfig>,
    Arc<dyn TokenRevocationStore>,
);

/// API documentation
#[derive(OpenApi)]
#[openapi(
    components(
        schemas(
            crate::error::ApiError,
            crate::models::Message,
            crate::models::NewThreadRequest,
            crate::models::ThreadSummary,
            crate::models::ThreadDetail,
            crate::models::ProposalSummary,
            crate::models::ProposalDetail,
            crate::models::NewProposalRequest,
            crate::models::ProposalStatus,
            crate::models::Vote,
            crate::models::VoteCounts,
            crate::models::VoteType,
            crate::models::NewVoteRequest,
            crate::models::ProposalVotesResponse,
            crate::models::GetThreadsQuery,
            crate::models::GetProposalsQuery,
            // Entity and transfer models
            crate::models::EntityType,
            crate::models::EntityRef,
            crate::models::Transfer,
            crate::models::TransferRequest,
            crate::models::TransferResponse,
        ),
    ),
    tags(
        (name = "AgoraNet", description = "ICN AgoraNet API"),
    ),
    info(
        title = "ICN AgoraNet API",
        version = "1.0.0",
        description = "ICN AgoraNet API for federation, cooperative, and community operations",
    ),
)]
struct ApiDoc;

/// Create the Axum application with all routes
pub fn create_app(app_state: AppState) -> Router {
    // Define the API documentation for OpenAPI
    let openapi = ApiDoc::openapi();
    
    // Extract components from the app state
    let (db, ws_state, jwt_config, token_revocation_store) = app_state.clone();
    
    // Create WebSocket router with its own state
    let ws_router = websocket_routes()
        .with_state((db.clone(), ws_state.clone(), jwt_config.clone()));
    
    // Create main API router with the full state
    let api_router = Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi))
        // Health and general APIs
        .route("/health", get(health_check_handler))
        .route("/api/v1/health", get(health_check_handler))
        
        // Forum/Discussion APIs
        .route("/threads", post(create_thread_handler).get(get_threads_handler))
        .route("/threads/:thread_id", get(get_thread_detail_handler))
        .route("/proposals", post(create_proposal_handler).get(get_proposals_handler))
        .route("/proposals/:proposal_id", get(get_proposal_detail_handler))
        .route("/proposals/:proposal_id/votes", get(get_proposal_votes_handler))
        .route("/votes", post(cast_vote_handler))
        
        // Organization-scoped authorized routes
        .route("/api/v1/receipts", get(get_receipts_authorized))
        .route("/api/v1/tokens/balances", get(get_token_balances_authorized))
        .route("/api/v1/tokens/transactions", get(get_token_transactions_authorized))
        .route("/api/v1/stats/receipts", get(get_receipt_stats_authorized))
        .route("/api/v1/stats/tokens", get(get_token_stats_authorized))
        
        // Federation coordination routes
        .route("/api/v1/federation/:federation_id/tokens", post(issue_jwt_token_handler))
        .route("/api/v1/federation/:federation_id/tokens/revoke", post(revoke_token_handler))
        .route("/api/v1/federation/:federation_id/tokens/rotate", post(rotate_token_handler))
        
        // Cross-entity transfer endpoints
        .route("/api/v1/federation/:federation_id/transfers", post(process_entity_transfer))
        .route("/api/v1/federation/:federation_id/transfers/batch", post(process_batch_transfers))
        .route("/api/v1/federation/:federation_id/transfers/query", get(query_transfers))
        .route("/api/v1/federation/:federation_id/ledger/stats", get(get_federation_ledger_stats))
        
        // Economic operation routes (cooperative scoped)
        .route("/api/v1/coop/:coop_id/transfer", post(process_token_transfer))
        
        // Governance routes (community scoped)
        .route("/api/v1/community/:community_id/governance", post(process_community_governance_action))
        
        // Monitoring routes
        .route("/internal/metrics-ui", get(metrics_dashboard_handler))
        
        // Common middleware
        .layer(
            ServiceBuilder::new()
                .layer(
                    CorsLayer::new()
                        .allow_origin(Any)
                        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                        .allow_headers([CONTENT_TYPE, AUTHORIZATION]),
                )
                .layer(TraceLayer::new_for_http())
        )
        .with_state(app_state);
    
    // Merge the API and WebSocket routers
    api_router.merge(ws_router)
}

/// Handler for the metrics dashboard UI
async fn metrics_dashboard_handler() -> impl IntoResponse {
    // Provide a basic HTML page that embeds the metrics from the exporter
    let html = r#"
    <!DOCTYPE html>
    <html>
    <head>
        <title>ICN Agoranet Metrics Dashboard</title>
        <style>
            body { font-family: Arial, sans-serif; margin: 0; padding: 20px; }
            h1 { color: #333; }
            .container { max-width: 1200px; margin: 0 auto; }
            .metrics-frame { width: 100%; height: 800px; border: 1px solid #ddd; }
            .refresh { margin-bottom: 10px; }
        </style>
    </head>
    <body>
        <div class="container">
            <h1>ICN Agoranet Metrics Dashboard</h1>
            <div class="refresh">
                <button onclick="document.getElementById('metrics-frame').src = 'http://localhost:9091/metrics'">
                    Refresh Metrics
                </button>
                <span id="last-refresh"></span>
            </div>
            <iframe id="metrics-frame" class="metrics-frame" src="http://localhost:9091/metrics"></iframe>
        </div>
        <script>
            // Auto-refresh every 5 seconds
            setInterval(() => {
                document.getElementById('metrics-frame').src = 'http://localhost:9091/metrics';
                document.getElementById('last-refresh').textContent = 
                    'Last refreshed: ' + new Date().toLocaleTimeString();
            }, 5000);
            
            document.getElementById('last-refresh').textContent = 
                'Last refreshed: ' + new Date().toLocaleTimeString();
        </script>
    </body>
    </html>
    "#;
    
    Html(html)
}
