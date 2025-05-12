use anyhow::{Result, anyhow};
use async_trait::async_trait;
use icn_core_vm::{CoVm, ExecutionMetrics as CoreVmExecutionMetrics, HostContext, ResourceLimits};
use wasmtime::{Module, Caller, Engine, Instance, Linker, Store, TypedFunc, Val, Trap};
use icn_types::runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics};
use icn_identity::{TrustBundle, TrustValidationError, Did};
use icn_economics::ResourceType;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use uuid::Uuid;
use std::str::FromStr;
use std::collections::HashMap;
use chrono::Utc;
use cid::Cid;
use icn_identity::KeyPair as IcnKeyPair;
use icn_types::mesh::MeshJob;
use icn_types::mesh::JobStatus as StandardJobStatus;
use icn_mesh_receipts::{ExecutionReceipt, sign_receipt_in_place};

// Import the context module
mod context;
pub use context::RuntimeContext;
pub use context::RuntimeContextBuilder;

// Import the host environment module
mod host_environment;
pub use host_environment::ConcreteHostEnvironment;

// Import the job execution context module
pub mod job_execution_context;

// Import the wasm module
mod wasm;
pub use wasm::register_host_functions;

/// Module cache trait for caching compiled WASM modules
#[async_trait]
pub trait ModuleCache: Send + Sync {
    /// Get a cached module by its CID
    async fn get_module(&self, cid: &str) -> Option<Module>;
    
    /// Store a module in the cache
    async fn store_module(&self, cid: &str, module: Module) -> Result<()>;
}

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

    #[error("Host environment not set")]
    HostEnvironmentNotSet,

    #[error("Instantiation failed: {0}")]
    Instantiation(String),

    #[error("Execution failed: {0}")]
    Execution(String),

    #[error("Function not found: {0}")]
    FunctionNotFound(String),
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
    
    /// Optional cooperative ID that this execution is associated with
    pub coop_id: Option<String>,
    
    /// Optional community ID that this execution is associated with
    pub community_id: Option<String>,
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

/// The ICN Runtime for executing governance proposals
pub struct Runtime {
    /// CoVM instance for executing WASM
    vm: CoVm,

    /// Storage backend
    storage: Arc<dyn RuntimeStorage>,
    
    /// Runtime context with shared DAG store
    context: RuntimeContext,

    /// Wasmtime engine
    engine: Engine,

    /// Wasmtime linker
    linker: Linker<wasm::linker::StoreData>,

    /// Wasmtime store
    store: Store<wasm::linker::StoreData>,

    /// Module cache
    module_cache: Option<Arc<dyn ModuleCache>>,

    /// Host environment
    host_env: Option<Arc<Mutex<ConcreteHostEnvironment>>>,
}

impl Runtime {
    /// Create a new runtime with specified storage
    pub fn new(storage: Arc<dyn RuntimeStorage>) -> Self {
        let engine = Engine::default();
        let linker = Linker::new(&engine);
        let store = Store::new(&engine, wasm::linker::StoreData::new());
        let module_cache = None;
        let host_env = None;
        Self {
            vm: CoVm::default(),
            storage,
            context: RuntimeContext::new(),
            engine,
            linker,
            store,
            module_cache,
            host_env,
        }
    }

    /// Create a new runtime with custom resource limits
    pub fn with_limits(storage: Arc<dyn RuntimeStorage>, limits: ResourceLimits) -> Self {
        let engine = Engine::new(&limits);
        let linker = Linker::new(&engine);
        let store = Store::new(&engine, wasm::linker::StoreData::new());
        let module_cache = None;
        let host_env = None;
        Self {
            vm: CoVm::new(limits),
            storage,
            context: RuntimeContext::new(),
            engine,
            linker,
            store,
            module_cache,
            host_env,
        }
    }
    
