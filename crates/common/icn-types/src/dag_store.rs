use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::dag::DagNode;
use crate::error::DagError;

/// Trait for DAG store operations.
#[async_trait::async_trait]
pub trait DagStore: Send + Sync {
    /// Retrieve a DAG node by ID
    async fn get(&self, id: &str) -> Result<Option<DagNode>, DagError>;

    /// Insert a DAG node
    async fn insert(&self, node: DagNode) -> Result<(), DagError>;

    /// Remove a DAG node by ID
    async fn remove(&self, id: &str) -> Result<(), DagError>;

    /// List all DAG nodes
    async fn list(&self) -> Result<Vec<DagNode>, DagError>;

    /// Begin a write batch for atomic multi-node operations.
    async fn begin_batch(&self) -> DagStoreBatch;
}

/// In-memory, async, transactional DAG store.
/// 
/// `SharedDagStore` provides an in-memory implementation of the `DagStore` trait
/// with full support for concurrent access through tokio's async-aware RwLock.
///
/// # Features
/// - Thread-safe concurrent access to DAG nodes
/// - Transactional batch operations via `DagStoreBatch`
/// - Optimized for read-heavy workloads (multiple readers can access simultaneously)
///
/// # Example
/// ```
/// use icn_types::dag_store::{DagStore, SharedDagStore};
/// use icn_types::dag::{DagNode, DagNodeBuilder, DagEventType};
///
/// #[tokio::main]
/// async fn main() {
///     let store = SharedDagStore::new();
///     
///     // Create a node
///     let node = DagNodeBuilder::new()
///         .content("test content".to_string())
///         .event_type(DagEventType::Genesis)
///         .scope_id("test-scope".to_string())
///         .timestamp(0)
///         .build()
///         .unwrap();
///     
///     // Store the node
///     let node_id = node.cid().unwrap().to_string();
///     store.insert(node.clone()).await.unwrap();
///     
///     // Retrieve it
///     let retrieved = store.get(&node_id).await.unwrap();
///     assert_eq!(retrieved, Some(node));
/// }
/// ```
#[derive(Clone, Default)]
pub struct SharedDagStore {
    // HashMap key is the CID of the DAG node as string
    inner: Arc<RwLock<HashMap<String, DagNode>>>,
}

impl SharedDagStore {
    /// Create a new empty SharedDagStore
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl DagStore for SharedDagStore {
    async fn get(&self, id: &str) -> Result<Option<DagNode>, DagError> {
        let map = self.inner.read().await;
        Ok(map.get(id).cloned())
    }

    async fn insert(&self, node: DagNode) -> Result<(), DagError> {
        let cid = node.cid()?;
        let id = cid.to_string();
        let mut map = self.inner.write().await;
        map.insert(id, node);
        Ok(())
    }

    async fn remove(&self, id: &str) -> Result<(), DagError> {
        let mut map = self.inner.write().await;
        map.remove(id);
        Ok(())
    }

    async fn list(&self) -> Result<Vec<DagNode>, DagError> {
        let map = self.inner.read().await;
        Ok(map.values().cloned().collect())
    }

    async fn begin_batch(&self) -> DagStoreBatch {
        DagStoreBatch::new(self.clone())
    }
}

/// Write-batch for atomic multi-node operations.
///
/// `DagStoreBatch` allows multiple DAG operations to be staged and committed
/// atomically. Changes are only visible after calling `commit()`.
///
/// # Example
/// ```
/// use icn_types::dag_store::{DagStore, SharedDagStore};
/// use icn_types::dag::{DagNode, DagNodeBuilder, DagEventType};
///
/// #[tokio::main]
/// async fn main() {
///     let store = SharedDagStore::new();
///     
///     // Create two nodes
///     let node1 = DagNodeBuilder::new()
///         .content("node 1".to_string())
///         .event_type(DagEventType::Genesis)
///         .scope_id("test-scope".to_string())
///         .timestamp(0)
///         .build()
///         .unwrap();
///     
///     let node2 = DagNodeBuilder::new()
///         .content("node 2".to_string())
///         .event_type(DagEventType::Proposal)
///         .scope_id("test-scope".to_string())
///         .timestamp(1)
///         .build()
///         .unwrap();
///     
///     // Begin a batch operation
///     let mut batch = store.begin_batch().await;
///     
///     // Stage operations
///     batch.insert(node1).await.unwrap();
///     batch.insert(node2).await.unwrap();
///     
///     // Commit all changes atomically
///     batch.commit().await.unwrap();
/// }
/// ```
pub struct DagStoreBatch {
    store: SharedDagStore,
    // None = remove, Some = insert
    staged: HashMap<String, Option<DagNode>>, 
    committed: bool,
}

impl DagStoreBatch {
    fn new(store: SharedDagStore) -> Self {
        Self { 
            store, 
            staged: HashMap::new(), 
            committed: false 
        }
    }

    /// Stage a node insertion in the batch
    pub async fn insert(&mut self, node: DagNode) -> Result<(), DagError> {
        let cid = node.cid()?;
        let id = cid.to_string();
        self.staged.insert(id, Some(node));
        Ok(())
    }

