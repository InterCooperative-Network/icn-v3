use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use std::collections::HashMap;
use uuid::Uuid;
use sqlx::Type;

// Timestamp alias for clarity
pub type Timestamp = DateTime<Utc>;

// Simple enum for resource types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    Cpu,
    Memory,
    Storage,
    Token,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct ThreadSummary {
    #[schema(example = "thread_abc123")]
    pub id: String,
    #[schema(example = "Discussion about new governance model")]
    pub title: String,
    pub created_at: Timestamp,
    #[schema(example = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwupk8vQT7GNz2wVXgE")]
    pub author_did: String,
    #[schema(example = "coop.nw")]
    pub scope: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct Message {
    #[schema(example = "msg_xyz789")]
    pub id: String,
    #[schema(example = "did:key:z6Mkj1h4h4kj1h4h4kj1h4h4kj1h4h4kj1h4h4kj1h4")]
    pub author_did: String,
    pub timestamp: Timestamp,
    #[schema(example = "I think this proposal makes sense.")]
    pub content: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct ThreadDetail {
    #[serde(flatten)]
    pub summary: ThreadSummary,
    pub messages: Vec<Message>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug, PartialEq)]
pub enum ProposalStatus {
    Draft,
    Open,
    Closed,
    Accepted,
    Rejected,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct VoteCounts {
    #[schema(example = 15)]
    pub approve: u32,
    #[schema(example = 3)]
    pub reject: u32,
    #[schema(example = 2)]
    pub abstain: u32,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct ProposalSummary {
    #[schema(example = "proposal_def456")]
    pub id: String,
    #[schema(example = "Implement new fee structure")]
    pub title: String,
    #[schema(example = "coop.nw.governance")]
    pub scope: String,
    pub status: ProposalStatus,
    pub vote_counts: VoteCounts,
    pub voting_deadline: Timestamp,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct ProposalDetail {
    #[serde(flatten)]
    pub summary: ProposalSummary,
    #[schema(example = "This proposal outlines a new fee structure for the network...")]
    pub full_text: String,
    #[schema(example = "thread_abc123")]
    pub linked_thread_id: Option<String>,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub enum VoteType {
    Approve,
    Reject,
    Abstain,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct Vote {
    #[schema(example = "proposal_def456")]
    pub proposal_id: String,
    #[schema(example = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwupk8vQT7GNz2wVXgE")]
    pub voter_did: String,
    pub vote_type: VoteType,
    pub timestamp: Timestamp,
    #[schema(example = "I approve because this aligns with our long-term goals.")]
    pub justification: Option<String>,
}

// Request Structs

#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct NewThreadRequest {
    #[schema(example = "New Thread Title")]
    pub title: String,
    #[schema(example = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwupk8vQT7GNz2wVXgE")]
    pub author_did: String,
    #[schema(example = "coop.nw")]
    pub scope: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct NewProposalRequest {
    #[schema(example = "Proposal for new feature X")]
    pub title: String,
    #[schema(example = "Detailed text explaining feature X...")]
    pub full_text: String,
    #[schema(example = "coop.nw.governance")]
    pub scope: String,
    #[schema(example = "thread_abc123")]
    pub linked_thread_id: Option<String>,
    pub voting_deadline: Option<Timestamp>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct NewVoteRequest {
    #[schema(example = "proposal_def456")]
    pub proposal_id: String,
    #[schema(example = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwupk8vQT7GNz2wVXgE")]
    pub voter_did: String,
    pub vote_type: VoteType,
    #[schema(example = "My reason for this vote...")]
    pub justification: Option<String>,
}

// Query parameters for GET /threads
#[derive(Deserialize, ToSchema, IntoParams, Debug)]
pub struct GetThreadsQuery {
    #[schema(example = "coop.nw")]
    pub scope: Option<String>,
    #[schema(example = 10)]
    pub limit: Option<u32>,
}

// Query parameters for GET /proposals
#[derive(Deserialize, ToSchema, IntoParams, Debug)]
pub struct GetProposalsQuery {
    #[schema(example = "coop.nw.governance")]
    pub scope: Option<String>,
    pub status: Option<ProposalStatus>,
    // type is a reserved keyword in Rust, so let's use proposal_type
    // For simplicity, let's assume ProposalType is similar to a string or enum for now.
    // If ProposalType needs to be an enum, it should be defined similar to ProposalStatus.
    #[schema(example = "Funding")]
    pub proposal_type: Option<String>, // Assuming 'type' query param refers to a category of proposal
}

// Response for GET /votes/:proposal_id
#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct ProposalVotesResponse {
    pub proposal_id: String,
    pub votes: Vec<Vote>,
}

// New models for organization-scoped API

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct ExecutionReceiptSummary {
    #[schema(example = "bafy2bzacedz7h3vxthx4nm3uoif2vyxbpnmyifzwwp2sgoj5exptpa2hbk7mg")]
    pub cid: String,
    #[schema(example = "did:icn:node1")]
    pub executor: String,
    pub resource_usage: HashMap<String, u64>,
    pub timestamp: Timestamp,
    #[schema(example = "coop-123")]
    pub coop_id: Option<String>,
    #[schema(example = "community-456")]
    pub community_id: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct ExecutionReceiptDetail {
    #[serde(flatten)]
    pub summary: ExecutionReceiptSummary,
    #[schema(example = "bafy2bzacedxxxyyy")]
    pub task_cid: String,
    pub anchored_cids: Vec<String>,
    #[schema(example = "base64encodedstring")]
    pub signature: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct TokenBalance {
    #[schema(example = "did:icn:user1")]
    pub did: String,
    #[schema(example = 15000)]
    pub balance: u64,
    #[schema(example = "coop-123")]
    pub coop_id: Option<String>,
    #[schema(example = "community-456")]
    pub community_id: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct TokenTransaction {
    #[schema(example = "tx-1")]
    pub id: String,
    #[schema(example = "did:icn:treasury")]
    pub from_did: String,
    #[schema(example = "did:icn:node1")]
    pub to_did: String,
    #[schema(example = 500)]
    pub amount: u64,
    #[schema(example = "mint")]
    pub operation: String,
    pub timestamp: Timestamp,
    #[schema(example = "coop-123")]
    pub from_coop_id: Option<String>,
    #[schema(example = "community-456")]
    pub from_community_id: Option<String>,
    #[schema(example = "coop-123")]
    pub to_coop_id: Option<String>,
    #[schema(example = "community-456")]
    pub to_community_id: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct ReceiptStats {
    #[schema(example = 150)]
    pub total_receipts: u64,
    #[schema(example = 450)]
    pub avg_cpu_usage: u64,
    #[schema(example = 1024)]
    pub avg_memory_usage: u64,
    #[schema(example = 5000)]
    pub avg_storage_usage: u64,
    pub receipts_by_executor: HashMap<String, u64>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct TokenStats {
    #[schema(example = 60000)]
    pub total_minted: u64,
    #[schema(example = 5000)]
    pub total_burnt: u64,
    #[schema(example = 5)]
    pub active_accounts: u64,
    #[schema(example = 10000)]
    pub daily_volume: Option<u64>,
}

// Query parameters for GET /receipts
#[derive(Deserialize, ToSchema, IntoParams, Debug)]
pub struct GetReceiptsQuery {
    #[schema(example = "2023-05-10")]
    pub date: Option<String>,
    #[schema(example = "did:icn:node1")]
    pub executor: Option<String>,
    #[schema(example = "coop-123")]
    pub coop_id: Option<String>,
    #[schema(example = "community-456")]
    pub community_id: Option<String>,
    #[schema(example = 10)]
    pub limit: Option<u32>,
    #[schema(example = 0)]
    pub offset: Option<u32>,
}

// Query parameters for GET /tokens/balances
#[derive(Deserialize, ToSchema, IntoParams, Debug)]
pub struct GetTokenBalancesQuery {
    #[schema(example = "did:icn:user1")]
    pub account: Option<String>,
    #[schema(example = "coop-123")]
    pub coop_id: Option<String>,
    #[schema(example = "community-456")]
    pub community_id: Option<String>,
    #[schema(example = 10)]
    pub limit: Option<u32>,
    #[schema(example = 0)]
    pub offset: Option<u32>,
}

// Query parameters for GET /tokens/transactions
#[derive(Deserialize, ToSchema, IntoParams, Debug)]
pub struct GetTokenTransactionsQuery {
    #[schema(example = "2023-05-10")]
    pub date: Option<String>,
    #[schema(example = "did:icn:user1")]
    pub account: Option<String>,
    #[schema(example = "coop-123")]
    pub coop_id: Option<String>,
    #[schema(example = "community-456")]
    pub community_id: Option<String>,
    #[schema(example = 10)]
    pub limit: Option<u32>,
    #[schema(example = 0)]
    pub offset: Option<u32>,
}

// Response for GET /receipts/stats
#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct ReceiptStatsResponse {
    pub stats: ReceiptStats,
    #[schema(example = "coop-123")]
    pub coop_id: Option<String>,
    #[schema(example = "community-456")]
    pub community_id: Option<String>,
}

// Response for GET /tokens/stats
#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct TokenStatsResponse {
    pub stats: TokenStats,
    #[schema(example = "coop-123")]
    pub coop_id: Option<String>,
    #[schema(example = "community-456")]
    pub community_id: Option<String>,
}

/// Represents the type of an entity in the ledger (User, Community, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, Type, Eq, Hash, PartialEq)] 
#[sqlx(type_name = "entity_type", rename_all = "lowercase")]
pub enum EntityType {
    User,
    Community,
    Cooperative,
    Contract, // e.g., a smart contract address or identifier
    ResourceProvider, // Represents a node providing resources
    // Add other entity types as needed
}

/// Reference to any token-holding entity.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct EntityRef {
    /// Type of the entity (federation, coop, community, user)
    pub entity_type: EntityType,
    /// Identifier (DID or org ID)
    pub id: String,
}

/// A token transfer between any two entities.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Transfer {
    /// Unique transaction ID
    pub tx_id: Uuid,
    /// Governing federation ID
    pub federation_id: String,
    /// Source entity
    pub from: EntityRef,
    /// Destination entity
    pub to: EntityRef,
    /// Amount of tokens to transfer
    pub amount: u64,
    /// Fee charged for the transfer
    pub fee: u64,
    /// DID of the user initiating the transfer
    pub initiator: String,
    /// Timestamp of the transfer
    pub timestamp: DateTime<Utc>,
    /// Optional memo/description
    pub memo: Option<String>,
    /// Optional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Request to initiate a transfer between entities
#[derive(Serialize, Deserialize, Debug, ToSchema)]
pub struct TransferRequest {
    /// Source entity
    pub from: EntityRef,
    /// Destination entity
    pub to: EntityRef,
    /// Amount to transfer
    pub amount: u64,
    /// Optional memo/description
    pub memo: Option<String>,
    /// Optional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Response to a transfer request
#[derive(Serialize, Deserialize, Debug, ToSchema)]
pub struct TransferResponse {
    /// Unique transaction ID
    pub tx_id: Uuid,
    /// Completed transfer details
    pub transfer: Transfer,
    /// New balance of the source entity after transfer
    pub from_balance: u64,
    /// New balance of the destination entity after transfer
    pub to_balance: u64,
}