    /// Create a new runtime with specified context
    pub fn with_context(storage: Arc<dyn RuntimeStorage>, context: RuntimeContext) -> Self {
        let engine = Engine::default();
        let linker = Linker::new(&engine);
        let store = Store::new(&engine, wasm::linker::StoreData::new());
        let module_cache = None;
        let host_env = None;
        Self {
            vm: CoVm::default(),
            storage,
            context,
            engine,
            linker,
            store,
            module_cache,
            host_env,
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
        let vm_context = VmContext {
            executor_did: self.context.executor_id.clone().unwrap_or_else(|| "did:icn:system".to_string()),
            scope: Some(format!("proposal/{}", proposal_id)),
            epoch: None,
            code_cid: Some(proposal.wasm_cid.clone()),
            resource_limits: None,
            coop_id: None,
            community_id: None,
        };

        // Execute the WASM module in governance context
        let result = self.governance_execute_wasm(&wasm_bytes, vm_context)
            .map_err(|e| RuntimeError::ExecutionError(format!("Failed to execute WASM module: {}", e)))?;

        // Create the execution receipt
        let receipt = ExecutionReceipt {
            proposal_id: proposal_id.to_string(),
            wasm_cid: proposal.wasm_cid.clone(),
            ccl_cid: proposal.ccl_cid.clone(),
            metrics: result.metrics,
            anchored_cids: result.anchored_cids,
            resource_usage: result.resource_usage,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            dag_epoch: None,
            receipt_cid: None,
            federation_signature: None,
        };

        // Store the execution receipt
        let receipt_cid = self.storage.store_receipt(&receipt).await?;

        // Update the proposal state
        proposal.state = ProposalState::Executed;
        self.storage.update_proposal(&proposal).await?;

        // Return the receipt with updated CID
        let mut final_receipt = receipt;
        final_receipt.receipt_cid = Some(receipt_cid);
        Ok(final_receipt)
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

    /// Executes the loaded WASM module.
    pub fn execute_wasm(
        &self,
        wasm_bytes: &[u8],
        _function_name: Option<String>, // Parameter will be ignored, we always call _start
        _args: Vec<Val>, // Parameter will be ignored, _start takes no args
    ) -> Result<Option<Vec<Val>>, RuntimeError> {
        
        // let mut store = Store::new(&self.engine, StoreData::new());
        // If self.store is a Mutex<Store<StoreData>>, we need to lock it.
        // Or, if Store/StoreData should be created per execution:
        let mut store_data = wasm::linker::StoreData::new();
        if let Some(host_env_guard) = &self.host_env {
            // We need to be careful about cloning vs. sharing the host_env. 
            // If host_env contains Arc<Mutex<...>>, cloning the ConcreteHostEnvironment is okay.
            // For this execution, we take a snapshot or clone.
            store_data.set_host(host_env_guard.lock().unwrap().clone()); 
        } else {
            // Fallback or error if host_env is required but not set
            // This depends on whether a Runtime can be meaningfully used without a host_env
            // For now, let's assume a default/minimal host_env could be constructed or it's an error.
            // Or, the `new` constructor should ensure host_env is always Some via new_with_host_env.
            // Simplification: Assume host_env is always set by new_with_host_env for execute_wasm to be called meaningfully.
            return Err(RuntimeError::HostEnvironmentNotSet);
        }
        let mut store = Store::new(&self.engine, store_data);

        let module = self.load_module(wasm_bytes, &mut store)?;

        // Instantiate the module with the linker
        let instance = self.linker.instantiate(&mut store, &module)
            .map_err(|e| RuntimeError::Instantiation(e.to_string()))?;

        // Attempt to get the exported function "_start" with signature () -> i32
        match instance.get_typed_func::<(), i32, _>(&mut store, "_start") {
            Ok(start_func) => {
                // Call the function.
                match start_func.call(&mut store, ()) {
                    Ok(result_i32) => {
                        // Wrap the i32 result in the expected Option<Vec<Val>> format
                        Ok(Some(vec![Val::I32(result_i32)]))
                    }
                    Err(e) => {
                        // If the WASM function traps (e.g. explicit trap, division by zero)
                        Err(RuntimeError::Execution(e.to_string()))
                    }
                }
            }
            Err(e) => {
                // If "_start" function is not found or has a mismatched signature
                // For now, we try a fallback to the provided function_name if any, for compatibility.
                // However, the primary path is _start.
                // The prompt implies we should *only* try _start for the standardized flow.
                // So, if _start is not found, it's an error for this standardized path.
                eprintln!("Failed to get typed func '_start': {}. Consider previous execution method if this module doesn't use _start.", e);
                Err(RuntimeError::FunctionNotFound("_start".to_string()))
            }
        }
    }

    /// Helper to load (or get from cache) and compile module
    fn load_module(&self, wasm_bytes: &[u8], store: &mut Store) -> Result<Module> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| RuntimeError::LoadError(format!("Failed to compile WASM: {}", e)))?;
        Ok(module)
    }

