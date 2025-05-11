use std::sync::{Arc, Mutex, RwLock};
use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use cid::Cid;
use tokio::time::timeout;

use icn_identity::{Did, KeyPair as IcnKeyPair};
use icn_types::mesh::{MeshJob, MeshJobParams, JobId as IcnJobId, JobStatus as StandardJobStatus, OrganizationScopeIdentifier};
use icn_types::reputation::ReputationRecord;
use icn_runtime::context::RuntimeContext;
use std::collections::HashSet; // Added for assigned_by_originator checks
use libp2p::PeerId; // Added for checking executor DID in bid

use planetary_mesh::node::MeshNode; // Assuming MeshNode is public or pub(crate)
// Assuming InternalNodeAction is a type used by the event loop, adjust path if necessary
use planetary_mesh::node::InternalNodeAction; 
use planetary_mesh::protocol::{MeshProtocolMessage, JobManifest, Bid, AssignJobV1, ExecutionReceiptAvailableV1};
use tokio::sync::mpsc::{self, Receiver, Sender}; // Ensure Sender is imported from mpsc
use planetary_mesh::node::{NodeCommand, KnownReceiptInfo, TestObservedReputationSubmission}; // Import TestObservedReputationSubmission

// Mock or minimal reputation service URL for testing
const MOCK_REPUTATION_SERVICE_URL: &str = "http://127.0.0.1:12345"; // Placeholder

async fn setup_node(
    keypair: IcnKeyPair,
    listen_addr: Option<String>,
    rep_url: Option<String>,
) -> Result<(MeshNode, Receiver<InternalNodeAction>, Sender<NodeCommand>), Box<dyn std::error::Error>> {
    let runtime_job_queue = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let local_runtime_context = Some(Arc::new(RuntimeContext::new()));

    // Create the command channel for this node instance
    let (command_tx, command_rx) = mpsc::channel(32); // Channel buffer size of 32

    let (node, internal_action_rx) = MeshNode::new(
        keypair,
        listen_addr,
        runtime_job_queue,
        local_runtime_context,
        rep_url,
        command_rx, // Pass the receiver end to the node
    )
    .await?;
    Ok((node, internal_action_rx, command_tx)) // Return the sender end to the test
}

