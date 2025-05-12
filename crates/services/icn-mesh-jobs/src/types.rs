use serde::{Deserialize, Serialize};
use cid::Cid;
use icn_identity::Did;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequest {
    pub id: String,
    pub owner_did: Did,
    pub cid: Cid,
    pub requirements: JobRequirements,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequirements {
    pub cpu_cores: u32,
    pub memory_mb: u32,
    pub storage_gb: u32,
    pub max_price: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bid {
    pub job_id: String,
    pub bidder_did: Did,
    pub price: u64,
    pub resources: JobRequirements,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidEvaluation {
    pub bid: Bid,
    pub score: f64,
    pub explanation: BidExplanation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidExplanation {
    pub price_score: f64,
    pub resource_match_score: f64,
    pub reputation_score: f64,
    pub timeliness_score: f64,
    pub total_score: f64,
} 