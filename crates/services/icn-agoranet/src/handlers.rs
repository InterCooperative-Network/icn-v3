use axum::{
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use std::sync::{Arc, RwLock}; // For in-memory storage
                              // use serde_json::json; // Removed unused import
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::*; // Added ApiError import

// For now, we'll use in-memory storage.
// In a real application, this would be a database connection pool.
pub type Db = Arc<RwLock<InMemoryStore>>;

#[derive(Debug, Default)] // Added Default to satisfy clippy::new_without_default
pub struct InMemoryStore {
    threads: Vec<ThreadDetail>,
    proposals: Vec<ProposalDetail>,
    votes: Vec<Vote>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        // Initialize with some example data for now
        let example_thread_id = format!("thread_{}", Uuid::new_v4());
        let example_proposal_id = format!("proposal_{}", Uuid::new_v4());
        let now = Utc::now();

        Self {
            threads: vec![ThreadDetail {
                summary: ThreadSummary {
                    id: example_thread_id.clone(),
                    title: "Example Thread 1: Discussing the Future".to_string(),
                    created_at: now,
                    author_did: "did:example:author1".to_string(),
                    scope: "coop.nw".to_string(),
                },
                messages: vec![
                    Message {
                        id: format!("msg_{}", Uuid::new_v4()),
                        author_did: "did:example:author1".to_string(),
                        timestamp: now,
                        content: "Initial message in thread 1.".to_string(),
                    },
                    Message {
                        id: format!("msg_{}", Uuid::new_v4()),
                        author_did: "did:example:author2".to_string(),
                        timestamp: now,
                        content: "Replying to thread 1.".to_string(),
                    },
                ],
            }],
            proposals: vec![ProposalDetail {
                summary: ProposalSummary {
                    id: example_proposal_id.clone(),
                    title: "Example Proposal: New Tokenomics".to_string(),
                    status: ProposalStatus::Open,
                    vote_counts: VoteCounts {
                        approve: 5,
                        reject: 1,
                        abstain: 0,
                    },
                    voting_deadline: now + chrono::Duration::days(7),
                    scope: "coop.nw.governance".to_string(),
                },
                full_text:
                    "This is the full text of the example proposal regarding new tokenomics..."
                        .to_string(),
                linked_thread_id: Some(example_thread_id.clone()),
            }],
            votes: vec![
                Vote {
                    proposal_id: example_proposal_id.clone(),
                    voter_did: "did:example:voter1".to_string(),
                    vote_type: VoteType::Approve,
                    timestamp: now,
                    justification: Some("This seems like a good idea.".to_string()),
                },
                Vote {
                    proposal_id: example_proposal_id.clone(),
                    voter_did: "did:example:voter2".to_string(),
                    vote_type: VoteType::Reject,
                    timestamp: now,
                    justification: Some("I have some concerns.".to_string()),
                },
            ],
        }
    }

    pub fn add_proposal_for_test(&mut self, proposal: ProposalDetail) {
        self.proposals.push(proposal);
    }

    pub fn add_vote_for_test(&mut self, vote: Vote) {
        let proposal_id_clone = vote.proposal_id.clone();
        let vote_type_clone = vote.vote_type;

        self.votes.push(vote);

        if let Some(proposal_detail) = self.proposals.iter_mut().find(|p| p.summary.id == proposal_id_clone) {
            match vote_type_clone {
                VoteType::Approve => proposal_detail.summary.vote_counts.approve += 1,
                VoteType::Reject => proposal_detail.summary.vote_counts.reject += 1,
                VoteType::Abstain => proposal_detail.summary.vote_counts.abstain += 1,
            }
        }
    }
}

// GET /threads
#[utoipa::path(
    get,
    path = "/threads",
    params(
        GetThreadsQuery
    ),
    responses(
        (status = 200, description = "List of thread summaries", body = Vec<ThreadSummary>, example = json!([
            {
                "id": "thread_abc123",
                "title": "Discussion about new governance model",
                "created_at": "2024-01-01T12:00:00Z",
                "author_did": "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwupk8vQT7GNz2wVXgE",
                "scope": "coop.nw"
            },
            {
                "id": "thread_xyz789",
                "title": "Ideas for community grants",
                "created_at": "2024-01-02T15:30:00Z",
                "author_did": "did:key:z6Mkj1h4h4kj1h4h4kj1h4h4kj1h4h4kj1h4h4kj1h4",
                "scope": "coop.nw.grants"
            }
        ]))
    )
)]
pub async fn get_threads_handler(
    Query(params): Query<GetThreadsQuery>,
    State(db): State<Db>,
) -> Result<Json<Vec<ThreadSummary>>, ApiError> {
    let store = db
        .read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    let threads = store
        .threads
        .iter()
        .filter(|td| params.scope.as_ref().is_none_or(|s| td.summary.scope == *s)) // clippy: unnecessary_map_or
        .map(|td| td.summary.clone())
        .take(params.limit.unwrap_or(u32::MAX) as usize) // clippy: legacy_numeric_constants
        .collect();
    Ok(Json(threads))
}

