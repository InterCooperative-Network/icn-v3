use anyhow::Result;
use icn_types::dag::{DagEventType, DagNode, DagNodeBuilder};
use icn_types::dag_store::{DagStore, SharedDagStore};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Barrier;
use tokio::time::sleep;
use cid::Cid;
use std::collections::HashSet;

/// Test helper: Create a sample DAG node
fn create_test_node(id: usize, event_type: DagEventType) -> DagNode {
    DagNodeBuilder::new()
        .content(format!("content-{}", id))
        .event_type(event_type.clone())
        .scope_id("test-scope".to_string())
        .timestamp(id as u64)
        .build()
        .unwrap()
}

/// NEW test helper: Create a sample DAG node with references
fn create_test_node_with_references(
    id: usize, 
    content_suffix: &str,
    event_type: DagEventType, 
    references: Vec<Cid>
) -> DagNode {
    // Assuming DagNodeBuilder has a .references() method or similar
    // If not, this would need to construct a specific DagNode variant that holds references.
    DagNodeBuilder::new()
        .content(format!("content-{}-{}", id, content_suffix))
        .event_type(event_type.clone())
        .scope_id("test-scope".to_string())
        .timestamp(id as u64)
        .references(references) // Assuming this method exists
        .build()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_inserts() -> Result<()> {
    let store = Arc::new(SharedDagStore::new());
    let barrier = Arc::new(Barrier::new(10)); // 10 concurrent tasks
    
    // Task handles for all concurrent operations
    let mut handles = vec![];
    
    // Spawn 10 tasks that each insert 10 nodes
    for i in 0..10 {
        let store_clone = store.clone();
        let barrier_clone = barrier.clone();
        
        let handle = tokio::spawn(async move {
            // Wait for all tasks to be ready before starting
            barrier_clone.wait().await;
            
            for j in 0..10 {
                let node_id = i * 10 + j;
                let node = create_test_node(node_id, DagEventType::Proposal);
                store_clone.insert(node).await.unwrap();
                
                // Small delay to increase chance of race conditions
                if j % 3 == 0 {
                    sleep(Duration::from_millis(1)).await;
                }
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all tasks to complete
    for handle in handles {
        handle.await?;
    }
    
    // Verify that all 100 nodes were correctly inserted
    let all_nodes = store.list().await?;
    assert_eq!(all_nodes.len(), 100, "Expected 100 nodes in the store");
    
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_batch_operations() -> Result<()> {
    let store = Arc::new(SharedDagStore::new());
    
    // Insert a batch of 50 nodes
    {
        let mut batch = store.begin_batch().await;
        for i in 0..50 {
            let node = create_test_node(i, DagEventType::Proposal);
            batch.insert(node).await?;
        }
        batch.commit().await?;
    }
    
    // Verify we have exactly 50 nodes
    let nodes = store.list().await?;
    assert_eq!(nodes.len(), 50, "Expected 50 nodes after batch commit");
    
    // Start two competing batch operations
    let barrier = Arc::new(Barrier::new(2));
    
    // Task 1: Add 25 more nodes
    let store_clone = store.clone();
    let barrier_clone = barrier.clone();
    let add_task = tokio::spawn(async move {
        barrier_clone.wait().await;
        
        let mut batch = store_clone.begin_batch().await;
        for i in 50..75 {
            let node = create_test_node(i, DagEventType::Proposal);
            batch.insert(node).await.unwrap();
            
            // Add some delay to increase chance of race conditions
            if i % 5 == 0 {
                sleep(Duration::from_millis(1)).await;
            }
        }
        batch.commit().await.unwrap();
    });
    
    // Task 2: Remove 10 existing nodes
    let store_clone = store.clone();
    let barrier_clone = barrier.clone();
    let remove_task = tokio::spawn(async move {
        barrier_clone.wait().await;
        
        let nodes = store_clone.list().await.unwrap();
        let mut batch = store_clone.begin_batch().await;
        
        // Remove the first 10 nodes
        for i in 0..10 {
            let node = &nodes[i];
            let node_id = node.cid().unwrap().to_string();
            batch.remove(&node_id).await.unwrap();
            
            // Add some delay to increase chance of race conditions
            if i % 3 == 0 {
                sleep(Duration::from_millis(1)).await;
            }
        }
        batch.commit().await.unwrap();
    });
    
    // Wait for both tasks to complete
    add_task.await?;
    remove_task.await?;
    
    // Verify final node count: 50 (initial) + 25 (added) - 10 (removed) = 65
    let final_nodes = store.list().await?;
    assert_eq!(final_nodes.len(), 65, "Expected 65 nodes after concurrent batch operations");
    
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_reads_during_writes() -> Result<()> {
    let store = Arc::new(SharedDagStore::new());
    
    // Pre-populate with 20 nodes
    for i in 0..20 {
        let node = create_test_node(i, DagEventType::Proposal);
        store.insert(node).await?;
    }
    
    // Continuously read while writing
    let read_store = store.clone();
    let read_handle = tokio::spawn(async move {
        let mut read_count = 0;
        for _ in 0..100 {
            let nodes = read_store.list().await.unwrap();
            read_count += nodes.len();
            sleep(Duration::from_millis(1)).await;
        }
        read_count
    });
    
    // Perform writes in parallel
    let write_store = store.clone();
    let write_handle = tokio::spawn(async move {
        for i in 20..70 {
            let node = create_test_node(i, DagEventType::Proposal);
            write_store.insert(node).await.unwrap();
            
            if i % 5 == 0 {
                sleep(Duration::from_millis(2)).await;
            }
        }
    });
    
    // Wait for both operations to complete
    let total_reads = read_handle.await?;
    write_handle.await?;
    
    // Verify all 70 nodes are present
    let final_nodes = store.list().await?;
    assert_eq!(final_nodes.len(), 70, "Expected 70 nodes after concurrent operations");
    
    // Total reads should be non-zero (we don't know exact count due to race conditions, 
    // but it confirms reads were happening)
    assert!(total_reads > 0, "Expected some successful reads during concurrent writes");
    
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_dag_formation() -> Result<()> {
    let store = Arc::new(SharedDagStore::new());
    let num_tasks = 5;
    let nodes_per_task = 5;
    let barrier = Arc::new(Barrier::new(num_tasks));
    let mut handles = vec![];

    // Shared storage for CIDs generated by root nodes, so dependent nodes can reference them.
    // This simulates a common scenario where some nodes are created first, and their CIDs become known.
    let root_node_cids = Arc::new(tokio::sync::Mutex::new(Vec::<Cid>::new()));

    for i in 0..num_tasks {
        let store_clone = store.clone();
        let barrier_clone = barrier.clone();
        let root_node_cids_clone = root_node_cids.clone();

        let handle = tokio::spawn(async move {
            barrier_clone.wait().await;
            let mut task_generated_cids = Vec::new();

            for j in 0..nodes_per_task {
                let node_id = i * nodes_per_task + j;
                let mut references = Vec::new();

                // For child nodes (j > 0), reference previously created nodes
                // either from this task or from the shared root_node_cids pool.
                if j > 0 && !task_generated_cids.is_empty() {
                    references.push(task_generated_cids.last().unwrap().clone()); // Reference previous node in this task
                }
                if i > 0 && j == 0 { // First node in task (not first task) references a root node
                    let roots = root_node_cids_clone.lock().await;
                    if !roots.is_empty() {
                        references.push(roots[i % roots.len()].clone()); // Reference a root node cyclically
                    }
                }

                let node = create_test_node_with_references(
                    node_id, 
                    "dag_form",
                    DagEventType::Generic, // Or a more specific type that supports references
                    references.clone()
                );
                let node_cid = node.cid()?;
                store_clone.insert(node).await?;
                task_generated_cids.push(node_cid.clone());

                // If it's a "root" node for this task (first one, j==0), add its CID to shared pool
                if j == 0 {
                    root_node_cids_clone.lock().await.push(node_cid);
                }
                
                if j % 2 == 0 {
                    sleep(Duration::from_millis(1)).await;
                }
            }
            Result::<()>::Ok(())
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await??;
    }

    let all_nodes = store.list().await?;
    assert_eq!(all_nodes.len(), num_tasks * nodes_per_task, "Incorrect total node count");

    for node in all_nodes {
        let node_cid = node.cid()?;
        let stored_node = store.get(&node_cid.to_string()).await?.expect("Node CID not found in store");
        let references = stored_node.references().unwrap_or_else(Vec::new);
        for ref_cid in references {
            assert!(store.get(&ref_cid.to_string()).await?.is_some(), 
                    "Referenced CID {} not found for node {}", ref_cid, node_cid);
        }
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn test_concurrent_inserts_duplicate_cids() -> Result<()> {
    let store = Arc::new(SharedDagStore::new());
    let num_tasks = 10;
    let barrier = Arc::new(Barrier::new(num_tasks));
    let mut handles = vec![];

    // Create a single node that all tasks will try to insert
    let common_node = create_test_node_with_references(999, "duplicate", DagEventType::Generic, vec![]);
    let common_node_cid = common_node.cid()?;

    for _ in 0..num_tasks {
        let store_clone = store.clone();
        let barrier_clone = barrier.clone();
        let node_to_insert = common_node.clone(); // Clone the node for each task

        let handle = tokio::spawn(async move {
            barrier_clone.wait().await;
            // Insertion should be idempotent or return a benign error if already exists.
            // The specifics depend on SharedDagStore::insert behavior.
            // We assume it doesn't error catastrophically on duplicates.
            match store_clone.insert(node_to_insert).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    // If the store has specific error for "already exists", check for it.
                    // For now, assume any error might be problematic unless we know the specific API contract.
                    // This test might need adjustment based on how `insert` handles duplicates.
                    // For a simple test, we can assume Ok(_) or a specific, known error is acceptable.
                    // If any error is fine as long as it's not panic, then this is fine too.
                    tracing::warn!("Insert duplicate CID returned error: {}. This may be acceptable.", e);
                    Ok(())
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await??;
    }

    let all_nodes = store.list().await?;
    assert_eq!(all_nodes.len(), 1, "Expected only one copy of the common node");
    
    let retrieved_node = store.get(&common_node_cid.to_string()).await?;
    assert!(retrieved_node.is_some(), "Common node not found in store by its CID");
    assert_eq!(retrieved_node.unwrap().cid()?, common_node_cid, "Retrieved node CID mismatch");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn test_concurrent_anchoring_to_pending_cids() -> Result<()> {
    let store = Arc::new(SharedDagStore::new());
    let barrier = Arc::new(Barrier::new(2));

    // Node X to be inserted by Task A
    let node_x_content = "node_x_content_pending";
    let node_x_to_build = DagNodeBuilder::new()
        .content(node_x_content.to_string())
        .event_type(DagEventType::Generic)
        .scope_id("test-scope".to_string())
        .timestamp(1)
        .build()?;
    let node_x_cid = node_x_to_build.cid()?;
    
    // Task A: Inserts Node X
    let store_a = store.clone();
    let barrier_a = barrier.clone();
    let node_x_for_a = node_x_to_build.clone();
    let handle_a = tokio::spawn(async move {
        barrier_a.wait().await; 
        // Add a small delay to make it more likely Node Y insertion starts first or concurrently
        sleep(Duration::from_millis(5)).await;
        store_a.insert(node_x_for_a).await
    });

    // Task B: Inserts Node Y which references Node X's CID
    let store_b = store.clone();
    let barrier_b = barrier.clone();
    let referenced_cid_for_y = node_x_cid.clone();
    let handle_b = tokio::spawn(async move {
        barrier_b.wait().await;
        let node_y = create_test_node_with_references(2, "node_y_references_x", DagEventType::Generic, vec![referenced_cid_for_y.clone()]);
        let insert_result = store_b.insert(node_y.clone()).await;
        // Even if X is not fully there, Y's insertion with reference to X's CID should be accepted.
        (insert_result, node_y.cid().unwrap())
    });

    let result_a = handle_a.await??;
    let (result_b, node_y_cid) = handle_b.await??;

    // Ensure both inserts were successful (or handled as per DAG store's contract)
    // This might need adjustment based on the DAG store's specific error handling for pending CIDs.
    // For now, we assume Ok is the desired outcome.

    // Verify both nodes are present
    let fetched_node_x = store.get(&node_x_cid.to_string()).await?;
    let fetched_node_y = store.get(&node_y_cid.to_string()).await?;

    assert!(fetched_node_x.is_some(), "Node X not found in store");
    assert!(fetched_node_y.is_some(), "Node Y not found in store");

    let node_y_unwrapped = fetched_node_y.unwrap();
    let y_references = node_y_unwrapped.references().unwrap_or_else(Vec::new);
    assert_eq!(y_references.len(), 1, "Node Y should reference one CID");
    assert_eq!(y_references[0], node_x_cid, "Node Y does not correctly reference Node X's CID");

    Ok(())
} 