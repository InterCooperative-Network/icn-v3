// use crate::config::RuntimeConfig; // Removed unused import
// use icn_core_vm::{HostContext, ResourceLimits}; // Removed HostContext, ResourceLimits. If VmType is used, it's on a different line or this import is now empty.
use icn_identity::{KeyPair, TrustValidator}; // Removed Did, KeyPair as IcnKeyPair, TrustBundle
// use icn_metrics::runtime::RuntimeMetrics;
// use icn_reputation_integration::{HttpReputationUpdater, ReputationUpdater}; // Removed as per clippy
// use icn_mesh_protocol::MeshJobServiceConfig; // Removed as per clippy (grep showed only import line)
use icn_economics::{Economics, LedgerKey, mana::ManaManager, ResourceAuthorizationPolicy, ResourcePolicyEnforcer, ManaRepositoryAdapter}; // ResourceType removed
use icn_economics::mana::{InMemoryManaLedger, ManaLedger, ManaRegenerator};
use icn_identity::IdentityIndex;
use icn_types::dag_store::SharedDagStore;
use icn_types::mesh::MeshJob;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::RwLock;
use crate::reputation_integration::ReputationScoringConfig;
// use crate::RuntimeStorage; // Removed unused import
use std::time::Duration;

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

    /// Policy enforcer for the new economics system
    pub policy_enforcer: Arc<ResourcePolicyEnforcer>,

    /// Mana repository for the new economics system
    pub mana_repository: Arc<ManaRepositoryAdapter<L>>,

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

    pub reputation_scoring_config: ReputationScoringConfig,
    pub mana_tick_interval: Option<Duration>,
}

// General impl block for accessors and methods not requiring L: Default
impl<L: ManaLedger + Send + Sync + 'static> RuntimeContext<L> {
    /// Get a reference to the trust validator, if present
    pub fn trust_validator(&self) -> Option<&Arc<TrustValidator>> {
        self.trust_validator.as_ref()
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

    /// Accessors for new components
    pub fn policy_enforcer(&self) -> Arc<ResourcePolicyEnforcer> {
        self.policy_enforcer.clone()
    }

    pub fn mana_repository(&self) -> Arc<ManaRepositoryAdapter<L>> {
        self.mana_repository.clone()
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
    
    /// Set the identity index
    pub fn with_identity_index(mut self, index: Arc<IdentityIndex>) -> Self {
        self.identity_index = Some(index);
        self
    }
}

impl<L: ManaLedger + Send + Sync + 'static + Default> RuntimeContext<L> {
    /// Create a new context with default values
    pub fn new() -> Self {
        let default_ledger = Arc::new(L::default());
        let mana_repo_adapter = Arc::new(ManaRepositoryAdapter::new(default_ledger.clone()));
        let boxed_mana_repo_adapter_for_enforcer = Box::new(ManaRepositoryAdapter::new(default_ledger));
        
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
            policy_enforcer: Arc::new(ResourcePolicyEnforcer::new(boxed_mana_repo_adapter_for_enforcer)),
            mana_repository: mana_repo_adapter,
            interactive_input_queue: Arc::new(Mutex::new(VecDeque::new())),
            execution_status: ExecutionStatus::Running,
            identity_index: None,
            identity: None,
            reputation_service_url: None,
            mesh_job_service_url: None,
            reputation_scoring_config: ReputationScoringConfig::default(),
            mana_tick_interval: None,
        }
    }

    /// Create a new context with a specific DAG store
    pub fn with_dag_store(dag_store: Arc<SharedDagStore>) -> Self {
        let default_ledger = Arc::new(L::default());
        let mana_repo_adapter = Arc::new(ManaRepositoryAdapter::new(default_ledger.clone()));
        let boxed_mana_repo_adapter_for_enforcer = Box::new(ManaRepositoryAdapter::new(default_ledger));
        
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
            policy_enforcer: Arc::new(ResourcePolicyEnforcer::new(boxed_mana_repo_adapter_for_enforcer)),
            mana_repository: mana_repo_adapter,
            interactive_input_queue: Arc::new(Mutex::new(VecDeque::new())),
            execution_status: ExecutionStatus::Running,
            identity_index: None,
            identity: None,
            reputation_service_url: None,
            mesh_job_service_url: None,
            reputation_scoring_config: ReputationScoringConfig::default(),
            mana_tick_interval: None,
        }
    }

    /// Return a builder for this context
    pub fn builder() -> RuntimeContextBuilder<L> {
        RuntimeContextBuilder::new()
    }
}

