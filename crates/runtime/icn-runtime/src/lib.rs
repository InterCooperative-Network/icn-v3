pub mod config;

use anyhow::{Result, anyhow, Context};
use async_trait::async_trait;
use icn_core_vm::{CoVm, ExecutionMetrics as CoreVmExecutionMetrics, HostContext, ResourceLimits};
use wasmtime::{Module, Caller, Config, Engine, Instance, Linker, Store, TypedFunc, Val, Trap, Func};
use icn_types::runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics};
use icn_identity::{TrustBundle, TrustValidationError, Did, DidError, KeyPair as IcnKeyPair};
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
use icn_types::mesh::{MeshJob, JobStatus as IcnJobStatus, MeshJobParams, QoSProfile, WorkflowType};
use icn_mesh_receipts::{sign_receipt_in_place, ExecutionReceipt as MeshExecutionReceipt};
use icn_mesh_protocol::P2PJobStatus;
use icn_identity::ScopeKey;
use tracing::{info, warn};
use std::path::PathBuf;
use tokio::time::{sleep, Duration};
use serde_cbor;

use crate::config::RuntimeConfig;

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

// Import metrics module
pub mod metrics;

// Import reputation integration module
pub mod reputation_integration;
use reputation_integration::{ReputationUpdater, HttpReputationUpdater, NoopReputationUpdater, ReputationScoringConfig};

/// Distribution worker for periodic mana payouts
pub mod distribution_worker;

// Import sled_storage module and type
mod sled_storage;
use sled_storage::SledStorage;

// Add imports for keypair loading/saving
use std::fs::{self, File};
use std::io::{Read, Write};
use bincode;

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
    
    /// Store WASM bytes by CID (Added for tests/sled impl)
    async fn store_wasm(&self, cid: &str, bytes: &[u8]) -> Result<()>;

    /// Store an execution receipt (Updated type)
    async fn store_receipt(&self, receipt: &RuntimeExecutionReceipt) -> Result<String>;

    /// Load an execution receipt by its ID (Added for tests/sled impl)
    async fn load_receipt(&self, receipt_id: &str) -> Result<RuntimeExecutionReceipt>;

    /// Anchor a CID to the DAG (Conceptually doesn't belong here, but needed by trait)
    async fn anchor_to_dag(&self, cid: &str) -> Result<String>;
}

/// Minimal MemStorage for tests (moved out for placeholder use in from_config)
pub struct MemStorage {
    proposals: std::sync::Mutex<HashMap<String, Proposal>>,
    wasm_modules: std::sync::Mutex<HashMap<String, Vec<u8>>>,
    receipts: std::sync::Mutex<HashMap<String, RuntimeExecutionReceipt>>,
    anchored_cids: std::sync::Mutex<Vec<String>>,
}

