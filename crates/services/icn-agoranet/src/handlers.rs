use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use std::sync::{Arc, RwLock}; // For in-memory storage
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Duration};
use serde_json::Value;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error::ApiError;
use crate::models::*; // Added ApiError import
use crate::auth::{
    AuthenticatedRequest, AuthError, 
    TokenIssueRequest, TokenResponse,
    issue_token, ensure_federation_admin,
    JwtConfig,
    check_transfer_from_permission,
    revocation::TokenRevocationStore
};
use crate::models::{
    EntityRef, 
    EntityType, 
    Transfer, 
    TransferRequest, 
    TransferResponse,
};
use crate::websocket::WebSocketState;

// Define the errors locally using similar structure
#[derive(Debug, Error)]
pub enum TransferError {
    #[error("insufficient balance")]
    InsufficientBalance,
    
    #[error("invalid amount")]
    InvalidAmount,
    
    #[error("entity not found: {0}")]
    EntityNotFound(String),
    
    #[error("transfer not found: {0}")]
    TransferNotFound(Uuid),
    
    #[error("federation mismatch")]
    FederationMismatch,
    
    #[error("internal error: {0}")]
    Internal(String),
}

// Define the query parameters locally
#[derive(Debug, Deserialize)]
pub struct TransferQuery {
    /// Federation ID to filter by
    pub federation_id: Option<String>,
    /// Entity ID to filter by (from or to)
    pub entity_id: Option<String>,
    /// Entity type to filter by
    pub entity_type: Option<EntityType>,
    /// Only include transfers where the entity is the source
    pub from_only: Option<bool>,
    /// Only include transfers where the entity is the destination
    pub to_only: Option<bool>,
    /// Start date for filtering
    pub start_date: Option<DateTime<Utc>>,
    /// End date for filtering
    pub end_date: Option<DateTime<Utc>>,
    /// Minimum amount to include
    pub min_amount: Option<u64>,
    /// Maximum amount to include
    pub max_amount: Option<u64>,
    /// Limit the number of results
    pub limit: Option<u32>,
    /// Offset for pagination
    pub offset: Option<u32>,
}

// Define the batch response locally
#[derive(Debug, Serialize)]
pub struct BatchTransferResponse {
    /// Number of successful transfers
    pub successful: usize,
    /// Number of failed transfers
    pub failed: usize,
    /// IDs of successful transfers
    pub successful_ids: Vec<Uuid>,
    /// Failed transfers with error messages
    pub failed_transfers: Vec<(usize, String)>,
    /// Total amount transferred successfully
    pub total_transferred: u64,
    /// Total fees collected
    pub total_fees: u64,
}

// Define ledger statistics locally
#[derive(Debug, Serialize)]
pub struct LedgerStats {
    /// Total number of transfers
    pub total_transfers: usize,
    /// Total volume transferred
    pub total_volume: u64,
    /// Total fees collected
    pub total_fees: u64,
    /// Total number of entities in the ledger
    pub total_entities: usize,
    /// Number of active entities (with non-zero balance)
    pub active_entities: usize,
    /// Entity with highest balance
    pub highest_balance_entity: Option<EntityRef>,
    /// Highest balance amount
    pub highest_balance: u64,
    /// Total transfers in the last 24 hours
    pub transfers_last_24h: usize,
    /// Volume in the last 24 hours
    pub volume_last_24h: u64,
}

// Define the ledger and ledger store types
#[derive(Debug)]
pub struct Ledger {
    /// Entity balances by ID and entity type
    balances: HashMap<String, u64>, // Simplified to just use the entity ID as key
    /// Historical transfers
    transfers: Vec<Transfer>,
    /// Federation balances (federation_id -> total balance)
    federation_stats: HashMap<String, u64>,
}

// Thread-safe ledger with read-write locking
pub type LedgerStore = Arc<RwLock<Ledger>>;

// For now, we'll use in-memory storage.
// In a real application, this would be a database connection pool.
pub type Db = Arc<RwLock<InMemoryStore>>;

#[derive(Debug, Default)] // Added Default to satisfy clippy::new_without_default
pub struct InMemoryStore {
    threads: Vec<ThreadDetail>,
    proposals: Vec<ProposalDetail>,
    votes: Vec<Vote>,
    // New fields for organization-scoped resources
    receipts: Vec<ExecutionReceiptDetail>,
    token_balances: Vec<TokenBalance>,
    token_transactions: Vec<TokenTransaction>,
    // Add the ledger for persistent balance tracking
    ledger: Option<LedgerStore>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        // Initialize with some example data for now
        let example_thread_id = format!("thread_{}", Uuid::new_v4());
        let example_proposal_id = format!("proposal_{}", Uuid::new_v4());
        let now = Utc::now();

        // Create mock receipts with organization scoping
        let receipts = vec![
            // Global receipt (no org scoping)
            ExecutionReceiptDetail {
                summary: ExecutionReceiptSummary {
                    cid: format!("bafy2bzace{}", Uuid::new_v4()),
                    executor: "did:icn:node1".to_string(),
                    resource_usage: HashMap::from([
                        ("CPU".to_string(), 250),
                        ("Memory".to_string(), 1024),
                    ]),
                    timestamp: now - Duration::hours(1),
                    coop_id: None,
                    community_id: None,
                },
                task_cid: format!("task-{}", Uuid::new_v4()),
                anchored_cids: vec![format!("anchor-{}", Uuid::new_v4())],
                signature: "SomeBase64Signature==".to_string(),
            },
            // Cooperative-scoped receipt
            ExecutionReceiptDetail {
                summary: ExecutionReceiptSummary {
                    cid: format!("bafy2bzace{}", Uuid::new_v4()),
                    executor: "did:icn:node2".to_string(),
                    resource_usage: HashMap::from([
                        ("CPU".to_string(), 300),
                        ("Memory".to_string(), 2048),
                    ]),
                    timestamp: now - Duration::hours(2),
                    coop_id: Some("coop-123".to_string()),
                    community_id: None,
                },
                task_cid: format!("task-{}", Uuid::new_v4()),
                anchored_cids: vec![format!("anchor-{}", Uuid::new_v4())],
                signature: "AnotherBase64Signature==".to_string(),
            },
            // Community-scoped receipt
            ExecutionReceiptDetail {
                summary: ExecutionReceiptSummary {
                    cid: format!("bafy2bzace{}", Uuid::new_v4()),
                    executor: "did:icn:node3".to_string(),
                    resource_usage: HashMap::from([
                        ("CPU".to_string(), 150),
                        ("Memory".to_string(), 512),
                    ]),
                    timestamp: now - Duration::hours(3),
                    coop_id: Some("coop-123".to_string()),
                    community_id: Some("community-456".to_string()),
                },
                task_cid: format!("task-{}", Uuid::new_v4()),
                anchored_cids: vec![format!("anchor-{}", Uuid::new_v4())],
                signature: "ThirdBase64Signature==".to_string(),
            },
        ];

        // Create mock token balances with organization scoping
        let token_balances = vec![
            // Global balance (no org scoping)
            TokenBalance {
                did: "did:icn:user1".to_string(),
                balance: 15000,
                coop_id: None,
                community_id: None,
            },
            // Cooperative-scoped balance
            TokenBalance {
                did: "did:icn:user1".to_string(),
                balance: 5000,
                coop_id: Some("coop-123".to_string()),
                community_id: None,
            },
            // Community-scoped balance
            TokenBalance {
                did: "did:icn:user1".to_string(),
                balance: 2000,
                coop_id: Some("coop-123".to_string()),
                community_id: Some("community-456".to_string()),
            },
            // Another user with different balances
            TokenBalance {
                did: "did:icn:user2".to_string(),
                balance: 25000,
                coop_id: None,
                community_id: None,
            },
            TokenBalance {
                did: "did:icn:user2".to_string(),
                balance: 10000,
                coop_id: Some("coop-123".to_string()),
                community_id: None,
            },
        ];