    /// Execute a WASM binary with the given context in governance mode
    /// This allows token minting and other privileged operations
    pub fn governance_execute_wasm(&self, wasm_bytes: &[u8], context: VmContext) -> Result<ExecutionResult> {
        // Convert the VM context to a host context
        let host_context = self.vm_context_to_host_context(context.clone());

        // Create a wasmtime store and register the economics host functions
        let mut linker = wasmtime::Linker::new(self.vm.engine());
        let mut store = wasmtime::Store::new(self.vm.engine(), wasm::linker::StoreData::new());
        
        // Set up the host environment in the store data with governance context
        let host_env = ConcreteHostEnvironment::new_governance(
            Arc::new(self.context.clone()),
            context.executor_did.parse().unwrap_or_else(|_| Did::from_str("did:icn:invalid").unwrap())
        );
        store.data_mut().set_host(host_env);
        
        // Register the economic host functions
        wasm::linker::register_host_functions(&mut linker)?;
        
        // Execute the WASM module
        let module = Module::new(self.vm.engine(), wasm_bytes)
            .map_err(|e| RuntimeError::ExecutionError(format!("Failed to compile WASM: {}", e)))?;
        
        // Set initial fuel based on limits
        let initial_fuel = if let Some(limits) = &context.resource_limits {
            limits.max_fuel
        } else {
            10_000_000 // Default reasonable limit
        };
        store.set_fuel(initial_fuel)?;
        
        // Instantiate the module with the linker
        let instance = linker.instantiate(&mut store, &module)
            .map_err(|e| RuntimeError::ExecutionError(format!("Failed to instantiate WASM: {}", e)))?;
            
        // Call the entrypoint function
        let entrypoint = instance.get_typed_func::<(), ()>(&mut store, "_start")
            .map_err(|e| RuntimeError::ExecutionError(format!("Failed to find entrypoint: {}", e)))?;
        
        let execution_result = entrypoint.call(&mut store, ())
            .map_err(|e| RuntimeError::ExecutionError(format!("WASM execution failed: {}", e)))?;
            
        // Get consumed fuel
        let fuel_consumed = initial_fuel - store.get_fuel().unwrap_or(initial_fuel);
        
        let result = ExecutionResult {
            metrics: CoreVmExecutionMetrics {
                fuel_used: fuel_consumed,
                ..Default::default()
            },
            anchored_cids: vec![],
            resource_usage: vec![],
            logs: vec![],
        };
        
        Ok(result)
    }

