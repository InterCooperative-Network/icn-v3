// use crate::config::RuntimeConfig; // Removed unused import
// use icn_core_vm::{HostContext, ResourceLimits}; // Removed HostContext, ResourceLimits. If VmType is used, it's on a different line or this import is now empty.
use icn_identity::{KeyPair, TrustValidator}; // Removed Did, KeyPair as IcnKeyPair, TrustBundle
// use icn_metrics::runtime::RuntimeMetrics;
// use icn_reputation_integration::{HttpReputationUpdater, ReputationUpdater}; // Removed as per clippy
// use icn_mesh_protocol::MeshJobServiceConfig; // Removed as per clippy (grep showed only import line)
use icn_economics::{Economics, LedgerKey, mana::ManaManager, ResourceAuthorizationPolicy}; // ResourceType removed
use icn_economics::mana::{InMemoryManaLedger, ManaLedger, ManaRegenerator};
use icn_identity::IdentityIndex;
use icn_types::dag_store::SharedDagStore;
use icn_types::mesh::MeshJob;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::RwLock;

/// High-level execution state of the currently running job / stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionStatus {
    Running,
    Completed,
    Failed,
}

/// Runtime context for execution environments
///
/// Provides shared infrastructure and state needed across the runtime,
/// including access to the DAG store for anchoring and querying
/// governance events and receipts, and the TrustValidator for verifying
/// trust bundles.
#[derive(Clone)]
pub struct RuntimeContext<L: ManaLedger + Send + Sync + 'static = InMemoryManaLedger> {
    /// Shared DAG store for transaction and anchor operations
    pub dag_store: Arc<SharedDagStore>,

    /// Shared DAG store for mesh receipts
    pub receipt_store: Arc<SharedDagStore>,

    /// Federation identifier
    pub federation_id: Option<String>,

    /// Executor identifier (node ID or DID)
    pub executor_id: Option<String>,

    /// Trust validator for verifying trust bundles
    pub trust_validator: Option<Arc<TrustValidator>>,

    /// Economics engine for resource management
    pub economics: Arc<Economics>,

    /// Resource usage ledger - maps (DID, ResourceType) to amount
    pub resource_ledger: Arc<RwLock<HashMap<LedgerKey, u64>>>,

    /// Queue for mesh jobs submitted via host_submit_mesh_job awaiting P2P dispatch
    pub pending_mesh_jobs: Arc<Mutex<VecDeque<MeshJob>>>,

    /// Regenerating execution resource pools ("mana") by DID/org
    pub mana_manager: Arc<Mutex<ManaManager>>,

    /// Mana regenerator
    pub mana_regenerator: Option<Arc<ManaRegenerator<L>>>,

    /// Simple FIFO queue of raw interactive input messages pushed by the host.
    pub interactive_input_queue: Arc<Mutex<VecDeque<Vec<u8>>>>,

    /// Current high-level execution status.
    pub execution_status: ExecutionStatus,

    /// Optional identity index for DID -> org lookups
    pub identity_index: Option<Arc<IdentityIndex>>,

    /// Optional identity for the runtime
    identity: Option<KeyPair>,

    /// Optional reputation service URL
    reputation_service_url: Option<String>,

    /// Optional mesh job service URL
    mesh_job_service_url: Option<String>,
}

impl<L: ManaLedger + Send + Sync + 'static> RuntimeContext<L> {
    /// Create a new context with default values
    pub fn new() -> Self {
        Self {
            dag_store: Arc::new(SharedDagStore::new()),
            receipt_store: Arc::new(SharedDagStore::new()),
            federation_id: None,
            executor_id: None,
            trust_validator: None,
            economics: Arc::new(Economics::new(ResourceAuthorizationPolicy::default())),
            resource_ledger: Arc::new(RwLock::new(HashMap::new())),
            pending_mesh_jobs: Arc::new(Mutex::new(VecDeque::new())),
            mana_manager: Arc::new(Mutex::new(ManaManager::new())),
            mana_regenerator: None,
            interactive_input_queue: Arc::new(Mutex::new(VecDeque::new())),
            execution_status: ExecutionStatus::Running,
            identity_index: None,
            identity: None,
            reputation_service_url: None,
            mesh_job_service_url: None,
        }
    }

    /// Create a new context with a specific DAG store
    pub fn with_dag_store(dag_store: Arc<SharedDagStore>) -> Self {
        Self {
            dag_store,
            receipt_store: Arc::new(SharedDagStore::new()),
            federation_id: None,
            executor_id: None,
            trust_validator: None,
            economics: Arc::new(Economics::new(ResourceAuthorizationPolicy::default())),
            resource_ledger: Arc::new(RwLock::new(HashMap::new())),
            pending_mesh_jobs: Arc::new(Mutex::new(VecDeque::new())),
            mana_manager: Arc::new(Mutex::new(ManaManager::new())),
            mana_regenerator: None,
            interactive_input_queue: Arc::new(Mutex::new(VecDeque::new())),
            execution_status: ExecutionStatus::Running,
            identity_index: None,
            identity: None,
            reputation_service_url: None,
            mesh_job_service_url: None,
        }
    }

    /// Set the receipt store
    pub fn with_receipt_store(mut self, receipt_store: Arc<SharedDagStore>) -> Self {
        self.receipt_store = receipt_store;
        self
    }

    /// Set the federation ID
    pub fn with_federation_id(mut self, federation_id: impl Into<String>) -> Self {
        self.federation_id = Some(federation_id.into());
        self
    }

    /// Set the executor ID
    pub fn with_executor_id(mut self, executor_id: impl Into<String>) -> Self {
        self.executor_id = Some(executor_id.into());
        self
    }

    /// Set the trust validator
    pub fn with_trust_validator(mut self, trust_validator: Arc<TrustValidator>) -> Self {
        self.trust_validator = Some(trust_validator);
        self
    }

    /// Set the economics engine
    pub fn with_economics(mut self, economics: Arc<Economics>) -> Self {
        self.economics = economics;
        self
    }

    /// Get a reference to the trust validator, if present
    pub fn trust_validator(&self) -> Option<&Arc<TrustValidator>> {
        self.trust_validator.as_ref()
    }

    /// Set the identity index
    pub fn with_identity_index(mut self, index: Arc<IdentityIndex>) -> Self {
        self.identity_index = Some(index);
        self
    }

    /// Return a builder for this context
    pub fn builder() -> RuntimeContextBuilder<L> {
        RuntimeContextBuilder::new()
    }

    /// Update the execution status atomically.
    pub fn update_status(&mut self, status: ExecutionStatus) {
        self.execution_status = status;
    }

    pub fn dag_store(&self) -> Arc<SharedDagStore> {
        self.dag_store.clone()
    }

    pub fn identity(&self) -> Option<&KeyPair> {
        self.identity.as_ref()
    }

    pub fn reputation_service_url(&self) -> Option<&String> {
        self.reputation_service_url.as_ref()
    }

    pub fn mesh_job_service_url(&self) -> Option<&String> {
        self.mesh_job_service_url.as_ref()
    }
}