    /// Stage a node removal in the batch
    pub async fn remove(&mut self, id: &str) -> Result<(), DagError> {
        self.staged.insert(id.to_string(), None);
        Ok(())
    }

    /// Atomically commit all staged changes
    pub async fn commit(mut self) -> Result<(), DagError> {
        let mut map = self.store.inner.write().await;
        for (id, op) in self.staged.drain() {
            match op {
                Some(node) => { map.insert(id, node); }
                None => { map.remove(&id); }
            }
        }
        self.committed = true;
        Ok(())
    }

    /// Discard all staged changes
    pub fn rollback(mut self) {
        self.staged.clear();
        self.committed = true;
    }
}

impl Drop for DagStoreBatch {
    fn drop(&mut self) {
        // If not committed or rolled back, auto-rollback on drop
        if !self.committed {
            self.staged.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::{DagEventType, DagNodeBuilder};
    use tokio::task;

    #[tokio::test]
    async fn test_basic_crud() {
        let store = SharedDagStore::new();
        
        // Create a test node
        let node = DagNodeBuilder::new()
            .content("test content".to_string())
            .event_type(DagEventType::Genesis)
            .scope_id("test-scope".to_string())
            .timestamp(0)
            .build()
            .unwrap();
        
        let node_id = node.cid().unwrap().to_string();
        
        // Insert
        store.insert(node.clone()).await.unwrap();
        assert_eq!(store.get(&node_id).await.unwrap(), Some(node.clone()));
        
        // Remove
        store.remove(&node_id).await.unwrap();
        assert_eq!(store.get(&node_id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_concurrent_reads_and_writes() {
        let store = SharedDagStore::new();
        
        // Create a test node
        let node = DagNodeBuilder::new()
            .content("concurrent test".to_string())
            .event_type(DagEventType::Genesis)
            .scope_id("test-scope".to_string())
            .timestamp(0)
            .build()
            .unwrap();
        let node_id = node.cid().unwrap().to_string();
        store.insert(node.clone()).await.unwrap();

        let store_clone = store.clone();
        let read_task = task::spawn(async move {
            for _ in 0..10 {
                let _ = store_clone.get(&node_id).await.unwrap();
                tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
            }
        });

        let store_clone2 = store.clone();
        let write_task = task::spawn(async move {
            for i in 1..=5 {
                let updated_node = DagNodeBuilder::new()
                    .content(format!("updated content {}", i))
                    .event_type(DagEventType::Proposal)
                    .scope_id("test-scope".to_string())
                    .timestamp(i as u64)
                    .build()
                    .unwrap();
                store_clone2.insert(updated_node).await.unwrap(); // This will use the new node's CID as key
                tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
            }
        });

        read_task.await.unwrap();
        write_task.await.unwrap();

        // Verify final state, e.g. count or specific node content if CIDs are tracked
        // For this simple test, we just ensure tasks completed.
        // A more robust test would check if the specific nodes are present/absent as expected.
    }
    
    #[tokio::test]
    async fn test_batch_commit_and_rollback() {
        let store = SharedDagStore::new();

        let node1 = DagNodeBuilder::new().content("node1".into()).event_type(DagEventType::Proposal).scope_id("s1".into()).timestamp(1).build().unwrap();
        let node1_id = node1.cid().unwrap().to_string();
        let node2 = DagNodeBuilder::new().content("node2".into()).event_type(DagEventType::Proposal).scope_id("s2".into()).timestamp(2).build().unwrap();
        let node2_id = node2.cid().unwrap().to_string();
        let node3 = DagNodeBuilder::new().content("node3".into()).event_type(DagEventType::Proposal).scope_id("s3".into()).timestamp(3).build().unwrap();
        let node3_id = node3.cid().unwrap().to_string();

        // Test commit
        let mut batch = store.begin_batch().await;
        batch.insert(node1.clone()).await.unwrap();
        batch.insert(node2.clone()).await.unwrap();
        batch.commit().await.unwrap();

        assert!(store.get(&node1_id).await.unwrap().is_some());
        assert!(store.get(&node2_id).await.unwrap().is_some());

        // Test rollback by dropping
        {
            let mut batch_rollback = store.begin_batch().await;
            batch_rollback.insert(node3.clone()).await.unwrap();
            // batch_rollback is dropped here, triggering rollback
        }
        assert!(store.get(&node3_id).await.unwrap().is_none());

        // Test explicit rollback
        let mut batch_explicit_rollback = store.begin_batch().await;
        let node4 = DagNodeBuilder::new().content("node4".into()).event_type(DagEventType::Proposal).scope_id("s4".into()).timestamp(4).build().unwrap();
        let node4_id = node4.cid().unwrap().to_string();
        batch_explicit_rollback.insert(node4.clone()).await.unwrap();
        batch_explicit_rollback.rollback(); // Explicitly rollback
        assert!(store.get(&node4_id).await.unwrap().is_none());
        
        // Test remove in batch
        let mut batch_remove = store.begin_batch().await;
        batch_remove.remove(&node1_id).await.unwrap();
        batch_remove.commit().await.unwrap();
        assert!(store.get(&node1_id).await.unwrap().is_none());
        assert!(store.get(&node2_id).await.unwrap().is_some()); // node2 should still be there
    }
} 