use cid::Cid;
use chrono::{DateTime, Utc};
use icn_identity::Did;
use serde::{Serialize, Deserialize};

// Assuming TokenAmount and DID are defined elsewhere, possibly in a common types module or imported.
// For now, let's use placeholders.
/// Amount of ICN tokens (in the smallest indivisible unit)
pub type TokenAmount = u64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobRequest {
    pub wasm_cid: Cid,
    pub description: String,
    pub requirements: ResourceRequirements,
    pub deadline: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub cpu: u32,
    pub memory_mb: u32,
    pub storage_mb: u32,
    pub bandwidth: u32, // Assuming kbps or similar unit
}

// Placeholder for ResourceEstimate, assuming it's similar to ResourceRequirements for now
// or could be more detailed, e.g., including estimated duration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceEstimate {
    pub cpu: u32,
    pub memory_mb: u32,
    pub storage_mb: u32,
    pub bandwidth: u32,
    pub estimated_duration_secs: Option<u64>,
}

// NOTE: Eq and Hash have been removed from Bid due to the inclusion of Option<f64> for reputation_score.
// f64 does not implement Eq or Hash. If these traits are strictly needed for Bid in the future,
// consider using a wrapper for f64 (e.g., ordered_float::NotNan) or representing the score differently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] // Removed Eq, Hash
pub struct Bid {
    pub job_id: Cid, // Assuming JobRequest itself will be a CID or have an ID that is a CID
    pub bidder: Did,
    pub price: TokenAmount,
    pub estimate: ResourceEstimate, // Bidder's estimate of resources they'll use/provide
    pub reputation_score: Option<f64>, // Added as per discussion
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Bidding,
    Assigned { bidder: Did },
    Running,
    Completed,
    Failed { reason: String }, // Added a reason field for failure
}

// Example of how Cid might be used if JobRequest objects are identified by their hash
// This is conceptual and depends on how Job IDs are actually generated and managed.
// fn generate_job_id(job_request: &JobRequest) -> Cid {
//     // Logic to serialize and hash job_request to a CID
//     // This is highly dependent on the serialization format and hashing algorithm used in ICN
//     // For now, this is just a placeholder idea.
//     let data = format!("{:?}", job_request); // Simplified serialization
//     // This is not a real CID generation, just a conceptual placeholder
//     Cid::try_from(data).expect("Failed to create placeholder CID") 
// }

// TODO:
// - Clarify how Job IDs (CIDs) are generated for JobRequests. (Assuming for now that a JobRequest object can be serialized and hashed to a CID)
// - Refine ResourceEstimate if it needs more specific fields than ResourceRequirements. (Current estimate is a good start)
// - Consider if `deadline` in `JobRequest` should be `DateTime<Utc>` or a simpler `Timestamp` type if available. (DateTime<Utc> is robust)
// - Add serialization/deserialization derives (e.g., Serde) once dependencies are clear. (Done) 