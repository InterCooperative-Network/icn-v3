use anyhow::Result;
use async_trait::async_trait;
use icn_core_vm::{CoVm, ExecutionMetrics as CoreVmExecutionMetrics, HostContext, ResourceLimits};
#[cfg(feature = "legacy-identity")]
use icn_identity_core::vc::{ExecutionMetrics as VcExecutionMetrics, ExecutionReceiptCredential};
use icn_identity::{TrustBundle, TrustValidationError, Did};
use icn_economics::ResourceType;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

// Import the context module
mod context;
pub use context::RuntimeContext;
pub use context::RuntimeContextBuilder;

// Import the host environment module
mod host_environment;
pub use host_environment::ConcreteHostEnvironment;

// Import the wasm module
mod wasm;
pub use wasm::register_host_functions;

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

    #[error("Resource authorization failed: {0}")]
    AuthorizationFailed(String),
    
    #[error("Trust bundle verification failed: {0}")]
    TrustBundleVerificationError(#[from] TrustValidationError),
    
    #[error("No trust validator configured")]
    NoTrustValidator,
}

/// Context for WASM virtual machine execution
#[derive(Debug, Clone, Default)]
pub struct VmContext {
    /// DID of the executor
    pub executor_did: String,

    /// Scope of the execution
    pub scope: Option<String>,

    /// Epoch of the DAG at execution time
    pub epoch: Option<String>,

    /// CID of the code being executed
    pub code_cid: Option<String>,

    /// Resource limits
    pub resource_limits: Option<ResourceLimits>,
}

/// Result of a WASM execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// The metrics collected during execution
    pub metrics: CoreVmExecutionMetrics,

    /// List of CIDs anchored during execution
    pub anchored_cids: Vec<String>,

    /// Resource usage during execution
    pub resource_usage: Vec<(String, u64)>,

    /// Log messages produced during execution
    pub logs: Vec<String>,
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
    pub metrics: CoreVmExecutionMetrics,

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
    
    /// Runtime context with shared DAG store
    context: RuntimeContext,
}

impl Runtime {
    /// Create a new runtime with specified storage
    pub fn new(storage: Arc<dyn RuntimeStorage>) -> Self {
        Self {
            vm: CoVm::default(),
            storage,
            context: RuntimeContext::new(),
        }
    }

    /// Create a new runtime with custom resource limits
    pub fn with_limits(storage: Arc<dyn RuntimeStorage>, limits: ResourceLimits) -> Self {
        Self {
            vm: CoVm::new(limits),
            storage,
            context: RuntimeContext::new(),
        }
    }
    
    /// Create a new runtime with specified context
    pub fn with_context(storage: Arc<dyn RuntimeStorage>, context: RuntimeContext) -> Self {
        Self {
            vm: CoVm::default(),
            storage,
            context,
        }
    }
    
    /// Get a reference to the runtime context
    pub fn context(&self) -> &RuntimeContext {
        &self.context
    }
    
    /// Get the shared DAG store
    pub fn dag_store(&self) -> Arc<icn_types::dag_store::SharedDagStore> {
        self.context.dag_store.clone()
    }

