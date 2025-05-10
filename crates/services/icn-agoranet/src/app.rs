use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
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
};
use crate::auth_handlers::{
    issue_jwt_token_handler, revoke_token_handler, rotate_token_handler,
    start_revocation_cleanup
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
};
use crate::websocket::websocket_routes;
use crate::auth::{JwtConfig, revocation::InMemoryRevocationStore};

/// API documentation
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::api_health,
        crate::handlers::create_thread_handler,
        crate::handlers::get_threads_handler,
        crate::handlers::get_thread_detail_handler,
        crate::handlers::create_proposal_handler,
        crate::handlers::get_proposals_handler,
        crate::handlers::get_proposal_detail_handler,
        crate::handlers::cast_vote_handler,
        crate::handlers::get_proposal_votes_handler,
    ),
    components(
        schemas(
            crate::models::NewThreadRequest,
            crate::models::ThreadSummary,
            crate::models::ThreadDetail,
            crate::models::NewProposalRequest,
            crate::models::ProposalSummary,
            crate::models::ProposalDetail,
            crate::models::NewVoteRequest,
            crate::models::Vote,
            crate::models::VoteType,
        )
    ),
    tags(
        (name = "ICN AgoraNet API", description = "ICN Governance API")
    )
)]
struct ApiDoc;

/// Create the Axum application with all routes
pub fn create_app(store: Db) -> Router {
    // Define the API documentation for OpenAPI
    let openapi = ApiDoc::openapi();
    
    // Create a JWT config for auth
    let jwt_config = Arc::new(JwtConfig::default());
    
    // Create the WebSocket state
    let ws_state = crate::websocket::WebSocketState::new();
    
    // Create a token revocation store
    let revocation_store = Arc::new(InMemoryRevocationStore::new()) as Arc<dyn crate::auth::revocation::TokenRevocationStore>;
    
    // Start the revocation cleanup process
    crate::handlers::start_revocation_cleanup(revocation_store.clone());
    
    // Start the WebSocket simulation if the simulation flag is set
    if std::env::var("ENABLE_WS_SIMULATION").is_ok() {
        tracing::info!("Starting WebSocket simulation mode");
        ws_state.clone().start_simulation();
    }
    
    // Create the main router with all routes
    let app = Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi))
        .route("/health", get(health_check_handler))
        .route("/threads", post(create_thread_handler).get(get_threads_handler))
        .route("/threads/:thread_id", get(get_thread_detail_handler))
        .route("/proposals", post(create_proposal_handler).get(get_proposals_handler))
        .route("/proposals/:proposal_id", get(get_proposal_detail_handler))
        .route("/proposals/:proposal_id/votes", get(get_proposal_votes_handler))
        .route("/votes", post(cast_vote_handler))
        .nest("/api/v1", api_v1_routes()) // Nest API v1 routes under /api/v1
        .merge(websocket_routes()) // Merge WebSocket routes
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
        .with_state((store, ws_state, jwt_config, revocation_store));
    
    app
}

/// Create API v1 routes (placeholder for future expansion)
fn api_v1_routes() -> Router<(Db, crate::websocket::WebSocketState, Arc<JwtConfig>, Arc<dyn crate::auth::revocation::TokenRevocationStore>)> {
    Router::new()
        .route("/health", get(health_check_handler))
        // Organization-scoped authorized routes
        .route("/receipts", get(get_receipts_authorized))
        .route("/tokens/balances", get(get_token_balances_authorized))
        .route("/tokens/transactions", get(get_token_transactions_authorized))
        .route("/stats/receipts", get(get_receipt_stats_authorized))
        .route("/stats/tokens", get(get_token_stats_authorized))
        // Federation-specific routes for authorization management
        .route("/federation/:federation_id/tokens", post(issue_jwt_token_handler))
        .route("/federation/:federation_id/tokens/revoke", post(revoke_token_handler))
        .route("/federation/:federation_id/tokens/rotate", post(rotate_token_handler))
}
