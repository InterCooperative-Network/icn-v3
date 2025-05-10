use axum::{
    routing::{get, post},
    Router,
};
// use std::sync::{Arc, RwLock}; // Removed unused Arc, RwLock
use tower_http::cors::{Any, CorsLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

// Import all necessary components from the current crate
use crate::error::ApiError;
use crate::handlers::{
    cast_vote_handler,
    create_proposal_handler,
    create_thread_handler,
    get_proposal_detail_handler,
    get_proposal_votes_handler,
    get_proposals_handler,
    get_thread_detail_handler,
    get_threads_handler,
    health_check_handler,
    Db, // InMemoryStore, // Removed unused InMemoryStore
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

// Define the OpenAPI documentation structure
// This should be identical to the one in main.rs
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::health_check_handler,
        crate::handlers::get_threads_handler, crate::handlers::create_thread_handler, crate::handlers::get_thread_detail_handler,
        crate::handlers::get_proposals_handler, crate::handlers::create_proposal_handler, crate::handlers::get_proposal_detail_handler,
        crate::handlers::cast_vote_handler, crate::handlers::get_proposal_votes_handler
    ),
    components(
        schemas(
            ThreadSummary, ThreadDetail, Message,
            ProposalSummary, ProposalDetail, Vote,
            VoteCounts, ProposalStatus, VoteType,
            NewThreadRequest, NewProposalRequest, NewVoteRequest,
            GetThreadsQuery, GetProposalsQuery, ProposalVotesResponse,
            ApiError
        )
    ),
    tags(
        (name = "AgoraNet API", description = "ICN Deliberation Layer API")
    )
)]
struct ApiDoc;

pub fn create_app(db: Db) -> Router {
    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .route("/health", get(health_check_handler))
        .route(
            "/threads",
            get(get_threads_handler).post(create_thread_handler),
        )
        .route("/threads/:id", get(get_thread_detail_handler))
        .route(
            "/proposals",
            get(get_proposals_handler).post(create_proposal_handler),
        )
        .route("/proposals/:id", get(get_proposal_detail_handler))
        .route("/votes", post(cast_vote_handler))
        .route(
            "/proposals/:proposal_id/votes",
            get(get_proposal_votes_handler),
        )
        .layer(cors)
        .with_state(db)
}
