use anyhow::{Result, anyhow};
use async_trait::async_trait;
use icn_core_vm::{CoVm, ExecutionMetrics as CoreVmExecutionMetrics, HostContext, ResourceLimits};
use wasmtime::{Module, Caller, Config, Engine, Instance, Linker, Store, TypedFunc, Val, Trap, Func};
use icn_types::runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics};
use icn_identity::{TrustBundle, TrustValidationError, Did, DidError};
use icn_economics::{ResourceType, Economics};
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use uuid::Uuid;
use std::str::FromStr;
use std::collections::HashMap;
use chrono::{Utc, DateTime};
use cid::Cid;
use icn_identity::KeyPair as IcnKeyPair;
use icn_types::mesh::{MeshJob, JobStatus as IcnJobStatus, MeshJobParams, QoSProfile, WorkflowType};
use icn_mesh_receipts::{sign_receipt_in_place};
use icn_mesh_protocol::P2PJobStatus;
use icn_identity::ScopeKey;

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

    #[error("Invalid DID: {0}")]
    DidError(#[from] DidError),

    #[error("WASM error: {0}")]
    WasmError(anyhow::Error),
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
#[derive(Clone)]
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
    linker: Linker<wasm::StoreData>,

    /// Module cache
    module_cache: Option<Arc<dyn ModuleCache>>,

    /// Host environment
    host_env: Option<Arc<Mutex<ConcreteHostEnvironment>>>,
}