        // Create mock token transactions with organization scoping
        let token_transactions = vec![
            // Global transaction (no org scoping)
            TokenTransaction {
                id: format!("tx-{}", Uuid::new_v4()),
                from_did: "did:icn:treasury".to_string(),
                to_did: "did:icn:user1".to_string(),
                amount: 1000,
                operation: "mint".to_string(),
                timestamp: now - Duration::days(1),
                from_coop_id: None,
                from_community_id: None,
                to_coop_id: None,
                to_community_id: None,
            },
            // Cooperative-scoped transaction
            TokenTransaction {
                id: format!("tx-{}", Uuid::new_v4()),
                from_did: "did:icn:user1".to_string(),
                to_did: "did:icn:user2".to_string(),
                amount: 500,
                operation: "transfer".to_string(),
                timestamp: now - Duration::days(2),
                from_coop_id: Some("coop-123".to_string()),
                from_community_id: None,
                to_coop_id: Some("coop-123".to_string()),
                to_community_id: None,
            },
            // Community-scoped transaction
            TokenTransaction {
                id: format!("tx-{}", Uuid::new_v4()),
                from_did: "did:icn:user2".to_string(),
                to_did: "did:icn:treasury".to_string(),
                amount: 200,
                operation: "burn".to_string(),
                timestamp: now - Duration::days(3),
                from_coop_id: Some("coop-123".to_string()),
                from_community_id: Some("community-456".to_string()),
                to_coop_id: None,
                to_community_id: None,
            },
        ];

        // Create the store with all initialized data
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
                    voting_deadline: now + Duration::days(7),
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
            receipts,
            token_balances,
            token_transactions,
            ledger: None,
        }
    }

    pub fn add_proposal_for_test(&mut self, proposal: ProposalDetail) {
        self.proposals.push(proposal);
    }

    pub fn add_vote_for_test(&mut self, vote: Vote) {
        let proposal_id_clone = vote.proposal_id.clone();
        let vote_type_clone = vote.vote_type;

        self.votes.push(vote);

        if let Some(proposal_detail) = self
            .proposals
            .iter_mut()
            .find(|p| p.summary.id == proposal_id_clone)
        {
            match vote_type_clone {
                VoteType::Approve => proposal_detail.summary.vote_counts.approve += 1,
                VoteType::Reject => proposal_detail.summary.vote_counts.reject += 1,
                VoteType::Abstain => proposal_detail.summary.vote_counts.abstain += 1,
            }
        }
    }

    // New methods for organization-scoped resources

    // Filter receipts based on query parameters
    pub fn filter_receipts(&self, params: &GetReceiptsQuery) -> Vec<ExecutionReceiptSummary> {
        self.receipts
            .iter()
            .filter(|r| {
                // Filter by executor if provided
                if let Some(executor) = &params.executor {
                    if r.summary.executor != *executor {
                        return false;
                    }
                }
                
                // Filter by date if provided
                if let Some(date) = &params.date {
                    let receipt_date = r.summary.timestamp.date().to_string();
                    if !receipt_date.starts_with(date) {
                        return false;
                    }
                }
                
                // Filter by cooperative ID
                if let Some(coop_id) = &params.coop_id {
                    match &r.summary.coop_id {
                        Some(receipt_coop_id) if receipt_coop_id == coop_id => {}
                        _ => return false,
                    }
                }
                
                // Filter by community ID
                if let Some(community_id) = &params.community_id {
                    match &r.summary.community_id {
                        Some(receipt_community_id) if receipt_community_id == community_id => {}
                        _ => return false,
                    }
                }
                
                true
            })
            .map(|r| r.summary.clone())
            .collect()
    }
    
    // Filter token balances based on query parameters
    pub fn filter_token_balances(&self, params: &GetTokenBalancesQuery) -> Vec<TokenBalance> {
        self.token_balances
            .iter()
            .filter(|b| {
                // Filter by account if provided
                if let Some(account) = &params.account {
                    if b.did != *account {
                        return false;
                    }
                }
                
                // Filter by cooperative ID
                if let Some(coop_id) = &params.coop_id {
                    match &b.coop_id {
                        Some(balance_coop_id) if balance_coop_id == coop_id => {}
                        _ => return false,
                    }
                }
                
                // Filter by community ID
                if let Some(community_id) = &params.community_id {
                    match &b.community_id {
                        Some(balance_community_id) if balance_community_id == community_id => {}
                        _ => return false,
                    }
                }
                
                true
            })
            .cloned()
            .collect()
    }
    
    // Filter token transactions based on query parameters
    pub fn filter_token_transactions(&self, params: &GetTokenTransactionsQuery) -> Vec<TokenTransaction> {
        self.token_transactions
            .iter()
            .filter(|tx| {
                // Filter by account if provided (either sender or recipient)
                if let Some(account) = &params.account {
                    if tx.from_did != *account && tx.to_did != *account {
                        return false;
                    }
                }
                
                // Filter by date if provided
                if let Some(date) = &params.date {
                    let tx_date = tx.timestamp.date().to_string();
                    if !tx_date.starts_with(date) {
                        return false;
                    }
                }
                
                // Filter by cooperative ID (either sender or recipient's coop)
                if let Some(coop_id) = &params.coop_id {
                    let from_match = tx.from_coop_id.as_ref().map_or(false, |id| id == coop_id);
                    let to_match = tx.to_coop_id.as_ref().map_or(false, |id| id == coop_id);
                    if !from_match && !to_match {
                        return false;
                    }
                }
                
                // Filter by community ID (either sender or recipient's community)
                if let Some(community_id) = &params.community_id {
                    let from_match = tx.from_community_id.as_ref().map_or(false, |id| id == community_id);
                    let to_match = tx.to_community_id.as_ref().map_or(false, |id| id == community_id);
                    if !from_match && !to_match {
                        return false;
                    }
                }
                
                true
            })
            .cloned()
            .collect()
    }
    
    // Calculate receipt statistics for a specific organization scope
    pub fn get_receipt_stats(&self, coop_id: Option<&str>, community_id: Option<&str>) -> ReceiptStats {
        // Filter receipts based on org scope
        let filtered_receipts: Vec<_> = self.receipts
            .iter()
            .filter(|r| {
                if let Some(cid) = coop_id {
                    match &r.summary.coop_id {
                        Some(receipt_coop_id) if receipt_coop_id == cid => {}
                        _ => return false,
                    }
                }
                
                if let Some(cid) = community_id {
                    match &r.summary.community_id {
                        Some(receipt_community_id) if receipt_community_id == cid => {}
                        _ => return false,
                    }
                }
                
                true
            })
            .collect();
        
        let total_receipts = filtered_receipts.len() as u64;
        
        // Calculate average resource usage
        let mut total_cpu = 0;
        let mut total_memory = 0;
        let mut total_storage = 0;
        let mut receipts_by_executor: HashMap<String, u64> = HashMap::new();
        
        for receipt in filtered_receipts {
            // Count by executor
            let executor = &receipt.summary.executor;
            *receipts_by_executor.entry(executor.clone()).or_insert(0) += 1;
            
            // Sum resource usage
            let cpu = receipt.summary.resource_usage.get("CPU").copied().unwrap_or(0);
            let memory = receipt.summary.resource_usage.get("Memory").copied().unwrap_or(0);
            let storage = receipt.summary.resource_usage.get("Storage").copied().unwrap_or(0);
            
            total_cpu += cpu;
            total_memory += memory;
            total_storage += storage;
        }
        
        // Calculate averages, avoiding division by zero
        let avg_cpu = if total_receipts > 0 { total_cpu / total_receipts } else { 0 };
        let avg_memory = if total_receipts > 0 { total_memory / total_receipts } else { 0 };
        let avg_storage = if total_receipts > 0 { total_storage / total_receipts } else { 0 };
        
        ReceiptStats {
            total_receipts,
            avg_cpu_usage: avg_cpu,
            avg_memory_usage: avg_memory,
            avg_storage_usage: avg_storage,
            receipts_by_executor,
        }
    }
    
    // Calculate token statistics for a specific organization scope
    pub fn get_token_stats(&self, coop_id: Option<&str>, community_id: Option<&str>) -> TokenStats {
        // Filter transactions based on org scope
        let filtered_transactions: Vec<_> = self.token_transactions
            .iter()
            .filter(|tx| {
                if let Some(cid) = coop_id {
                    let from_match = tx.from_coop_id.as_ref().map_or(false, |id| id == cid);
                    let to_match = tx.to_coop_id.as_ref().map_or(false, |id| id == cid);
                    if !from_match && !to_match {
                        return false;
                    }
                }
                
                if let Some(cid) = community_id {
                    let from_match = tx.from_community_id.as_ref().map_or(false, |id| id == cid);
                    let to_match = tx.to_community_id.as_ref().map_or(false, |id| id == cid);
                    if !from_match && !to_match {
                        return false;
                    }
                }
                
                true
            })
            .collect();
        
        // Calculate stats
        let mut total_minted = 0;
        let mut total_burnt = 0;
        let mut daily_volume = 0;
        let mut active_accounts = std::collections::HashSet::new();
        
        for tx in filtered_transactions {
            match tx.operation.as_str() {
                "mint" => total_minted += tx.amount,
                "burn" => total_burnt += tx.amount,
                _ => {}
            }
            
            daily_volume += tx.amount;
            active_accounts.insert(tx.from_did.clone());
            active_accounts.insert(tx.to_did.clone());
        }
        
        TokenStats {
            total_minted,
            total_burnt,
            active_accounts: active_accounts.len() as u64,
            daily_volume: Some(daily_volume),
        }
    }
    
    // Add a setter method for the ledger field
    pub fn set_ledger(&mut self, ledger: impl Into<Option<LedgerStore>>) {
        self.ledger = ledger.into();
    }
    
    // Add a getter method to retrieve the ledger
    pub fn get_ledger(&self) -> Option<LedgerStore> {
        self.ledger.clone()
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
    Path(id): Path<String>,
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
    Path(id): Path<String>,
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
    Path(proposal_id): Path<String>,
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

// GET /receipts
#[utoipa::path(
    get,
    path = "/receipts",
    params(
        GetReceiptsQuery
    ),
    responses(
        (status = 200, description = "List of execution receipt summaries", body = Vec<ExecutionReceiptSummary>)
    )
)]
pub async fn get_receipts_handler(
    Query(params): Query<GetReceiptsQuery>,
    State(db): State<Db>,
) -> Result<Json<Vec<ExecutionReceiptSummary>>, ApiError> {
    let store = db
        .read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    
    let receipts = store.filter_receipts(&params);
    
    // Apply pagination if specified
    let paginated_receipts = match (params.offset, params.limit) {
        (Some(offset), Some(limit)) => {
            receipts
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect()
        }
        (Some(offset), None) => receipts.into_iter().skip(offset as usize).collect(),
        (None, Some(limit)) => receipts.into_iter().take(limit as usize).collect(),
        (None, None) => receipts,
    };
    
    Ok(Json(paginated_receipts))
}

