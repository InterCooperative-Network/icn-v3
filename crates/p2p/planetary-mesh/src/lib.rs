#![cfg(feature = "full_mesh")]

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use icn_core_vm::{CoVm, ExecutionMetrics, HostContext, ResourceLimits};
use icn_economics::ScopedResourceToken;
// use icn_identity_core::did::Did;
type Did = String; // DIDs are strings in the format did:key:...
                   // use icn_identity_core::vc::ExecutionReceiptCredential;

// Import standardized ExecutionReceipt and JobStatus
use icn_mesh_receipts::{ExecutionReceipt, ReceiptError};
use icn_types::mesh::JobStatus as StandardJobStatus; // Alias to avoid conflict with local JobStatus

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

pub mod protocol;
pub use protocol::{JobId, MeshProtocolMessage};

pub mod behaviour;
pub use behaviour::{MeshBehaviour, MeshBehaviourEvent, CAPABILITY_TOPIC};

pub mod node;
pub use node::MeshNode;

pub mod reputation_integration;
pub use reputation_integration::{BidEvaluatorConfig, DefaultReputationClient, ReputationClient};

// Add the new metrics module
pub mod metrics;

/// Error types specific to the planetary mesh
#[derive(Error, Debug)]
pub enum MeshError {
    #[error("Job not found: {0}")]
    JobNotFound(String),

    #[error("Invalid job manifest: {0}")]
    InvalidManifest(String),

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Network error: {0}")]
    NetworkError(String),
}

/// Job priority levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum JobPriority {
    Low,
    Medium,
    High,
    Critical,
}

/// Status of a job in the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobStatus {
    /// Job created but not yet submitted to the network
    Created,

    /// Job submitted and waiting for bids
    Submitted,

    /// Job assigned to a node for execution
    Assigned { node_id: String },

    /// Job execution in progress
    Running {
        node_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        current_stage_index: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        current_stage_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        progress_percent: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status_message: Option<String>,
    },

    /// Job completed successfully
    Completed {
        node_id: String,
        /// CID of the execution receipt
        receipt_cid: String,
    },

    /// Job failed
    Failed {
        node_id: Option<String>,
        error: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage_index: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage_id: Option<String>,
    },

    /// Job cancelled by the submitter
    Cancelled,

    /// Job pending user input
    PendingUserInput {
        node_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage_index: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt_cid: Option<String>,
    },

    /// Job awaiting next stage
    AwaitingNextStage {
        node_id: String,
        completed_stage_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        completed_stage_id: Option<String>,
        next_stage_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        next_stage_id: Option<String>,
    },
}

/// Compute resource requirements
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComputeRequirements {
    /// Minimum memory in MB
    pub min_memory_mb: u32,

    /// Minimum CPU cores
    pub min_cpu_cores: u32,

    /// Minimum storage in MB
    pub min_storage_mb: u32,

    /// Maximum execution time in seconds
    pub max_execution_time_secs: u64,

    /// Required features (e.g., "gpu", "avx", etc.)
    pub required_features: Vec<String>,
}

/// A job manifest for distributed execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobManifest {
    /// Unique ID for this job
    pub id: String,

    /// Job submitter DID
    pub submitter_did: String,

    /// Description of the job
    pub description: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Expiration timestamp
    pub expires_at: Option<DateTime<Utc>>,

    /// CID of the WASM module to execute
    pub wasm_cid: String,

    /// Source CCL CID if applicable
    pub ccl_cid: Option<String>,

    /// Input data CID
    pub input_data_cid: Option<String>,

    /// Output data location
    pub output_location: Option<String>,

    /// Compute requirements
    pub requirements: ComputeRequirements,

    /// Job priority
    pub priority: JobPriority,

    /// Resource token for this job
    pub resource_token: ScopedResourceToken,

    /// Trust requirements (e.g., required credentials)
    pub trust_requirements: Vec<String>,

    /// Current status
    pub status: JobStatus,
}