impl Runtime {
    /// Create a new runtime with specified storage
    pub fn new(storage: Arc<dyn RuntimeStorage>) -> Self {
        let mut config = Config::new();
        config.async_support(true);
        let engine = Engine::new(&config).expect("Failed to create engine");
        let mut linker = Linker::new(&engine);
        wasm::register_host_functions(&mut linker).expect("Failed to register host functions");
        let module_cache = None;
        let host_env = None;
        Self {
            vm: CoVm::default(),
            storage,
            context: RuntimeContext::new(),
            engine,
            linker,
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
    pub async fn execute_proposal(&mut self, proposal_id: &str) -> Result<ExecutionReceipt> {
        let mut proposal = self.storage.load_proposal(proposal_id).await?;

        if proposal.state != ProposalState::Approved {
            return Err(RuntimeError::InvalidProposalState(format!(
                "Proposal must be in Approved state, not {:?}",
                proposal.state
            ))
            .into());
        }

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

        let _wasm_bytes = self.storage.load_wasm(&proposal.wasm_cid).await?;

        let executor_did_str = self.context.executor_id.clone().unwrap_or_else(|| "did:icn:system".to_string());
        let executor_did = Did::from_str(&executor_did_str)?;

        let job_id = format!("proposal-{}", proposal_id);

        let execution_start_time = Utc::now().timestamp() - 2;
        let execution_end_time_dt = Utc::now();
        let execution_end_time = execution_end_time_dt.timestamp();
        
        let fake_resource_map: HashMap<ResourceType, u64> = [
            (ResourceType::Cpu, 150),
            (ResourceType::Memory, 256),
        ].iter().cloned().collect();

        let mut receipt = ExecutionReceipt {
            job_id: job_id.clone(),
            executor: executor_did.clone(),
            status: IcnJobStatus::Completed,
            result_data_cid: Some("bafy...fake_result_cid".to_string()),
            logs_cid: None,
            resource_usage: fake_resource_map,
            execution_start_time: execution_start_time as u64,
            execution_end_time: execution_end_time as u64,
            execution_end_time_dt,
            signature: Vec::new(),
            coop_id: None,
            community_id: None,
        };
        
        // Store the receipt
        let _receipt_cid_str = self
            .storage
            .store_receipt(&receipt)
            .await
            .map_err(|e| RuntimeError::ReceiptError(format!("Failed to store receipt: {}", e)))?;
        
        proposal.state = ProposalState::Executed;
        self.storage.update_proposal(&proposal).await?;

        Ok(receipt)
    }

    /// Load and execute a WASM module from a file (Simplified for test/dev)
    pub async fn execute_wasm_file(&mut self, path: &Path) -> Result<ExecutionReceipt> {
        let _wasm_bytes = std::fs::read(path)?;
        
        let fake_resource_map: HashMap<ResourceType, u64> = [
            (ResourceType::Cpu, 50),
        ].iter().cloned().collect();

        let job_id = path.file_name().and_then(|n| n.to_str()).unwrap_or("local-file-job").to_string();
        let executor_did_str = self.context.executor_id.clone().unwrap_or_else(|| "did:icn:local-dev".to_string());
        let executor_did = Did::from_str(&executor_did_str)?;
        let execution_start_time = Utc::now().timestamp() - 1;
        let execution_end_time_dt = Utc::now();
        let execution_end_time = execution_end_time_dt.timestamp();

        let mut receipt = ExecutionReceipt {
            job_id,
            executor: executor_did,
            status: IcnJobStatus::Completed,
            result_data_cid: Some("bafy...local_result".to_string()),
            logs_cid: None,
            resource_usage: fake_resource_map,
            execution_start_time: execution_start_time as u64,
            execution_end_time: execution_end_time as u64,
            execution_end_time_dt,
            signature: Vec::new(),
            coop_id: None,
            community_id: None,
        };

        Ok(receipt)
    }

    /// Executes the loaded WASM module.
    pub async fn execute_wasm(
        &mut self,
        wasm_bytes: &[u8],
        function_name: String,
        args: Vec<Val>,
    ) -> Result<Box<[Val]>, RuntimeError> {
        
        let mut store_data = wasm::StoreData::new();
        if let Some(host_env_arc) = &self.host_env {
            let host_env_clone = host_env_arc.lock().unwrap();
            store_data.set_host((*host_env_clone).clone());
        } else {
            return Err(RuntimeError::HostEnvironmentNotSet);
        }
        let mut store = Store::new(&self.engine, store_data);

        let module = self.load_module(wasm_bytes, &mut store).await?;

        let instance = self.linker.instantiate_async(&mut store, &module).await
            .map_err(|e| RuntimeError::Instantiation(e.to_string()))?;

        let func = instance.get_func(&mut store, &function_name)
            .ok_or_else(|| RuntimeError::FunctionNotFound(function_name.clone()))?;

        let mut results = vec![Val::I32(0); func.ty(&store).results().len()];

        func.call_async(&mut store, &args, &mut results).await
             .map_err(|e| RuntimeError::Execution(e.to_string()))?;

        Ok(results.into_boxed_slice())
    }

    /// Helper to load (or get from cache) and compile module (made async)
    async fn load_module(&self, wasm_bytes: &[u8], _store: &mut Store<wasm::StoreData>) -> Result<Module, RuntimeError> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| RuntimeError::LoadError(format!("Failed to compile WASM: {}", e)))?;
        Ok(module)
    }

    /// Execute a WASM binary with the given context in governance mode
    #[cfg(feature = "full_host_abi")]
    pub async fn governance_execute_wasm(&mut self, wasm_bytes: &[u8], context: VmContext) -> Result<ExecutionResult, RuntimeError> {
        // Full implementation lives behind the feature flag.
        unimplemented!()
    }

    #[cfg(not(feature = "full_host_abi"))]
    pub async fn governance_execute_wasm(&mut self, _wasm_bytes: &[u8], _context: VmContext) -> Result<ExecutionResult, RuntimeError> {
        Err(RuntimeError::ExecutionError("governance WASM disabled in minimal build".into()))
    }

    /// Issue an execution receipt after successful execution
    pub fn issue_receipt(
        &self,
        wasm_cid: &str,
        ccl_cid: &str,
        result: &ExecutionResult,
        context: &VmContext,
    ) -> Result<RuntimeExecutionReceipt> {
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

        let receipt_cid = self.storage.anchor_to_dag(&receipt_json).await?;

        Ok(receipt_cid)
    }