// GET /receipts/{cid}
#[utoipa::path(
    get,
    path = "/receipts/{cid}",
    params(
        ("cid" = String, Path, description = "Receipt CID")
    ),
    responses(
        (status = 200, description = "Receipt detail", body = ExecutionReceiptDetail),
        (status = 404, description = "Receipt not found")
    )
)]
pub async fn get_receipt_detail_handler(
    Path(cid): Path<String>,
    State(db): State<Db>,
) -> Result<Json<ExecutionReceiptDetail>, ApiError> {
    let store = db
        .read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    
    store
        .receipts
        .iter()
        .find(|r| r.summary.cid == cid)
        .map(|r| Json(r.clone()))
        .ok_or_else(|| ApiError::NotFound(format!("Receipt with CID {} not found", cid)))
}

// GET /receipts/stats
#[utoipa::path(
    get,
    path = "/receipts/stats",
    params(
        ("coop_id" = Option<String>, Query, description = "Cooperative ID to filter by"),
        ("community_id" = Option<String>, Query, description = "Community ID to filter by")
    ),
    responses(
        (status = 200, description = "Receipt statistics", body = ReceiptStatsResponse)
    )
)]
pub async fn get_receipt_stats_handler(
    Query(params): Query<GetReceiptsQuery>,
    State(db): State<Db>,
) -> Result<Json<ReceiptStatsResponse>, ApiError> {
    let store = db
        .read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    
    let stats = store.get_receipt_stats(
        params.coop_id.as_deref(),
        params.community_id.as_deref()
    );
    
    let response = ReceiptStatsResponse {
        stats,
        coop_id: params.coop_id,
        community_id: params.community_id,
    };
    
    Ok(Json(response))
}

// GET /tokens/balances
#[utoipa::path(
    get,
    path = "/tokens/balances",
    params(
        GetTokenBalancesQuery
    ),
    responses(
        (status = 200, description = "List of token balances", body = Vec<TokenBalance>)
    )
)]
pub async fn get_token_balances_handler(
    Query(params): Query<GetTokenBalancesQuery>,
    State(db): State<Db>,
) -> Result<Json<Vec<TokenBalance>>, ApiError> {
    let store = db
        .read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    
    let balances = store.filter_token_balances(&params);
    
    // Apply pagination if specified
    let paginated_balances = match (params.offset, params.limit) {
        (Some(offset), Some(limit)) => {
            balances
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect()
        }
        (Some(offset), None) => balances.into_iter().skip(offset as usize).collect(),
        (None, Some(limit)) => balances.into_iter().take(limit as usize).collect(),
        (None, None) => balances,
    };
    
    Ok(Json(paginated_balances))
}

// GET /tokens/transactions
#[utoipa::path(
    get,
    path = "/tokens/transactions",
    params(
        GetTokenTransactionsQuery
    ),
    responses(
        (status = 200, description = "List of token transactions", body = Vec<TokenTransaction>)
    )
)]
pub async fn get_token_transactions_handler(
    Query(params): Query<GetTokenTransactionsQuery>,
    State(db): State<Db>,
) -> Result<Json<Vec<TokenTransaction>>, ApiError> {
    let store = db
        .read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    
    let transactions = store.filter_token_transactions(&params);
    
    // Apply pagination if specified
    let paginated_transactions = match (params.offset, params.limit) {
        (Some(offset), Some(limit)) => {
            transactions
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect()
        }
        (Some(offset), None) => transactions.into_iter().skip(offset as usize).collect(),
        (None, Some(limit)) => transactions.into_iter().take(limit as usize).collect(),
        (None, None) => transactions,
    };
    
    Ok(Json(paginated_transactions))
}