impl<L: ManaLedger + Send + Sync + 'static> Default for RuntimeContext<L> {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder pattern for RuntimeContext
pub struct RuntimeContextBuilder<L: ManaLedger + Send + Sync + 'static = InMemoryManaLedger> {
    dag_store: Option<Arc<SharedDagStore>>,
    receipt_store: Option<Arc<SharedDagStore>>,
    federation_id: Option<String>,
    executor_id: Option<String>,
    trust_validator: Option<Arc<TrustValidator>>,
    economics: Option<Arc<Economics>>,
    identity_index: Option<Arc<IdentityIndex>>,
    identity: Option<KeyPair>,
    reputation_service_url: Option<String>,
    mesh_job_service_url: Option<String>,
    mana_regenerator: Option<Arc<ManaRegenerator<L>>>,
}

impl<L: ManaLedger + Send + Sync + 'static> RuntimeContextBuilder<L> {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            dag_store: None,
            receipt_store: None,
            federation_id: None,
            executor_id: None,
            trust_validator: None,
            economics: None,
            identity_index: None,
            identity: None,
            reputation_service_url: None,
            mesh_job_service_url: None,
            mana_regenerator: None,
        }
    }

    /// Set the DAG store
    pub fn with_dag_store(mut self, dag_store: Arc<SharedDagStore>) -> Self {
        self.dag_store = Some(dag_store);
        self
    }

    /// Set the receipt store
    pub fn with_receipt_store(mut self, receipt_store: Arc<SharedDagStore>) -> Self {
        self.receipt_store = Some(receipt_store);
        self
    }

    /// Set the federation ID
    pub fn with_federation_id(mut self, federation_id: impl Into<String>) -> Self {
        self.federation_id = Some(federation_id.into());
        self
    }

    /// Set the executor ID
    pub fn with_executor_id(mut self, executor_id: impl Into<String>) -> Self {
        self.executor_id = Some(executor_id.into());
        self
    }

    /// Set the trust validator
    pub fn with_trust_validator(mut self, trust_validator: Arc<TrustValidator>) -> Self {
        self.trust_validator = Some(trust_validator);
        self
    }

    /// Set the economics engine
    pub fn with_economics(mut self, economics: Arc<Economics>) -> Self {
        self.economics = Some(economics);
        self
    }

    /// Set the identity index
    pub fn with_identity_index(mut self, index: Arc<IdentityIndex>) -> Self {
        self.identity_index = Some(index);
        self
    }

    /// Set the identity for the runtime
    pub fn with_identity(mut self, identity: KeyPair) -> Self {
        self.identity = Some(identity);
        self
    }

    /// Set the reputation service URL for automatic reputation updates
    pub fn with_reputation_service(mut self, url: String) -> Self {
        self.reputation_service_url = Some(url);
        self
    }

    /// Set the mesh job service URL
    pub fn with_mesh_job_service_url(mut self, url: String) -> Self {
        self.mesh_job_service_url = Some(url);
        self
    }

    /// Set the mana regenerator
    pub fn with_mana_regenerator(mut self, regen: Arc<ManaRegenerator<L>>) -> Self {
        self.mana_regenerator = Some(regen);
        self
    }

    /// Build the RuntimeContext
    pub fn build(self) -> RuntimeContext<L> {
        let default_context = RuntimeContext::<L>::new();
        RuntimeContext {
            dag_store: self.dag_store.unwrap_or(default_context.dag_store),
            receipt_store: self.receipt_store.unwrap_or(default_context.receipt_store),
            federation_id: self.federation_id,
            executor_id: self.executor_id,
            trust_validator: self.trust_validator,
            economics: self.economics.unwrap_or(default_context.economics),
            resource_ledger: default_context.resource_ledger,
            pending_mesh_jobs: default_context.pending_mesh_jobs,
            mana_manager: default_context.mana_manager,
            mana_regenerator: self.mana_regenerator,
            interactive_input_queue: default_context.interactive_input_queue,
            execution_status: default_context.execution_status,
            identity_index: self.identity_index,
            identity: self.identity,
            reputation_service_url: self.reputation_service_url,
            mesh_job_service_url: self.mesh_job_service_url,
        }
    }
}
