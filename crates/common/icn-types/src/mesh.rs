use serde::{Deserialize, Serialize};
use icn_economics::ResourceType;
use icn_identity::Did; // Assuming Did is available from icn_identity
use crate::org::{CooperativeId, CommunityId}; // Assuming these are in icn_types::org

/// Quality of Service profile for a Mesh Job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QoSProfile {
    BestEffort,
    LowLatency,
    CostOptimized,
    GuaranteedCompletion,
}

/// Parameters for submitting a new mesh computation job.
/// This structure is expected to be serialized (e.g., to CBOR) and passed
/// as the payload to the `host_submit_mesh_job` ABI function.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshJobParams {
    /// CID of the WASM module to execute.
    pub wasm_cid: String,
    /// Human-readable description of the job.
    pub description: String,
    /// List of resources required for the job, specifying the type and amount.
    /// e.g., [(ResourceType::Cpu, 1000), (ResourceType::Memory, 2048)]
    pub resources_required: Vec<(ResourceType, u64)>,
    /// Desired Quality of Service profile for the job execution.
    pub qos_profile: QoSProfile,
    /// Optional deadline for job completion, as a Unix timestamp (seconds since epoch).
    pub deadline: Option<u64>,
    /// Optional CID of input data required for the job.
    pub input_data_cid: Option<String>,
    pub max_acceptable_bid_tokens: Option<u64>,
}

/// Represents an organizational scope for a job or receipt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OrgScopeIdentifier {
    pub coop_id: Option<CooperativeId>,
    pub community_id: Option<CommunityId>,
}

/// Represents a mesh computation job within the ICN system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshJob {
    /// Unique identifier for the job.
    /// e.g., "job_uuid" or a CID derived from parameters + nonce.
    pub job_id: String,
    /// The parameters defining the job.
    pub params: MeshJobParams,
    /// DID of the entity that originated/submitted the job.
    pub originator_did: Did,
    /// Optional organizational scope associated with the job's originator or context.
    pub originator_org_scope: Option<OrgScopeIdentifier>,
    /// Timestamp of when the job was submitted to the ICN, as a Unix timestamp (seconds since epoch).
    pub submission_timestamp: u64,
}

/// Status of a Mesh Job execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobStatus {
    InProgress,
    Completed,
    Failed,
    Cancelled, // Adding this as it was in planetary-mesh JobStatus
} 