// GET /tokens/stats
#[utoipa::path(
    get,
    path = "/tokens/stats",
    params(
        ("coop_id" = Option<String>, Query, description = "Cooperative ID to filter by"),
        ("community_id" = Option<String>, Query, description = "Community ID to filter by")
    ),
    responses(
        (status = 200, description = "Token statistics", body = TokenStatsResponse)
    )
)]
pub async fn get_token_stats_handler(
    Query(params): Query<GetTokenTransactionsQuery>,
    State(db): State<Db>,
) -> Result<Json<TokenStatsResponse>, ApiError> {
    let store = db
        .read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    
    let stats = store.get_token_stats(
        params.coop_id.as_deref(),
        params.community_id.as_deref()
    );
    
    let response = TokenStatsResponse {
        stats,
        coop_id: params.coop_id,
        community_id: params.community_id,
    };
    
    Ok(Json(response))
}

// Health check handler
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

/// Query parameters for receipt statistics
#[derive(Debug, Deserialize)]
pub struct GetReceiptStatsQuery {
    /// Federation ID to filter by
    pub federation_id: Option<String>,
    /// Cooperative ID to filter by
    pub coop_id: Option<String>,
    /// Community ID to filter by
    pub community_id: Option<String>,
}

/// Query parameters for token statistics
#[derive(Debug, Deserialize)]
pub struct GetTokenStatsQuery {
    /// Federation ID to filter by
    pub federation_id: Option<String>,
    /// Cooperative ID to filter by
    pub coop_id: Option<String>,
    /// Community ID to filter by
    pub community_id: Option<String>,
}

/// Endpoint for accessing execution receipts with authorization
pub async fn get_receipts_authorized(
    auth: AuthenticatedRequest,
    Query(params): Query<GetReceiptsQuery>,
    State(db): State<Db>,
) -> Result<Json<Vec<ExecutionReceiptSummary>>, AuthError> {
    // Check if the user has access to the requested organization scope
    if !auth.claims.has_org_scope_access(
        None, // We don't have federation_id in the model yet
        params.coop_id.as_deref(),
        params.community_id.as_deref(),
    ) {
        return Err(AuthError::UnauthorizedOrgAccess);
    }
    
    // Access granted, retrieve the receipts using filter_receipts
    let store = db.read()
        .map_err(|_| AuthError::Internal("Failed to acquire read lock".to_string()))?;
    
    let receipts = store.filter_receipts(&params);
    
    Ok(Json(receipts))
}

/// Endpoint for accessing token balances with authorization
pub async fn get_token_balances_authorized(
    auth: AuthenticatedRequest,
    Query(params): Query<GetTokenBalancesQuery>,
    State(db): State<Db>,
) -> Result<Json<Vec<TokenBalance>>, AuthError> {
    // Check if the user has access to the requested organization scope
    if !auth.claims.has_org_scope_access(
        None, // We don't have federation_id in the model yet
        params.coop_id.as_deref(),
        params.community_id.as_deref(),
    ) {
        return Err(AuthError::UnauthorizedOrgAccess);
    }
    
    // Access granted, retrieve the token balances
    let store = db.read()
        .map_err(|_| AuthError::Internal("Failed to acquire read lock".to_string()))?;
    
    let balances = store.filter_token_balances(&params);
    
    Ok(Json(balances))
}

/// Endpoint for accessing token transactions with authorization
pub async fn get_token_transactions_authorized(
    auth: AuthenticatedRequest,
    Query(params): Query<GetTokenTransactionsQuery>,
    State(db): State<Db>,
) -> Result<Json<Vec<TokenTransaction>>, AuthError> {
    // Check if the user has access to the requested organization scope
    if !auth.claims.has_org_scope_access(
        None, // We don't have federation_id in QueryParams yet
        params.coop_id.as_deref(),
        params.community_id.as_deref(),
    ) {
        return Err(AuthError::UnauthorizedOrgAccess);
    }
    
    // For economic operations in cooperatives, we need the operator role
    if let Some(coop_id) = &params.coop_id {
        // For pure cooperative operations, require the operator role
        if params.community_id.is_none() {
            if !auth.claims.has_coop_operator_role(coop_id) {
                return Err(AuthError::NotCoopOperator);
            }
        }
    }
    
    // Filter transactions based on query parameters
    let store = db.read()
        .map_err(|_| AuthError::Internal("Failed to acquire read lock".to_string()))?;
    
    let transactions = store.filter_token_transactions(&params);
    
    Ok(Json(transactions))
}

/// Endpoint for accessing receipt statistics with authorization
pub async fn get_receipt_stats_authorized(
    auth: AuthenticatedRequest,
    Query(params): Query<GetReceiptStatsQuery>,
    State(db): State<Db>,
) -> Result<Json<ReceiptStatsResponse>, AuthError> {
    // Check if the user has access to the requested organization scope
    if !auth.claims.has_org_scope_access(
        params.federation_id.as_deref(),
        params.coop_id.as_deref(),
        params.community_id.as_deref(),
    ) {
        return Err(AuthError::UnauthorizedOrgAccess);
    }
    
    // Access granted, retrieve the receipt statistics
    let store = db.read()
        .map_err(|_| AuthError::Internal("Failed to acquire read lock".to_string()))?;
    
    let stats = store.get_receipt_stats(
        params.coop_id.as_deref(),
        params.community_id.as_deref()
    );
    
    let response = ReceiptStatsResponse {
        stats,
        coop_id: params.coop_id,
        community_id: params.community_id,
    };
    
    Ok(Json(response))
}

/// Endpoint for accessing token statistics with authorization
pub async fn get_token_stats_authorized(
    auth: AuthenticatedRequest,
    Query(params): Query<GetTokenStatsQuery>,
    State(db): State<Db>,
) -> Result<Json<TokenStatsResponse>, AuthError> {
    // Check if the user has access to the requested organization scope
    if !auth.claims.has_org_scope_access(
        params.federation_id.as_deref(),
        params.coop_id.as_deref(),
        params.community_id.as_deref(),
    ) {
        return Err(AuthError::UnauthorizedOrgAccess);
    }
    
    // Access granted, retrieve the token statistics
    let store = db.read()
        .map_err(|_| AuthError::Internal("Failed to acquire read lock".to_string()))?;
    
    let stats = store.get_token_stats(
        params.coop_id.as_deref(),
        params.community_id.as_deref()
    );
    
    let response = TokenStatsResponse {
        stats,
        coop_id: params.coop_id,
        community_id: params.community_id,
    };
    
    Ok(Json(response))
}

/// Process a request to issue a new JWT token for a user with specific organization scopes
/// This endpoint is only accessible by federation admins
pub async fn issue_jwt_token_handler(
    State((db, _, jwt_config)): State<(Db, crate::websocket::WebSocketState, Arc<JwtConfig>)>,
    auth: AuthenticatedRequest,
    Path(federation_id): Path<String>,
    Json(payload): Json<crate::auth::TokenIssueRequest>,
) -> Result<Json<crate::auth::TokenResponse>, AuthError> {
    // Ensure the requesting user has federation admin role
    crate::auth::ensure_federation_admin(auth, &federation_id).await?;
    
    // Verify that user isn't trying to grant access to federations they don't control
    if let Some(fed_ids) = &payload.federation_ids {
        for fed_id in fed_ids {
            if fed_id != &federation_id {
                return Err(AuthError::UnauthorizedOrgAccess);
            }
        }
    }
    
    // Get the federation issuer
    let issuer = Some(format!("federation:{}", federation_id));
    
    // Issue the token
    let token_response = issue_token(&payload, issuer, &jwt_config)?;
    
    // Log token issuance action
    tracing::info!(
        "JWT token issued for {} by federation admin, expiring at {}",
        payload.subject,
        token_response.expires_at
    );
    
    Ok(Json(token_response))
}

