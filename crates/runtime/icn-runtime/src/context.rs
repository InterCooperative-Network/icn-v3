use std::sync::Arc;
use icn_types::dag_store::SharedDagStore;

/// Runtime context for execution environments
///
/// Provides shared infrastructure and state needed across the runtime,
/// including access to the DAG store for anchoring and querying 
/// governance events and receipts.
#[derive(Clone)]
pub struct RuntimeContext {
    /// Shared DAG store for transaction and anchor operations
    pub dag_store: Arc<SharedDagStore>,
    
    /// Federation identifier
    pub federation_id: Option<String>,
    
    /// Executor identifier (node ID or DID)
    pub executor_id: Option<String>,
}

impl RuntimeContext {
    /// Create a new context with default values
    pub fn new() -> Self {
        Self {
            dag_store: Arc::new(SharedDagStore::new()),
            federation_id: None,
            executor_id: None,
        }
    }

    /// Create a new context with a specific DAG store
    pub fn with_dag_store(dag_store: Arc<SharedDagStore>) -> Self {
        Self {
            dag_store,
            federation_id: None,
            executor_id: None,
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
}

impl Default for RuntimeContext {
    fn default() -> Self {
        Self::new()
    }
} 