// GET /threads/:id
#[utoipa::path(
    get,
    path = "/threads/{id}",
    params(
        ("id" = String, Path, description = "Thread ID")
    ),
    responses(
        (status = 200, description = "Thread detail", body = ThreadDetail),
        (status = 404, description = "Thread not found", body = ApiError, example = json!({ "error": "Thread not found" }))
    )
)]
pub async fn get_thread_detail_handler(
    AxumPath(id): AxumPath<String>,
    State(db): State<Db>,
) -> Result<Json<ThreadDetail>, ApiError> {
    let store = db
        .read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    store
        .threads
        .iter()
        .find(|td| td.summary.id == id)
        .map(|td| Json(td.clone()))
        .ok_or_else(|| ApiError::NotFound(format!("Thread with id {} not found", id)))
}

// POST /threads
#[utoipa::path(
    post,
    path = "/threads",
    request_body = NewThreadRequest,
    responses(
        (status = 201, description = "Thread created successfully", body = ThreadSummary)
    )
)]
pub async fn create_thread_handler(
    State(db): State<Db>,
    Json(payload): Json<NewThreadRequest>,
) -> Result<(StatusCode, Json<ThreadSummary>), ApiError> {
    let mut store = db
        .write()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire write lock".to_string()))?;
    let new_id = format!("thread_{}", Uuid::new_v4());
    let thread_summary = ThreadSummary {
        id: new_id.clone(),
        title: payload.title,
        created_at: Utc::now(),
        author_did: payload.author_did,
        scope: payload.scope,
    };
    let thread_detail = ThreadDetail {
        summary: thread_summary.clone(),
        messages: Vec::new(),
    };
    store.threads.push(thread_detail);
    Ok((StatusCode::CREATED, Json(thread_summary)))
}

// GET /proposals
#[utoipa::path(
    get,
    path = "/proposals",
    params(
        GetProposalsQuery
    ),
    responses(
        (status = 200, description = "List of proposal summaries", body = Vec<ProposalSummary>)
    )
)]
pub async fn get_proposals_handler(
    Query(params): Query<GetProposalsQuery>,
    State(db): State<Db>,
) -> Result<Json<Vec<ProposalSummary>>, ApiError> {
    let store = db
        .read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    let proposals: Vec<ProposalSummary> = store
        .proposals
        .iter()
        .filter(|pd| params.scope.as_ref().is_none_or(|s| pd.summary.scope == *s)) // clippy: unnecessary_map_or
        .filter(|pd| {
            params
                .status
                .as_ref()
                .is_none_or(|s| pd.summary.status == *s)
        }) // clippy: unnecessary_map_or
        .filter(|_pd| params.proposal_type.as_ref().is_none_or(|_| true)) // clippy: unnecessary_map_or
        .map(|pd| pd.summary.clone())
        .collect();
    Ok(Json(proposals))
}

// GET /proposals/:id
#[utoipa::path(
    get,
    path = "/proposals/{id}",
    params(
        ("id" = String, Path, description = "Proposal ID")
    ),
    responses(
        (status = 200, description = "Proposal detail", body = ProposalDetail, example = json!({
            "id": "proposal_def456",
            "title": "Implement new fee structure",
            "scope": "coop.nw.governance",
            "status": "Open",
            "vote_counts": { "approve": 15, "reject": 3, "abstain": 2 },
            "voting_deadline": "2024-01-15T18:00:00Z",
            "full_text": "This proposal outlines a new fee structure for the network...",
            "linked_thread_id": "thread_abc123"
        })),
        (status = 404, description = "Proposal not found", body = ApiError, example = json!({ "error": "Proposal not found" }))
    )
)]
pub async fn get_proposal_detail_handler(
    AxumPath(id): AxumPath<String>,
    State(db): State<Db>,
) -> Result<Json<ProposalDetail>, ApiError> {
    let store = db
        .read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    store
        .proposals
        .iter()
        .find(|pd| pd.summary.id == id)
        .map(|pd| Json(pd.clone()))
        .ok_or_else(|| ApiError::NotFound(format!("Proposal with id {} not found", id)))
}