/// Revoke a JWT token
/// This endpoint is only accessible by federation admins
pub async fn revoke_token_handler(
    State((db, _, jwt_config, revocation_store)): State<(Db, crate::websocket::WebSocketState, Arc<JwtConfig>, Arc<dyn crate::auth::revocation::TokenRevocationStore>)>,
    auth: AuthenticatedRequest,
    Path(federation_id): Path<String>,
    Json(payload): Json<crate::auth::revocation::RevokeTokenRequest>,
) -> Result<Json<crate::auth::revocation::RevokeTokenResponse>, AuthError> {
    // Ensure the requesting user has federation admin role
    crate::auth::ensure_federation_admin(auth.clone(), &federation_id).await?;
    
    // We need either a jti or a subject to revoke
    if payload.jti.is_none() && payload.subject.is_none() {
        return Err(AuthError::InvalidTokenFormat);
    }
    
    let now = Utc::now();
    let mut revoked = false;
    let mut revoked_jti = None;
    let mut revoked_subject = None;
    
    // If we have a JTI, revoke that specific token
    if let Some(jti) = &payload.jti {
        let revoked_token = crate::auth::revocation::RevokedToken {
            jti: jti.clone(),
            subject: payload.subject.clone().unwrap_or_else(|| "unknown".to_string()),
            issuer: Some(format!("federation:{}", federation_id)),
            revoked_at: now,
            reason: payload.reason.clone(),
            revoked_by: auth.claims.sub.clone(),
        };
        
        revoked = revocation_store.revoke_token(revoked_token);
        revoked_jti = Some(jti.clone());
    } 
    // If we have a subject, revoke all tokens for that subject
    else if let Some(subject) = &payload.subject {
        // Create a dummy token with the subject
        let revoked_token = crate::auth::revocation::RevokedToken {
            jti: format!("revoked-{}-{}", subject, Uuid::new_v4()),
            subject: subject.clone(),
            issuer: Some(format!("federation:{}", federation_id)),
            revoked_at: now,
            reason: payload.reason.clone(),
            revoked_by: auth.claims.sub.clone(),
        };
        
        revoked = revocation_store.revoke_token(revoked_token);
        revoked_subject = Some(subject.clone());
    }
    
    // Log the revocation action
    if revoked {
        tracing::info!(
            "Token revoked by {} for federation {}: jti={:?}, subject={:?}, reason={:?}",
            auth.claims.sub,
            federation_id,
            revoked_jti,
            revoked_subject,
            payload.reason
        );
    }
    
    // Return the response
    let response = crate::auth::revocation::RevokeTokenResponse {
        revoked,
        revoked_at: now,
        jti: revoked_jti,
        subject: revoked_subject,
        issuer: Some(format!("federation:{}", federation_id)),
    };
    
    Ok(Json(response))
}

/// Rotate a JWT token (revoke old and issue new)
/// This endpoint is only accessible by federation admins
pub async fn rotate_token_handler(
    State((db, _, jwt_config, revocation_store)): State<(Db, crate::websocket::WebSocketState, Arc<JwtConfig>, Arc<dyn crate::auth::revocation::TokenRevocationStore>)>,
    auth: AuthenticatedRequest,
    Path(federation_id): Path<String>,
    Json(payload): Json<crate::auth::revocation::RotateTokenRequest>,
) -> Result<Json<crate::auth::TokenResponse>, AuthError> {
    // Ensure the requesting user has federation admin role
    crate::auth::ensure_federation_admin(auth.clone(), &federation_id).await?;
    
    // Verify that user isn't trying to grant access to federations they don't control
    if let Some(fed_ids) = &payload.federation_ids {
        for fed_id in fed_ids {
            if fed_id != &federation_id {
                return Err(AuthError::UnauthorizedOrgAccess);
            }
        }
    }
    
    // First, revoke the old token
    let revoked_token = crate::auth::revocation::RevokedToken {
        jti: payload.current_jti.clone(),
        subject: payload.subject.clone(),
        issuer: Some(format!("federation:{}", federation_id)),
        revoked_at: Utc::now(),
        reason: payload.reason.clone().or(Some("Token rotation".to_string())),
        revoked_by: auth.claims.sub.clone(),
    };
    
    let revoked = revocation_store.revoke_token(revoked_token);
    
    if !revoked {
        tracing::warn!("Failed to revoke token {} during rotation", payload.current_jti);
        // Continue anyway since we're issuing a new token
    }
    
    // Now, issue a new token
    let token_request = crate::auth::TokenIssueRequest {
        subject: payload.subject.clone(),
        expires_in: payload.expires_in,
        federation_ids: payload.federation_ids.clone(),
        coop_ids: payload.coop_ids.clone(),
        community_ids: payload.community_ids.clone(),
        roles: payload.roles.clone(),
    };
    
    let issuer = Some(format!("federation:{}", federation_id));
    let token_response = issue_token(&token_request, issuer, &jwt_config)?;
    
    // Log the token rotation
    tracing::info!(
        "Token rotated by {} for subject {} in federation {}: old_jti={}, new_jti={:?}",
        auth.claims.sub,
        payload.subject,
        federation_id,
        payload.current_jti,
        token_response.token_id
    );
    
    Ok(Json(token_response))
}

/// Start periodic cleanup of expired revocations
pub fn start_revocation_cleanup(revocation_store: Arc<dyn crate::auth::revocation::TokenRevocationStore>) {
    use tokio::time::{interval, Duration};
    
    let cleanup_interval = Duration::from_secs(3600); // Once per hour
    let retention_period = Duration::from_secs(86400 * 30); // 30 days
    
    tokio::spawn(async move {
        let mut interval = interval(cleanup_interval);
        
        loop {
            interval.tick().await;
            
            // Calculate the cutoff time (now - retention period)
            let cutoff = Utc::now() - chrono::Duration::seconds(retention_period.as_secs() as i64);
            
            // Perform the cleanup
            let removed = revocation_store.clear_expired_revocations(cutoff);
            
            if removed > 0 {
                tracing::info!("Cleaned up {} expired token revocations", removed);
            }
        }
    });
}

