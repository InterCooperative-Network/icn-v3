use axum::{
    routing::{get, post},
    Router,
};
// use icn_agoranet::app::create_app; // Removed unused import
use icn_agoranet::handlers::Db;
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
        icn_agoranet::handlers::get_proposal_votes_handler
    ),
    components(
        schemas(
            icn_agoranet::models::ThreadSummary, icn_agoranet::models::ThreadDetail, icn_agoranet::models::Message,
            icn_agoranet::models::ProposalSummary, icn_agoranet::models::ProposalDetail, icn_agoranet::models::Vote,
            icn_agoranet::models::VoteCounts, icn_agoranet::models::ProposalStatus, icn_agoranet::models::VoteType,
            icn_agoranet::models::NewThreadRequest, icn_agoranet::models::NewProposalRequest, icn_agoranet::models::NewVoteRequest,
            icn_agoranet::models::GetThreadsQuery, icn_agoranet::models::GetProposalsQuery, icn_agoranet::models::ProposalVotesResponse,
            icn_agoranet::error::ApiError
        )
    ),
    tags(
        (name = "AgoraNet API", description = "ICN Deliberation Layer API")
    )
)]
struct ApiDoc;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "icn_agoranet=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize in-memory store
    let db: Db = Arc::new(RwLock::new(InMemoryStore::new()));

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .route("/health", get(icn_agoranet::handlers::health_check_handler))
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
        .layer(cors)
        // .layer(Extension(db)); // Extension layer is not needed when using with_state
        .with_state(db); // Pass the state to the router

    let addr = SocketAddr::from(([0, 0, 0, 0], 8787));
    tracing::debug!("listening on {}", addr);
    tracing::debug!("Swagger UI available at http://{}/docs", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await.unwrap(), app)
        .await
        .unwrap();
}

// Root handler removed as it's not part of the API spec and /docs serves the UI home.