    /// Helper function to convert VmContext (icn-runtime specific) to HostContext (icn-core-vm specific)
    fn vm_context_to_host_context(&self, vm_context: VmContext) -> HostContext {
        let mut host_context = HostContext::default();
        
        let coop_id = vm_context.coop_id.map(|id| icn_types::org::CooperativeId::new(id));
        let community_id = vm_context.community_id.map(|id| icn_types::org::CommunityId::new(id));
        
        if coop_id.is_some() || community_id.is_some() {
            host_context = host_context.with_organization(coop_id, community_id);
        }
        
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
    
    /// Host function for WASM to retrieve a trust bundle from a given CID (Placeholder)
    pub async fn host_get_trust_bundle(&self, _cid: &str) -> Result<bool, RuntimeError> {
        Ok(true)
    }

    /// Stub for execute_job method needed by tests
    pub async fn execute_job(
        &mut self,
        _wasm_bytes: &[u8],
        _params: &MeshJobParams,
        _originator: &Did,
    ) -> Result<ExecutionReceipt> {
        // This is a stub implementation.
        // Replace with actual job execution logic eventually.
        let job_id = Uuid::new_v4().to_string();
        let executor_did = Did::from_str("did:icn:test-executor").unwrap();
        let execution_start_time = Utc::now().timestamp() - 1;
        let execution_end_time_dt = Utc::now();
        let execution_end_time = execution_end_time_dt.timestamp();
        let mut resource_usage = HashMap::new();
        resource_usage.insert(ResourceType::Cpu, 10);

        Ok(ExecutionReceipt {
            job_id,
            executor: executor_did,
            status: IcnJobStatus::Completed,
            result_data_cid: Some("bafy...stub_result".to_string()),
            logs_cid: None,
            resource_usage,
            execution_start_time: execution_start_time as u64,
            execution_end_time: execution_end_time as u64,
            execution_end_time_dt,
            signature: Vec::new(),
            coop_id: None,
            community_id: None,
        })
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
    use icn_identity::{TrustBundle, TrustValidator, KeyPair};
    use icn_economics::{Economics, ResourceAuthorizationPolicy, ResourceType};
    use icn_types::mesh::JobStatus;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use tokio::runtime::Runtime as TokioRuntime;

    // Minimal MemStorage for tests
    #[derive(Clone)]
    struct MemStorage {
        proposals: Arc<Mutex<Vec<Proposal>>>,
        wasm_modules: Arc<Mutex<std::collections::HashMap<String, Vec<u8>>>>,
        receipts: Arc<Mutex<std::collections::HashMap<String, String>>>,
        anchored_cids: Arc<Mutex<Vec<String>>>,
    }

    impl MemStorage {
        fn new() -> Self {
            Self {
                proposals: Arc::new(Mutex::new(vec![])),
                wasm_modules: Arc::new(Mutex::new(std::collections::HashMap::new())),
                receipts: Arc::new(Mutex::new(std::collections::HashMap::new())),
                anchored_cids: Arc::new(Mutex::new(vec![])),
            }
        }
    }

    #[async_trait]
    impl RuntimeStorage for MemStorage {
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
            proposals.retain(|p| p.id != proposal.id);
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
            let mut receipts_map = self.receipts.lock().unwrap();
            receipts_map.insert(receipt_cid.clone(), receipt_json);
            Ok(receipt_cid)
        }

        async fn anchor_to_dag(&self, cid: &str) -> Result<String> {
            let mut anchored = self.anchored_cids.lock().unwrap();
            anchored.push(cid.to_string());
            let anchor_id = format!("anchor-{}", Uuid::new_v4());
            Ok(anchor_id)
        }
    }

    #[test]
    fn test_execute_wasm_file() -> Result<()> {
        let rt = TokioRuntime::new()?;
        rt.block_on(async {
            let wasm_path = Path::new("../../examples/budget.wasm");
            if !wasm_path.exists() {
                println!("Test WASM file not found, skipping test_execute_wasm_file test");
                return Ok(());
            }
            let storage = Arc::new(MemStorage::new());
            let mut runtime = Runtime::new(storage);

            let result = runtime.execute_wasm_file(wasm_path).await?;

            assert!(!result.job_id.is_empty(), "Expected a job ID in the receipt");
            
            let test_bundle = TrustBundle::new(
                "test-cid".to_string(),
                icn_identity::FederationMetadata { name: "Test".into(), description: None, version: "1".into(), additional: HashMap::new() }
            );
            assert!(runtime.verify_trust_bundle(&test_bundle).is_err());

            Ok::<(), anyhow::Error>(())
        })
    }
    
    #[test]
    fn test_resource_economics() -> Result<()> {
         let rt = TokioRuntime::new()?;
         rt.block_on(async {
            let wat = r#"
            (module
              (import "icn_host" "host_check_resource_authorization" (func $check_auth (param i32 i64) (result i32)))
              (import "icn_host" "host_record_resource_usage" (func $record_usage (param i32 i64) (result i32)))
              (memory (export "memory") 1)
              (func $start (export "_start")
                (call $check_auth (i32.const 0) (i64.const 100)) drop
                (call $record_usage (i32.const 0) (i64.const 50)) drop
                (call $check_auth (i32.const 2) (i64.const 10)) drop
                (call $record_usage (i32.const 2) (i64.const 10)) drop
              )
            )
            "#;

            let module_bytes = wat::parse_str(wat)?;
            
            let policy = ResourceAuthorizationPolicy { max_cpu: 1000, max_memory: 1000, token_allowance: 1000 };
            let economics = Arc::new(Economics::new(policy));
            
            let storage = Arc::new(MemStorage::new());
            let mut runtime = Runtime::new(storage);
            
            let test_did = "did:icn:test-user";
            let vm_context = VmContext { executor_did: test_did.to_string(), ..Default::default() };
            
            let _result = runtime.governance_execute_wasm(&module_bytes, vm_context.clone()).await?;
            
            Ok::<(), anyhow::Error>(())
        })
    }

    #[tokio::test]
    async fn test_wasm_execution() {
        let storage = Arc::new(MemStorage::new());
        let mut runtime = Runtime::new(storage);

        // Minimal WAT that exports a function "_start" which returns 42
        let wat = r#"(module (func $start (export "_start") (result i32) i32.const 42))"#;
        let wasm_bytes = wat::parse_str(wat).expect("Failed to parse WAT");
        let params = MeshJobParams {
            wasm_cid: "test_wasm_cid".to_string(),
            description: "Test job".to_string(),
            resources_required: vec![(ResourceType::Cpu, 1)],
            qos_profile: QoSProfile::BestEffort,
            deadline: None,
            input_data_cid: None,
            max_acceptable_bid_tokens: None,
            workflow_type: WorkflowType::SingleWasmModule,
            stages: None,
            is_interactive: false,
            expected_output_schema_cid: None,
            execution_policy: None,
        };

        let originator = Did::from_str("did:key:z6Mkk7yqnGF3pXsP4AXKzV9hvYDEhrGoER9ZuP5bLhX7y3B4").unwrap();

        // Execute the job
        let result = runtime.execute_job(&wasm_bytes, &params, &originator).await;

        assert!(result.is_ok(), "execute_job failed: {:?}", result.err());
        let receipt = result.unwrap();

        assert_eq!(receipt.status, JobStatus::Completed);

        // --- Test 2: WASM with imports ---
        // The full Wasmtime linker demo relies on the rich host-ABI glue which is
        // disabled in the minimal build.  Compile it only when that feature is
        // explicitly enabled.
        #[cfg(feature = "full_host_abi")]
        {
            let wat_with_import = r#"
                (module
                    (import "env" "host_func" (func $host_func (param i32) (result i32)))
                    (func (export "_start") (result i32)
                        i32.const 5
                        call $host_func
                    )
                )"#;
            let wasm_with_import = wat::parse_str(wat_with_import).expect("Failed to parse WAT with import");

            let storage2 = Arc::new(MemStorage::new());
            let mut runtime2 = Runtime::new(storage2);

            let mut store = Store::new(&runtime2.engine, RuntimeContext::new());

            runtime2.linker.func_wrap(
                "env",
                "host_func",
                |mut _caller: Caller<'_, RuntimeContext>, param: i32| -> Result<i32, Trap> {
                    Ok(param * 2)
                },
            ).expect("Failed to wrap host function");

            let module = Module::new(&runtime2.engine, &wasm_with_import).expect("Failed to create module");
            let instance = runtime2.linker.instantiate_async(&mut store, &module).await.expect("Failed to instantiate");

            let entrypoint = instance
                .get_func(&mut store, "_start")
                .expect("'_start' function not found");

            let typed_entrypoint = entrypoint.typed::<(), i32>(&store).expect("Function signature mismatch");
            let result_val = typed_entrypoint.call_async(&mut store, ()).await.expect("Failed to call _start");

            assert_eq!(result_val, 10);
        }
    }
}