/// Process a transfer between entities
pub async fn process_entity_transfer(
    State((db, ws_state, jwt_config, revocation_store)): State<(Db, WebSocketState, Arc<JwtConfig>, Arc<dyn TokenRevocationStore>)>,
    auth: AuthenticatedRequest,
    Path(federation_id): Path<String>,
    Json(request): Json<TransferRequest>,
) -> Result<Json<TransferResponse>, ApiError> {
    // Validate that the user has federation access
    if !auth.claims.has_federation_access(&federation_id) {
        return Err(ApiError::Unauthorized("No access to this federation".to_string()));
    }
    
    // Check if the user has permission to transfer from the source entity
    check_transfer_from_permission(&auth, &request.from)
        .await
        .map_err(|e| ApiError::Forbidden(e.to_string()))?;
    
    // Ensure amount is greater than zero
    if request.amount == 0 {
        return Err(ApiError::BadRequest("Transfer amount must be greater than zero".to_string()));
    }
    
    // Create a mock entity registry for this example
    // In a real implementation, this would be fetched from a database or service
    let entity_registry = get_mock_entity_registry();
    
    // Verify both entities belong to this federation
    if !verify_entity_in_federation(&request.from, &federation_id, &entity_registry) {
        return Err(ApiError::BadRequest(format!("Source entity does not belong to federation {}", federation_id)));
    }
    
    if !verify_entity_in_federation(&request.to, &federation_id, &entity_registry) {
        return Err(ApiError::BadRequest(format!("Destination entity does not belong to federation {}", federation_id)));
    }
    
    // Calculate the fee
    let fee = calculate_fee(request.amount, &request.from, &request.to);
    
    // Create the transfer record
    let transfer = create_transfer(
        &request,
        federation_id.clone(),
        auth.claims.sub.clone(),
        fee,
    );

    // Access the store and get a read lock to access the ledger
    let store_read_guard = db.read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    
    // Get a reference to the ledger
    let ledger_opt = match &store_read_guard.ledger {
        Some(ledger) => ledger.clone(),
        None => return Err(ApiError::InternalServerError("Ledger not initialized".to_string())),
    };

    // Release the read lock on the store
    drop(store_read_guard);
    
    // Get a write lock on the ledger to update balances
    let mut ledger_write_guard = ledger_opt.write()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire ledger write lock".to_string()))?;
    
    // Process the transfer in the ledger
    let processed_transfer = ledger_write_guard.process_transfer(transfer.clone())
        .map_err(|e| match e {
            TransferError::InsufficientBalance => 
                ApiError::BadRequest("Insufficient balance for transfer".to_string()),
            TransferError::InvalidAmount => 
                ApiError::BadRequest("Invalid transfer amount".to_string()),
            TransferError::EntityNotFound(id) => 
                ApiError::NotFound(format!("Entity not found: {}", id)),
            TransferError::FederationMismatch => 
                ApiError::BadRequest("Federation mismatch between entities".to_string()),
            _ => ApiError::InternalServerError(format!("Transfer error: {}", e)),
        })?;
    
    // Get updated balances
    let new_from_balance = ledger_write_guard.get_balance(&transfer.from);
    let new_to_balance = ledger_write_guard.get_balance(&transfer.to);
    
    // Release the ledger write lock
    drop(ledger_write_guard);
    
    // Broadcast the transfer event to appropriate WebSocket channels
    let channels = get_transfer_notification_channels(&transfer);
    for channel in channels {
        ws_state.send_event_to_channel(
            &channel,
            "transfer",
            &serde_json::json!({
                "transfer": transfer,
                "from_balance": new_from_balance,
                "to_balance": new_to_balance
            }),
        );
    }
    
    // Create and return the response
    let response = TransferResponse {
        tx_id: transfer.tx_id,
        transfer,
        from_balance: new_from_balance,
        to_balance: new_to_balance,
    };
    
    Ok(Json(response))
}

// Fee rates in parts per million (ppm)
const DEFAULT_TRANSFER_FEE_PPM: u64 = 2_000; // 0.2%
const USER_TO_USER_FEE_PPM: u64 = 1_000; // 0.1%
const COOP_OUTBOUND_FEE_PPM: u64 = 3_000; // 0.3%
const COMMUNITY_RECEIVE_FEE_PPM: u64 = 1_500; // 0.15%
const FEDERATION_OUTBOUND_FEE_PPM: u64 = 500; // 0.05%

/// Calculate the fee for a transfer between entities
fn calculate_fee(amount: u64, from: &EntityRef, to: &EntityRef) -> u64 {
    let rate_ppm = match (&from.entity_type, &to.entity_type) {
        (EntityType::User, EntityType::User) => USER_TO_USER_FEE_PPM,
        (EntityType::Cooperative, _) => COOP_OUTBOUND_FEE_PPM,
        (EntityType::Federation, _) => FEDERATION_OUTBOUND_FEE_PPM,
        (_, EntityType::Community) => COMMUNITY_RECEIVE_FEE_PPM,
        _ => DEFAULT_TRANSFER_FEE_PPM,
    };
    
    // Calculate fee with proper handling of potential overflow
    (amount as u128 * rate_ppm as u128 / 1_000_000u128) as u64
}

/// Verify that an entity belongs to a federation
fn verify_entity_in_federation(
    entity: &EntityRef, 
    federation_id: &str,
    entity_registry: &HashMap<String, String>, // id -> federation_id
) -> bool {
    // For users, we trust they belong to the federation if specified in the JWT
    if entity.entity_type == EntityType::User {
        return true;
    }
    
    // For organizations, verify against the entity registry
    entity_registry.get(&entity.id)
        .map_or(false, |fed_id| fed_id == federation_id)
}

/// Create a new transfer object from a request
fn create_transfer(
    request: &TransferRequest,
    federation_id: String,
    initiator: String,
    fee: u64,
) -> Transfer {
    Transfer {
        tx_id: Uuid::new_v4(),
        federation_id,
        from: request.from.clone(),
        to: request.to.clone(),
        amount: request.amount,
        fee,
        initiator,
        timestamp: Utc::now(),
        memo: request.memo.clone(),
        metadata: request.metadata.clone(),
    }
}

/// Format a WebSocket event channel ID for an entity
fn get_entity_channel(entity: &EntityRef) -> String {
    match entity.entity_type {
        EntityType::Federation => format!("federation:{}", entity.id),
        EntityType::Cooperative => format!("coop:{}", entity.id),
        EntityType::Community => format!("community:{}", entity.id),
        EntityType::User => format!("user:{}", entity.id),
    }
}

/// Get all channels that should receive notifications about a transfer
fn get_transfer_notification_channels(transfer: &Transfer) -> Vec<String> {
    let mut channels = vec![
        format!("federation:{}", transfer.federation_id)
    ];
    
    // Add from entity channel
    channels.push(get_entity_channel(&transfer.from));
    
    // Add to entity channel if different
    let to_channel = get_entity_channel(&transfer.to);
    if !channels.contains(&to_channel) {
        channels.push(to_channel);
    }
    
    channels
}

/// Helper function to get a mock entity registry
/// In a real implementation, this would be fetched from a database
fn get_mock_entity_registry() -> HashMap<String, String> {
    let mut registry = HashMap::new();
    
    // Federation entities
    registry.insert("federation1".to_string(), "federation1".to_string());
    registry.insert("federation2".to_string(), "federation2".to_string());
    
    // Cooperatives
    registry.insert("coop-econA".to_string(), "federation1".to_string());
    registry.insert("coop-econB".to_string(), "federation1".to_string());
    registry.insert("coop-econC".to_string(), "federation2".to_string());
    
    // Communities
    registry.insert("comm-govX".to_string(), "federation1".to_string());
    registry.insert("comm-govY".to_string(), "federation1".to_string());
    registry.insert("comm-govZ".to_string(), "federation2".to_string());
    
    registry
}

/// Helper function to get a mock balance for an entity
/// In a real implementation, this would be fetched from a database
fn get_mock_entity_balance(entity_id: &str) -> u64 {
    match entity_id {
        "federation1" => 1_000_000,
        "federation2" => 500_000,
        "coop-econA" => 250_000,
        "coop-econB" => 150_000,
        "coop-econC" => 100_000,
        "comm-govX" => 50_000,
        "comm-govY" => 25_000,
        "comm-govZ" => 10_000,
        // Default for users or unknown entities
        _ => 5_000,
    }
}

