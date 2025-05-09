use anyhow::{anyhow, Result};
use async_trait::async_trait;
use icn_core_vm::{CoVm, ExecutionMetrics, HostContext, ResourceLimits};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

/// Error types specific to the runtime
#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Failed to execute WASM module: {0}")]
    ExecutionError(String),
    
    #[error("Failed to load WASM module: {0}")]
    LoadError(String),
    
    #[error("Failed to generate execution receipt: {0}")]
    ReceiptError(String),
    
    #[error("Invalid proposal state: {0}")]
    InvalidProposalState(String),
}

/// Represents a governance proposal that can be executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    /// Unique identifier for the proposal
    pub id: String,
    
    /// Content ID (CID) of the compiled WASM module
    pub wasm_cid: String,
    
    /// Content ID (CID) of the source CCL
    pub ccl_cid: String,
    
    /// Current state of the proposal
    pub state: ProposalState,
    
    /// Quorum status
    pub quorum_status: QuorumStatus,
}

/// State of a governance proposal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalState {
    /// Proposal has been created but not yet voted on
    Created,
    
    /// Proposal is currently being voted on
    Voting,
    
    /// Proposal has been approved and is ready for execution
    Approved,
    
    /// Proposal has been rejected
    Rejected,
    
    /// Proposal has been executed
    Executed,
}

/// Status of quorum for a proposal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuorumStatus {
    /// Quorum has not been reached
    Pending,
    
    /// Majority quorum reached
    MajorityReached,
    
    /// Threshold quorum reached
    ThresholdReached,
    
    /// Weighted quorum reached
    WeightedReached,
    
    /// Quorum failed to reach
    Failed,
}

/// Storage interface for the runtime
#[async_trait]
pub trait RuntimeStorage: Send + Sync {
    /// Load a proposal by ID
    async fn load_proposal(&self, id: &str) -> Result<Proposal>;
    
    /// Update a proposal
    async fn update_proposal(&self, proposal: &Proposal) -> Result<()>;
    
    /// Load a WASM module by CID
    async fn load_wasm(&self, cid: &str) -> Result<Vec<u8>>;
    
    /// Store an execution receipt
    async fn store_receipt(&self, receipt: &ExecutionReceipt) -> Result<String>;
    
    /// Anchor a CID to the DAG
    async fn anchor_to_dag(&self, cid: &str) -> Result<String>;
}

/// Execution receipt issued after successful execution of a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    /// Proposal ID this receipt is for
    pub proposal_id: String,
    
    /// Content ID (CID) of the executed WASM module
    pub wasm_cid: String,
    
    /// Content ID (CID) of the source CCL
    pub ccl_cid: String,
    
    /// Execution metrics
    pub metrics: ExecutionMetrics,
    
    /// Anchored CIDs during execution
    pub anchored_cids: Vec<String>,
    
    /// Resource usage during execution
    pub resource_usage: Vec<(String, u64)>,
    
    /// Timestamp of execution
    pub timestamp: u64,
    
    /// DAG epoch of execution
    pub dag_epoch: Option<u64>,
    
    /// Receipt CID (filled after anchoring)
    pub receipt_cid: Option<String>,
    
    /// Signature from the executing federation
    pub federation_signature: Option<String>,
}

/// The ICN Runtime for executing governance proposals
pub struct Runtime {
    /// CoVM instance for executing WASM
    vm: CoVm,
    
    /// Storage backend
    storage: Arc<dyn RuntimeStorage>,
}

impl Runtime {
    /// Create a new runtime with specified storage
    pub fn new(storage: Arc<dyn RuntimeStorage>) -> Self {
        Self {
            vm: CoVm::default(),
            storage,
        }
    }
    
    /// Create a new runtime with custom resource limits
    pub fn with_limits(storage: Arc<dyn RuntimeStorage>, limits: ResourceLimits) -> Self {
        Self {
            vm: CoVm::new(limits),
            storage,
        }
    }
    
