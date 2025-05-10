use axum::{
    routing::{get, post},
    Router,
};
// use icn_agoranet::app::create_app; // Removed unused import
use icn_agoranet::handlers::Db;
use icn_agoranet::websocket::WebSocketState;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tower_http::cors::{Any, CorsLayer};
// use tower_http::trace::TraceLayer; // Removed unused import
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

// Import models and handlers from the crate
// use icn_agoranet::models::*; // No longer needed directly here
use icn_agoranet::handlers::InMemoryStore;
use icn_agoranet::ledger::{self, LedgerStore, PostgresLedgerStore};
use std::env;
mod auth;
mod auth_handlers;
mod error;
mod app;
mod handlers;
mod models;
mod org_handlers;
mod websocket;
mod transfers;

#[derive(OpenApi)]
#[openapi(
    paths(
        icn_agoranet::handlers::health_check_handler,
        icn_agoranet::handlers::get_threads_handler,
        icn_agoranet::handlers::create_thread_handler,
        icn_agoranet::handlers::get_thread_detail_handler,
        icn_agoranet::handlers::get_proposals_handler,
        icn_agoranet::handlers::create_proposal_handler,
        icn_agoranet::handlers::get_proposal_detail_handler,
        icn_agoranet::handlers::cast_vote_handler,
        icn_agoranet::handlers::get_proposal_votes_handler,
        // Organization-scoped endpoints
        icn_agoranet::handlers::get_receipts_handler,
        icn_agoranet::handlers::get_receipt_detail_handler,
        icn_agoranet::handlers::get_receipt_stats_handler,
        icn_agoranet::handlers::get_token_balances_handler,
        icn_agoranet::handlers::get_token_transactions_handler,
        icn_agoranet::handlers::get_token_stats_handler,
        // Ledger endpoints
        icn_agoranet::handlers::process_transfer_handler,
        icn_agoranet::handlers::query_transfers,
        icn_agoranet::handlers::batch_transfer_handler,
        icn_agoranet::handlers::get_federation_ledger_stats
    ),
    components(
        schemas(
            icn_agoranet::models::ThreadSummary, icn_agoranet::models::ThreadDetail, icn_agoranet::models::Message,
            icn_agoranet::models::ProposalSummary, icn_agoranet::models::ProposalDetail, icn_agoranet::models::Vote,
            icn_agoranet::models::VoteCounts, icn_agoranet::models::ProposalStatus, icn_agoranet::models::VoteType,
            icn_agoranet::models::NewThreadRequest, icn_agoranet::models::NewProposalRequest, icn_agoranet::models::NewVoteRequest,
            icn_agoranet::models::GetThreadsQuery, icn_agoranet::models::GetProposalsQuery, icn_agoranet::models::ProposalVotesResponse,
            // Organization-scoped schemas
            icn_agoranet::models::ExecutionReceiptSummary, icn_agoranet::models::ExecutionReceiptDetail,
            icn_agoranet::models::TokenBalance, icn_agoranet::models::TokenTransaction,
            icn_agoranet::models::ReceiptStats, icn_agoranet::models::TokenStats,
            icn_agoranet::models::GetReceiptsQuery, icn_agoranet::models::GetTokenBalancesQuery, icn_agoranet::models::GetTokenTransactionsQuery,
            icn_agoranet::models::ReceiptStatsResponse, icn_agoranet::models::TokenStatsResponse,
            // Ledger schemas
            icn_agoranet::models::Transfer, icn_agoranet::models::TransferRequest, icn_agoranet::models::TransferResponse,
            icn_agoranet::models::EntityRef, icn_agoranet::models::EntityType,
            icn_agoranet::handlers::LedgerStats, icn_agoranet::handlers::BatchTransferResponse,
            icn_agoranet::error::ApiError
        )
    ),
    tags(
        (name = "AgoraNet API", description = "ICN Deliberation Layer API")
    )
)]
struct ApiDoc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "icn_agoranet=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Get PostgreSQL connection string from environment variable
    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
        // Default connection string for local development
        "postgres://postgres:postgres@localhost:5432/icn_ledger".to_string()
    });

    // Initialize the ledger store
    let ledger_store: Arc<dyn LedgerStore + Send + Sync> = match env::var("USE_POSTGRES").unwrap_or_else(|_| "true".to_string()).as_str() {
        "true" => {
            // Create PostgreSQL ledger store
            tracing::info!("Initializing PostgreSQL ledger store");
            match ledger::create_pg_ledger_store(&database_url).await {
                Ok(store) => {
                    tracing::info!("PostgreSQL ledger store initialized successfully");
                    Arc::new(store)
                },
                Err(e) => {
                    tracing::error!("Failed to initialize PostgreSQL ledger store: {}", e);
                    tracing::info!("Falling back to in-memory ledger store");
                    Arc::new(transfers::create_example_ledger())
                }
            }
        },
        _ => {
            tracing::info!("Using in-memory ledger store");
            Arc::new(transfers::create_example_ledger())
        }
    };

    // Initialize in-memory store with ledger
    let mut store = InMemoryStore::new();
    store.set_ledger(ledger_store);
    let db: Db = Arc::new(RwLock::new(store));
    
    // Initialize WebSocket state
    let ws_state = WebSocketState::new();
    
    // Start event simulation (for development/testing)
    if std::env::var("SIMULATE_EVENTS").unwrap_or_else(|_| "true".into()) == "true" {
        tracing::info!("Starting WebSocket event simulation");
        ws_state.clone().start_simulation();
    }

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Initialize JWT configuration
    let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| "icn-debug-secret-key".to_string());
    let jwt_config = Arc::new(auth::JwtConfig::new(jwt_secret));
    
    // Initialize token revocation store
    let token_revocation_store: Arc<dyn auth::TokenRevocationStore> = 
        Arc::new(auth::InMemoryTokenRevocationStore::new());

    // Build the REST API router
    let rest_api = Router::new()
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .route("/health", get(icn_agoranet::handlers::health_check_handler))
        // Threads routes
        .route(
            "/threads",
            get(icn_agoranet::handlers::get_threads_handler)
                .post(icn_agoranet::handlers::create_thread_handler),
        )
        .route(
            "/threads/:id",
            get(icn_agoranet::handlers::get_thread_detail_handler),
        )
        // Proposals routes
        .route(
            "/proposals",
            get(icn_agoranet::handlers::get_proposals_handler)
                .post(icn_agoranet::handlers::create_proposal_handler),
        )
        .route(
            "/proposals/:id",
            get(icn_agoranet::handlers::get_proposal_detail_handler),
        )
        // Votes routes
        .route("/votes", post(icn_agoranet::handlers::cast_vote_handler))
        .route(
            "/proposals/:proposal_id/votes",
            get(icn_agoranet::handlers::get_proposal_votes_handler),
        )
        // Organization-scoped receipts routes
        .route(
            "/receipts",
            get(icn_agoranet::handlers::get_receipts_handler),
        )
        .route(
            "/receipts/:cid",
            get(icn_agoranet::handlers::get_receipt_detail_handler),
        )
        .route(
            "/receipts/stats",
            get(icn_agoranet::handlers::get_receipt_stats_handler),
        )
        // Organization-scoped token routes
        .route(
            "/tokens/balances",
            get(icn_agoranet::handlers::get_token_balances_handler),
        )
        .route(
            "/tokens/transactions",
            get(icn_agoranet::handlers::get_token_transactions_handler),
        )
        .route(
            "/tokens/stats",
            get(icn_agoranet::handlers::get_token_stats_handler),
        )
        // Ledger routes
        .route(
            "/federations/:federation_id/transfers",
            post(icn_agoranet::handlers::process_transfer_handler)
                .get(icn_agoranet::handlers::query_transfers),
        )
        .route(
            "/federations/:federation_id/batch-transfers",
            post(icn_agoranet::handlers::batch_transfer_handler),
        )
        .route(
            "/federations/:federation_id/ledger-stats",
            get(icn_agoranet::handlers::get_federation_ledger_stats),
        )
        .layer(cors.clone())
        .with_state((db.clone(), ws_state.clone(), jwt_config.clone(), token_revocation_store.clone()));
    
    // Combine REST API with WebSocket routes
    let app = rest_api.merge(
        icn_agoranet::websocket::websocket_routes()
            .layer(cors)
            .with_state((db, ws_state, jwt_config, token_revocation_store))
    );

    let addr = SocketAddr::from(([0, 0, 0, 0], 8787));
    tracing::info!("listening on {}", addr);
    tracing::info!("Swagger UI available at http://{}/docs", addr);
    tracing::info!("WebSocket API available at ws://{}/ws", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await.unwrap(), app)
        .await
        .unwrap();

    Ok(())
}

// Root handler removed as it's not part of the API spec and /docs serves the UI home.