// Add a new endpoint for querying transfers with filters
pub async fn query_transfers(
    State((db, _, _, _)): State<(Db, WebSocketState, Arc<JwtConfig>, Arc<dyn TokenRevocationStore>)>,
    auth: AuthenticatedRequest,
    Path(federation_id): Path<String>,
    Query(query): Query<TransferQuery>,
) -> Result<Json<Vec<Transfer>>, ApiError> {
    // Validate that the user has federation access
    if !auth.claims.has_federation_access(&federation_id) {
        return Err(ApiError::Unauthorized("No access to this federation".to_string()));
    }
    
    // Force the federation_id from the path parameter
    let mut filtered_query = query;
    filtered_query.federation_id = Some(federation_id);
    
    // Access the store and get the ledger
    let store = db.read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    
    // Get the ledger
    let ledger_opt = match &store.ledger {
        Some(ledger) => ledger.clone(),
        None => return Err(ApiError::InternalServerError("Ledger not initialized".to_string())),
    };
    
    drop(store);
    
    // Get a read lock on the ledger
    let ledger_guard = ledger_opt.read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire ledger read lock".to_string()))?;
    
    // Query transfers
    let transfers = ledger_guard.query_transfers(&filtered_query);
    
    // Convert references to owned values
    let owned_transfers: Vec<Transfer> = transfers.into_iter()
        .cloned()
        .collect();
    
    Ok(Json(owned_transfers))
}

// Add an endpoint for batch transfers
pub async fn process_batch_transfers(
    State((db, ws_state, jwt_config, revocation_store)): State<(Db, WebSocketState, Arc<JwtConfig>, Arc<dyn TokenRevocationStore>)>,
    auth: AuthenticatedRequest,
    Path(federation_id): Path<String>,
    Json(requests): Json<Vec<TransferRequest>>,
) -> Result<Json<BatchTransferResponse>, ApiError> {
    // Validate that the user has federation access
    if !auth.claims.has_federation_access(&federation_id) {
        return Err(ApiError::Unauthorized("No access to this federation".to_string()));
    }
    
    // Create a mock entity registry for this example
    let entity_registry = get_mock_entity_registry();
    
    // Prepare the transfers
    let mut transfers = Vec::new();
    
    for request in requests {
        // Check if the user has permission to transfer from the source entity
        if let Err(e) = check_transfer_from_permission(&auth, &request.from).await {
            // Skip this transfer and log the error
            continue;
        }
        
        // Skip transfers with invalid amounts
        if request.amount == 0 {
            continue;
        }
        
        // Skip if entities don't belong to the federation
        if !verify_entity_in_federation(&request.from, &federation_id, &entity_registry) ||
           !verify_entity_in_federation(&request.to, &federation_id, &entity_registry) {
            continue;
        }
        
        // Calculate the fee
        let fee = calculate_fee(request.amount, &request.from, &request.to);
        
        // Create the transfer
        let transfer = create_transfer(
            &request,
            federation_id.clone(),
            auth.claims.sub.clone(),
            fee,
        );
        
        transfers.push(transfer);
    }
    
    // Access the store and get the ledger
    let store = db.read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    
    // Get the ledger
    let ledger_opt = match &store.ledger {
        Some(ledger) => ledger.clone(),
        None => return Err(ApiError::InternalServerError("Ledger not initialized".to_string())),
    };
    
    drop(store);
    
    // Get a write lock on the ledger
    let mut ledger = ledger_opt.write()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire ledger write lock".to_string()))?;
    
    // Process the batch transfer
    let batch_result = ledger.process_batch_transfer(transfers);
    
    // Broadcast events for successful transfers
    for tx_id in &batch_result.successful_ids {
        if let Some(transfer) = ledger.find_transfer(tx_id) {
            let channels = get_transfer_notification_channels(transfer);
            for channel in channels {
                ws_state.send_event_to_channel(
                    &channel,
                    "transfer",
                    &serde_json::json!({
                        "transfer": transfer,
                        "from_balance": ledger.get_balance(&transfer.from),
                        "to_balance": ledger.get_balance(&transfer.to)
                    }),
                );
            }
        }
    }
    
    Ok(Json(batch_result))
}

// Add an endpoint to get ledger statistics for a federation
pub async fn get_federation_ledger_stats(
    State((db, _, _, _)): State<(Db, WebSocketState, Arc<JwtConfig>, Arc<dyn TokenRevocationStore>)>,
    auth: AuthenticatedRequest,
    Path(federation_id): Path<String>,
) -> Result<Json<LedgerStats>, ApiError> {
    // Validate that the user has federation access
    if !auth.claims.has_federation_access(&federation_id) {
        return Err(ApiError::Unauthorized("No access to this federation".to_string()));
    }
    
    // Access the store and get the ledger
    let store = db.read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire read lock".to_string()))?;
    
    // Get the ledger
    let ledger_opt = match &store.ledger {
        Some(ledger) => ledger.clone(),
        None => return Err(ApiError::InternalServerError("Ledger not initialized".to_string())),
    };
    
    drop(store);
    
    // Get a read lock on the ledger
    let ledger = ledger_opt.read()
        .map_err(|_| ApiError::InternalServerError("Failed to acquire ledger read lock".to_string()))?;
    
    // Get federation-specific statistics
    let stats = ledger.get_federation_stats(&federation_id)
        .ok_or_else(|| ApiError::NotFound(format!("Federation not found in ledger: {}", federation_id)))?;
    
    Ok(Json(stats))
}

// Implement the Ledger methods

impl Ledger {
    /// Create a new empty ledger
    pub fn new() -> Self {
        Self {
            balances: HashMap::new(),
            transfers: Vec::new(),
            federation_stats: HashMap::new(),
        }
    }
    
    /// Initialize with some example data
    pub fn with_example_data() -> Self {
        let mut ledger = Self::new();
        
        // Add some initial balances
        let entities = vec![
            (EntityRef { entity_type: EntityType::Federation, id: "federation1".to_string() }, 1_000_000),
            (EntityRef { entity_type: EntityType::Cooperative, id: "coop-econA".to_string() }, 250_000),
            (EntityRef { entity_type: EntityType::Cooperative, id: "coop-econB".to_string() }, 150_000),
            (EntityRef { entity_type: EntityType::Community, id: "comm-govX".to_string() }, 50_000),
            (EntityRef { entity_type: EntityType::Community, id: "comm-govY".to_string() }, 25_000),
            (EntityRef { entity_type: EntityType::User, id: "did:icn:user1".to_string() }, 5_000),
            (EntityRef { entity_type: EntityType::User, id: "did:icn:user2".to_string() }, 3_000),
        ];
        
        for (entity, balance) in entities {
            ledger.set_balance(&entity, balance);
            
            // Update federation stats
            if entity.entity_type == EntityType::Federation {
                ledger.federation_stats.insert(entity.id.clone(), balance);
            } else {
                // Assume all entities belong to federation1 for this example
                let fed_entry = ledger.federation_stats.entry("federation1".to_string()).or_insert(0);
                *fed_entry += balance;
            }
        }
        
        ledger
    }
    
    /// Get an entity's balance
    pub fn get_balance(&self, entity: &EntityRef) -> u64 {
        // Create a composite key from entity type and ID
        let key = format!("{}:{}", entity.entity_type.to_string(), entity.id);
        self.balances.get(&key).copied().unwrap_or(0)
    }
    
    /// Set an entity's balance directly
    pub fn set_balance(&mut self, entity: &EntityRef, balance: u64) {
        // Create a composite key from entity type and ID
        let key = format!("{}:{}", entity.entity_type.to_string(), entity.id);
        self.balances.insert(key, balance);
    }
    