    /// Issue an execution receipt after successful execution
    pub fn issue_receipt(
        &self,
        wasm_cid: &str,
        ccl_cid: &str,
        result: &ExecutionResult,
        context: &VmContext,
    ) -> Result<RuntimeExecutionReceipt> {
        // Convert ExecutionMetrics to VC ExecutionMetrics
        let vc_metrics = RuntimeExecutionMetrics {
            fuel_used: result.metrics.fuel_used,
            host_calls: result.metrics.host_calls,
            io_bytes: result.metrics.io_bytes,
        };

        let receipt_id = Uuid::new_v4().to_string();

        let receipt = RuntimeExecutionReceipt {
            id: receipt_id,
            issuer: context.executor_did.clone(),
            proposal_id: context.code_cid.clone().unwrap_or_default(),
            wasm_cid: wasm_cid.to_string(),
            ccl_cid: ccl_cid.to_string(),
            metrics: vc_metrics,
            anchored_cids: result.anchored_cids.clone(),
            resource_usage: result.resource_usage.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| RuntimeError::ReceiptError(e.to_string()))?
                .as_secs(),
            dag_epoch: context.epoch.as_ref().and_then(|s| s.parse().ok()),
            receipt_cid: None,
            signature: None,
        };

        Ok(receipt)
    }

    /// Anchor a receipt to the DAG and return the CID
    pub async fn anchor_receipt(&self, receipt: &RuntimeExecutionReceipt) -> Result<String> {
        let receipt_json = serde_json::to_string(receipt)
            .map_err(|e| RuntimeError::ReceiptError(e.to_string()))?;

        // Store the receipt
        let receipt_cid = self.storage.anchor_to_dag(&receipt_json).await?;

        Ok(receipt_cid)
    }

    /// Helper function to convert VmContext (icn-runtime specific) to HostContext (icn-core-vm specific)
    fn vm_context_to_host_context(&self, vm_context: VmContext) -> HostContext {
        // Create a HostContext with default values
        let mut host_context = HostContext::default();
        
        // Convert string coop_id and community_id to proper types if present
        let coop_id = vm_context.coop_id.map(|id| icn_types::org::CooperativeId::new(id));
        let community_id = vm_context.community_id.map(|id| icn_types::org::CommunityId::new(id));
        
        // Set organization IDs if present
        if coop_id.is_some() || community_id.is_some() {
            host_context = host_context.with_organization(coop_id, community_id);
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
    use icn_economics::{Economics, ResourceAuthorizationPolicy, ResourceType};
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
            coop_id: None,
            community_id: None,
        };

        // Execute the WASM module
        let result = runtime.execute_wasm(&wasm_bytes, None, vec![])?;

        // Verify that execution succeeded and metrics were collected
        assert!(result.is_some(), "Expected some execution result");

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
    
    #[tokio::test]
    async fn test_resource_economics() -> Result<()> {
        // Create a simple WAT (WebAssembly Text) module that calls the resource functions
        let wat = r#"
        (module
          (import "icn_host" "host_check_resource_authorization" (func $check_auth (param i32 i64) (result i32)))
          (import "icn_host" "host_record_resource_usage" (func $record_usage (param i32 i64) (result i32)))
          (func $start
            ;; Try to authorize CPU usage (resource type 0)
            (call $check_auth
              (i32.const 0)  ;; ResourceType::Cpu = 0
              (i64.const 100)) ;; Amount = 100
            drop
            
            ;; Record CPU usage
            (call $record_usage
              (i32.const 0)  ;; ResourceType::Cpu = 0
              (i64.const 50)) ;; Amount = 50
            drop
            
            ;; Try to authorize Token usage (resource type 2)
            (call $check_auth
              (i32.const 2)  ;; ResourceType::Token = 2
              (i64.const 10)) ;; Amount = 10
            drop
            
            ;; Record Token usage
            (call $record_usage
              (i32.const 2)  ;; ResourceType::Token = 2
              (i64.const 10)) ;; Amount = 10
            drop
          )
          (export "_start" (func $start))
        )
        "#;

        // Create a parser
        let engine = wasmtime::Engine::default();
        let module = Module::new(&engine, wat)?;
        
        // Create a policy that allows up to 1000 units of each resource type
        let policy = ResourceAuthorizationPolicy {
            max_cpu: 1000,
            max_memory: 1000,
            token_allowance: 1000,
        };
        let economics = Arc::new(Economics::new(policy));
        
        // Create a runtime with the economics engine
        let storage = Arc::new(MockStorage::new());
        let context = RuntimeContext::builder()
            .with_economics(economics.clone())
            .build();
        let runtime = Runtime::with_context(storage, context);
        
        // Create a VM context with a test DID
        let test_did = "did:icn:test-user";
        let vm_context = VmContext {
            executor_did: test_did.to_string(),
            scope: None,
            epoch: None,
            code_cid: None,
            resource_limits: None,
            coop_id: None,
            community_id: None,
        };
        
        // Execute the WASM module
        let result = runtime.execute_wasm(&module.serialize()?, None, vec![])?;
        
        // Verify resource usage was recorded
        let resource_ledger = runtime.context().resource_ledger.clone();
        
        // Check that CPU usage was recorded for the correct DID
        let cpu_usage = economics.get_usage(
            &Did::from_str(test_did).unwrap(),
            None,
            None,
            ResourceType::Cpu,
            &resource_ledger
        ).await;
        assert_eq!(cpu_usage, 50, "Expected 50 units of CPU resource usage");
        
        // Check that Token usage was recorded for the correct DID
        let token_usage = economics.get_usage(
            &Did::from_str(test_did).unwrap(),
            None,
            None,
            ResourceType::Token,
            &resource_ledger
        ).await;
        assert_eq!(token_usage, 10, "Expected 10 units of Token resource usage");
        
        // Create a second context with a different DID and cooperative/community
        let test_did2 = "did:icn:another-user";
        let vm_context2 = VmContext {
            executor_did: test_did2.to_string(),
            scope: None,
            epoch: None,
            code_cid: None,
            resource_limits: None,
            coop_id: Some("coop-123".to_string()),
            community_id: Some("community-456".to_string()),
        };
        
        // Execute the WASM module again with the second DID and organization context
        let _ = runtime.execute_wasm(&module.serialize()?, None, vec![])?;
        
        // Verify that each DID has its own separate resource tracking
        let cpu_usage1 = economics.get_usage(
            &Did::from_str(test_did).unwrap(),
            None,
            None,
            ResourceType::Cpu,
            &resource_ledger
        ).await;
        
        let coop_id = icn_types::org::CooperativeId::new("coop-123");
        let community_id = icn_types::org::CommunityId::new("community-456");
        
        let cpu_usage2 = economics.get_usage(
            &Did::from_str(test_did2).unwrap(),
            Some(&coop_id),
            Some(&community_id),
            ResourceType::Cpu,
            &resource_ledger
        ).await;
        
        assert_eq!(cpu_usage1, 50, "First user's CPU usage should still be 50");
        assert_eq!(cpu_usage2, 50, "Second user's CPU usage should be 50");
        
        // Get total CPU usage across all DIDs
        let total_cpu = economics.get_total_usage(ResourceType::Cpu, &resource_ledger).await;
        assert_eq!(total_cpu, 100, "Total CPU usage should be 100 (50 + 50)");
        
        // Get cooperative-specific usage
        let coop_cpu = economics.get_cooperative_usage(
            &coop_id,
            ResourceType::Cpu,
            &resource_ledger
        ).await;
        assert_eq!(coop_cpu, 50, "Cooperative CPU usage should be 50");
        
        // Get community-specific usage
        let community_cpu = economics.get_community_usage(
            &community_id,
            ResourceType::Cpu,
            &resource_ledger
        ).await;
        assert_eq!(community_cpu, 50, "Community CPU usage should be 50");
        
        Ok(())
    }
}