/// A bid from a node to execute a job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bid {
    /// ID of the job being bid on
    pub job_id: String,

    /// ID of the node making the bid
    pub node_id: String,

    /// Node DID
    pub node_did: String,

    /// Bid amount in resource units
    pub bid_amount: u64,

    /// Estimated execution time in seconds
    pub estimated_execution_time: u64,

    /// Timestamp of the bid
    pub timestamp: DateTime<Utc>,

    /// Expiration of the bid
    pub expires_at: DateTime<Utc>,

    /// Node capacity information
    pub node_capacity: NodeCapability,

    /// Reputation score of the node (0-100)
    pub reputation_score: u32,

    /// Optional proof of capability
    pub capability_proof: Option<String>,
}

/// Node capability information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapability {
    /// Node ID
    pub node_id: String,

    /// Node DID
    pub node_did: String,

    /// Available memory in MB
    pub available_memory_mb: u32,

    /// Available CPU cores
    pub available_cpu_cores: u32,

    /// Available storage in MB
    pub available_storage_mb: u32,

    /// CPU architecture
    pub cpu_architecture: String,

    /// Special features (e.g., "gpu", "avx", etc.)
    pub features: Vec<String>,

    /// Location information (optional)
    pub location: Option<String>,

    /// Network bandwidth in Mbps
    pub bandwidth_mbps: u32,

    /// Supported job types
    pub supported_job_types: Vec<String>,

    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

/// Job execution receipt with DAG anchoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobExecutionReceipt {
    /// Job ID
    pub job_id: String,

    /// Node ID that executed the job
    pub executor_node_id: String,

    /// Node DID that executed the job
    pub executor_node_did: String,

    /// Execution metrics
    pub metrics: ExecutionMetrics,

    /// Output data CID (if applicable)
    pub output_data_cid: Option<String>,

    /// Execution start time
    pub start_time: DateTime<Utc>,

    /// Execution end time
    pub end_time: DateTime<Utc>,

    /// Resource usage
    pub resource_usage: Vec<(String, u64)>,

    /// Receipt CID in the DAG
    pub receipt_cid: String,

    /// Federation verification status
    pub verified_by_federation: bool,

    /// Federation DID that verified the receipt
    pub verifier_did: Option<String>,

    /// Verification timestamp
    pub verified_at: Option<DateTime<Utc>>,

    /// Result status code (0 = success, non-zero = error)
    pub result_status: i32,

    /// Result hash (hash of output data)
    pub result_hash: Option<String>,

    /// Result metadata (JSON string with additional info)
    pub result_metadata: Option<String>,

    /// Execution logs
    pub execution_logs: Vec<String>,
}

/// Network behavior for P2P communication
pub struct NetworkBehavior {
    /// Libp2p event sender
    #[allow(dead_code)]
    event_sender: mpsc::Sender<NetworkEvent>,

    /// Connected peers
    #[allow(dead_code)]
    peers: HashSet<String>,
}

/// Network events
#[derive(Debug)]
pub enum NetworkEvent {
    /// New job available
    NewJob(JobManifest),

    /// New bid received
    NewBid(Bid),

    /// Job status update
    JobStatusUpdate { job_id: String, status: JobStatus }, // This is the local, detailed JobStatus

    /// New receipt available - updated to use standardized ExecutionReceipt
    NewReceipt(ExecutionReceipt),
}

impl PlanetaryMeshNode {
    /// Create a new planetary mesh node
    pub fn new(node_did: Did, capabilities: NodeCapability) -> Result<Self> {
        // Create a node ID from the DID
        let node_id = node_did.to_string().replace("did:key:", "node:");

        // Create a VM for WASM execution
        let vm = CoVm::new(ResourceLimits::default());

        Ok(Self {
            node_did,
            node_id,
            capabilities,
            jobs: Arc::new(Mutex::new(HashMap::new())),
            bids: Arc::new(Mutex::new(HashMap::new())),
            receipts: Arc::new(Mutex::new(HashMap::new())),
            vm,
            network: None,
        })
    }

    /// Execute a WASM module locally
    pub async fn execute_wasm(&self, wasm_bytes: &[u8]) -> Result<ExecutionMetrics> {
        // Set up a host context for execution
        let host_context = HostContext::default();

        // Execute the WASM module
        let updated_host_context = self
            .vm
            .execute(wasm_bytes, host_context)
            .map_err(|e| MeshError::ExecutionFailed(e.to_string()))?;

        // Extract metrics
        let metrics = updated_host_context.metrics.lock().unwrap().clone();

        Ok(metrics)
    }

