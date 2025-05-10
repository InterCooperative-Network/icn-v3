# DAG Store

The DAG Store is a core component of the InterCooperative Network (ICN) that provides thread-safe, transactional storage for DAG (Directed Acyclic Graph) nodes. This document explains its architecture, usage patterns, and integration with the runtime.

## Architecture

The DAG Store consists of several key components:

1. **DagStore Trait** - Defines the async interface for DAG storage operations.
2. **SharedDagStore** - An in-memory implementation using tokio's async-aware RwLock.
3. **DagStoreBatch** - Provides atomic multi-node operations through transactions.
4. **RuntimeContext** - Holds a reference to the SharedDagStore for use throughout the runtime.

![DAG Store Architecture](diagrams/dag_store.mmd)

## Key Features

- **Thread-safety** - Multiple components can safely access the DAG store concurrently.
- **Async API** - All operations are async, supporting non-blocking workflows.
- **Transactional Operations** - Changes can be staged and committed atomically.
- **Auto-rollback** - Transactions are automatically rolled back on drop if not committed.
- **Future-proof API** - The interface is designed to support persistent backends in the future.

## Usage Patterns

### Basic Operations

```rust
// Create a new store
let store = Arc::new(SharedDagStore::new());

// Insert a node
let node = DagNodeBuilder::new()
    .content("node content")
    .event_type(DagEventType::Proposal)
    .scope_id("test-scope")
    .timestamp(0)
    .build()?;
store.insert(node.clone()).await?;

// Retrieve a node
let node_id = node.cid()?.to_string();
let retrieved = store.get(&node_id).await?;

// List all nodes
let all_nodes = store.list().await?;
```

### Batch Operations

```rust
// Begin a transaction
let mut batch = store.begin_batch().await;

// Stage multiple operations
batch.insert(node1).await?;
batch.insert(node2).await?;
batch.remove(&node3_id).await?;

// Commit all changes atomically
batch.commit().await?;

// Or rollback on error
if error_condition {
    batch.rollback();
}
```

### Runtime Integration

The DAG store is integrated into the runtime through the `RuntimeContext`:

```rust
// Access from Runtime
let dag_store = runtime.dag_store();

// Or directly from context
let dag_store = runtime.context().dag_store.clone();
```

## Thread Safety and Concurrency

The `SharedDagStore` uses tokio's `RwLock` to ensure thread safety:

- Multiple readers can access the store simultaneously (non-blocking)
- Writers acquire exclusive access
- Batch operations stage changes privately, then acquire the lock only once at commit time

This design optimizes for read-heavy workloads while still providing safe write operations.

## Deterministic Replay

One of the key requirements for the DAG store is deterministic replay capability. When nodes are stored and replayed in the same order, they must produce identical state:

```rust
// Original DAG
let events = populate_dag(store1).await?;

// Replay events in the same order
let replay_store = Arc::new(SharedDagStore::new());
for event_id in &events {
    if let Some(node) = store1.get(event_id).await? {
        replay_store.insert(node).await?;
    }
}

// Verify identical state
assert_eq!(store1.get_state_hash().await?, replay_store.get_state_hash().await?);
```

## Future Extensions

The current implementation is in-memory, but the API is designed to support persistent backends:

1. **RocksDB Backend** - For single-node deployments requiring persistence.
2. **PostgreSQL Backend** - For cluster deployments with shared storage.
3. **Distributed Backend** - For fully decentralized deployments.

These can be implemented by providing alternative implementations of the `DagStore` trait. 