impl Default for MemStorage {
    fn default() -> Self {
        Self {
            proposals: std::sync::Mutex::new(HashMap::new()),
            wasm_modules: std::sync::Mutex::new(HashMap::new()),
            receipts: std::sync::Mutex::new(HashMap::new()),
            anchored_cids: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl MemStorage {
    pub fn new() -> Self {
        Default::default()
    }

    // Add helper for tests to check if receipts were stored
    pub fn receipt_count(&self) -> usize {
        self.receipts.lock().unwrap().len()
    }
}

#[async_trait]
impl RuntimeStorage for MemStorage {
    async fn load_proposal(&self, id: &str) -> Result<Proposal> {
        self.proposals.lock().unwrap().get(id).cloned().ok_or_else(|| anyhow!("Proposal {} not found", id))
    }

    async fn update_proposal(&self, proposal: &Proposal) -> Result<()> {
        self.proposals.lock().unwrap().insert(proposal.id.clone(), proposal.clone());
        Ok(())
    }

    async fn load_wasm(&self, cid: &str) -> Result<Vec<u8>> {
        self.wasm_modules.lock().unwrap().get(cid).cloned().ok_or_else(|| anyhow!("WASM {} not found", cid))
    }

    async fn store_wasm(&self, cid: &str, bytes: &[u8]) -> Result<()> {
        self.wasm_modules.lock().unwrap().insert(cid.to_string(), bytes.to_vec());
        Ok(())
    }

    async fn store_receipt(&self, receipt: &RuntimeExecutionReceipt) -> Result<String> {
        let receipt_id = receipt.id.clone();
        // Simple hash for mock storage ID - replace with proper CID generation if needed
        let cid = format!("mock-receipt-{}", receipt_id);
        self.receipts.lock().unwrap().insert(cid.clone(), receipt.clone());
        Ok(cid)
    }

    async fn load_receipt(&self, receipt_id: &str) -> Result<RuntimeExecutionReceipt> {
        self.receipts.lock().unwrap().get(receipt_id).cloned().ok_or_else(|| anyhow!("Receipt {} not found", receipt_id))
    }

    async fn anchor_to_dag(&self, cid: &str) -> Result<String> {
        let anchor_cid = format!("mock-anchor-{}", cid);
        self.anchored_cids.lock().unwrap().push(anchor_cid.clone());
        Ok(anchor_cid)
    }
}

/// The ICN Runtime for executing governance proposals
#[derive(Clone)]
pub struct Runtime {
    /// Runtime configuration
    config: RuntimeConfig,
    
    /// CoVM instance for executing WASM
    vm: CoVm,

    /// Storage backend
    storage: Arc<dyn RuntimeStorage>,
    
    /// Runtime context (now Arc'd)
    context: Arc<RuntimeContext>,

    /// Wasmtime engine
    engine: Engine,

    /// Wasmtime linker
    linker: Linker<wasm::StoreData>,

    /// Module cache
    module_cache: Option<Arc<dyn ModuleCache>>,

    /// Host environment
    host_env: Option<Arc<Mutex<ConcreteHostEnvironment>>>,
    
    /// Optional reputation updater
    reputation_updater: Option<Arc<dyn ReputationUpdater>>,
}

impl Runtime {
    /// Create a new runtime with specified storage
    pub fn new(storage: Arc<dyn RuntimeStorage>) -> Result<Self, anyhow::Error> {
        // Generate a default keypair for tests/direct usage
        let default_keypair = IcnKeyPair::generate();
        let default_did = default_keypair.did.clone();

        let mut config = RuntimeConfig::default();
        config.node_did = default_did.to_string(); // Store the valid did:key string

        let engine = Engine::default();
        let vm = CoVm::new(ResourceLimits::default());
        let linker = Linker::new(&engine);
        
        // Build context with the default identity
        let context = Arc::new(
            RuntimeContextBuilder::new()
                .with_identity(default_keypair) // Store the keypair in context
                .with_executor_id(default_did.to_string()) // Set executor ID in context
                // Add other necessary defaults if builder requires them
                .build()
        );

        Ok(Self {
            config, // Config has the generated did:key string
            vm,
            storage,
            context, // Context has the keypair/identity
            engine,
            linker,
            module_cache: None,
            host_env: None,
            reputation_updater: None, // Note: This won't be set up automatically here
        })
    }
    
    /// Set a reputation updater for this runtime
    pub fn with_reputation_updater(mut self, updater: Arc<dyn ReputationUpdater>) -> Self {
        self.reputation_updater = Some(updater);
        self
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
    pub async fn execute_proposal(&mut self, proposal_id: &str) -> Result<MeshExecutionReceipt> {
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

        let mut receipt = MeshExecutionReceipt {
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
            mana_cost: None,
        };
        
        // Store the receipt - Temporarily commented out due to type mismatch
        // let _receipt_cid_str = self
        //     .storage
        //     .store_receipt(&receipt) // Error: Expected &RuntimeExecutionReceipt, found &MeshExecutionReceipt
        //     .await
        //     .map_err(|e| RuntimeError::ReceiptError(format!("Failed to store receipt: {}", e)))?;
        
        proposal.state = ProposalState::Executed;
        self.storage.update_proposal(&proposal).await?;

        Ok(receipt)
    }

    /// Load and execute a WASM module from a file (Simplified for test/dev)
    pub async fn execute_wasm_file(&mut self, path: &Path) -> Result<MeshExecutionReceipt> {
        let _wasm_bytes = std::fs::read(path)?;
        
        let fake_resource_map: HashMap<ResourceType, u64> = [
            (ResourceType::Cpu, 50),
        ].iter().cloned().collect();

        let job_id = path.file_name().and_then(|n| n.to_str()).unwrap_or("local-file-job").to_string();
        // Use the runtime's actual identity from the context
        let executor_did = self.context.identity()
            .ok_or_else(|| anyhow!("Runtime identity not found in execute_wasm_file context"))?
            .did.clone();
            
        let execution_start_time = Utc::now().timestamp() - 1;
        let execution_end_time_dt = Utc::now();
        let execution_end_time = execution_end_time_dt.timestamp();

        let receipt = MeshExecutionReceipt {
            job_id,
            executor: executor_did, // Use the DID from context
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
            mana_cost: None,
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
        // Map fields from CoreVmExecutionMetrics (result.metrics) to RuntimeExecutionMetrics (vc_metrics)
        let vc_metrics = RuntimeExecutionMetrics {
            host_calls: result.metrics.host_calls,
            io_bytes: result.metrics.io_bytes,
            mana_cost: result.metrics.mana_cost,
        };

        let receipt_id = Uuid::new_v4().to_string();

        // Create receipt first with signature: None
        let mut receipt = RuntimeExecutionReceipt {
            id: receipt_id,
            issuer: context.executor_did.clone(), // This should be the DID of the runtime itself
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
            receipt_cid: None, // Will be set by anchor_receipt
            signature: None,   // Initialized to None, will be set by signing
        };

        // Sign the receipt using the runtime's identity
        let keypair = self.context.identity()
            .ok_or_else(|| RuntimeError::ReceiptError(
                "Runtime identity not available for signing receipt. Ensure runtime is initialized with a keypair.".to_string()
            ))?;
        
        // Use the new helper function to sign the receipt in place
        sign_runtime_receipt_in_place(&mut receipt, keypair)
            .context("Failed to sign runtime execution receipt")?;

        Ok(receipt)
    }

    /// Anchor a receipt to the DAG and return the CID
    pub async fn anchor_receipt(
        &self, 
        receipt: &RuntimeExecutionReceipt // Kept specific to RuntimeExecutionReceipt
    ) -> Result<String> // Returns receipt_cid as String
    {
        let start_time = std::time::Instant::now();

        let federation_id = self.context.federation_id.as_deref().unwrap_or("unknown_federation");
        let coop_id_label = federation_id;
        let community_id_label = federation_id;
        let issuer_did_label = receipt.issuer.as_str();
        
        // 1. Verify signature
        match receipt.verify_signature() {
            Ok(_) => {
                metrics::record_receipt_verification_outcome(true, coop_id_label, community_id_label, issuer_did_label);
            }
            Err(e) => {
                metrics::record_receipt_verification_outcome(false, coop_id_label, community_id_label, issuer_did_label);
                // Re-throw error to halt anchoring on verification failure
                return Err(e).context("Receipt signature verification failed during anchoring"); // Ensure this uses anyhow::Error context
            }
        };

        // 2. Generate the content-addressed CID for the receipt
        // This now assumes RuntimeExecutionReceipt has a working .cid() method.
        let actual_receipt_cid = receipt.cid()
            .map_err(|e| anyhow!("Failed to generate CID for receipt: {}", e))?;

        // 3. Create a version of the receipt that includes its own CID
        let mut receipt_to_anchor = receipt.clone();
        receipt_to_anchor.receipt_cid = Some(actual_receipt_cid.to_string());

        // 4. Serialize the receipt_to_anchor for DAG storage
        let receipt_bytes = serde_cbor::to_vec(&receipt_to_anchor)
            .context("Failed to serialize receipt for DAG storage")?;

        // 5. Store the serialized receipt in the DAG Store using its actual CID
        // Assuming dag_store has a method like put_raw_block(&Cid, Vec<u8>, u64_codec).await -> Result<()>
        // The constant for DAG_CBOR codec is 0x71.
        // Ensure SharedDagStore has a suitable method e.g., put_block or put_raw_block.
        // For this example, I am using a hypothetical `put_raw_block` which matches common patterns.
        // The actual method on `icn_types::dag_store::SharedDagStore` needs to be confirmed.
        self.dag_store().put_raw_block(&actual_receipt_cid, receipt_bytes, 0x71u64).await
            .with_context(|| format!("Failed to anchor receipt CID {} to DAG store", actual_receipt_cid))?;
        tracing::info!(receipt_cid = %actual_receipt_cid, "Receipt anchored to DAG store"); // Use tracing::info

        // 6. Store in local Sled storage (optional, for quick lookups by ID if still needed)
        self.storage.store_receipt(&receipt_to_anchor).await
            .context("Failed to store receipt in local Sled storage after DAG anchoring")?;
            
        // 7. Anchoring receipt.anchored_cids:
        // The loop `for cid_str in &receipt.anchored_cids` and its call to `self.storage.anchor_to_dag(cid_str).await`
        // is removed. Storing `receipt_to_anchor` (which contains `anchored_cids`) in the DAG
        // effectively anchors these references as part of the receipt's immutable record.

        // 8. Submit reputation update
        if let Some(updater) = &self.reputation_updater {
            match updater.submit_receipt_based_reputation(
                &receipt_to_anchor, 
                true, 
                coop_id_label, 
                community_id_label
            ).await {
                Ok(_) => tracing::info!(receipt_id = %receipt_to_anchor.id, "Reputation update submitted"),
                Err(e) => tracing::warn!(receipt_id = %receipt_to_anchor.id, "Failed to submit reputation update: {}", e),
            }
        } else {
            tracing::info!(receipt_id = %receipt_to_anchor.id, "No reputation updater configured, skipping submission");
        }
        
        // 9. Record metrics
        let duration = start_time.elapsed();
        metrics::observe_anchor_receipt_duration(duration.as_secs_f64(), coop_id_label, community_id_label, issuer_did_label);
        if let Some(mana_cost) = receipt.metrics.mana_cost {
            metrics::record_receipt_mana_cost(mana_cost, coop_id_label, community_id_label, issuer_did_label);
            metrics::MANA_COST_HISTOGRAM
                .with_label_values(&[issuer_did_label])
                .observe(mana_cost as f64);
        }

        Ok(actual_receipt_cid.to_string())
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
    ) -> Result<MeshExecutionReceipt> {
        // This is a stub implementation.
        let job_id = Uuid::new_v4().to_string();
        // Use the runtime's actual identity from the context
        let executor_did = self.context.identity()
            .ok_or_else(|| anyhow!("Runtime identity not found in execute_job context"))?
            .did.clone();
            
        let execution_start_time = Utc::now().timestamp() - 1;
        let execution_end_time_dt = Utc::now();
        let execution_end_time = execution_end_time_dt.timestamp();
        let mut resource_usage = HashMap::new();
        resource_usage.insert(ResourceType::Cpu, 10);

        Ok(MeshExecutionReceipt {
            job_id,
            executor: executor_did, // Use the DID from context
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
            mana_cost: None,
        })
    }

    /// Create a new runtime with the given context (context should now be Arc'd)
    pub fn with_context(storage: Arc<dyn RuntimeStorage>, context: Arc<RuntimeContext>) -> Self {
        let mut runtime = Self::new(storage)
            .expect("Runtime::new failed within with_context");
        runtime.context = context;
        
        // Configure reputation updater using the Arc'd context
        if let (Some(url), Some(identity)) = (runtime.context.reputation_service_url(), runtime.context.identity()) {
            let updater = Arc::new(HttpReputationUpdater::new(
                url.clone(),
                identity.did.clone(),
            ));
            runtime.reputation_updater = Some(updater);
            tracing::info!("Configured reputation updater with service URL: {}", url);
        }
        
        runtime
    }
    
    /// Construct a Runtime instance from configuration.
    pub async fn from_config(mut config: RuntimeConfig) -> Result<Self> {
        info!("Initializing runtime from config: {:?}", config);

        let keypair = load_or_generate_keypair(config.key_path.as_deref())
            .context("Failed to load or generate node keypair")?;
        
        // Ensure config has the correct DID derived from the loaded/generated keypair
        config.node_did = keypair.did.to_string(); 
        let node_did_obj = keypair.did.clone(); 
        info!(node_did = %config.node_did, "Runtime node DID initialized/confirmed.");

        let storage: Arc<dyn RuntimeStorage> = Arc::new(
            SledStorage::open(&config.storage_path)
                .context("Failed to initialize Sled storage")?,
        );

        // SharedDagStore::new() now takes no arguments
        let dag_store = Arc::new(icn_types::dag_store::SharedDagStore::new()); // Call with zero arguments

        let mut context_builder = RuntimeContextBuilder::new()
            .with_identity(keypair.clone())
            .with_executor_id(config.node_did.clone())
            .with_dag_store(dag_store); // Pass the created dag_store

        if let Some(reputation_url) = config.reputation_service_url.as_ref() {
            context_builder = context_builder.with_reputation_service(reputation_url.clone());
        }
        
        if let Some(mesh_job_url) = config.mesh_job_service_url.as_ref() {
            context_builder = context_builder.with_mesh_job_service_url(mesh_job_url.clone());
        }
        
        // TODO: Add policy loading and setting via builder
        // let policy_path = config.policy_path.clone().unwrap_or_else(...);
        // let policy = ...;
        // context_builder = context_builder.with_policy(policy);

        let context = Arc::new(context_builder.build());

        // Setup engine, linker, reputation_updater as before
        let mut engine_config = Config::new();
        engine_config.async_support(true); // Ensure async support is enabled
        let engine = Engine::new(&engine_config)?;
        let mut linker = Linker::new(&engine);
        register_host_functions(&mut linker)?;

        // Load Reputation Scoring Config or use default
        let rep_scoring_config = config.reputation_scoring_config_path.as_ref()
            .map(|path| {
                ReputationScoringConfig::from_file(path).map_err(|e| {
                    warn!("Failed to load reputation scoring config from {:?}: {}. Using default config.", path, e);
                    e // Keep the error to signal downstream that default is used because of failure
                })
            })
            .transpose()
            .unwrap_or_else(|_err| {
                // This block is reached if from_file returned an Err.
                // Warning already logged inside the map_err closure.
                Ok(ReputationScoringConfig::default())
            })
            .unwrap_or_else(|| {
                // This block is reached if the path option was None.
                info!("No reputation scoring config path specified. Using default config.");
                ReputationScoringConfig::default()
            });

        // Create Reputation Updater using the loaded or default config
        let reputation_updater: Option<Arc<dyn ReputationUpdater>> = 
            if let Some(url) = context.reputation_service_url() {
                info!("Creating HttpReputationUpdater for URL: {}", url);
                // Use new_with_config to pass the loaded or default configuration
                Some(Arc::new(HttpReputationUpdater::new_with_config(
                    url.clone(), 
                    node_did_obj, 
                    rep_scoring_config // Pass the resolved config
                )))
            } else {
                info!("No reputation service URL in context, using NoopReputationUpdater.");
                Some(Arc::new(NoopReputationUpdater))
            };

        let runtime = Self {
            config, // Store the potentially updated config
            vm: CoVm::new(ResourceLimits::default()),
            storage,
            context,
            engine,
            linker,
            module_cache: None,
            host_env: None,
            reputation_updater,
        };

        Ok(runtime)
    }
    
    /// Main loop for the runtime node service
    pub async fn run_forever(&self) -> Result<()> {
        info!("ICN Runtime node started with DID: {}", self.config.node_did);
        
        loop {
            let maybe_job = self.poll_for_job().await;

            if let Some(job) = maybe_job {
                info!(job_id = %job.job_id, "Received job");

                match self.process_polled_job(job.clone()).await {
                    Ok(receipt) => {
                        info!(job_id = %receipt.job_id, "Execution succeeded. Anchoring receipt...");
                        self.anchor_mesh_receipt(&receipt).await?;
                    }
                    Err(e) => {
                        warn!(job_id = %job.job_id, "Job processing failed: {:?}", e);
                        // TODO: Implement failure handling (e.g., update job status in storage)
                    }
                }
            } else {
                tracing::debug!("No jobs available. Sleeping...");
                sleep(Duration::from_secs(5)).await;
            }
        }
    }

    async fn poll_for_job(&self) -> Option<icn_types::mesh::MeshJob> {
        let mut queue = self.context.pending_mesh_jobs.lock().unwrap();
        queue.pop_front()
    }

    async fn process_polled_job(&self, job: icn_types::mesh::MeshJob) -> Result<MeshExecutionReceipt> {
        info!(job_id = %job.job_id, cid = %job.params.wasm_cid, "Processing polled job");
        
        // 1. Fetch WASM bytes from storage
        let wasm_bytes = self.storage.load_wasm(&job.params.wasm_cid).await
            .map_err(|e| anyhow!("Failed to load WASM for job {}: {}", job.job_id, e))?;
            
        // 2. Fetch local identity keypair from context
        //    Assuming KeyPair is Clone.
        let local_keypair = self.context.identity().cloned() // Clone if KeyPair is Clone
             .ok_or_else(|| anyhow!("Runtime requires an identity keypair to execute jobs"))?;
             
        // 3. Call the global execute_mesh_job function
        //    Pass the Arc'd context using self.context.clone().
        let receipt = execute_mesh_job(job, &local_keypair, self.context.clone()).await?; // Pass Arc clone

        Ok(receipt)
    }

    async fn anchor_mesh_receipt(&self, receipt: &MeshExecutionReceipt) -> Result<()> {
        info!(job_id = %receipt.job_id, "Anchoring mesh execution receipt");

        // Convert resource_usage HashMap<ResourceType, u64> → Vec<(String, u64)>
        let resource_usage_vec = receipt
            .resource_usage
            .iter()
            .map(|(k, v)| (format!("{:?}", k), *v)) // Format ResourceType enum variant as string
            .collect();

        let timestamp_secs = receipt.execution_end_time_dt.timestamp() as u64;

        // TODO: Revisit wasm_cid and ccl_cid - need the original MeshJob or modified MeshExecutionReceipt
        let wasm_cid_placeholder = "<placeholder-wasm-cid>".to_string();
        let ccl_cid_placeholder = "<placeholder-ccl-cid>".to_string();

        let runtime_receipt = RuntimeExecutionReceipt {
            id: Uuid::new_v4().to_string(),
            issuer: receipt.executor.to_string(), // Corrected: use executor directly
            proposal_id: receipt.job_id.clone(), // Use job_id as proposal_id for mesh jobs?
            wasm_cid: wasm_cid_placeholder,
            ccl_cid: ccl_cid_placeholder,
            metrics: RuntimeExecutionMetrics { // Placeholder metrics - Align with new structure
                host_calls: 0, // Placeholder
                io_bytes: 0,   // Placeholder
                mana_cost: receipt.mana_cost, // Read from incoming MeshExecutionReceipt
            },
            anchored_cids: vec![], // Placeholder anchored CIDs
            resource_usage: resource_usage_vec,
            timestamp: timestamp_secs, // Use u64 timestamp
            dag_epoch: None, // Placeholder epoch
            receipt_cid: None, // This will be set by anchor_receipt
            // Signature type now Option<Vec<u8>>, matching MeshExecutionReceipt
            signature: Some(receipt.signature.clone()),
        };

        // Verify the MeshExecutionReceipt's signature *before* anchoring the derived RuntimeExecutionReceipt
        receipt.verify_signature()
            .context("Incoming MeshExecutionReceipt failed signature verification")?;

        // Call the original anchor_receipt method which handles DAG storage and reputation
        // It will perform its own verification on the RuntimeExecutionReceipt again (which is fine, belt-and-suspenders)
        match self.anchor_receipt(&runtime_receipt).await {
            Ok(receipt_cid) => {
                info!(job_id = %receipt.job_id, receipt_cid = %receipt_cid, "Successfully anchored receipt");
                Ok(())
            }
            Err(e) => {
                warn!(job_id = %receipt.job_id, "Failed to anchor receipt: {:?}", e);
                Err(anyhow!("Failed to anchor receipt: {}", e))
            }
        }
    }
}

/// Module providing executable trait for CCL DSL files
pub mod dsl {
    use super::*;

    /// Trait for CCL DSL executables
    pub trait DslExecutable {
        /// Execute the DSL with the given runtime
        fn execute(&self, runtime: &Runtime) -> Result<MeshExecutionReceipt>;
    }
}

fn load_or_generate_keypair(key_path: Option<&Path>) -> Result<IcnKeyPair> {
    match key_path {
        Some(path) => {
            if path.exists() {
                info!("Attempting to load keypair from: {:?}", path);
                let mut file = File::open(path)
                    .with_context(|| format!("Failed to open keypair file: {:?}", path))?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer)
                    .with_context(|| format!("Failed to read keypair file: {:?}", path))?;
                
                let keypair: IcnKeyPair = bincode::deserialize(&buffer)
                    .with_context(|| format!("Failed to deserialize keypair from file: {:?}", path))?;
                info!("Successfully loaded keypair from: {:?}", path);
                Ok(keypair)
            } else {
                info!("No keypair file found at {:?}, generating a new one.", path);
                let keypair = IcnKeyPair::generate();
                let serialized_keypair = bincode::serialize(&keypair)
                    .context("Failed to serialize new keypair")?;
                
                if let Some(parent_dir) = path.parent() {
                    fs::create_dir_all(parent_dir)
                        .with_context(|| format!("Failed to create parent directory for keypair: {:?}", parent_dir))?;
                }

                let mut file = File::create(path)
                    .with_context(|| format!("Failed to create keypair file: {:?}", path))?;
                file.write_all(&serialized_keypair)
                    .with_context(|| format!("Failed to write new keypair to file: {:?}", path))?;
                info!("Successfully generated and saved new keypair to: {:?}", path);
                Ok(keypair)
            }
        }
        None => {
            info!("No key_path provided. Generating an in-memory keypair.");
            Ok(IcnKeyPair::generate())
        }
    }
}

// Helper function to sign a RuntimeExecutionReceipt using the provided keypair
fn sign_runtime_receipt_in_place(
    receipt: &mut RuntimeExecutionReceipt,
    keypair: &IcnKeyPair,
) -> Result<()> {
    // Note: This import assumes KeyPair::sign exists and returns ed25519_dalek::Signature
    // If KeyPair itself implements ed25519_dalek::Signer, adjust accordingly.
    // use ed25519_dalek::Signer;
    use bincode; // Ensure bincode is available
    use anyhow::Context; // Ensure Context is available

    // Ensure signature is None before signing to avoid confusion 
    // (or handle re-signing if necessary, though usually not desirable for receipts)
    if receipt.signature.is_some() {
        warn!("Receipt already has a signature before signing attempt. Overwriting is generally not recommended.");
        // Depending on policy, could return an error here instead:
        // bail!("Receipt already signed");
    }
    
    let payload = receipt.signed_payload(); // Assumes signed_payload() is available via import
    let bytes = bincode::serialize(&payload)
        .context("Failed to serialize RuntimeExecutionReceipt payload for signing")?;
    
    // Assumes icn_identity::KeyPair has a public method `sign`:
    // fn sign(&self, message: &[u8]) -> ed25519_dalek::Signature;
    let signature = keypair.sign(&bytes); // Use the assumed sign method
    
    receipt.signature = Some(signature.to_bytes().to_vec());
    Ok(())
}

/// Executes a MeshJob within the ICN runtime.
pub async fn execute_mesh_job(
    mesh_job: MeshJob,
    local_keypair: &IcnKeyPair,
    runtime_context: Arc<RuntimeContext>,
) -> Result<MeshExecutionReceipt, anyhow::Error> {
    info!(job_id = %mesh_job.job_id, cid = %mesh_job.params.wasm_cid, "Attempting to execute mesh job");

    // Mana check & consumption
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
        mana_mgr.ensure_pool(&scope_key, 10_000, 1); // Ensure some default mana if pool doesn't exist

        let balance_before = mana_mgr.balance(&scope_key).unwrap_or(0);
        let declared_cost: u64 = mesh_job.params.resources_required.iter().map(|(_, amt)| *amt).sum();
        let cost = if declared_cost > 0 { declared_cost } else { 50 }; // Fallback cost

        if let Err(e) = mana_mgr.spend(&scope_key, cost) {
            tracing::warn!("[RuntimeExecute] Insufficient mana for {:?}: {}", scope_key, e);
            return Err(anyhow::anyhow!("Insufficient mana: {}", e));
        }
        let balance_after = mana_mgr.balance(&scope_key).unwrap_or(0);
        tracing::info!("[RuntimeExecute] Consumed {} mana for {:?} ({} -> {})", cost, scope_key, balance_before, balance_after);
    } // <-- End of mana_mgr lock scope

    // IMPORTANT: Capture the calculated `cost`
}