    /// Execute a proposal by ID
    pub async fn execute_proposal(&self, proposal_id: &str) -> Result<ExecutionReceipt> {
        // Load the proposal
        let mut proposal = self.storage.load_proposal(proposal_id).await?;
        
        // Check if the proposal is in a state that can be executed
        if proposal.state != ProposalState::Approved {
            return Err(RuntimeError::InvalidProposalState(
                format!("Proposal must be in Approved state, not {:?}", proposal.state)
            ).into());
        }
        
        // Check if quorum has been reached
        match proposal.quorum_status {
            QuorumStatus::MajorityReached | 
            QuorumStatus::ThresholdReached | 
            QuorumStatus::WeightedReached => {
                // Quorum has been reached, continue with execution
            },
            _ => {
                return Err(RuntimeError::InvalidProposalState(
                    format!("Quorum must be reached, current status: {:?}", proposal.quorum_status)
                ).into());
            }
        }
        
        // Load the WASM module
        let wasm_bytes = self.storage.load_wasm(&proposal.wasm_cid).await
            .map_err(|e| RuntimeError::LoadError(format!("Failed to load WASM module: {}", e)))?;
        
        // Set up the execution context
        let mut context = HostContext::default();
        
        // Execute the WASM module
        self.vm.execute(&wasm_bytes, &mut context)
            .map_err(|e| RuntimeError::ExecutionError(format!("Failed to execute WASM module: {}", e)))?;
        
        // Extract execution metrics and results
        let metrics = context.metrics.lock().unwrap().clone();
        let anchored_cids = context.anchored_cids.lock().unwrap().clone();
        let resource_usage = context.resource_usage.lock().unwrap().clone();
        
        // Update proposal state
        proposal.state = ProposalState::Executed;
        self.storage.update_proposal(&proposal).await?;
        
        // Create the execution receipt
        let receipt = ExecutionReceipt {
            proposal_id: proposal_id.to_string(),
            wasm_cid: proposal.wasm_cid,
            ccl_cid: proposal.ccl_cid,
            metrics,
            anchored_cids,
            resource_usage,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            dag_epoch: None,
            receipt_cid: None,
            federation_signature: None,
        };
        
        // Store and anchor the receipt
        let receipt_cid = self.storage.store_receipt(&receipt).await?;
        
        // Anchor the receipt CID to the DAG
        let _dag_anchor = self.storage.anchor_to_dag(&receipt_cid).await?;
        
        // Return the receipt
        Ok(receipt)
    }
    
    /// Load and execute a WASM module from a file
    pub async fn execute_wasm_file(&self, path: &Path) -> Result<ExecutionReceipt> {
        // Read the WASM file
        let wasm_bytes = std::fs::read(path)
            .map_err(|e| RuntimeError::LoadError(format!("Failed to read WASM file {}: {}", path.display(), e)))?;
        
        // Set up the execution context
        let mut context = HostContext::default();
        
        // Execute the WASM module
        self.vm.execute(&wasm_bytes, &mut context)
            .map_err(|e| RuntimeError::ExecutionError(format!("Failed to execute WASM module: {}", e)))?;
        
        // Extract execution metrics and results
        let metrics = context.metrics.lock().unwrap().clone();
        let anchored_cids = context.anchored_cids.lock().unwrap().clone();
        let resource_usage = context.resource_usage.lock().unwrap().clone();
        
        // Create the execution receipt (without storing it)
        let receipt = ExecutionReceipt {
            proposal_id: path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            wasm_cid: "local-file".to_string(),
            ccl_cid: "local-file".to_string(),
            metrics,
            anchored_cids,
            resource_usage,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            dag_epoch: None,
            receipt_cid: None,
            federation_signature: None,
        };
        
        Ok(receipt)
    }
}

/// Module providing executable trait for CCL DSL files
pub mod dsl {
    use super::*;
    
    /// Trait for CCL DSL executables
    pub trait DslExecutable {
        /// Execute the DSL with the given runtime
        fn execute(&self, runtime: &Runtime) -> Result<ExecutionReceipt>;
    }
}

#[cfg(test)]
mod tests {
    // Tests will be added once we have the full implementation
} 