// POST /proposals
#[utoipa::path(
    post,
    path = "/proposals",
    request_body = NewProposalRequest,
    responses(
        (status = 201, description = "Proposal created successfully", body = ProposalSummary)
    )
)]
pub async fn create_proposal_handler(
    State(db): State<Db>,
    Json(payload): Json<NewProposalRequest>,
) -> Result<(StatusCode, Json<ProposalSummary>), ApiError> {
    let mut store = db
        .write()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire write lock".to_string()))?;
    let new_id = format!("proposal_{}", Uuid::new_v4());
    let proposal_summary = ProposalSummary {
        id: new_id.clone(),
        title: payload.title,
        scope: payload.scope,
        status: ProposalStatus::Open, // Default to Open
        vote_counts: VoteCounts {
            approve: 0,
            reject: 0,
            abstain: 0,
        },
        voting_deadline: payload
            .voting_deadline
            .unwrap_or_else(|| Utc::now() + chrono::Duration::days(7)), // Default voting period
    };
    let proposal_detail = ProposalDetail {
        summary: proposal_summary.clone(),
        full_text: payload.full_text,
        linked_thread_id: payload.linked_thread_id,
    };
    store.proposals.push(proposal_detail);
    Ok((StatusCode::CREATED, Json(proposal_summary)))
}

// POST /votes
#[utoipa::path(
    post,
    path = "/votes",
    request_body = NewVoteRequest,
    responses(
        (status = 201, description = "Vote cast successfully", body = Vote),
        (status = 400, description = "Invalid vote (e.g., proposal not open, voter already voted)", body = ApiError, example = json!({ "error": "Invalid vote" })),
        (status = 404, description = "Proposal not found", body = ApiError, example = json!({ "error": "Proposal not found" }))
    )
)]
pub async fn cast_vote_handler(
    State(db): State<Db>,
    Json(payload): Json<NewVoteRequest>,
) -> Result<(StatusCode, Json<Vote>), ApiError> {
    let mut store = db
        .write()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire write lock".to_string()))?;

    if store
        .votes
        .iter()
        .any(|v| v.proposal_id == payload.proposal_id && v.voter_did == payload.voter_did)
    {
        return Err(ApiError::BadRequest(format!(
            "Voter {} has already voted on proposal {}",
            payload.voter_did, payload.proposal_id
        )));
    }

    let proposal_detail = store
        .proposals
        .iter_mut()
        .find(|p| p.summary.id == payload.proposal_id)
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "Proposal with id {} not found",
                payload.proposal_id
            ))
        })?;

    if proposal_detail.summary.status != ProposalStatus::Open {
        return Err(ApiError::BadRequest(
            "Proposal is not open for voting".to_string(),
        ));
    }
    if Utc::now() > proposal_detail.summary.voting_deadline {
        proposal_detail.summary.status = ProposalStatus::Closed;
        return Err(ApiError::BadRequest(
            "Voting deadline has passed for this proposal".to_string(),
        ));
    }

    match payload.vote_type {
        VoteType::Approve => proposal_detail.summary.vote_counts.approve += 1,
        VoteType::Reject => proposal_detail.summary.vote_counts.reject += 1,
        VoteType::Abstain => proposal_detail.summary.vote_counts.abstain += 1,
    }

    let vote = Vote {
        proposal_id: payload.proposal_id.clone(),
        voter_did: payload.voter_did.clone(),
        vote_type: payload.vote_type,
        timestamp: Utc::now(),
        justification: payload.justification,
    };

    store.votes.push(vote.clone());

    Ok((StatusCode::CREATED, Json(vote)))
}

// GET /proposals/{proposal_id}/votes
#[utoipa::path(
    get,
    path = "/proposals/{proposal_id}/votes",
    params(
        ("proposal_id" = String, Path, description = "Proposal ID")
    ),
    responses(
        (status = 200, description = "List of votes for the proposal", body = ProposalVotesResponse),
        (status = 404, description = "Proposal not found", body = ApiError, example = json!({ "error": "Proposal not found" }))
    )
)]
pub async fn get_proposal_votes_handler(
    AxumPath(proposal_id): AxumPath<String>,
    State(db): State<Db>,
) -> Result<Json<ProposalVotesResponse>, ApiError> {
    let store = db
        .read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;

    if !store.proposals.iter().any(|p| p.summary.id == proposal_id) {
        return Err(ApiError::NotFound(format!(
            "Proposal with id {} not found",
            proposal_id
        )));
    }

    let votes_for_proposal: Vec<Vote> = store
        .votes
        .iter()
        .filter(|v| v.proposal_id == proposal_id)
        .cloned()
        .collect();

    Ok(Json(ProposalVotesResponse {
        proposal_id,
        votes: votes_for_proposal,
    }))
}

// Example health check endpoint
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is healthy")
    )
)]
pub async fn health_check_handler() -> StatusCode {
    StatusCode::OK
}
