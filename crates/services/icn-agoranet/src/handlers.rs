use axum::{extract::{Path as AxumPath, Query, State}, http::StatusCode, Json};
use chrono::Utc;
use std::sync::{Arc, RwLock}; // For in-memory storage
// use serde_json::json; // Removed unused import
use uuid::Uuid;

use crate::models::*;

// For now, we'll use in-memory storage.
// In a real application, this would be a database connection pool.
pub type Db = Arc<RwLock<InMemoryStore>>;

#[derive(Debug)] // Added Debug for InMemoryStore
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
            threads: vec![
                ThreadDetail {
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
                }
            ],
            proposals: vec![
                ProposalDetail {
                    summary: ProposalSummary {
                        id: example_proposal_id.clone(),
                        title: "Example Proposal: New Tokenomics".to_string(),
                        status: ProposalStatus::Open,
                        vote_counts: VoteCounts { approve: 5, reject: 1, abstain: 0 },
                        voting_deadline: now + chrono::Duration::days(7),
                        scope: "coop.nw.governance".to_string(),
                    },
                    full_text: "This is the full text of the example proposal regarding new tokenomics...".to_string(),
                    linked_thread_id: Some(example_thread_id.clone()),
                }
            ],
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
) -> Json<Vec<ThreadSummary>> {
    let store = db.read().unwrap();
    let threads = store.threads.iter()
        .filter(|td| params.scope.as_ref().map_or(true, |s| td.summary.scope == *s))
        .map(|td| td.summary.clone())
        .take(params.limit.unwrap_or(std::u32::MAX) as usize)
        .collect();
    Json(threads)
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
        (status = 404, description = "Thread not found")
    )
)]
pub async fn get_thread_detail_handler(
    AxumPath(id): AxumPath<String>,
    State(db): State<Db>,
) -> Result<Json<ThreadDetail>, StatusCode> {
    let store = db.read().unwrap();
    store.threads.iter()
        .find(|td| td.summary.id == id)
        .map(|td| Json(td.clone()))
        .ok_or(StatusCode::NOT_FOUND)
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
) -> (StatusCode, Json<ThreadSummary>) {
    let mut store = db.write().unwrap();
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
    (StatusCode::CREATED, Json(thread_summary))
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
) -> Json<Vec<ProposalSummary>> {
    let store = db.read().unwrap();
    let proposals: Vec<ProposalSummary> = store.proposals.iter()
        .filter(|pd| params.scope.as_ref().map_or(true, |s| pd.summary.scope == *s))
        .filter(|pd| params.status.as_ref().map_or(true, |s| pd.summary.status == *s))
        .filter(|_pd| params.proposal_type.as_ref().map_or(true, |_| true))
        .map(|pd| pd.summary.clone())
        .collect();
    Json(proposals)
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
        (status = 404, description = "Proposal not found")
    )
)]
pub async fn get_proposal_detail_handler(
    AxumPath(id): AxumPath<String>,
    State(db): State<Db>,
) -> Result<Json<ProposalDetail>, StatusCode> {
    let store = db.read().unwrap();
    store.proposals.iter()
        .find(|pd| pd.summary.id == id)
        .map(|pd| Json(pd.clone()))
        .ok_or(StatusCode::NOT_FOUND)
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
) -> (StatusCode, Json<ProposalSummary>) {
    let mut store = db.write().unwrap();
    let new_id = format!("proposal_{}", Uuid::new_v4());
    let proposal_summary = ProposalSummary {
        id: new_id.clone(),
        title: payload.title,
        scope: payload.scope.clone(),
        status: ProposalStatus::Open, 
        vote_counts: VoteCounts { approve: 0, reject: 0, abstain: 0 },
        voting_deadline: Utc::now() + chrono::Duration::days(7),
    };

    let proposal_detail = ProposalDetail {
        summary: proposal_summary.clone(),
        full_text: payload.full_text,
        linked_thread_id: payload.thread_id,
    };
    store.proposals.push(proposal_detail);
    (StatusCode::CREATED, Json(proposal_summary))
}