    /// Load and execute a WASM module from a file
    pub async fn execute_wasm_file(&self, path: &Path) -> Result<ExecutionMetrics> {
        let wasm_bytes = std::fs::read(path)
            .map_err(|e| MeshError::ExecutionFailed(format!("Failed to read WASM: {}", e)))?;
        let host_context = HostContext::default();
        let updated_host_context = self
            .vm
            .execute(&wasm_bytes, host_context)
            .map_err(|e| MeshError::ExecutionFailed(e.to_string()))?;
        let metrics = updated_host_context.metrics.lock().unwrap().clone();
        Ok(metrics)
    }

    /// Create a job execution receipt
    pub async fn create_execution_receipt(
        &self,
        job_id: &str,
        job_status: StandardJobStatus, // Use standardized JobStatus for receipt
        metrics: &ExecutionMetrics,    // Pass by reference
        resource_usage_vec: Vec<(String, u64)>, // Changed from direct HashMap to allow conversion
        result_data_cid: Option<String>,
        logs_cid: Option<String>, // Added for logs
        execution_start_time_unix: u64, // Added start time
                                  // signature will be generated internally if not provided, or taken as param if pre-signed
    ) -> Result<ExecutionReceipt> {
        let now_dt = Utc::now();
        let execution_end_time_unix = now_dt.timestamp() as u64;

        // Convert resource_usage Vec<(String, u64)> to HashMap<ResourceType, u64>
        // This requires ResourceType to be parsable from String or for the input to change.
        // For now, assuming a helper or direct construction if ResourceType is simple.
        // This part might need further refinement based on how ResourceType is handled.
        let mut resource_usage_map = HashMap::new();
        for (rt_str, amount) in resource_usage_vec {
            // Placeholder: This conversion needs to be robust.
            // Assuming icn_economics::ResourceType can be created from rt_str
            // For example, if ResourceType implements FromStr or similar.
            // If not, this mapping logic needs to be defined.
            // For simplicity, let's assume a direct mapping for Cpu, Memory, Io for now.
            use icn_economics::ResourceType;
            let key = match rt_str.to_lowercase().as_str() {
                "cpu" | "compute" => ResourceType::Cpu,
                "memory" | "mem" => ResourceType::Memory,
                "io" => ResourceType::Io,
                _ => continue, // Skip unknown resource types for now or handle error
            };
            resource_usage_map.insert(key, amount);
        }

        // Placeholder for actual signature generation
        let signature_bytes = Vec::new(); // In a real scenario, sign the relevant fields

        let receipt = ExecutionReceipt {
            job_id: job_id.to_string(),
            executor: self.node_did.clone(), // Assuming self.node_did is the Did String
            status: job_status,
            result_data_cid,
            logs_cid,
            resource_usage: resource_usage_map,
            execution_start_time: execution_start_time_unix,
            execution_end_time: execution_end_time_unix,
            execution_end_time_dt: now_dt,
            signature: signature_bytes, // Placeholder
            coop_id: None,              // TODO: Determine how to populate these if needed
            community_id: None,         // TODO: Determine how to populate these if needed
        };

        // Store the receipt locally
        // The key for the receipts map should probably be the receipt's CID or the job_id.
        // Using job_id for now.
        {
            let mut receipts_store = self.receipts.lock().unwrap();
            receipts_store.insert(job_id.to_string(), receipt.clone());
        }

        // TODO: Broadcast receipt to the network (using receipt.cid()?)

        Ok(receipt)
    }
}

#[async_trait]
impl MeshNode for PlanetaryMeshNode {
    fn node_did(&self) -> &Did {
        &self.node_did
    }

    fn node_id(&self) -> &str {
        &self.node_id
    }

    fn capabilities(&self) -> NodeCapability {
        self.capabilities.clone()
    }

    async fn submit_job(&self, manifest: JobManifest) -> Result<String> {
        // Store the job
        let mut jobs = self.jobs.lock().unwrap();
        let job_id = manifest.id.clone();
        jobs.insert(job_id.clone(), manifest);

        // In a real implementation, we would publish the job to the network

        Ok(job_id)
    }