#[tokio::test]
#[ignore] // Ignored by default as it will be a longer-running integration test
async fn test_full_job_lifecycle() {
    // 1. Setup: Create keypairs and DIDs for originator and executor(s)
    let originator_kp = IcnKeyPair::generate();
    let originator_did = originator_kp.did.clone();
    let executor1_kp = IcnKeyPair::generate();
    let executor1_did = executor1_kp.did.clone();
    // let executor2_kp = IcnKeyPair::generate();
    // let executor2_did = executor2_kp.did.clone();

    println!("Originator DID: {}", originator_did);
    println!("Executor 1 DID: {}", executor1_did);

    // 2. Initialize MeshNodes and get command senders
    let (originator_node_instance, originator_internal_rx, originator_command_tx) = 
        setup_node(originator_kp.clone(), Some("/ip4/127.0.0.1/tcp/0".to_string()), Some(MOCK_REPUTATION_SERVICE_URL.to_string()))
        .await.expect("Failed to setup originator node");
    
    let (executor1_node_instance, executor1_internal_rx, executor1_command_tx) = 
        setup_node(executor1_kp.clone(), Some("/ip4/127.0.0.1/tcp/0".to_string()), Some(MOCK_REPUTATION_SERVICE_URL.to_string()))
        .await.expect("Failed to setup executor1 node");
    
    let originator_peer_id = originator_node_instance.local_peer_id();
    let executor1_peer_id = executor1_node_instance.local_peer_id();

    println!("Originator Peer ID: {}", originator_peer_id);
    println!("Executor 1 Peer ID: {}", executor1_peer_id);

    // Start the event loops for each node in separate tokio tasks
    let originator_handle = tokio::spawn(async move { originator_node_instance.run_event_loop(originator_internal_rx).await });
    let executor1_handle = tokio::spawn(async move { executor1_node_instance.run_event_loop(executor1_internal_rx).await });

    // Allow some time for nodes to start up and discover each other (mDNS or Kademlia)
    // In a real test, explicit peering or waiting for discovery events would be better.
    tokio::time::sleep(Duration::from_secs(3)).await;


    // 3. Create and Announce Job by Originator
    let job_id: IcnJobId = format!("test-job-{}", Utc::now().timestamp_millis());
    let mesh_job_params = MeshJobParams {
        wasm_cid: "bafyreibmicpv3gzfxmlsx7qvyfigt765hsdgdnkrhdk2qdsdlvgnpvchuq".to_string(), // Example CID
        ccl_cid: None,
        description: Some("A test job for the full lifecycle".to_string()),
        required_resources_json: r#"{"min_cpu_cores": 1, "min_memory_mb": 128}"#.to_string(),
        max_execution_time_secs: Some(60),
        output_location: None,
        is_interactive: false,
        stages: None,
        workflow_type: icn_types::mesh::WorkflowType::SingleWasmModule,
        execution_policy: None,
        trust_requirements: None,
    };
    let job_to_announce = MeshJob {
        job_id: job_id.clone(),
        params: mesh_job_params,
        originator_did: originator_did.clone(),
        originator_org_scope: Some(OrganizationScopeIdentifier::Personal(originator_did.clone())), // Example scope
        submission_timestamp: Utc::now().timestamp(),
    };

    // Accessing the swarm directly for this is not ideal for a test, prefer a method on MeshNode
    // For now, assume announce_job handles publishing.
    // Let's assume MeshNode has a way to get its state for assertions, e.g., Arc<RwLock<InnerState>>
    // Or, we'll use helper functions to query state via channels if available.
    
    // We need direct access to the `MeshNode`'s fields for assertions or use methods.
    // For this test, we'll assume direct access to Arc<RwLock<...>> fields is possible for checks.
    // This might require making them pub(crate) or providing accessor methods.
    // If MeshNode instance is moved into the tokio::spawn, we need another way to interact with its state.
    // Let's clone the Arcs for the state variables we need to check.

    let originator_announced_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_announced_originated_jobs_arc(&originator_node_instance)); // Placeholder for actual access
    let originator_bids = Arc::clone(&planetary_mesh::node::test_utils::get_bids_arc(&originator_node_instance)); // Placeholder
    let originator_assigned_by_originator = Arc::clone(&planetary_mesh::node::test_utils::get_assigned_by_originator_arc(&originator_node_instance)); // Placeholder
    let originator_receipt_store_dag_nodes = Arc::clone(&planetary_mesh::node::test_utils::get_receipt_store_dag_nodes_arc(&originator_node_instance)); // Placeholder
    let originator_balance_store = Arc::clone(&planetary_mesh::node::test_utils::get_balance_store_arc(&originator_node_instance)); // Placeholder
    let originator_known_receipt_cids = Arc::clone(&planetary_mesh::node::test_utils::get_known_receipt_cids_arc(&originator_node_instance)); // Placeholder
    let originator_observed_reputation_submissions = Arc::clone(&planetary_mesh::node::test_utils::get_test_observed_reputation_submissions_arc(&originator_node_instance));
    
    let executor_available_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_available_jobs_on_mesh_arc(&executor1_node_instance)); // Placeholder
    let executor_assigned_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_assigned_jobs_arc(&executor1_node_instance)); // Placeholder
    let executor_balance_store = Arc::clone(&planetary_mesh::node::test_utils::get_balance_store_arc(&executor1_node_instance)); // Placeholder


    // Originator announces the job
    println!("Announcing job: {}", job_id);
    // Use the command sender to announce the job
    test_utils::command_originator_to_announce_job(&originator_command_tx, job_to_announce.clone())
        .await
        .expect("Failed to send AnnounceJob command to originator");
    println!("Job {} announcement command sent to originator", job_id);


    // 4. Executor Nodes Submit Bids
    println!("Executor 1 waiting for job announcement...");
    timeout(Duration::from_secs(10), async {
        loop {
            if executor_available_jobs.read().unwrap().contains_key(&job_id) {
                println!("Executor 1 found job {} on the mesh.", job_id);
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("Executor 1 timed out waiting for job announcement");

    let bid_price = 100; // Example bid price
    let bid_by_executor1 = Bid {
        job_id: job_id.clone(),
        executor_did: executor1_did.clone(),
        price: bid_price,
        timestamp: Utc::now().timestamp(),
        //execution_guarantees: None, // Add if needed
    };
    
    println!("Executor 1 submitting bid for job {}", job_id);
    // Use the command sender to submit the bid
    test_utils::command_executor_to_submit_bid(&executor1_command_tx, bid_by_executor1.clone())
        .await
        .expect("Failed to send SubmitBid command to executor1");
    println!("Executor 1 submitted bid command for job {}", job_id);


    // 5. Originator Selects Bid and Assigns Job
    println!("Originator waiting for bids...");
    timeout(Duration::from_secs(10), async {
        loop {
            let bids_map = originator_bids.read().unwrap();
            if let Some(bids_for_job) = bids_map.get(&job_id) {
                if !bids_for_job.is_empty() {
                    println!("Originator found {} bid(s) for job {}.", bids_for_job.len(), job_id);
                    assert_eq!(bids_for_job[0].executor_did, executor1_did, "Bid not from expected executor.");
                    assert_eq!(bids_for_job[0].price, bid_price, "Bid price mismatch.");
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("Originator timed out waiting for bids");

    println!("Originator waiting for job assignment to occur...");
    // The assignment happens via executor_selection_interval in MeshNode's event loop.
    // We need to wait for AssignJobV1 to be sent by originator and received by executor.
    timeout(Duration::from_secs(15), async { // Increased timeout for selection interval
        loop {
            // Check if originator marked it as assigned
            let is_assigned_by_originator = originator_assigned_by_originator.read().unwrap().contains(&job_id);
            // Check if executor received the assignment
            let is_assigned_to_executor = executor_assigned_jobs.read().unwrap().contains_key(&job_id);

            if is_assigned_by_originator && is_assigned_to_executor {
                println!("Job {} successfully assigned to Executor 1.", job_id);
                let assigned_job_details = executor_assigned_jobs.read().unwrap();
                let (_manifest, assigned_bid) = assigned_job_details.get(&job_id).unwrap();
                assert_eq!(assigned_bid.executor_did, executor1_did);
                assert_eq!(assigned_bid.price, bid_price);
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }).await.expect("Timed out waiting for job assignment confirmation");


    // 6. Executor Executes Job and Announces Receipt
    println!("Executor 1 triggering execution for job {}...", job_id);
    // To call trigger_execution_for_job, we need JobManifest and Bid. Executor has this in `assigned_jobs`.
    // This step might be automatically triggered by the executor's event loop upon receiving AssignJobV1.
    // If not, we'd call a method on executor1_node. For now, assume it's handled internally
    // or a test helper is needed.
    // Let's assume the executor node automatically calls `trigger_execution_for_job`
    // when an `AssignJobV1` is processed and the job is in its `assigned_jobs`.
    // We then wait for the `ExecutionReceiptAvailableV1` to be announced by the executor
    // and received by the originator.

    // To verify, we can check if the originator has seen the receipt announcement.
    // The originator stores known receipt CIDs or processes them.
    // Let's assume a placeholder for originator_node.known_receipt_cids or similar state:
    let originator_known_receipt_cids = Arc::clone(&planetary_mesh::node::test_utils::get_known_receipt_cids_arc(&originator_node_instance)); // Placeholder
    let mut receipt_cid_found: Option<Cid> = None;

    println!("Originator waiting for execution receipt announcement for job {}...", job_id);
    timeout(Duration::from_secs(10), async {
        loop {
            // This check is a bit indirect. Ideally, we'd listen for ExecutionReceiptAvailableV1
            // or check a state variable that explicitly tracks receipts for originated jobs.
            // For now, we'll check if a receipt related to the job appears in the originator's dag_store
            // after it has been fetched and verified.
            let dag_nodes = originator_receipt_store_dag_nodes.read().unwrap();
            for (cid, node) in dag_nodes.iter() {
                // Deserialize node to ExecutionReceipt and check job_id
                // This is complex, so for now, let's assume if any receipt comes in for this job, it's good.
                // A better check would be to find the specific receipt for this job_id.
                // The receipt itself contains the job_id.
                // The `trigger_economic_settlement` and `trigger_reputation_update` are called after anchoring.
                // So checking for balance change might be a good proxy too.
                // Let's assume `known_receipt_cids` stores CID -> (JobId, ExecutorDid)
                let known_cids_map = originator_known_receipt_cids.read().unwrap(); // This map needs to exist.
                if let Some(cid) = known_cids_map.iter().find_map(|(c, (j_id, exec_did))| {
                    if *j_id == job_id && *exec_did == executor1_did { Some(*c) } else { None }
                }) {
                    println!("Originator received announcement for receipt CID: {} for job {}", cid, job_id);
                    receipt_cid_found = Some(cid);
                    break;
                }
            }
            if receipt_cid_found.is_some() { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("Originator timed out waiting for execution receipt announcement");
    
    let _receipt_cid = receipt_cid_found.expect("Receipt CID not found after wait");


    // 7. Originator Fetches, Verifies, Anchors Receipt, and Settles
    // This is largely handled by the originator's event loop upon receiving ExecutionReceiptAvailableV1.
    // We need to verify the outcomes.

    println!("Verifying receipt anchoring, economic settlement, and reputation update...");
    tokio::time::sleep(Duration::from_secs(5)).await; // Give time for async post-receipt processing

    // Verify receipt is anchored
    // This check is already implicitly part of finding receipt_cid_found if it checks dag_store.
    // We can make it more explicit by fetching the receipt by CID from originator's store.
    // assert!(originator_node_instance.local_runtime_context.as_ref().unwrap().receipt_store.dag_nodes.read().unwrap().contains_key(&receipt_cid));
    // println!("Receipt {} successfully anchored by originator.", receipt_cid);

    // Verify economic settlement
    // Initial balances are usually 0 or some default if not set. RuntimeContext sets them to 0.
    let originator_final_balance = originator_balance_store.read().unwrap().get(&originator_did).copied().unwrap_or(0);
    let executor1_final_balance = executor_balance_store.read().unwrap().get(&executor1_did).copied().unwrap_or(0);

    // Assuming originator starts with more than `bid_price` or we mock initial balances.
    // For this test, let's assume they start at 0 and originator gets a magic top-up or goes into negative.
    // A proper setup would involve pre-funding the originator.
    // For simplicity, let's check executor's balance increased by bid_price.
    // And originator's decreased (or check relative if not starting at 0).
    // For now, let's assume `RuntimeContext` allows direct balance manipulation for tests or starts with enough.
    // We will assume the `transfer_balance_direct` works.
    // Let's check if executor's balance is now `bid_price` (assuming it started at 0).
    assert_eq!(executor1_final_balance, bid_price, "Executor 1 balance incorrect after settlement.");
    println!("Economic settlement verified. Executor 1 balance: {}", executor1_final_balance);
    // We'd also check originator's balance: assert_eq!(originator_final_balance, INITIAL_ORIGINATOR_BALANCE - bid_price);

    // Verify reputation record is "submitted" by checking our observed list
    println!("Waiting for reputation submission to be observed for job {}...", job_id);
    let captured_receipt_cid = _receipt_cid; // Use the receipt_cid obtained earlier

    timeout(Duration::from_secs(10), async {
        loop {
            let observed_submissions = originator_observed_reputation_submissions.read().unwrap();
            if let Some(submission) = observed_submissions.iter().find(|s| {
                s.job_id == job_id && 
                s.executor_did == executor1_did && 
                s.anchor_cid == captured_receipt_cid
            }) {
                println!("Observed reputation submission: {:?}", submission);
                // Assert on the details of the submission
                assert_eq!(submission.outcome, StandardJobStatus::Completed, "Reputation outcome mismatch."); // Or Succeeded, depending on your logic
                // assert_eq!(submission.timestamp, expected_timestamp_from_receipt); // If needed
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("Timed out waiting for observed reputation submission");
    println!("Reputation update submission successfully verified for executor {} regarding job {}.", executor1_did, job_id);


    // 8. Assertions (more can be added)
    // - Check job status (e.g., originator_node_instance.completed_job_receipt_cids should contain this job)
    // let completed_receipts = originator_node_instance.completed_job_receipt_cids.read().unwrap();
    // assert!(completed_receipts.get(&job_id).map_or(false, |cid_set| cid_set.contains(&receipt_cid)));
    // println!("Job {} status verified as completed with receipt {}.", job_id, receipt_cid);
    
    // More detailed assertions:
    // - Originator's view of the job as completed.
    // - Executor's view of the job as completed.
    // - No unexpected errors in logs (if using a log capture mechanism).


    // 9. Teardown: Shutdown nodes gracefully
    println!("Test steps completed. Tearing down nodes.");
    originator_handle.abort();
    executor1_handle.abort();

    match originator_handle.await {
        Ok(Err(e)) => eprintln!("Originator event loop error: {:?}", e),
        Err(e) if e.is_cancelled() => println!("Originator event loop aborted successfully."),
        Err(e) => eprintln!("Originator event loop panicked: {:?}", e),
        _ => {}
    }
    match executor1_handle.await {
        Ok(Err(e)) => eprintln!("Executor 1 event loop error: {:?}", e),
        Err(e) if e.is_cancelled() => println!("Executor 1 event loop aborted successfully."),
        Err(e) => eprintln!("Executor 1 event loop panicked: {:?}", e),
        _ => {}
    }

    println!("Test finished.");
    assert!(true, "Full job lifecycle test completed basic checks.");
}

// Helper module for accessing MeshNode internals in tests.
// This is a placeholder for how you might access internal state.
// Ideally, MeshNode provides methods or uses channels for state observation in tests.
mod test_utils {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use cid::Cid;
    use icn_identity::Did;
    use std::sync::{Arc, RwLock};
    use planetary_mesh::node::MeshNode;
    use icn_types::mesh::{MeshJob, JobId as IcnJobId};
    use planetary_mesh::protocol::{JobManifest, Bid};
    use icn_runtime::runtime::RuntimeBalanceStore;
    use planetary_mesh::node::KnownReceiptInfo; // Assuming this type for known_receipt_cids
    use tokio::sync::mpsc::Sender; // Ensure Sender is imported for function signatures


    // These functions are placeholders. You'll need to implement them based on how MeshNode exposes its state.
    // This might involve adding pub(crate) fields, specific test accessor methods, or using channels.

    pub fn get_announced_originated_jobs_arc(node: &MeshNode) -> Arc<RwLock<HashMap<IcnJobId, (JobManifest, MeshJob)>>> {
        // Assuming MeshNode has a field like:
        // pub announced_originated_jobs: Arc<RwLock<HashMap<IcnJobId, (JobManifest, MeshJob)>>>;
        node.announced_originated_jobs.clone()
    }

    pub fn get_bids_arc(node: &MeshNode) -> Arc<RwLock<HashMap<IcnJobId, Vec<Bid>>>> {
        // Assuming MeshNode has a field like:
        // pub bids: Arc<RwLock<HashMap<IcnJobId, Vec<Bid>>>>>;
        node.bids.clone()
    }

    pub fn get_assigned_by_originator_arc(node: &MeshNode) -> Arc<RwLock<HashSet<IcnJobId>>> {
        // Assuming MeshNode has a field like:
        // pub assigned_by_originator: Arc<RwLock<HashSet<IcnJobId>>>>;
        node.assigned_by_originator.clone()
    }
    
    pub fn get_receipt_store_dag_nodes_arc(node: &MeshNode) -> Arc<RwLock<HashMap<Cid, Vec<u8>>>> {
        // Assuming MeshNode has a field like:
        // pub local_runtime_context: Option<Arc<RuntimeContext>>;
        // And RuntimeContext has:
        // pub receipt_store: Arc<RwLock<DagStore>>; (or similar structure for dag_nodes)
        // For simplicity, let's assume RuntimeContext directly exposes dag_nodes for its receipt_store
        node.local_runtime_context.as_ref()
            .expect("RuntimeContext not initialized in MeshNode for test")
            .receipt_store.dag_nodes.clone()
    }

    pub fn get_balance_store_arc(node: &MeshNode) -> Arc<RwLock<RuntimeBalanceStore>> {
        // Assuming MeshNode has a field like:
        // pub local_runtime_context: Option<Arc<RuntimeContext>>;
        // And RuntimeContext has:
        // pub balance_store: Arc<RwLock<RuntimeBalanceStore>>;
        node.local_runtime_context.as_ref()
            .expect("RuntimeContext not initialized in MeshNode for test")
            .balance_store.clone()
    }

    pub fn get_available_jobs_on_mesh_arc(node: &MeshNode) -> Arc<RwLock<HashMap<IcnJobId, JobManifest>>> {
        // Assuming MeshNode has a field like:
        // pub available_jobs_on_mesh: Arc<RwLock<HashMap<IcnJobId, JobManifest>>>>;
        node.available_jobs_on_mesh.clone()
    }

    pub fn get_assigned_jobs_arc(node: &MeshNode) -> Arc<RwLock<HashMap<IcnJobId, (JobManifest, Bid)>>> {
        // Assuming MeshNode has a field like:
        // pub assigned_jobs: Arc<RwLock<HashMap<IcnJobId, (JobManifest, Bid)>>>>;
        node.assigned_jobs.clone()
    }
    
    pub fn get_known_receipt_cids_arc(node: &MeshNode) -> Arc<RwLock<HashMap<Cid, KnownReceiptInfo>>> {
        // Assuming MeshNode has a field like:
        // pub known_receipt_cids: Arc<RwLock<HashMap<Cid, KnownReceiptInfo>>>>;
        // where KnownReceiptInfo might be struct KnownReceiptInfo { job_id: IcnJobId, executor_did: Did, announced_at: i64 }
        node.known_receipt_cids.clone()
    }

    // Add accessor for the new test_observed_reputation_submissions field
    pub fn get_test_observed_reputation_submissions_arc(node: &MeshNode) -> Arc<RwLock<Vec<TestObservedReputationSubmission>>> {
        node.test_observed_reputation_submissions.clone()
    }

    // Updated command functions to use the Sender<NodeCommand>
    pub async fn command_originator_to_announce_job(
        tx: &Sender<NodeCommand>,
        job: MeshJob,
    ) -> Result<(), String> {
        tx.send(NodeCommand::AnnounceJob(job))
            .await
            .map_err(|e| format!("Failed to send AnnounceJob command: {:?}", e))
    }

    pub async fn command_executor_to_submit_bid(
        tx: &Sender<NodeCommand>,
        bid: Bid,
    ) -> Result<(), String> {
        tx.send(NodeCommand::SubmitBid(bid))
            .await
            .map_err(|e| format!("Failed to send SubmitBid command: {:?}", e))
    }
}