    /// Process a transfer between entities
    pub fn process_transfer(&mut self, transfer: Transfer) -> Result<Transfer, TransferError> {
        // Validate the transfer
        if transfer.amount == 0 {
            return Err(TransferError::InvalidAmount);
        }
        
        // Check if source has sufficient balance
        let from_balance = self.get_balance(&transfer.from);
        if from_balance < transfer.amount + transfer.fee {
            return Err(TransferError::InsufficientBalance);
        }
        
        // Update balances
        let new_from_balance = from_balance - transfer.amount - transfer.fee;
        self.set_balance(&transfer.from, new_from_balance);
        
        let to_balance = self.get_balance(&transfer.to);
        let new_to_balance = to_balance + transfer.amount;
        self.set_balance(&transfer.to, new_to_balance);
        
        // Record the transfer
        self.transfers.push(transfer.clone());
        
        // Update federation stats
        if let Some(stats) = self.federation_stats.get_mut(&transfer.federation_id) {
            // Fees remain in the federation as a whole
            *stats += transfer.fee;
        }
        
        Ok(transfer)
    }
    
    /// Process multiple transfers in one operation
    pub fn process_batch_transfer(
        &mut self, 
        transfers: Vec<Transfer>
    ) -> BatchTransferResponse {
        let mut response = BatchTransferResponse {
            successful: 0,
            failed: 0,
            successful_ids: Vec::new(),
            failed_transfers: Vec::new(),
            total_transferred: 0,
            total_fees: 0,
        };
        
        for (index, transfer) in transfers.into_iter().enumerate() {
            match self.process_transfer(transfer) {
                Ok(processed) => {
                    response.successful += 1;
                    response.successful_ids.push(processed.tx_id);
                    response.total_transferred += processed.amount;
                    response.total_fees += processed.fee;
                },
                Err(err) => {
                    response.failed += 1;
                    response.failed_transfers.push((index, err.to_string()));
                }
            }
        }
        
        response
    }
    
    /// Find a transfer by ID
    pub fn find_transfer(&self, tx_id: &Uuid) -> Option<&Transfer> {
        self.transfers.iter().find(|t| &t.tx_id == tx_id)
    }
    
    /// Query transfers based on filters
    pub fn query_transfers(&self, query: &TransferQuery) -> Vec<&Transfer> {
        let mut results: Vec<&Transfer> = self.transfers.iter()
            .filter(|t| {
                // Filter by federation
                if let Some(fed_id) = &query.federation_id {
                    if t.federation_id != *fed_id {
                        return false;
                    }
                }
                
                // Filter by entity
                if let Some(entity_id) = &query.entity_id {
                    let from_match = t.from.id == *entity_id;
                    let to_match = t.to.id == *entity_id;
                    
                    match (query.from_only, query.to_only) {
                        (Some(true), _) => if !from_match { return false; },
                        (_, Some(true)) => if !to_match { return false; },
                        _ => if !from_match && !to_match { return false; }
                    }
                    
                    // Filter by entity type if both ID and type provided
                    if let Some(entity_type) = &query.entity_type {
                        if (from_match && t.from.entity_type != *entity_type) ||
                           (to_match && t.to.entity_type != *entity_type) {
                            return false;
                        }
                    }
                } 
                // If only entity type provided without ID
                else if let Some(entity_type) = &query.entity_type {
                    if t.from.entity_type != *entity_type && t.to.entity_type != *entity_type {
                        return false;
                    }
                }
                
                // Filter by date range
                if let Some(start) = query.start_date {
                    if t.timestamp < start {
                        return false;
                    }
                }
                
                if let Some(end) = query.end_date {
                    if t.timestamp > end {
                        return false;
                    }
                }
                
                // Filter by amount
                if let Some(min) = query.min_amount {
                    if t.amount < min {
                        return false;
                    }
                }
                
                if let Some(max) = query.max_amount {
                    if t.amount > max {
                        return false;
                    }
                }
                
                true
            })
            .collect();
        
        // Apply sorting - newest first
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        // Apply pagination
        if let Some(offset) = query.offset {
            let offset = offset as usize;
            if offset < results.len() {
                results = results.into_iter().skip(offset).collect();
            } else {
                results = Vec::new();
            }
        }
        
        if let Some(limit) = query.limit {
            let limit = limit as usize;
            if results.len() > limit {
                results.truncate(limit);
            }
        }
        
        results
    }
    
    /// Get ledger statistics
    pub fn get_stats(&self) -> LedgerStats {
        // Calculate total volume and fees
        let (total_volume, total_fees) = self.transfers.iter()
            .fold((0, 0), |(vol, fees), t| (vol + t.amount, fees + t.fee));
        
        // Find active entities
        let active_entities = self.balances.values().filter(|&b| *b > 0).count();
        
        // Find entity with highest balance
        let highest_balance_entry = self.balances.iter()
            .max_by_key(|(_, balance)| *balance);
        
        let (highest_balance_entity, highest_balance) = match highest_balance_entry {
            Some(((id, entity_type), balance)) => {
                let entity = EntityRef {
                    entity_type: entity_type.clone(),
                    id: id.clone(),
                };
                (Some(entity), *balance)
            },
            None => (None, 0),
        };
        
        // Calculate activity in the last 24 hours
        let day_ago = Utc::now() - chrono::Duration::days(1);
        let recent_transfers: Vec<_> = self.transfers.iter()
            .filter(|t| t.timestamp > day_ago)
            .collect();
        
        let transfers_last_24h = recent_transfers.len();
        let volume_last_24h = recent_transfers.iter()
            .fold(0, |sum, t| sum + t.amount);
        
        LedgerStats {
            total_transfers: self.transfers.len(),
            total_volume,
            total_fees,
            total_entities: self.balances.len(),
            active_entities,
            highest_balance_entity,
            highest_balance,
            transfers_last_24h,
            volume_last_24h,
        }
    }
    
    /// Get federation-specific statistics
    pub fn get_federation_stats(&self, federation_id: &str) -> Option<LedgerStats> {
        // Check if federation exists
        if !self.federation_stats.contains_key(federation_id) {
            return None;
        }
        
        // Filter transfers for this federation
        let fed_transfers: Vec<_> = self.transfers.iter()
            .filter(|t| t.federation_id == federation_id)
            .collect();
        
        // Calculate total volume and fees
        let (total_volume, total_fees) = fed_transfers.iter()
            .fold((0, 0), |(vol, fees), t| (vol + t.amount, fees + t.fee));
        
        // Filter active entities in this federation
        let fed_entities: Vec<_> = self.balances.iter()
            .filter(|((id, _), balance)| {
                // For simplicity, we're assuming entities with balance belong to the federation
                // In a real implementation, we'd have explicit federation membership
                **balance > 0
            })
            .collect();
        
        let active_entities = fed_entities.len();
        
        // Find entity with highest balance
        let highest_balance_entry = fed_entities.into_iter()
            .max_by_key(|(_, balance)| *balance);
        
        let (highest_balance_entity, highest_balance) = match highest_balance_entry {
            Some(((id, entity_type), balance)) => {
                let entity = EntityRef {
                    entity_type: entity_type.clone(),
                    id: id.clone(),
                };
                (Some(entity), *balance)
            },
            None => (None, 0),
        };
        
        // Calculate activity in the last 24 hours
        let day_ago = Utc::now() - chrono::Duration::days(1);
        let recent_transfers: Vec<_> = fed_transfers.iter()
            .filter(|t| t.timestamp > day_ago)
            .collect();
        
        let transfers_last_24h = recent_transfers.len();
        let volume_last_24h = recent_transfers.iter()
            .fold(0, |sum, t| sum + t.amount);
        
        Some(LedgerStats {
            total_transfers: fed_transfers.len(),
            total_volume,
            total_fees,
            total_entities: self.balances.len(), // Simplifying for now
            active_entities,
            highest_balance_entity,
            highest_balance,
            transfers_last_24h,
            volume_last_24h,
        })
    }
}

/// Create a new ledger store with example data
pub fn create_example_ledger() -> LedgerStore {
    Arc::new(RwLock::new(Ledger::with_example_data()))
}