    async fn get_job_status(&self, job_id: &str) -> Result<JobStatus> {
        let jobs = self.jobs.lock().unwrap();
        let job = jobs
            .get(job_id)
            .ok_or_else(|| MeshError::JobNotFound(job_id.to_string()))?;

        Ok(job.status.clone())
    }

    async fn list_jobs(&self) -> Result<Vec<JobManifest>> {
        let jobs = self.jobs.lock().unwrap();
        let job_list = jobs.values().cloned().collect();

        Ok(job_list)
    }

    async fn get_bids(&self, job_id: &str) -> Result<Vec<Bid>> {
        let bids = self.bids.lock().unwrap();
        let job_bids = bids.get(job_id).cloned().unwrap_or_default();

        Ok(job_bids)
    }

    async fn accept_bid(&self, job_id: &str, node_id: &str) -> Result<()> {
        // Update job status
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs
            .get_mut(job_id)
            .ok_or_else(|| MeshError::JobNotFound(job_id.to_string()))?;

        job.status = JobStatus::Assigned {
            node_id: node_id.to_string(),
        };

        // In a real implementation, we would notify the winning node

        Ok(())
    }

    async fn submit_bid(&self, job_id: &str, bid: Bid) -> Result<()> {
        // Store the bid
        let mut bids = self.bids.lock().unwrap();
        let job_bids = bids.entry(job_id.to_string()).or_default();
        job_bids.push(bid);

        // In a real implementation, we would publish the bid to the network

        Ok(())
    }

    async fn cancel_job(&self, job_id: &str) -> Result<()> {
        // Update job status
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs
            .get_mut(job_id)
            .ok_or_else(|| MeshError::JobNotFound(job_id.to_string()))?;

        job.status = JobStatus::Cancelled; // This is the local, detailed JobStatus

        // In a real implementation, we would notify the network

        Ok(())
    }

    async fn get_job_receipt(&self, job_id: &str) -> Result<Option<ExecutionReceipt>> {
        // Updated return type
        let receipts = self.receipts.lock().unwrap();
        let receipt = receipts.get(job_id).cloned();

        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_job_submission_and_status() {
        // Create a test node
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string();
        let capabilities = NodeCapability {
            node_id: "test-node-1".to_string(),
            node_did: did.clone(),
            available_memory_mb: 1024,
            available_cpu_cores: 4,
            available_storage_mb: 10240,
            cpu_architecture: "x86_64".to_string(),
            features: vec!["avx".to_string(), "sse4".to_string()],
            location: Some("us-west".to_string()),
            bandwidth_mbps: 1000,
            supported_job_types: vec!["compute".to_string(), "storage".to_string()],
            updated_at: Utc::now(),
        };

        let node = PlanetaryMeshNode::new(did, capabilities).unwrap();

        // Create a job manifest
        let job_id = Uuid::new_v4().to_string();
        let token = ScopedResourceToken {
            resource_type: "compute".to_string(),
            amount: 100,
            scope: "test-scope".to_string(),
            expires_at: None,
            issuer: None,
        };

        let manifest = JobManifest {
            id: job_id.clone(),
            submitter_did: "did:key:z6MkuBsxRsRu3PU1VzZ5xnqNtXWRwLtrGdxdMeMFuxP5xyVp".to_string(),
            description: "Test job".to_string(),
            created_at: Utc::now(),
            expires_at: None,
            wasm_cid: "wasm-cid".to_string(),
            ccl_cid: None,
            input_data_cid: None,
            output_location: None,
            requirements: ComputeRequirements {
                min_memory_mb: 512,
                min_cpu_cores: 2,
                min_storage_mb: 1024,
                max_execution_time_secs: 60,
                required_features: vec![],
            },
            priority: JobPriority::Medium,
            resource_token: token,
            trust_requirements: vec![],
            status: JobStatus::Created,
        };

        // Submit the job
        let submitted_id = node.submit_job(manifest).await.unwrap();
        assert_eq!(submitted_id, job_id);

        // Check the job status
        let status = node.get_job_status(&job_id).await.unwrap();
        assert_eq!(status, JobStatus::Created);

        // List jobs
        let jobs = node.list_jobs().await.unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, job_id);
    }
}
