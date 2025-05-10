use std::sync::Arc;
use icn_types::dag_store::SharedDagStore;
use icn_identity::TrustValidator;
use icn_economics::{Economics, ResourceAuthorizationPolicy, ResourceType};
use std::collections::HashMap;
use tokio::sync::RwLock;

/// Runtime context for execution environments
///
/// Provides shared infrastructure and state needed across the runtime,
/// including access to the DAG store for anchoring and querying 
/// governance events and receipts, and the TrustValidator for verifying
/// trust bundles.
#[derive(Clone)]
pub struct RuntimeContext {
    /// Shared DAG store for transaction and anchor operations
    pub dag_store: Arc<SharedDagStore>,
    
    /// Federation identifier
    pub federation_id: Option<String>,
    
    /// Executor identifier (node ID or DID)
    pub executor_id: Option<String>,
    
    /// Trust validator for verifying trust bundles
    pub trust_validator: Option<Arc<TrustValidator>>,

    /// Economics engine for resource management
    pub economics: Arc<Economics>,

    /// Resource usage ledger
    pub resource_ledger: Arc<RwLock<HashMap<ResourceType, u64>>>,
}

impl RuntimeContext {
    /// Create a new context with default values
    pub fn new() -> Self {
        Self {
            dag_store: Arc::new(SharedDagStore::new()),
            federation_id: None,
            executor_id: None,
            trust_validator: None,
            economics: Arc::new(Economics::new(ResourceAuthorizationPolicy::default())),
            resource_ledger: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new context with a specific DAG store
    pub fn with_dag_store(dag_store: Arc<SharedDagStore>) -> Self {
        Self {
            dag_store,
            federation_id: None,
            executor_id: None,
            trust_validator: None,
            economics: Arc::new(Economics::new(ResourceAuthorizationPolicy::default())),
            resource_ledger: Arc::new(RwLock::new(HashMap::new())),
        }
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

    /// Return a builder for this context
    pub fn builder() -> RuntimeContextBuilder {
        RuntimeContextBuilder::new()
    }
}

impl Default for RuntimeContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder pattern for RuntimeContext
pub struct RuntimeContextBuilder {
    dag_store: Option<Arc<SharedDagStore>>,
    federation_id: Option<String>,
    executor_id: Option<String>,
    trust_validator: Option<Arc<TrustValidator>>,
    economics: Option<Arc<Economics>>,
}

impl RuntimeContextBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            dag_store: None,
            federation_id: None,
            executor_id: None,
            trust_validator: None,
            economics: None,
        }
    }

    /// Set the DAG store
    pub fn with_dag_store(mut self, dag_store: Arc<SharedDagStore>) -> Self {
        self.dag_store = Some(dag_store);
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

    /// Build the RuntimeContext
    pub fn build(self) -> RuntimeContext {
        let dag_store = self.dag_store.unwrap_or_else(|| Arc::new(SharedDagStore::new()));
        let economics = self.economics.unwrap_or_else(|| {
            Arc::new(Economics::new(ResourceAuthorizationPolicy::default()))
        });
        let resource_ledger = Arc::new(RwLock::new(HashMap::new()));

        RuntimeContext {
            dag_store,
            federation_id: self.federation_id,
            executor_id: self.executor_id,
            trust_validator: self.trust_validator,
            economics,
            resource_ledger,
        }
    }
} 