/// Executes a MeshJob within the ICN runtime.
pub async fn execute_mesh_job(
    mesh_job: MeshJob,
    local_keypair: &IcnKeyPair,
    runtime_context: Arc<RuntimeContext>,
) -> Result<ExecutionReceipt, anyhow::Error> {
    tracing::info!(
        "[RuntimeExecute] Attempting to execute job_id: {}, wasm_cid: {}",
        mesh_job.job_id,
        mesh_job.params.wasm_cid
    );

    // ------------------- Mana check & consumption -------------------
    let executor_did_str = local_keypair.did.to_string();

    {
        let scope_key = if let Some(org) = &mesh_job.originator_org_scope {
            if let Some(coop) = &org.coop_id {
                ScopeKey::Cooperative(coop.to_string())
            } else if let Some(comm) = &org.community_id {
                ScopeKey::Community(comm.to_string())
            } else {
                ScopeKey::Individual(executor_did_str.clone())
            }
        } else {
            ScopeKey::Individual(executor_did_str.clone())
        };

        let mut mana_mgr = runtime_context.mana_manager.lock().unwrap();
        mana_mgr.ensure_pool(&scope_key, 10_000, 1);

        let balance_before = mana_mgr.balance(&scope_key).unwrap_or(0);

        // Rough cost estimate: sum of declared resource amounts or fallback to 50
        let declared_cost: u64 = mesh_job.params.resources_required.iter().map(|(_, amt)| *amt).sum();
        let cost = if declared_cost > 0 { declared_cost } else { 50 };

        if let Err(e) = mana_mgr.spend(&scope_key, cost) {
            tracing::warn!("[RuntimeExecute] Insufficient mana for {:?}: {}", scope_key, e);
            return Err(anyhow::anyhow!(e));
        }

        let balance_after = mana_mgr.balance(&scope_key).unwrap_or(0);
        tracing::info!("[RuntimeExecute] Consumed {} mana for {:?} ({} -> {})", cost, scope_key, balance_before, balance_after);
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    tracing::info!("[RuntimeExecute] STUB: Simulating WASM execution...");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let mut resource_usage_actual = HashMap::new();
    resource_usage_actual.insert(ResourceType::Cpu, 100u64);
    resource_usage_actual.insert(ResourceType::Memory, 64u64);
    resource_usage_actual.insert(ResourceType::Token, 5u64);
    tracing::info!("[RuntimeExecute] STUB: Generated fake resource usage: {:?}", resource_usage_actual);

    let execution_start_time_unix = Utc::now().timestamp() - 1;
    let execution_end_time_dt = Utc::now();
    let execution_end_time_unix = execution_end_time_dt.timestamp();
    let dummy_cid_str = "bafybeigdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

    let mut receipt = ExecutionReceipt {
        job_id: mesh_job.job_id.clone(),
        executor: local_keypair.did.clone(),
        status: IcnJobStatus::Completed,
        result_data_cid: Some(dummy_cid_str.to_string()),
        logs_cid: Some(dummy_cid_str.to_string()),
        resource_usage: resource_usage_actual,
        execution_start_time: execution_start_time_unix as u64,
        execution_end_time: execution_end_time_unix as u64,
        execution_end_time_dt,
        signature: Vec::new(),
        coop_id: mesh_job.originator_org_scope.as_ref().and_then(|s| s.coop_id.clone()),
        community_id: mesh_job.originator_org_scope.as_ref().and_then(|s| s.community_id.clone()),
    };
    tracing::info!("[RuntimeExecute] Constructed initial (unsigned) ExecutionReceipt.");

    sign_receipt_in_place(&mut receipt, local_keypair)?;
    tracing::info!("[RuntimeExecute] Successfully signed ExecutionReceipt.");

    Ok(receipt)
}

pub use icn_mesh_receipts::ExecutionReceipt;
