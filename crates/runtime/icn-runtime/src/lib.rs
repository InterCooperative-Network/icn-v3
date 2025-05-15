pub mod config;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use ed25519_dalek::VerifyingKey;
use icn_core_vm::{ExecutionMetrics as CoreVmExecutionMetrics, ResourceLimits};
pub use icn_economics::mana::{InMemoryManaLedger, ManaLedger, ManaRegenerator, RegenerationPolicy};
use icn_economics::ResourceType;
use icn_identity::{Did, DidError, KeyPair as IcnKeyPair, TrustBundle, TrustValidationError};
use icn_mesh_receipts::ExecutionReceipt as MeshExecutionReceipt;
use icn_types::dag::{DagEventType, DagNode};
use icn_types::dag_store::DagStore;
use icn_types::mesh::{JobStatus as IcnJobStatus, MeshJob, MeshJobParams};
use icn_types::runtime_receipt::{RuntimeExecutionMetrics, RuntimeExecutionReceipt};
use icn_types::VerifiableReceipt;
use icn_types::JobFailureReason;
use icn_mesh_protocol::P2PJobStatus;
use icn_types::error::IcnError;
use icn_types::error::EconomicsError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::time::{sleep, Duration};
use tracing::{info, warn, debug, error};
use uuid::Uuid;
use wasmtime::{
    Engine, Linker, Module, Store, Val,
};

use std::str::FromStr;
use std::fs::{self, File};
use std::io::{Read, Write};

use crate::config::RuntimeConfig;

// Import the context module
pub mod context;
pub use context::RuntimeContext;
pub use context::RuntimeContextBuilder;

// Import the host environment module
pub mod host_environment;
pub use host_environment::ConcreteHostEnvironment;

// Import the job execution context module
pub mod job_execution_context;

// Import the wasm module
pub mod wasm;
pub use wasm::register_host_functions;

// Import metrics module
pub mod metrics;

// Import reputation integration module
pub mod reputation_integration;
use reputation_integration::{
    ReputationUpdater,
};

/// Distribution worker for periodic mana payouts
pub mod distribution_worker;

// Import sled_storage module and type
pub mod sled_storage;
// use sled_storage::SledStorage;

// Add imports for keypair loading/saving
// use bincode;
// use std::fs::{self, File};
// use std::io::{Read, Write};

// Add at the top with other constants
const DEFAULT_MANA_COST: u64 = 100;

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
        self.proposals
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("Proposal {} not found", id))
    }

    async fn update_proposal(&self, proposal: &Proposal) -> Result<()> {
        self.proposals
            .lock()
            .unwrap()
            .insert(proposal.id.clone(), proposal.clone());
        Ok(())
    }

    async fn load_wasm(&self, cid: &str) -> Result<Vec<u8>> {
        self.wasm_modules
            .lock()
            .unwrap()
            .get(cid)
            .cloned()
            .ok_or_else(|| anyhow!("WASM {} not found", cid))
    }

    async fn store_wasm(&self, cid: &str, bytes: &[u8]) -> Result<()> {
        self.wasm_modules
            .lock()
            .unwrap()
            .insert(cid.to_string(), bytes.to_vec());
        Ok(())
    }

    async fn store_receipt(&self, receipt: &RuntimeExecutionReceipt) -> Result<String> {
        let receipt_id = receipt.id.clone();
        // Simple hash for mock storage ID - replace with proper CID generation if needed
        let cid = format!("mock-receipt-{}", receipt_id);
        self.receipts
            .lock()
            .unwrap()
            .insert(cid.clone(), receipt.clone());
        Ok(cid)
    }

    async fn load_receipt(&self, receipt_id: &str) -> Result<RuntimeExecutionReceipt> {
        self.receipts
            .lock()
            .unwrap()
            .get(receipt_id)
            .cloned()
            .ok_or_else(|| anyhow!("Receipt {} not found", receipt_id))
    }

    async fn anchor_to_dag(&self, cid: &str) -> Result<String> {
        let anchor_cid = format!("mock-anchor-{}", cid);
        self.anchored_cids.lock().unwrap().push(anchor_cid.clone());
        Ok(anchor_cid)
    }
}

/// The ICN Runtime for executing governance proposals
#[derive(Clone)]
pub struct Runtime<L: ManaLedger + Send + Sync + 'static> {
    /// Runtime configuration
    config: RuntimeConfig,

    /// Storage backend
    storage: Arc<dyn RuntimeStorage>,

    /// Runtime context (now Arc'd and generic)
    context: Arc<RuntimeContext<L>>,

    /// Wasmtime engine
    engine: Engine,

    /// Wasmtime linker
    linker: Linker<wasm::StoreData>,

    /// Host environment
    host_env: Option<Arc<Mutex<ConcreteHostEnvironment>>>,

    /// Optional reputation updater
    reputation_updater: Option<Arc<dyn ReputationUpdater>>,
}

