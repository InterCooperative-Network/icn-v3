use cid::Cid;
use chrono::{DateTime, Utc};
use icn_identity::Did;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

use crate::mesh::MeshJobParams;

/// Amount of ICN tokens (in the smallest indivisible unit)
pub type TokenAmount = u64;

/// Refactored JobRequest to hold MeshJobParams and essential identifiers.
/// This is the primary structure used by the icn-mesh-jobs service to define a job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] // Eq and Hash might be problematic if MeshJobParams contains f64 directly or indirectly.
pub struct JobRequest {
    /// Unique identifier for the job, typically a CID.
    pub id: Cid,
    /// The detailed parameters defining the job, including execution policy.
    pub params: MeshJobParams,
    /// DID of the entity that originated/submitted the job.
    pub originator_did: Did,
}

/// Placeholder for ResourceEstimate, assuming it's similar to ResourceRequirements for now
/// or could be more detailed, e.g., including estimated duration.
/// This is used in the Bid struct.
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
    #[serde(skip_serializing_if = "Option::is_none")] // Don't serialize if None (e.g., before DB insert)
    pub id: Option<i64>, // <-- NEW FIELD: Database ID of the bid
    pub job_id: Cid, // Assuming JobRequest itself will be a CID or have an ID that is a CID
    pub bidder: Did,
    pub price: TokenAmount,
    pub estimate: ResourceEstimate, // Bidder's estimate of resources they'll use/provide
    pub reputation_score: Option<f64>, // Added as per discussion
    pub node_metadata: Option<NodeMetadata>,
}

// New struct for Bid node_metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeMetadata {
    pub region: Option<String>,
    pub reputation: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Bidding,
    Assigned { bidder: Did },
    Running { runner: Did },
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