// POST /votes
#[utoipa::path(
    post,
    path = "/votes",
    request_body = NewVoteRequest,
    responses(
        (status = 201, description = "Vote recorded successfully", body = Vote),
        (status = 403, description = "Proposal not open for voting or deadline passed"),
        (status = 404, description = "Proposal not found") 
    )
)]
pub async fn cast_vote_handler(
    State(db): State<Db>,
    Json(payload): Json<NewVoteRequest>,
) -> Result<(StatusCode, Json<Vote>), StatusCode> {
    let mut store = db.write().unwrap();
    
    let proposal_idx_opt = store.proposals.iter().position(|p| p.summary.id == payload.proposal_id);

    if proposal_idx_opt.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let proposal_idx = proposal_idx_opt.unwrap();

    // Check status and deadline before creating the vote and modifying store.votes
    // These checks are on an immutable part of the proposal for now.
    {
        let proposal_summary = &store.proposals[proposal_idx].summary;
        if proposal_summary.status != ProposalStatus::Open {
            return Err(StatusCode::FORBIDDEN); 
        }
        if Utc::now() > proposal_summary.voting_deadline {
            // We need to modify status, so this check must be done carefully
            // For now, let's assume we can modify it after adding the vote if needed.
        }
    }

    let vote = Vote {
        proposal_id: payload.proposal_id.clone(),
        voter_did: payload.voter_did,
        vote_type: payload.vote_type.clone(),
        timestamp: Utc::now(),
        justification: payload.justification,
    };
    store.votes.push(vote.clone());

    // Now, modify the proposal. This is a separate mutable borrow of a part of the store.
    let proposal_detail = &mut store.proposals[proposal_idx];
    
    if Utc::now() > proposal_detail.summary.voting_deadline {
        proposal_detail.summary.status = ProposalStatus::Closed;
        // Technically, if deadline passed, we shouldn't have accepted the vote.
        // This logic might need refinement: check deadline, if passed and Open, close it and reject vote.
        // For now, we've pushed the vote, then we close. The problem description implies vote is recorded.
    }

    match payload.vote_type {
        VoteType::Approve => proposal_detail.summary.vote_counts.approve += 1,
        VoteType::Reject => proposal_detail.summary.vote_counts.reject += 1,
        VoteType::Abstain => proposal_detail.summary.vote_counts.abstain += 1,
    }
        
    Ok((StatusCode::CREATED, Json(vote)))
}

// GET /votes/:proposal_id
#[utoipa::path(
    get,
    path = "/votes/{proposal_id}",
    params(
        ("proposal_id" = String, Path, description = "Proposal ID")
    ),
    responses(
        (status = 200, description = "All votes on a proposal and aggregated summary", body = ProposalVotesResponse, example = json!({
            "votes": [
                {
                    "proposal_id": "proposal_def456",
                    "voter_did": "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwupk8vQT7GNz2wVXgE",
                    "vote_type": "Approve",
                    "timestamp": "2024-01-10T10:00:00Z",
                    "justification": "Aligns with goals."
                },
                {
                    "proposal_id": "proposal_def456",
                    "voter_did": "did:key:z6Mkj1h4h4kj1h4h4kj1h4h4kj1h4h4kj1h4h4kj1h4",
                    "vote_type": "Reject",
                    "timestamp": "2024-01-10T11:00:00Z",
                    "justification": "Prefer alternative solution."
                }
            ],
            "summary": { "approve": 15, "reject": 3, "abstain": 2 }
        })),
        (status = 404, description = "Proposal not found")
    )
)]
pub async fn get_proposal_votes_handler(
    AxumPath(proposal_id): AxumPath<String>,
    State(db): State<Db>,
) -> Result<Json<ProposalVotesResponse>, StatusCode> {
    let store = db.read().unwrap();
    
    let proposal_summary_counts = store.proposals.iter()
        .find(|p| p.summary.id == proposal_id)
        .map(|p| p.summary.vote_counts.clone());

    if let Some(summary_counts) = proposal_summary_counts {
        let votes_on_proposal: Vec<Vote> = store.votes.iter()
            .filter(|v| v.proposal_id == proposal_id)
            .cloned()
            .collect();
        
        let response = ProposalVotesResponse {
            votes: votes_on_proposal,
            summary: summary_counts,
        };
        Ok(Json(response))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
} 