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

/// MODIFIED test helper: Create a sample DAG node with a single parent reference
fn create_test_node_with_parent(
    id: usize,
    content_suffix: &str,
    event_type: DagEventType,
    parent_cid: Option<Cid>, // Changed from Vec<Cid> to Option<Cid>
) -> DagNode {
    let mut builder = DagNodeBuilder::new()
        .content(format!("content-{}-{}", id, content_suffix))
        .event_type(event_type.clone())
        .scope_id("test-scope".to_string())
        .timestamp(id as u64);

    if let Some(p_cid) = parent_cid {
        builder = builder.parent(p_cid); // Use .parent()
    }
    builder.build().unwrap()
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

#[tokio::test]
async fn test_concurrent_dag_formation() {
    let store = SharedDagStore::new();
    let num_tasks = 5;
    let nodes_per_task = 3;
    let barrier = Arc::new(Barrier::new(num_tasks));
    let mut handles = vec![];

    for i in 0..num_tasks {
        let store_clone = store.clone();
        let barrier_clone = barrier.clone();
        let handle = tokio::spawn(async move {
            let mut task_generated_cids: Vec<Cid> = Vec::new();
            let mut parent_cid_for_next_node: Option<Cid> = None;

            barrier_clone.wait().await; // Synchronize start

            for j in 0..nodes_per_task {
                let node_content_suffix = format!("task{}-node{}", i, j);
                let node = create_test_node_with_parent(
                    i * nodes_per_task + j, // Unique ID for the node
                    &node_content_suffix,
                    DagEventType::Anchor, // Corrected event type
                    parent_cid_for_next_node.clone(), // Use cloned parent_cid for node creation
                );
                // Get the CID before attempting to insert
                let current_node_cid = node.cid().expect("Failed to calculate node CID before insert");
                
                // Insert the node
                match store_clone.insert(node.clone()).await { // node.clone() is important if used after insert
                    Ok(()) => { // insert returns Ok(()) on success
                        task_generated_cids.push(current_node_cid.clone());
                        parent_cid_for_next_node = Some(current_node_cid); // Next node in this task will reference this one
                    }
                    Err(e) => {
                        panic!("Failed to insert node: {:?}", e);
                    }
                }
                
                // Small delay to increase chance of interleaving
                if j % 2 == 0 {
                    sleep(Duration::from_millis( (i % 3 + 1) as u64)).await;
                }
            }
            task_generated_cids // Return CIDs generated by this task
        });
        handles.push(handle);
    }

    let mut all_generated_cids = HashSet::new();
    let mut results_from_tasks = Vec::new();

    for handle in handles {
        match handle.await {
            Ok(task_cids) => {
                results_from_tasks.push(task_cids.clone());
                for cid in task_cids {
                    all_generated_cids.insert(cid);
                }
            }
            Err(e) => {
                panic!("A task panicked: {:?}", e);
            }
        }
    }

    // Verification
    // 1. Total number of unique nodes in the store should be num_tasks * nodes_per_task
    let all_nodes_in_store = store.list().await.expect("Failed to list nodes from store");
    assert_eq!(
        all_nodes_in_store.len(),
        num_tasks * nodes_per_task,
        "Mismatch in total number of nodes stored."
    );
    // Also check against the count of unique CIDs collected from tasks
    assert_eq!(
        all_generated_cids.len(),
        num_tasks * nodes_per_task,
        "Mismatch in total number of unique CIDs generated by tasks."
    );


    // 2. Verify each generated CID can be fetched and parent links are correct
    for task_cids_group in results_from_tasks {
        let mut expected_parent_cid: Option<Cid> = None;
        for cid in task_cids_group {
            let node_from_store = store.get(&cid.to_string()).await
                .expect("Failed to get node by CID")
                .expect("Node with generated CID not found in store");

            // Check parent CID
            assert_eq!(node_from_store.parent, expected_parent_cid, "Parent CID mismatch for node {}", cid); // Removed .as_ref()
            
            expected_parent_cid = Some(cid.clone()); // For the next node in this task's chain
        }
    }
    
    // Optional: Verify all CIDs in store are among those generated
    let store_cids_set: HashSet<Cid> = all_nodes_in_store.into_iter().map(|n| n.cid().unwrap()).collect();
    assert_eq!(store_cids_set, all_generated_cids, "Mismatch between CIDs in store and CIDs generated by tasks.");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_inserts_duplicate_cids() -> Result<()> {
    let store = Arc::new(SharedDagStore::new());
    let num_threads = 10;
    let barrier = Arc::new(Barrier::new(num_threads));

    // Create a single node that all tasks will try to insert
    // Use a valid DagEventType, e.g., ArbitraryData
    let common_node = create_test_node_with_parent(999, "duplicate", DagEventType::ArbitraryData, None);
    let common_node_cid = common_node.cid()?;

    let mut handles = vec![];
    for _ in 0..num_threads {
        let store_clone = Arc::clone(&store);
        let barrier_clone = Arc::clone(&barrier);
        let node_to_insert = common_node.clone(); // Clone the node for each task

        let handle = tokio::spawn(async move {
            barrier_clone.wait().await;
            // Attempt to insert the common node.
            // We expect this to succeed for one task and be a no-op (or idempotent success) for others.
            match store_clone.insert(node_to_insert).await {
                Ok(()) => { // insert returns Ok(()) on success
                    // The CID is already known (common_node_cid).
                    // The act of insertion (or re-insertion) is what's being tested for idempotency.
                    Ok(())
                }
                Err(e) => {
                    // SharedDagStore::insert currently overwrites, so an error here would be unexpected.
                    eprintln!("Insert failed unexpectedly in duplicate test: {:?}, for CID: {}", e, common_node_cid);
                    Err(anyhow::Error::new(e).context("Insert failed unexpectedly"))
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        // handle.await will return Result<Result<(), JoinError>, JoinError>
        // First unwrap JoinError, then the task's Result
        handle.await??;
    }

    // Verification: Only one copy of the node should be in the store.
    let all_nodes = store.list().await?;
    assert_eq!(all_nodes.len(), 1, "Expected only one node in the store after duplicate inserts.");

    // And that node should be the one we tried to insert.
    let retrieved_node = store.get(&common_node_cid).await?.expect("Common node not found by CID.");
    assert_eq!(retrieved_node.cid()?, common_node_cid, "Retrieved node CID mismatch.");
    // Optionally, check content if DagNode equality is well-defined
    // assert_eq!(retrieved_node, common_node, "Retrieved node content mismatch.");

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
        let node_y = create_test_node_with_parent(2, "node_y_references_x", DagEventType::Generic, Some(referenced_cid_for_y.clone()));
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