    /// Execute a proposal by ID
    pub async fn execute_proposal(&self, proposal_id: &str) -> Result<ExecutionReceipt> {
        // Load the proposal
        let mut proposal = self.storage.load_proposal(proposal_id).await?;

        // Check if the proposal is in a state that can be executed
        if proposal.state != ProposalState::Approved {
            return Err(RuntimeError::InvalidProposalState(format!(
                "Proposal must be in Approved state, not {:?}",
                proposal.state
            ))
            .into());
        }

        // Check if quorum has been reached
        match proposal.quorum_status {
            QuorumStatus::MajorityReached
            | QuorumStatus::ThresholdReached
            | QuorumStatus::WeightedReached => {
                // Quorum has been reached, continue with execution
            }
            _ => {
                return Err(RuntimeError::InvalidProposalState(format!(
                    "Quorum must be reached, current status: {:?}",
                    proposal.quorum_status
                ))
                .into());
            }
        }

        // Load the WASM module
        let wasm_bytes = self
            .storage
            .load_wasm(&proposal.wasm_cid)
            .await
            .map_err(|e| RuntimeError::LoadError(format!("Failed to load WASM module: {}", e)))?;

        // Set up the execution context
        let context = HostContext::default();

        // Execute the WASM module
        let updated_context = self.vm.execute(&wasm_bytes, context).map_err(|e| {
            RuntimeError::ExecutionError(format!("Failed to execute WASM module: {}", e))
        })?;

        // Extract execution metrics and results
        let final_metrics = {
            let guard = updated_context.metrics.lock().unwrap();
            guard.clone()
        };
        let final_anchored_cids = {
            let guard = updated_context.anchored_cids.lock().unwrap();
            guard.clone()
        };
        let final_resource_usage = {
            let guard = updated_context.resource_usage.lock().unwrap();
            guard.clone()
        };
        let _final_logs = {
            let guard = updated_context.logs.lock().unwrap();
            guard.clone()
        };

        // Update proposal state
        proposal.state = ProposalState::Executed;
        self.storage.update_proposal(&proposal).await?;

        // Create the execution receipt
        let receipt = ExecutionReceipt {
            proposal_id: proposal_id.to_string(),
            wasm_cid: proposal.wasm_cid,
            ccl_cid: proposal.ccl_cid,
            metrics: final_metrics,
            anchored_cids: final_anchored_cids,
            resource_usage: final_resource_usage,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
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
        let wasm_bytes = std::fs::read(path).map_err(|e| {
            RuntimeError::LoadError(format!(
                "Failed to read WASM file {}: {}",
                path.display(),
                e
            ))
        })?;

        // Set up the execution context
        let context = HostContext::default();

        // Execute the WASM module
        let updated_context = self.vm.execute(&wasm_bytes, context).map_err(|e| {
            RuntimeError::ExecutionError(format!("Failed to execute WASM module: {}", e))
        })?;

        // Extract execution metrics and results
        let final_metrics = {
            let guard = updated_context.metrics.lock().unwrap();
            guard.clone()
        };
        let final_anchored_cids = {
            let guard = updated_context.anchored_cids.lock().unwrap();
            guard.clone()
        };
        let final_resource_usage = {
            let guard = updated_context.resource_usage.lock().unwrap();
            guard.clone()
        };
        let _final_logs = {
            let guard = updated_context.logs.lock().unwrap();
            guard.clone()
        };

        // Create the execution receipt (without storing it)
        let receipt = ExecutionReceipt {
            proposal_id: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            wasm_cid: "local-file".to_string(),
            ccl_cid: "local-file".to_string(),
            metrics: final_metrics,
            anchored_cids: final_anchored_cids,
            resource_usage: final_resource_usage,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            dag_epoch: None,
            receipt_cid: None,
            federation_signature: None,
        };

        Ok(receipt)
    }

    /// Execute a WASM binary with the given context
    pub fn execute_wasm(&self, wasm_bytes: &[u8], context: VmContext) -> Result<ExecutionResult> {
        // Convert the VM context to a host context
        let host_context = self.vm_context_to_host_context(context);

        // Create a wasmtime store and register the economics host functions
        let mut linker = wasmtime::Linker::new(self.vm.engine());
        let mut store = wasmtime::Store::new(self.vm.engine(), wasm::linker::StoreData::new());
        
        // Set up the host environment in the store data
        let host_env = ConcreteHostEnvironment::new(
            Arc::new(self.context.clone()),
            context.executor_did.clone()
        );
        store.data_mut().set_host(host_env);
        
        // Register the economic host functions
        wasm::linker::register_host_functions(&mut linker)?;
        
        // Execute the WASM module
        let updated_host_context = self
            .vm
            .execute_with_linker(wasm_bytes, host_context, &linker, &mut store)
            .map_err(|e| RuntimeError::ExecutionError(format!("Failed to execute WASM: {}", e)))?;

        // Extract the final metrics and other data from the host context
        let metrics_guard = updated_host_context.metrics.lock().unwrap();
        let final_metrics = metrics_guard.clone();
        drop(metrics_guard);

        let anchored_cids_guard = updated_host_context.anchored_cids.lock().unwrap();
        let final_anchored_cids = anchored_cids_guard.clone();
        drop(anchored_cids_guard);

        let resource_usage_guard = updated_host_context.resource_usage.lock().unwrap();
        let final_resource_usage = resource_usage_guard.clone();
        drop(resource_usage_guard);

        let logs_guard = updated_host_context.logs.lock().unwrap();
        let final_logs = logs_guard.clone();
        drop(logs_guard);

        Ok(ExecutionResult {
            metrics: final_metrics,
            anchored_cids: final_anchored_cids,
            resource_usage: final_resource_usage,
            logs: final_logs,
        })
    }

    /// Issue an execution receipt after successful execution
    #[cfg(feature = "legacy-identity")]
    pub fn issue_receipt(
        &self,
        wasm_cid: &str,
        ccl_cid: &str,
        result: &ExecutionResult,
        context: &VmContext,
    ) -> Result<ExecutionReceiptCredential> {
        // Convert ExecutionMetrics to VC ExecutionMetrics
        let vc_metrics = VcExecutionMetrics {
            fuel_used: result.metrics.fuel_used,
            host_calls: result.metrics.host_calls,
            io_bytes: result.metrics.io_bytes,
        };

        // Create a unique ID for the receipt
        let receipt_id = format!("urn:icn:receipt:{}", Uuid::new_v4());

        // Create the execution receipt
        let receipt = ExecutionReceiptCredential::new(
            receipt_id,
            context.executor_did.clone(),           // issuer
            format!("proposal-{}", Uuid::new_v4()), // placeholder proposal ID
            wasm_cid.to_string(),
            ccl_cid.to_string(),
            vc_metrics,
            result.anchored_cids.clone(),
            result.resource_usage.clone(),
            chrono::Utc::now().timestamp_millis() as u64,
            None, // dag_epoch
            None, // receipt_cid
            None, // signature
        );

        Ok(receipt)
    }

    /// Anchor a receipt to the DAG and return the CID
    #[cfg(feature = "legacy-identity")]
    pub async fn anchor_receipt(&self, receipt: &ExecutionReceiptCredential) -> Result<String> {
        // Convert to JSON
        let receipt_json = serde_json::to_string(receipt).map_err(|e| {
            RuntimeError::ReceiptError(format!("Failed to serialize receipt: {}", e))
        })?;

        // Store the receipt
        let receipt_cid = self.storage.anchor_to_dag(&receipt_json).await?;

        Ok(receipt_cid)
    }

    /// Helper function to convert VmContext (icn-runtime specific) to HostContext (icn-core-vm specific)
    fn vm_context_to_host_context(&self, vm_context: VmContext) -> HostContext {
        // Create a ConcreteHostEnvironment to handle economics functions
        let host_env = ConcreteHostEnvironment::new(
            Arc::new(self.context.clone()),
            vm_context.executor_did.clone()
        );
        
        // Create a StoreData to hold the host environment
        let mut store_data = wasm::linker::StoreData::new();
        store_data.set_host(host_env);
        
        // Set up a HostContext with default values
        let mut host_context = HostContext::default();
        
        // If resource limits are provided in the VM context, apply them to the host context
        if let Some(limits) = &vm_context.resource_limits {
            // Update metrics to track these limits
            let mut metrics = host_context.metrics.lock().unwrap();
            metrics.max_fuel = limits.max_fuel;
            metrics.max_host_calls = limits.max_host_calls as u64;
            metrics.max_io_bytes = limits.max_io_bytes;
            metrics.max_anchored_cids = limits.max_anchored_cids;
            metrics.max_job_submissions = limits.max_job_submissions;
        }
        
        // Return the configured host context
        host_context
    }

    /// Verify a trust bundle using the configured trust validator
    pub fn verify_trust_bundle(&self, bundle: &TrustBundle) -> Result<(), RuntimeError> {
        let validator = self.context.trust_validator()
            .ok_or(RuntimeError::NoTrustValidator)?;
            
        validator.set_trust_bundle(bundle.clone())
            .map_err(RuntimeError::TrustBundleVerificationError)
    }
    
    /// Register a trusted signer with DID and verifying key
    pub fn register_trusted_signer(&self, did: Did, key: VerifyingKey) -> Result<(), RuntimeError> {
        let validator = self.context.trust_validator()
            .ok_or(RuntimeError::NoTrustValidator)?;
        
        validator.register_signer(did, key);
        Ok(())
    }
    
    /// Check if a signer is authorized
    pub fn is_authorized_signer(&self, did: &Did) -> Result<bool, RuntimeError> {
        let validator = self.context.trust_validator()
            .ok_or(RuntimeError::NoTrustValidator)?;
            
        validator.is_authorized_signer(did)
            .map_err(RuntimeError::TrustBundleVerificationError)
    }
    
    /// Host function for WASM to retrieve a trust bundle from a given CID
    pub async fn host_get_trust_bundle(&self, _cid: &str) -> Result<bool, RuntimeError> {
        // This would normally retrieve a trust bundle from storage and verify it
        // For now, just a stub that returns success
        // In a real implementation, we would:
        // 1. Retrieve the trust bundle from storage by CID
        // 2. Verify it using the trust validator
        // 3. Return true if verification succeeds
        
        // Check if we have a trust validator
        if self.context.trust_validator().is_none() {
            return Err(RuntimeError::NoTrustValidator);
        }
        
        // For now, just return true if we have a trust validator
        Ok(true)
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
    use super::*;
    use anyhow::anyhow;
    use icn_identity::{TrustBundle, TrustValidator};
    use std::fs;
    use std::sync::{Arc, Mutex};

    // A mock storage implementation for testing
    struct MockStorage {
        proposals: Mutex<Vec<Proposal>>,
        wasm_modules: Mutex<std::collections::HashMap<String, Vec<u8>>>,
        receipts: Mutex<std::collections::HashMap<String, String>>,
        anchored_cids: Mutex<Vec<String>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                proposals: Mutex::new(vec![]),
                wasm_modules: Mutex::new(std::collections::HashMap::new()),
                receipts: Mutex::new(std::collections::HashMap::new()),
                anchored_cids: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl RuntimeStorage for MockStorage {
        async fn load_proposal(&self, id: &str) -> Result<Proposal> {
            let proposals = self.proposals.lock().unwrap();
            proposals
                .iter()
                .find(|p| p.id == id)
                .cloned()
                .ok_or_else(|| anyhow!("Proposal not found"))
        }

        async fn update_proposal(&self, proposal: &Proposal) -> Result<()> {
            let mut proposals = self.proposals.lock().unwrap();

            // Remove existing proposal with the same ID
            proposals.retain(|p| p.id != proposal.id);

            // Add the updated proposal
            proposals.push(proposal.clone());

            Ok(())
        }

        async fn load_wasm(&self, cid: &str) -> Result<Vec<u8>> {
            let modules = self.wasm_modules.lock().unwrap();
            modules
                .get(cid)
                .cloned()
                .ok_or_else(|| anyhow!("WASM module not found"))
        }

        async fn store_receipt(&self, receipt: &ExecutionReceipt) -> Result<String> {
            let receipt_json = serde_json::to_string(receipt)?;
            let receipt_cid = format!("receipt-{}", Uuid::new_v4());

            let mut receipts = self.receipts.lock().unwrap();
            receipts.insert(receipt_cid.clone(), receipt_json);

            Ok(receipt_cid)
        }

        async fn anchor_to_dag(&self, cid: &str) -> Result<String> {
            let mut anchored = self.anchored_cids.lock().unwrap();
            anchored.push(cid.to_string());

            let anchor_id = format!("anchor-{}", Uuid::new_v4());
            Ok(anchor_id)
        }
    }

    #[tokio::test]
    async fn test_execute_wasm_file() -> Result<()> {
        // This test requires a compiled WASM file from CCL/DSL
        // For testing, we'll check if the file exists first
        let wasm_path = Path::new("../../../examples/budget.wasm");

        if !wasm_path.exists() {
            println!("Test WASM file not found, skipping test_execute_wasm_file test");
            return Ok(());
        }

        // Read the WASM file
        let wasm_bytes = fs::read(wasm_path)?;

        // Create a runtime with mock storage and trust validator
        let storage = Arc::new(MockStorage::new());
        let trust_validator = Arc::new(TrustValidator::new());
        let context = RuntimeContext::new()
            .with_trust_validator(trust_validator);
        let runtime = Runtime::with_context(storage, context);

        // Create a VM context
        let context = VmContext {
            executor_did: "did:icn:test".to_string(),
            scope: Some("test-scope".to_string()),
            epoch: Some("2023-01-01".to_string()),
            code_cid: Some("test-cid".to_string()),
            resource_limits: None,
        };

        // Execute the WASM module
        let result = runtime.execute_wasm(&wasm_bytes, context.clone())?;

        // Verify that execution succeeded and metrics were collected
        assert!(result.metrics.fuel_used > 0, "Expected fuel usage metrics");

        // Test trust bundle verification
        let test_bundle = TrustBundle::new(
            "test-cid".to_string(),
            icn_identity::FederationMetadata {
                name: "Test Federation".to_string(),
                description: Some("Test Description".to_string()),
                version: "1.0".to_string(),
                additional: std::collections::HashMap::new(),
            }
        );
        
        // This will fail because no signers are registered and no quorum proof is added
        assert!(runtime.verify_trust_bundle(&test_bundle).is_err());

        Ok(())
    }
}
