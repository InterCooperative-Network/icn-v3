use icn_types::mesh::MeshJob;
use icn_identity::Did;
use std::collections::HashMap;
use icn_economics::ResourceType;
use serde::{Serialize, Deserialize};

/// Type alias for JobId, which is typically a String.
pub type JobId = String;

/// Represents the different types of messages exchanged in the ICN Mesh Compute protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MeshProtocolMessage {
    /// Announces a new job to the network.
    JobAnnouncementV1(MeshJob),
    /// Advertises the capabilities of an executor node.
    CapabilityAdvertisementV1(NodeCapability),
    /// Sent by a potential executor node to the job originator to express interest in a job.
    JobInterestV1 {
        job_id: JobId,
        executor_did: Did,
        // TODO: Potentially add a summary of why they are interested or basic capability match
        // For example: estimated_bid_range: Option<(u64, u64)>, capability_summary_hash: Option<String>
    },
    // Future message types:
    // JobBidV1 { job_id: JobId, executor_did: Did, bid_amount_tokens: u64, specific_commitments: Option<String> },
    // AcceptBidV1 { job_id: JobId, winning_executor_did: Did },
    // RejectBidV1 { job_id: JobId, executor_did: Did, reason: Option<String> },
    // JobStatusUpdateV1 { job_id: JobId, status: JobStatus, executor_did: Did, message: Option<String> },
    // ResultAnnouncementV1 { job_id: JobId, executor_did: Did, result_cid: String, receipt_cid: String },
}

/// Describes the capabilities of an executor node on the mesh network.
/// This information is advertised by nodes to allow originators (or brokers) to find suitable executors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapability {
    /// The DID of the node advertising its capabilities.
    pub node_did: Did,
    /// Maximum capacity of various resources the node can offer for a single job or in total.
    /// e.g., {ResourceType::Cpu: 8000 (cores*1000), ResourceType::Memory: 16384 (MB)}
    pub available_resources: HashMap<ResourceType, u64>,
    /// List of WASM engine names and versions supported by the node.
    /// e.g., ["wasmtime_v1.0", "wasmedge_v0.10"]
    pub supported_wasm_engines: Vec<String>,
    /// Current load factor of the node, typically ranging from 0.0 (idle) to 1.0 (fully utilized).
    /// This can help in dynamic job scheduling and balancing.
    pub current_load_factor: f32,
    /// An optional reputation score for the node, based on past performance and reliability.
    /// The exact mechanism for calculating and verifying reputation is TBD.
    pub reputation_score: Option<u32>,
    /// Optional geographical region where the node is located.
    /// e.g., "us-east-1", "eu-central", "asia-pacific-tokyo"
    pub geographical_region: Option<String>,
    /// Custom features or attributes of the node, for specialized jobs.
    /// e.g., {"gpu_model": "NVIDIA_A100", "has_sgx": "true"}
    pub custom_features: HashMap<String, String>,
    // TODO: Consider adding:
    // pub supported_qos_profiles: Vec<String>, // e.g., ["BestEffort", "LowLatency"]
    // pub max_concurrent_jobs: Option<u32>,
    // pub last_updated_timestamp: u64, // When this capability info was last updated
    // pub network_bandwidth_mbps: Option<u32>,
    // pub supported_job_types: Vec<String>, // If jobs can be categorized beyond WASM execution
} 