impl<L: ManaLedger + Send + Sync + 'static + Default> Default for RuntimeContext<L> {
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
    reputation_scoring_config: Option<ReputationScoringConfig>,
    mana_tick_interval: Option<Duration>,
    policy_enforcer: Option<Arc<ResourcePolicyEnforcer>>,
    mana_repository: Option<Arc<ManaRepositoryAdapter<L>>>,
}

impl<L: ManaLedger + Send + Sync + 'static + Default> RuntimeContextBuilder<L> {
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
            reputation_scoring_config: None,
            mana_tick_interval: None,
            policy_enforcer: None,
            mana_repository: None,
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

    /// Set the reputation scoring config
    pub fn with_reputation_scoring_config(mut self, config: ReputationScoringConfig) -> Self {
        self.reputation_scoring_config = Some(config);
        self
    }

    /// Set the mana tick interval
    pub fn with_mana_tick_interval(mut self, interval: Duration) -> Self {
        self.mana_tick_interval = Some(interval);
        self
    }

    pub fn with_policy_enforcer(mut self, enforcer: Arc<ResourcePolicyEnforcer>) -> Self {
        self.policy_enforcer = Some(enforcer);
        self
    }

    pub fn with_mana_repository(mut self, repository: Arc<ManaRepositoryAdapter<L>>) -> Self {
        self.mana_repository = Some(repository);
        self
    }

    /// Build the RuntimeContext
    pub fn build(self) -> RuntimeContext<L> {
        let default_ledger_for_builder = Arc::new(L::default());
        let default_mana_repo_adapter_for_builder = Arc::new(ManaRepositoryAdapter::new(default_ledger_for_builder.clone()));
        let default_boxed_repo_for_enforcer = Box::new(ManaRepositoryAdapter::new(default_ledger_for_builder));
        let default_policy_enforcer_for_builder = Arc::new(ResourcePolicyEnforcer::new(default_boxed_repo_for_enforcer));

        RuntimeContext {
            dag_store: self.dag_store.unwrap_or_else(|| Arc::new(SharedDagStore::new())),
            receipt_store: self.receipt_store.unwrap_or_else(|| Arc::new(SharedDagStore::new())),
            federation_id: self.federation_id,
            executor_id: self.executor_id,
            trust_validator: self.trust_validator,
            economics: self.economics.unwrap_or_else(|| Arc::new(Economics::new(ResourceAuthorizationPolicy::default()))),
            resource_ledger: Arc::new(RwLock::new(HashMap::new())),
            pending_mesh_jobs: Arc::new(Mutex::new(VecDeque::new())),
            mana_manager: Arc::new(Mutex::new(ManaManager::new())),
            mana_regenerator: self.mana_regenerator,
            policy_enforcer: self.policy_enforcer.unwrap_or(default_policy_enforcer_for_builder),
            mana_repository: self.mana_repository.unwrap_or(default_mana_repo_adapter_for_builder),
            interactive_input_queue: Arc::new(Mutex::new(VecDeque::new())),
            execution_status: ExecutionStatus::Running,
            identity_index: self.identity_index,
            identity: self.identity,
            reputation_service_url: self.reputation_service_url,
            mesh_job_service_url: self.mesh_job_service_url,
            reputation_scoring_config: self.reputation_scoring_config.unwrap_or_default(),
            mana_tick_interval: self.mana_tick_interval,
        }
    }
}

impl<L: ManaLedger + Send + Sync + Default + 'static> Default for RuntimeContextBuilder<L> {
    fn default() -> Self {
        Self::new()
    }
}