impl<L: ManaLedger + Send + Sync + 'static> Runtime<L> {
    /// Create a new runtime with specified storage, typically for InMemoryManaLedger
    pub fn new(storage: Arc<dyn RuntimeStorage>) -> Result<Self, anyhow::Error>
    where
        L: Default, // L must be Default for this constructor
    {
        let default_keypair = IcnKeyPair::generate();
        let default_did = default_keypair.did.clone();

        let mut runtime_config = RuntimeConfig::default();
        runtime_config.node_did = default_did.to_string();

        let engine = Engine::default();
        let mut linker = Linker::new(&engine);
        crate::wasm::register_host_functions(&mut linker)?;

        let ledger = Arc::new(L::default());
        let policy = RegenerationPolicy::FixedRatePerTick(10);
        let regenerator = Arc::new(ManaRegenerator::new(ledger.clone(), policy));

        let context: Arc<RuntimeContext<L>> = Arc::new(
            RuntimeContextBuilder::<L>::new()
                .with_identity(default_keypair)
                .with_executor_id(default_did.to_string())
                .with_mana_regenerator(regenerator)
                .build(),
        );

        Ok(Self {
            config: runtime_config,
            storage,
            context,
            engine,
            linker,
            host_env: None,
            reputation_updater: None,
        })
    }

    /// Set a reputation updater for this runtime
    pub fn with_reputation_updater(mut self, updater: Arc<dyn ReputationUpdater>) -> Self {
        self.reputation_updater = Some(updater);
        self
    }

    /// Get a reference to the runtime context
    pub fn context(&self) -> &RuntimeContext<L> {
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

        let executor_did_str = self
            .context
            .executor_id
            .clone()
            .unwrap_or_else(|| "did:icn:system".to_string());
        let executor_did = Did::from_str(&executor_did_str)?;

        let job_id = format!("proposal-{}", proposal_id);

        let execution_start_time = Utc::now().timestamp() - 2;
        let execution_end_time_dt = Utc::now();
        let execution_end_time = execution_end_time_dt.timestamp();

        let fake_resource_map: HashMap<ResourceType, u64> =
            [(ResourceType::Cpu, 150), (ResourceType::Memory, 256)]
                .iter()
                .cloned()
                .collect();

        let receipt = MeshExecutionReceipt {
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

        let fake_resource_map: HashMap<ResourceType, u64> =
            [(ResourceType::Cpu, 50)].iter().cloned().collect();

        let job_id = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("local-file-job")
            .to_string();
        // Use the runtime's actual identity from the context
        let executor_did = self
            .context
            .identity()
            .ok_or_else(|| anyhow!("Runtime identity not found in execute_wasm_file context"))?
            .did
            .clone();

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
        #[cfg(not(feature = "full_host_abi"))]
        let store_creator = |engine: &Engine, host_env_arc: &Option<Arc<Mutex<ConcreteHostEnvironment>>>| -> Result<Store<wasm::StoreData>, RuntimeError> {
            let mut store_data = wasm::StoreData::new();
            if let Some(env_arc) = host_env_arc {
                let env_clone = env_arc.lock().map_err(|_| RuntimeError::ExecutionError("Host env mutex poisoned".to_string()))?;
                store_data.set_host(env_clone.clone());
                Ok(Store::new(engine, store_data))
            } else {
                Err(RuntimeError::HostEnvironmentNotSet)
            }
        };

        #[cfg(feature = "full_host_abi")]
        let store_creator = |engine: &Engine, host_env_arc: &Option<Arc<Mutex<ConcreteHostEnvironment>>>| -> Result<Store<wasm::StoreData>, RuntimeError> {
            if let Some(env_arc) = host_env_arc {
                let env_clone = env_arc.lock().map_err(|_| RuntimeError::ExecutionError("Host env mutex poisoned".to_string()))?;
                // When full_host_abi is ON, wasm::StoreData is ConcreteHostEnvironment
                Ok(Store::new(engine, env_clone.clone()))
            } else {
                Err(RuntimeError::HostEnvironmentNotSet)
            }
        };

        let mut store = store_creator(&self.engine, &self.host_env)?;

        let module = self.load_module(wasm_bytes, &mut store).await?;

        let instance = self
            .linker
            .instantiate_async(&mut store, &module)
            .await
            .map_err(|e| RuntimeError::Instantiation(e.to_string()))?;

        let func = instance
            .get_func(&mut store, &function_name)
            .ok_or_else(|| RuntimeError::FunctionNotFound(function_name.clone()))?;

        let mut results = vec![Val::I32(0); func.ty(&store).results().len()];

        func.call_async(&mut store, &args, &mut results)
            .await
            .map_err(|e| RuntimeError::Execution(e.to_string()))?;

        Ok(results.into_boxed_slice())
    }

    /// Helper to load (or get from cache) and compile module (made async)
    async fn load_module(
        &self,
        wasm_bytes: &[u8],
        _store: &mut Store<wasm::StoreData>,
    ) -> Result<Module, RuntimeError> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| RuntimeError::LoadError(format!("Failed to compile WASM: {}", e)))?;
        Ok(module)
    }

    /// Execute a WASM binary with the given context in governance mode
    #[cfg(feature = "full_host_abi")]
    pub async fn governance_execute_wasm(
        &mut self,
        wasm_bytes: &[u8],
        context: VmContext,
    ) -> Result<ExecutionResult, RuntimeError> {
        // Full implementation lives behind the feature flag.
        unimplemented!()
    }

    #[cfg(not(feature = "full_host_abi"))]
    pub async fn governance_execute_wasm(
        &mut self,
        _wasm_bytes: &[u8],
        _context: VmContext,
    ) -> Result<ExecutionResult, RuntimeError> {
        Err(RuntimeError::ExecutionError(
            "governance WASM disabled in minimal build".into(),
        ))
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
        receipt: &RuntimeExecutionReceipt, // Kept specific to RuntimeExecutionReceipt
    ) -> Result<String> // Returns receipt_cid as String
    {
        let start_time = std::time::Instant::now();

        let federation_id = self
            .context
            .federation_id
            .as_deref()
            .unwrap_or("unknown_federation");
        let coop_id_label = federation_id;
        let community_id_label = federation_id;
        let issuer_did_label = receipt.issuer.as_str();

        // 1. Verify signature
        match receipt.verify_signature() {
            Ok(_) => {
                metrics::record_receipt_verification_outcome(
                    true,
                    coop_id_label,
                    community_id_label,
                    issuer_did_label,
                );
            }
            Err(e) => {
                metrics::record_receipt_verification_outcome(
                    false,
                    coop_id_label,
                    community_id_label,
                    issuer_did_label,
                );
                // Re-throw error to halt anchoring on verification failure
                return Err(e).context("Receipt signature verification failed during anchoring");
                // Ensure this uses anyhow::Error context
            }
        };

        // 2. Generate the content-addressed CID for the receipt
        // This now assumes RuntimeExecutionReceipt has a working .cid() method.
        let actual_receipt_cid = receipt
            .cid()
            .map_err(|e| anyhow!("Failed to generate CID for receipt: {}", e))?;

        // 3. Create a version of the receipt that includes its own CID
        let mut receipt_to_anchor = receipt.clone();
        receipt_to_anchor.receipt_cid = Some(actual_receipt_cid.to_string());

        // 4. Serialize the receipt_to_anchor for DAG storage
        let _receipt_bytes = serde_cbor::to_vec(&receipt_to_anchor)
            .context("Failed to serialize receipt for DAG storage")?;

        // 5. Store the serialized receipt in the DAG Store using its actual CID
        // OLD: self.dag_store().put_raw_block(&actual_receipt_cid, receipt_bytes, 0x71u64).await
        //    .with_context(|| format!("Failed to anchor receipt CID {} to DAG store", actual_receipt_cid))?;

        // NEW: Construct DagNode and insert
        let receipt_json_string = serde_json::to_string(&receipt_to_anchor)
            .context("Failed to serialize receipt to JSON string for DagNode content")?;

        let dag_node_for_receipt = DagNode {
            content: receipt_json_string,
            parent: None, // TODO: Determine parent if applicable. For now, assuming root or standalone.
            event_type: DagEventType::Receipt,
            timestamp: receipt_to_anchor.timestamp,
            scope_id: issuer_did_label.to_string(), // Using issuer's DID as scope for this example
        };

        // The CID of this dag_node_for_receipt will be different from actual_receipt_cid if DagNode adds metadata.
        // The insert method will calculate it internally.
        self.dag_store().insert(dag_node_for_receipt).await
            .with_context(|| format!("Failed to insert receipt DagNode (derived from original CID {}) into DAG store", actual_receipt_cid))?;
        tracing::info!(original_receipt_cid = %actual_receipt_cid, "Receipt (as DagNode) submitted to DAG store");

        // 6. Store in local Sled storage (optional, for quick lookups by ID if still needed)
        self.storage
            .store_receipt(&receipt_to_anchor)
            .await
            .context("Failed to store receipt in local Sled storage after DAG anchoring")?;

        // 7. Anchoring receipt.anchored_cids:
        // The loop `for cid_str in &receipt.anchored_cids` and its call to `self.storage.anchor_to_dag(cid_str).await`
        // is removed. Storing `receipt_to_anchor` (which contains `anchored_cids`) in the DAG
        // effectively anchors these references as part of the receipt's immutable record.

        // 8. Submit reputation update
        if let Some(updater) = &self.reputation_updater {
            match updater
                .submit_receipt_based_reputation(
                    &receipt_to_anchor,
                    true,
                    coop_id_label,
                    community_id_label,
                )
                .await
            {
                Ok(_) => {
                    tracing::info!(receipt_id = %receipt_to_anchor.id, "Reputation update submitted")
                }
                Err(e) => {
                    tracing::warn!(receipt_id = %receipt_to_anchor.id, "Failed to submit reputation update: {}", e)
                }
            }
        } else {
            tracing::info!(receipt_id = %receipt_to_anchor.id, "No reputation updater configured, skipping submission");
        }

        // Perform Mana Deduction if applicable
        if let Some(cost) = receipt_to_anchor.metrics.mana_cost {
            if cost > 0 {
                if let Some(updater) = &self.reputation_updater {
                    match Did::from_str(&receipt_to_anchor.issuer) {
                        Ok(executor_did_val) => {
                            match updater
                                .submit_mana_deduction(
                                    &executor_did_val,
                                    cost,
                                    coop_id_label,
                                    community_id_label,
                                )
                                .await
                            {
                                Ok(_) => tracing::info!(
                                    receipt_id = %receipt_to_anchor.id,
                                    executor = %receipt_to_anchor.issuer,
                                    mana_deducted = cost,
                                    "Mana deduction submitted successfully."
                                ),
                                Err(e) => tracing::warn!(
                                    receipt_id = %receipt_to_anchor.id,
                                    executor = %receipt_to_anchor.issuer,
                                    "Failed to submit mana deduction: {}", e
                                ),
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                receipt_id = %receipt_to_anchor.id,
                                issuer_did = %receipt_to_anchor.issuer,
                                "Failed to parse issuer DID for mana deduction: {}. Skipping deduction.", e
                            );
                        }
                    }
                }
            } else {
                tracing::debug!(
                    receipt_id = %receipt_to_anchor.id,
                    mana_cost = cost,
                    "Mana cost is zero or not set for receipt, skipping deduction."
                );
            }
        } else {
            tracing::debug!(
                receipt_id = %receipt_to_anchor.id,
                "No mana cost found in receipt metrics, skipping deduction."
            );
        }

        // 9. Record metrics
        let duration = start_time.elapsed();
        metrics::observe_anchor_receipt_duration(
            duration.as_secs_f64(),
            coop_id_label,
            community_id_label,
            issuer_did_label,
        );
        if let Some(mana_cost) = receipt.metrics.mana_cost {
            metrics::record_receipt_mana_cost(
                mana_cost,
                coop_id_label,
                community_id_label,
                issuer_did_label,
            );
            metrics::MANA_COST_HISTOGRAM
                .with_label_values(&[issuer_did_label])
                .observe(mana_cost as f64);
        }

        Ok(actual_receipt_cid.to_string())
    }

    /// Verify a trust bundle using the configured trust validator
    pub fn verify_trust_bundle(&self, bundle: &TrustBundle) -> Result<(), RuntimeError> {
        let validator = self
            .context
            .trust_validator()
            .ok_or(RuntimeError::NoTrustValidator)?;

        validator
            .set_trust_bundle(bundle.clone())
            .map_err(RuntimeError::TrustBundleVerificationError)
    }

    /// Register a trusted signer with DID and verifying key
    pub fn register_trusted_signer(&self, did: Did, key: VerifyingKey) -> Result<(), RuntimeError> {
        let validator = self
            .context
            .trust_validator()
            .ok_or(RuntimeError::NoTrustValidator)?;

        validator.register_signer(did, key);
        Ok(())
    }

    /// Check if a signer is authorized
    pub fn is_authorized_signer(&self, did: &Did) -> Result<bool, RuntimeError> {
        let validator = self
            .context
            .trust_validator()
            .ok_or(RuntimeError::NoTrustValidator)?;

        validator
            .is_authorized_signer(did)
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
        // Placeholder: Actual job execution logic is more complex and involves CoVM.
        // This might need to be adjusted based on the actual structure.
        // For now, ensure it compiles and respects the Runtime<L> structure.

        // Example: Construct a dummy receipt.
        let job_id = Uuid::new_v4().to_string();
        let executor_did = self.context.identity().map_or_else(
            || Did::from_str("did:error:no_identity").unwrap(), // Should handle error properly
            |kp| kp.did.clone(),
        );

        let fake_resource_map: HashMap<ResourceType, u64> = [
            (ResourceType::Cpu, 10), // Example values
            (ResourceType::Memory, 64),
        ]
        .iter()
        .cloned()
        .collect();

        let execution_start_time = Utc::now().timestamp() - 1;
        let execution_end_time_dt = Utc::now();
        let execution_end_time = execution_end_time_dt.timestamp();

        Ok(MeshExecutionReceipt {
            job_id,
            executor: executor_did,
            status: IcnJobStatus::Completed, // Assuming success for placeholder
            result_data_cid: Some("bafy...placeholder_result".to_string()),
            logs_cid: None,
            resource_usage: fake_resource_map,
            execution_start_time: execution_start_time as u64,
            execution_end_time: execution_end_time as u64,
            execution_end_time_dt,
            signature: Vec::new(),
            coop_id: None,
            community_id: None,
            mana_cost: _params.explicit_mana_cost, // Or calculated cost
        })
    }

    /// Create a new runtime with the given context (context should now be Arc'd and generic)
    pub fn with_context(storage: Arc<dyn RuntimeStorage>, context: Arc<RuntimeContext<L>>) -> Self {
        // Assuming 'config' should be derived from context or a default.
        // For simplicity, let's use a default config here if not available in context.
        // Or, this constructor might need to take a Config as well.
        // This needs to align with how Runtime is typically constructed with external contexts.

        let node_did_str = context.executor_id.clone().unwrap_or_else(|| {
            context.identity().map_or_else(
                || IcnKeyPair::generate().did.to_string(), // Fallback if no identity/executor_id
                |kp| kp.did.to_string(),
            )
        });

        let config = RuntimeConfig {
            node_did: node_did_str,
            // Other fields might need to be derived or defaulted
            ..Default::default()
        };

        let engine = Engine::default();
        let mut linker = Linker::new(&engine);
        crate::wasm::register_host_functions(&mut linker)
            .expect("Failed to register host functions for Runtime::with_context");

        Self {
            config,
            storage,
            context,
            engine,
            linker,
            host_env: None,
            reputation_updater: None,
        }
    }

    /// Main loop for the runtime node service
    pub async fn run_forever(self) -> Result<()> {
        info!(
            "ICN Runtime node started with DID: {}",
            self.config.node_did
        );

        loop {
            let maybe_job = self.poll_for_job().await;

            if let Some(job) = maybe_job {
                info!(job_id = %job.job_id, "Received job");

                match self.process_polled_job(job.clone()).await {
                    Ok(receipt) => {
                        if receipt.status == IcnJobStatus::Failed {
                            warn!(
                                job_id = %receipt.job_id,
                                "Job processing returned Ok(receipt), but receipt status is Failed."
                            );

                            let failure_reason = JobFailureReason::ExecutionError(
                                "Job completed with a 'Failed' status in its execution receipt"
                                    .to_string(),
                            );

                            let executor_node_did_str = self.config.node_did.clone();
                            let parsed_node_did = match Did::from_str(&executor_node_did_str) {
                                Ok(did) => did,
                                Err(did_parse_err) => {
                                    error!(
                                        "CRITICAL: Runtime's configured node_did '{}' is invalid: {}. Cannot report job failure accurately.",
                                        executor_node_did_str, did_parse_err
                                    );
                                    return Err(anyhow!(
                                        "Runtime configuration error: node_did '{}' is invalid: {}",
                                        executor_node_did_str,
                                        did_parse_err
                                    ));
                                }
                            };

                            let failed_status_update = P2PJobStatus::Failed {
                                node_id: parsed_node_did,
                                reason: failure_reason,
                            };

                            warn!(
                                job_id = %receipt.job_id,
                                "Job processing indicates failure in receipt. Status: {:?}",
                                failed_status_update
                            );
                            // TODO: Implement actual failure reporting mechanism for this case too.
                            
                            // Skip anchoring a failed job's receipt if it explicitly failed.
                            // Or, if failed receipts *should* be anchored, remove continue and adjust logic.
                            // For now, skipping.
                            continue; 
                        }

                        // If receipt.status is not Failed, proceed as normal.
                        info!(job_id = %receipt.job_id, "Execution succeeded (receipt status is not Failed). Anchoring receipt...");
                        self.anchor_mesh_receipt(&receipt).await?;
                    }
                    Err(e) => {
                        warn!(job_id = %job.job_id, "Job processing failed: {:?}", e);
                        
                        let failure_reason = if let Some(icn_err) = e.downcast_ref::<IcnError>() {
                            match icn_err {
                                IcnError::Io(_) => JobFailureReason::NetworkError,
                                IcnError::Serialization(_) => JobFailureReason::OutputError,
                                IcnError::InvalidUri(_) => JobFailureReason::InvalidInput,
                                IcnError::NotFound(_) => JobFailureReason::NotFound,
                                IcnError::PermissionDenied(s) => {
                                    // PermissionDenied is unit, use ExecutionError to keep message
                                    JobFailureReason::ExecutionError(format!("Permission denied: {}", s))
                                }
                                IcnError::Identity(_) => JobFailureReason::PermissionDenied, // General category

                                IcnError::Economics(econ_err) => match econ_err {
                                    EconomicsError::QuotaExceeded { .. } | EconomicsError::RateLimitExceeded { .. } => {
                                        JobFailureReason::ResourceLimitExceeded
                                    }
                                    EconomicsError::AccessDenied { .. } => JobFailureReason::PermissionDenied,
                                    _ => JobFailureReason::ExecutionError(format!("Economics error: {}", econ_err)),
                                },

                                IcnError::Crypto(err) => JobFailureReason::ExecutionError(format!("Crypto error: {}", err)),
                                IcnError::Dag(err) => JobFailureReason::ExecutionError(format!("DAG error: {}", err)),
                                IcnError::Multicodec(err) => JobFailureReason::ExecutionError(format!("Multicodec error: {}", err)),
                                IcnError::Trust(err) => JobFailureReason::ExecutionError(format!("Trust error: {}", err)),
                                IcnError::Mesh(err) => JobFailureReason::ExecutionError(format!("Mesh error: {}", err)),
                                IcnError::Timeout(s) => JobFailureReason::ExecutionError(format!("Timeout: {}", s)),
                                IcnError::Config(s) => JobFailureReason::ExecutionError(format!("Config error: {}", s)),
                                IcnError::Storage(s) => JobFailureReason::ExecutionError(format!("Storage error: {}", s)),
                                IcnError::Database(s) => JobFailureReason::ExecutionError(format!("Database error: {}", s)),
                                IcnError::Plugin(s) => JobFailureReason::ExecutionError(format!("Plugin error: {}", s)),
                                IcnError::Consensus(s) => JobFailureReason::ExecutionError(format!("Consensus error: {}", s)),
                                IcnError::InvalidOperation(s) => JobFailureReason::ExecutionError(format!("Invalid operation: {}", s)),
                                
                                IcnError::General(s) => JobFailureReason::Unknown(s.clone()),
                                
                                // Catch-all for any IcnError variants not explicitly handled.
                                _ => JobFailureReason::Unknown(format!("An unclassified ICN error occurred: {}", icn_err)),
                            }
                        } else {
                            // Fallback if 'e' is not an IcnError
                            JobFailureReason::ExecutionError(e.to_string())
                        };
                        
                        let executor_node_did_str = self.config.node_did.clone();
                        match Did::from_str(&executor_node_did_str) {
                            Ok(parsed_node_did) => {
                                let failed_status_update = P2PJobStatus::Failed {
                                    node_id: parsed_node_did,
                                    reason: failure_reason,
                                };

                                // TODO: Implement actual failure reporting mechanism.
                                // This could involve:
                                // 1. Finding the JobExecutionContext for this job_id and calling ctx.update_status(failed_status_update).
                                // 2. Sending an HTTP request to icn-mesh-jobs to mark the job as failed.
                                // 3. Broadcasting a P2P message with this status update.
                                error!(
                                    job_id = %job.job_id,
                                    status = ?failed_status_update,
                                    "Job failed. Status constructed. Reporting mechanism is TBD."
                                );
                            }
                            Err(did_parse_err) => {
                                error!(
                                    job_id = %job.job_id,
                                    original_job_error = ?e,
                                    node_did_parse_error = ?did_parse_err,
                                    invalid_configured_node_did = %executor_node_did_str,
                                    "Original job failed. Additionally, the runtime's configured node DID is invalid. Cannot form P2PJobStatus::Failed for reporting."
                                );
                                // At this point, we can't report the P2PJobStatus::Failed properly.
                                // The original job failure still stands.
                            }
                        }
                    }
                }
            } else {
                tracing::debug!("No jobs available. Sleeping...");
                sleep(Duration::from_secs(5)).await;
            }
        }
    }

    async fn poll_for_job(&self) -> Option<icn_types::mesh::MeshJob> {
        // Implementation for polling jobs from mesh service
        // This would use self.context.mesh_job_service_url() and an HTTP client
        // For now, returning None
        if let Some(url) = self.context.mesh_job_service_url() {
            debug!("Polling for jobs at: {}", url);
            // Replace with actual HTTP client logic, e.g., reqwest
            // This is a placeholder. A real implementation would make an HTTP GET request.
            // For example:
            // match reqwest::get(format!(\"{}/next-job\", url)).await {
            //     Ok(response) => match response.json::<icn_types::mesh::MeshJob>().await {
            //         Ok(job) => Some(job),
            //         Err(e) => { error!(\"Failed to parse job: {}\", e); None }
            //     },
            //     Err(e) => { error!(\"Failed to poll for job: {}\", e); None }
            // }
            None // Placeholder
        } else {
            None
        }
    }

    async fn process_polled_job(
        &self,
        job: icn_types::mesh::MeshJob,
    ) -> Result<MeshExecutionReceipt> {
        info!("Processing polled job ID: {:?}", job.job_id);

        let cid_string = &job.params.wasm_cid;
        let _wasm_bytes = self.storage.load_wasm(cid_string.as_str()).await.map_err(|e| {
            anyhow!(
                "Failed to load WASM for job {} (CID: {}): {}",
                job.job_id.as_str(),
                cid_string,
                e
            )
        })?;

        let local_keypair = self
            .context
            .identity()
            .ok_or_else(|| anyhow!("Runtime identity not set for job processing"))?;

        let originator_did_str = job.originator_did.as_str();
        let _originator_did = Did::from_str(originator_did_str)?;

        let receipt = execute_mesh_job(job, local_keypair, self.context.clone()).await?;

        if receipt.status == IcnJobStatus::Completed {
            self.anchor_mesh_receipt(&receipt).await?;
        }
        Ok(receipt)
    }

    pub async fn anchor_mesh_receipt(&self, receipt: &MeshExecutionReceipt) -> Result<()> {
        // Placeholder for anchoring logic (e.g., to DAG, blockchain)
        info!("Anchoring mesh receipt for job ID: {}", receipt.job_id);
        // Example: Storing receipt CID or hash somewhere
        // self.storage.anchor_to_dag(&receipt.job_id).await?; // Assuming job_id is CID-like or used as key

        // If reputation_updater is present and mana_cost is Some and > 0
        if let Some(updater) = &self.reputation_updater {
            if let Some(mana_cost) = receipt.mana_cost {
                if mana_cost > 0 {
                    let coop_id = receipt.coop_id.as_ref().map(|id| id.0.as_str()).unwrap_or("default_coop");
                    let community_id = receipt.community_id.as_ref().map(|id| id.0.as_str()).unwrap_or("default_community");
                    if let Err(e) = updater
                        .submit_mana_deduction(&receipt.executor, mana_cost, coop_id, community_id)
                        .await
                    {
                        error!(
                            "Failed to submit mana deduction for job {}: {}",
                            receipt.job_id, e
                        );
                        // Decide if this should be a hard error for anchoring
                    } else {
                        info!(
                            "Submitted mana deduction of {} for job {} by executor {}",
                            mana_cost, receipt.job_id, receipt.executor
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn tick_mana(&self) -> Result<()> {
        if let Some(regenerator) = &self.context.mana_regenerator {
            debug!("Ticking mana regeneration...");
            match regenerator.tick().await {
                Ok(details) => {
                    if !details.errors.is_empty() {
                        debug!(
                            "Mana tick: Processed {} DIDs, regenerated {} DIDs, {} errors",
                            details.processed_dids_count,
                            details.regenerated_dids_count,
                            details.errors.len()
                        );
                        for error in &details.errors {
                            error!("Mana regeneration error: {:?}", error);
                        }
                    } else {
                        debug!("Mana tick: No DIDs to regenerate or no errors.");
                    }
                    Ok(())
                }
                Err(e) => {
                    error!("Mana regeneration tick failed: {}", e);
                    Err(anyhow!("Mana regeneration tick failed: {}", e))
                }
            }
        } else {
            debug!("Mana regenerator not configured, skipping tick.");
            Ok(())
        }
    }
}

/// Module providing executable trait for CCL DSL files
pub mod dsl {
    use super::*;

    /// Trait for CCL DSL executables
    pub trait DslExecutable {
        /// Execute the DSL with the given runtime
        fn execute(&self, runtime: &Runtime<InMemoryManaLedger>) -> Result<MeshExecutionReceipt>;
    }
}

pub fn load_or_generate_keypair(key_path: Option<&Path>) -> Result<IcnKeyPair> {
    match key_path {
        Some(path) => {
            if path.exists() {
                info!("Attempting to load keypair from: {:?}", path);
                let mut file = File::open(path)
                    .with_context(|| format!("Failed to open keypair file: {:?}", path))?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer)
                    .with_context(|| format!("Failed to read keypair file: {:?}", path))?;

                let keypair: IcnKeyPair = bincode::deserialize(&buffer).with_context(|| {
                    format!("Failed to deserialize keypair from file: {:?}", path)
                })?;
                info!("Successfully loaded keypair from: {:?}", path);
                Ok(keypair)
            } else {
                info!("No keypair file found at {:?}, generating a new one.", path);
                let keypair = IcnKeyPair::generate();
                let serialized_keypair =
                    bincode::serialize(&keypair).context("Failed to serialize new keypair")?;

                if let Some(parent_dir) = path.parent() {
                    fs::create_dir_all(parent_dir).with_context(|| {
                        format!(
                            "Failed to create parent directory for keypair: {:?}",
                            parent_dir
                        )
                    })?;
                }

                let mut file = File::create(path)
                    .with_context(|| format!("Failed to create keypair file: {:?}", path))?;
                file.write_all(&serialized_keypair)
                    .with_context(|| format!("Failed to write new keypair to file: {:?}", path))?;
                info!(
                    "Successfully generated and saved new keypair to: {:?}",
                    path
                );
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
    use anyhow::Context;
    use bincode; // Ensure bincode is available // Ensure Context is available

    // Ensure signature is None before signing to avoid confusion
    // (or handle re-signing if necessary, though usually not desirable for receipts)
    if receipt.signature.is_some() {
        warn!("Receipt already has a signature before signing attempt. Overwriting is generally not recommended.");
        // Depending on policy, could return an error here instead:
        // bail!("Receipt already signed");
    }

    // Corrected: Use get_payload_for_signing() from the VerifiableReceipt trait
    let payload = receipt
        .get_payload_for_signing()
        .context("Failed to get payload from RuntimeExecutionReceipt for signing")?;
    let bytes = bincode::serialize(&payload)
        .context("Failed to serialize RuntimeExecutionReceipt payload for signing")?;

    // Assumes icn_identity::KeyPair has a public method `sign`:
    // fn sign(&self, message: &[u8]) -> ed25519_dalek::Signature;
    let signature = keypair.sign(&bytes); // Use the assumed sign method

    receipt.signature = Some(signature.to_bytes().to_vec());
    Ok(())
}

/// Executes a MeshJob within the ICN runtime.
pub async fn execute_mesh_job<L: ManaLedger + Send + Sync + 'static>(
    mesh_job: MeshJob,
    local_keypair: &IcnKeyPair,
    _runtime_context: Arc<RuntimeContext<L>>, // Prefix unused variable
) -> Result<MeshExecutionReceipt, anyhow::Error> {
    info!(
        "Executing mesh job: {:?} with executor {}",
        mesh_job.job_id, local_keypair.did
    );
    // ... (rest of the logic from the original execute_mesh_job)
    // ... using runtime_context.storage(), runtime_context.mana_regenerator if needed for cost calculation, etc.

    // Determine mana_cost (priority: explicit, then resource sum, then default)
    let calculated_mana_cost = mesh_job.params.explicit_mana_cost.unwrap_or_else(|| {
        if !mesh_job.params.resources_required.is_empty() {
            mesh_job.params.resources_required.iter().map(|(_, amount)| *amount).sum()
        } else {
            DEFAULT_MANA_COST
        }
    });
    let final_mana_cost = if calculated_mana_cost == 0 && !mesh_job.params.resources_required.is_empty() {
        DEFAULT_MANA_COST
    } else {
        calculated_mana_cost
    };

    // Simulate execution
    let execution_start_time = Utc::now().timestamp_millis() as u64;
    // Simulate some work
    tokio::time::sleep(std::time::Duration::from_millis(
        100 + final_mana_cost as u64,
    ))
    .await; // Sleep proportional to cost
    let execution_end_time_dt = Utc::now();
    let execution_end_time = execution_end_time_dt.timestamp_millis() as u64;

    // Dummy result CID and resource usage
    let result_cid = Some(format!(
        "bafyresimulatedresult{}",
        mesh_job.job_id.as_str()
    ));
    let resource_usage = mesh_job.params.resources_required.iter().map(|(rt, amount)| (rt.clone(), *amount)).collect();

    let mut receipt = MeshExecutionReceipt {
        job_id: mesh_job.job_id.clone(),
        executor: mesh_job.originator_did.clone(),
        status: IcnJobStatus::Completed,
        result_data_cid: result_cid,
        logs_cid: None,
        resource_usage,
        execution_start_time,
        execution_end_time,
        execution_end_time_dt,
        signature: Vec::new(),
        coop_id: None,
        community_id: None,
        mana_cost: Some(final_mana_cost),
    };

    // Sign the receipt
    let receipt_bytes_for_signing = serde_cbor::to_vec(&receipt).unwrap_or_default();
    receipt.signature = local_keypair.sign(&receipt_bytes_for_signing).to_vec();

    info!(
        "Finished executing mesh job: {:?}, Mana cost: {}",
        receipt.job_id, final_mana_cost
    );
    Ok(receipt)
}
