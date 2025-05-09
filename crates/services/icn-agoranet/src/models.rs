use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

// Timestamp alias for clarity
pub type Timestamp = DateTime<Utc>;

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

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug, PartialEq)]
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