/// Executes a MeshJob within the ICN runtime (currently stubbed).
///
/// This function simulates fetching and executing a WASM binary based on the
/// provided MeshJob, measures fake resource usage, and constructs a signed
/// ExecutionReceipt.
pub async fn execute_mesh_job(
    mesh_job: MeshJob,
    local_keypair: &IcnKeyPair, // For signing the receipt
    runtime_context: Option<Arc<RuntimeContext>>, // Placeholder for actual runtime context
) -> Result<ExecutionReceipt, anyhow::Error> {
    tracing::info!(
        "[RuntimeExecute] Attempting to execute job_id: {}, wasm_cid: {}",
        mesh_job.job_id,
        mesh_job.params.wasm_cid
    );

    // 1. Placeholder for WASM Fetch
    tracing::info!(
        "[RuntimeExecute] STUB: Fetching WASM binary for CID: {}",
        mesh_job.params.wasm_cid
    );
    // In a real scenario, this would involve IPFS/network calls or local cache access.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await; // Simulate fetch delay

    // 2. Placeholder for RuntimeContext usage
    if let Some(ctx) = &runtime_context {
        tracing::info!("[RuntimeExecute] STUB: RuntimeContext provided (id: {:?}). Actual use TBD.", ctx.id());
        // Actual runtime might use this for host functions, environment setup, etc.
    } else {
        tracing::info!("[RuntimeExecute] STUB: No RuntimeContext provided. Using dummy/default behavior.");
    }

    // 3. Placeholder for WASM Execution
    tracing::info!("[RuntimeExecute] STUB: Simulating WASM execution for job_id: {}...", mesh_job.job_id);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await; // Simulate execution delay
    tracing::info!("[RuntimeExecute] STUB: WASM execution simulation finished for job_id: {}.", mesh_job.job_id);

    // 4. Fake Resource Usage
    let mut resource_usage_actual = HashMap::new();
    resource_usage_actual.insert(ResourceType::Cpu, 100u64); // e.g., 100 millicores or abstract units
    resource_usage_actual.insert(ResourceType::Memory, 64u64); // e.g., 64 MiB
    resource_usage_actual.insert(ResourceType::NetworkOutbound, 1024u64); // e.g., 1KB
    tracing::info!("[RuntimeExecute] STUB: Generated fake resource usage: {:?}", resource_usage_actual);

    // 5. Construct ExecutionReceipt
    let execution_start_time_unix = Utc::now().timestamp() - 1; // 1 second ago
    let execution_end_time_dt = Utc::now();
    let execution_end_time_unix = execution_end_time_dt.timestamp();

    // Create a dummy CID string - ensure it's a valid format if Cid::try_from is used later
    let dummy_cid_str = "bafybeigdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

    let mut receipt = ExecutionReceipt {
        job_id: mesh_job.job_id.clone(), // Assuming job_id in MeshJob is the correct IcnJobId (String)
        executor: local_keypair.did.clone(),
        status: StandardJobStatus::CompletedSuccess, // Assuming successful execution for the stub
        result_data_cid: Some(dummy_cid_str.to_string()), // Placeholder
        logs_cid: Some(dummy_cid_str.to_string()),       // Placeholder
        resource_usage: resource_usage_actual,
        execution_start_time: execution_start_time_unix as u64,
        execution_end_time: execution_end_time_unix as u64,
        execution_end_time_dt, // Store the DateTime<Utc> as well
        signature: Vec::new(), // Will be filled by sign_receipt_in_place
        coop_id: mesh_job.originator_org_scope.as_ref().and_then(|s| s.coop_id.clone()),
        community_id: mesh_job.originator_org_scope.as_ref().and_then(|s| s.community_id.clone()),
    };
    tracing::info!("[RuntimeExecute] Constructed initial (unsigned) ExecutionReceipt for job_id: {}.", mesh_job.job_id);

    // 6. Sign the Receipt
    if let Err(e) = sign_receipt_in_place(&mut receipt, local_keypair) {
        tracing::error!("[RuntimeExecute] Failed to sign ExecutionReceipt for job_id: {}: {:?}", mesh_job.job_id, e);
        return Err(anyhow!("Failed to sign execution receipt: {}", e));
    }
    tracing::info!("[RuntimeExecute] Successfully signed ExecutionReceipt for job_id: {}.", mesh_job.job_id);

    Ok(receipt)
}
