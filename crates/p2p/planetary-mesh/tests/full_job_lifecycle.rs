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
use icn_types::jobs::policy::ExecutionPolicy; // Ensure ExecutionPolicy is imported

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
    // 1. Setup: Create keypairs and DIDs
    let originator_kp = IcnKeyPair::generate();
    let originator_did = originator_kp.did.clone();
    
    let executor1_kp = IcnKeyPair::generate();
    let executor1_did = executor1_kp.did.clone();
    
    let executor2_kp = IcnKeyPair::generate(); // New: Executor 2 Keypair
    let executor2_did = executor2_kp.did.clone(); // New: Executor 2 DID

    println!("Originator DID: {}", originator_did);
    println!("Executor 1 DID: {}", executor1_did);
    println!("Executor 2 DID: {}", executor2_did); // New: Print Executor 2 DID

    // 2. Initialize MeshNodes and get command senders
    let (originator_node_instance, originator_internal_rx, originator_command_tx) = 
        setup_node(originator_kp.clone(), Some("/ip4/127.0.0.1/tcp/0".to_string()), Some(MOCK_REPUTATION_SERVICE_URL.to_string()))
        .await.expect("Failed to setup originator node");
    
    let (executor1_node_instance, executor1_internal_rx, executor1_command_tx) = 
        setup_node(executor1_kp.clone(), Some("/ip4/127.0.0.1/tcp/0".to_string()), Some(MOCK_REPUTATION_SERVICE_URL.to_string()))
        .await.expect("Failed to setup executor1 node");

    // New: Setup Executor 2 Node
    let (executor2_node_instance, executor2_internal_rx, executor2_command_tx) = 
        setup_node(executor2_kp.clone(), Some("/ip4/127.0.0.1/tcp/0".to_string()), Some(MOCK_REPUTATION_SERVICE_URL.to_string()))
        .await.expect("Failed to setup executor2 node");
    
    let _originator_peer_id = originator_node_instance.local_peer_id();
    let _executor1_peer_id = executor1_node_instance.local_peer_id();
    let _executor2_peer_id = executor2_node_instance.local_peer_id(); // New: Executor 2 Peer ID

    // Start the event loops for each node
    let originator_handle = tokio::spawn(async move { originator_node_instance.run_event_loop(originator_internal_rx).await });
    let executor1_handle = tokio::spawn(async move { executor1_node_instance.run_event_loop(executor1_internal_rx).await });
    let executor2_handle = tokio::spawn(async move { executor2_node_instance.run_event_loop(executor2_internal_rx).await }); // New: Start Executor 2 loop

    tokio::time::sleep(Duration::from_secs(5)).await; // Increased sleep for 3 nodes

    // Define an ExecutionPolicy for the job
    let job_execution_policy = ExecutionPolicy {
        min_reputation_score: Some(70.0), // Example: Min reputation of 70
        max_price: Some(150),             // Example: Max price of 150
        preferred_regions: None,          // Example: No region preference for now
        weight_price: Some(0.4),          // Example: Price weight
        weight_reputation: Some(0.6),     // Example: Reputation weight (higher than price)
        required_ccl_level: None,
        custom_policy_script: None,
    };

    // 3. Create and Announce Job by Originator
    let job_id: IcnJobId = format!("test-policy-job-{}", Utc::now().timestamp_millis());
    let mesh_job_params = MeshJobParams {
        wasm_cid: "bafyreibmicpv3gzfxmlsx7qvyfigt765hsdgdnkrhdk2qdsdlvgnpvchuq".to_string(),
        ccl_cid: None,
        description: Some("A test job with an execution policy".to_string()),
        execution_policy: Some(job_execution_policy.clone()), // Attach the policy
        required_resources_json: r#"{"min_cpu_cores": 1, "min_memory_mb": 128}"#.to_string(),
        max_execution_time_secs: Some(60),
        output_location: None,
        is_interactive: false,
        stages: None,
        workflow_type: icn_types::mesh::WorkflowType::SingleWasmModule,        
        trust_requirements: None,
    };
    let job_to_announce = MeshJob {
        job_id: job_id.clone(),
        params: mesh_job_params,
        originator_did: originator_did.clone(),
        originator_org_scope: Some(OrganizationScopeIdentifier::Personal(originator_did.clone())),
        submission_timestamp: Utc::now().timestamp(),
    };

    // Clone Arcs for state checking
    let originator_bids = Arc::clone(&planetary_mesh::node::test_utils::get_bids_arc(&originator_node_instance));
    let originator_assigned_by_originator = Arc::clone(&planetary_mesh::node::test_utils::get_assigned_by_originator_arc(&originator_node_instance));
    let originator_known_receipt_cids = Arc::clone(&planetary_mesh::node::test_utils::get_known_receipt_cids_arc(&originator_node_instance));
    let originator_observed_reputation_submissions = Arc::clone(&planetary_mesh::node::test_utils::get_test_observed_reputation_submissions_arc(&originator_node_instance));
    let originator_balance_store = Arc::clone(&planetary_mesh::node::test_utils::get_balance_store_arc(&originator_node_instance));
    
    let executor1_available_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_available_jobs_on_mesh_arc(&executor1_node_instance));
    let executor1_assigned_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_assigned_jobs_arc(&executor1_node_instance));
    let executor1_balance_store = Arc::clone(&planetary_mesh::node::test_utils::get_balance_store_arc(&executor1_node_instance));

    // New: State Arcs for Executor 2
    let executor2_available_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_available_jobs_on_mesh_arc(&executor2_node_instance));
    let executor2_assigned_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_assigned_jobs_arc(&executor2_node_instance));
    let executor2_balance_store = Arc::clone(&planetary_mesh::node::test_utils::get_balance_store_arc(&executor2_node_instance));

    // Set Mock Reputations on Originator Node
    let mut mock_reputations = HashMap::new();
    mock_reputations.insert(executor1_did.clone(), 75.0); // Executor 1: Rep 75
    mock_reputations.insert(executor2_did.clone(), 90.0); // Executor 2: Rep 90 (higher)
    
    println!("Setting mock reputations on originator: {:?}", mock_reputations);
    test_utils::command_node_to_set_mock_reputations(&originator_command_tx, mock_reputations)
        .await
        .expect("Failed to send SetMockReputations command to originator");

    // Originator announces the job
    println!("Announcing job with policy: {}", job_id);
    test_utils::command_originator_to_announce_job(&originator_command_tx, job_to_announce.clone())
        .await
        .expect("Failed to send AnnounceJob command to originator");
    println!("Job {} announcement command sent to originator", job_id);

    // 4. Executors Submit Bids
    // Executor 1 waits for job and submits bid
    println!("Executor 1 waiting for job announcement...");
    timeout(Duration::from_secs(10), async {
        loop {
            if executor1_available_jobs.read().unwrap().contains_key(&job_id) { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("Executor 1 timed out waiting for job announcement");
    let bid_price_ex1 = 100; // Lower price
    let bid_by_executor1 = Bid { job_id: job_id.clone(), executor_did: executor1_did.clone(), price: bid_price_ex1, timestamp: Utc::now().timestamp() };
    println!("Executor 1 submitting bid (Price: {}, MockRep: 75.0) for job {}", bid_price_ex1, job_id);
    test_utils::command_executor_to_submit_bid(&executor1_command_tx, bid_by_executor1.clone()).await.expect("Executor 1 failed to submit bid");

    // New: Executor 2 waits for job and submits bid
    println!("Executor 2 waiting for job announcement...");
    timeout(Duration::from_secs(10), async {
        loop {
            if executor2_available_jobs.read().unwrap().contains_key(&job_id) { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("Executor 2 timed out waiting for job announcement");
    let bid_price_ex2 = 120; // Slightly higher price, but higher reputation
    let bid_by_executor2 = Bid { job_id: job_id.clone(), executor_did: executor2_did.clone(), price: bid_price_ex2, timestamp: Utc::now().timestamp() };
    println!("Executor 2 submitting bid (Price: {}, MockRep: 90.0) for job {}", bid_price_ex2, job_id);
    test_utils::command_executor_to_submit_bid(&executor2_command_tx, bid_by_executor2.clone()).await.expect("Executor 2 failed to submit bid");

    // 5. Originator Selects Bid and Assigns Job (Expect Executor 2 to win due to policy)
    let expected_winner_did = executor2_did.clone();
    let expected_winning_price = bid_price_ex2;

    println!("Originator waiting for bids and policy-based assignment (expecting {} to win)..", expected_winner_did);
    timeout(Duration::from_secs(20), async { // Increased timeout for selection interval + 2 bids
        loop {
            let assigned_job_details_ex1 = executor1_assigned_jobs.read().unwrap();
            let assigned_job_details_ex2 = executor2_assigned_jobs.read().unwrap();
            
            let is_assigned_by_originator = originator_assigned_by_originator.read().unwrap().contains(&job_id);
            let is_assigned_to_expected_winner = assigned_job_details_ex2.contains_key(&job_id);
            let is_assigned_to_other_executor = assigned_job_details_ex1.contains_key(&job_id);

            if is_assigned_by_originator && is_assigned_to_expected_winner {
                assert!(!is_assigned_to_other_executor, "Job incorrectly assigned to executor 1 as well!");
                println!("Job {} successfully assigned to expected winner: {}.", job_id, expected_winner_did);
                let (_manifest, assigned_bid) = assigned_job_details_ex2.get(&job_id).unwrap();
                assert_eq!(assigned_bid.executor_did, expected_winner_did, "Assigned to wrong executor DID.");
                assert_eq!(assigned_bid.price, expected_winning_price, "Assigned with wrong price.");
                break;
            }
            if is_assigned_by_originator && is_assigned_to_other_executor {
                 panic!("Job {} was incorrectly assigned to Executor 1 instead of expected Executor 2!", job_id);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }).await.expect("Timed out waiting for job assignment to the expected winner (Executor 2)");

    // 6. Winning Executor (Executor 2) Executes Job and Announces Receipt
    println!("Winning Executor ({}) triggering execution for job {}...", expected_winner_did, job_id);
    let mut receipt_cid_found: Option<Cid> = None;
    println!("Originator waiting for execution receipt announcement from {} for job {}...", expected_winner_did, job_id);
    timeout(Duration::from_secs(15), async { // Increased timeout for execution + announcement
        loop {
            let known_cids_map = originator_known_receipt_cids.read().unwrap();
            if let Some(cid) = known_cids_map.iter().find_map(|(c, info)| {
                if info.job_id == job_id && info.executor_did == expected_winner_did { Some(*c) } else { None }
            }) {
                println!("Originator received announcement for receipt CID: {} for job {} from {}", cid, job_id, expected_winner_did);
                receipt_cid_found = Some(cid);
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }).await.expect("Originator timed out waiting for execution receipt announcement from expected winner");
    let captured_receipt_cid = receipt_cid_found.expect("Receipt CID not found after wait");

    // 7. Originator Fetches, Verifies, Anchors Receipt, and Settles with Winning Executor
    println!("Verifying receipt anchoring, economic settlement with {}, and reputation update...", expected_winner_did);
    tokio::time::sleep(Duration::from_secs(5)).await; // Give time for async post-receipt processing

    // Verify economic settlement with Executor 2
    let executor2_final_balance = executor2_balance_store.read().unwrap().get(&expected_winner_did).copied().unwrap_or(0);
    assert_eq!(executor2_final_balance, expected_winning_price, "Executor 2 balance incorrect after settlement.");
    let executor1_final_balance = executor1_balance_store.read().unwrap().get(&executor1_did).copied().unwrap_or(0);
    assert_eq!(executor1_final_balance, 0, "Executor 1 balance should be unchanged."); // Executor 1 should not have been paid
    println!("Economic settlement verified. {} balance: {}", expected_winner_did, executor2_final_balance);

    // Verify reputation record is "submitted" for the winning executor (Executor 2)
    println!("Waiting for reputation submission to be observed for job {} (executor {})..", job_id, expected_winner_did);
    timeout(Duration::from_secs(10), async {
        loop {
            let observed_submissions = originator_observed_reputation_submissions.read().unwrap();
            if let Some(submission) = observed_submissions.iter().find(|s| {
                s.job_id == job_id && 
                s.executor_did == expected_winner_did && 
                s.outcome == StandardJobStatus::Succeeded && // Assuming success for this main flow
                s.anchor_cid.is_some()
            }) {
                println!(
                    "Reputation submission for job {} (executor {}) observed on originator with anchor_cid: {:?}.", 
                    job_id, expected_winner_did, submission.anchor_cid
                );
                // Assert the anchor CID is present and matches the one in the DAG store (if checking originator's DAG)
                let originator_dag_store = planetary_mesh::node::test_utils::get_receipt_store_dag_nodes_arc(&originator_node_instance); // Assuming this gets DAG store
                assert!(
                    originator_dag_store.read().unwrap().contains_key(&submission.anchor_cid.unwrap()),
                    "Originator DAG store does not contain the anchored reputation record CID: {:?}",
                    submission.anchor_cid.unwrap()
                );
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }).await.expect("Timed out waiting for originator to observe reputation submission for the winning executor");

    // --- NEW: Executor verifies the reputation record issued by the originator ---
    println!("Executor ({}) waiting to receive, fetch, and verify reputation record from Originator ({}) for job {}...", 
        expected_winner_did, originator_did, job_id);
    
    // Wait for the executor node (expected_winner_did, which is executor2) to receive, fetch, and verify the reputation record.
    tokio::time::sleep(Duration::from_secs(10)).await; // Allow time for gossip, Kademlia fetch, and processing. Adjust if needed.

    // Get the CID from the originator's reputation submission (we already have originator_observed_reputation_submissions)
    let submissions_on_originator = originator_observed_reputation_submissions.read().unwrap();
    let last_submission_by_originator = submissions_on_originator.iter()
        .filter(|s| s.job_id == job_id && s.executor_did == expected_winner_did)
        .last()
        .expect("Originator should have submitted a reputation record for the winning executor");
    
    let expected_reputation_anchor_cid = last_submission_by_originator.anchor_cid.expect("Expected anchor_cid in originator's reputation submission");

    // Check that the winning executor (Executor 2) has fetched and verified it
    // Determine which executor node instance is the winner
    let winning_executor_node_instance = if expected_winner_did == executor1_did { &executor1_node_instance } else { &executor2_node_instance };
    
    let executor_verified_records_map = test_utils::get_verified_reputation_records_arc(winning_executor_node_instance);
    let records_map_reader = executor_verified_records_map.read().unwrap();

    let verified_record_on_executor = records_map_reader.get(&expected_reputation_anchor_cid);
    assert!(
        verified_record_on_executor.is_some(),
        "Winning executor ({}) did not store the verified reputation record with CID: {}. Records found: {:?}",
        expected_winner_did, expected_reputation_anchor_cid, records_map_reader.keys()
    );

    let verified_record = verified_record_on_executor.unwrap();
    assert_eq!(verified_record.issuer, originator_did, "Reputation record issuer DID mismatch on executor.");
    assert_eq!(verified_record.subject, expected_winner_did, "Reputation record subject DID mismatch on executor.");
    assert_eq!(verified_record.event.job_id(), job_id, "Reputation record job_id mismatch on executor."); // Assuming ReputationUpdateEvent has a job_id() accessor

    tracing::info!(
        "Winning executor ({}) successfully verified and stored the reputation record (CID: {}) from Originator ({}).", 
        expected_winner_did, expected_reputation_anchor_cid, originator_did
    );
    // --- END OF NEW VERIFICATION LOGIC ---

    // 9. Teardown: Shutdown nodes gracefully
    println!("Test steps completed. Tearing down nodes.");
    originator_handle.abort();
    executor1_handle.abort();
    executor2_handle.abort(); // New: Teardown Executor 2

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
    match executor2_handle.await {
        Ok(Err(e)) => eprintln!("Executor 2 event loop error: {:?}", e),
        Err(e) if e.is_cancelled() => println!("Executor 2 event loop aborted successfully."),
        Err(e) => eprintln!("Executor 2 event loop panicked: {:?}", e),
        _ => {}
    }

    println!("Test finished.");
    assert!(true, "Full job lifecycle test with policy completed basic checks.");
}

// New test function for policy edge case: low reputation
#[tokio::test]
#[ignore] // Mark as ignored for now, can be run explicitly
async fn test_policy_rejects_all_bidders_due_to_low_reputation() {
    // 1. Setup: Keypairs and DIDs for originator and two executors
    let originator_kp = IcnKeyPair::generate();
    let originator_did = originator_kp.did.clone();
    let executor1_kp = IcnKeyPair::generate();
    let executor1_did = executor1_kp.did.clone();
    let executor2_kp = IcnKeyPair::generate();
    let executor2_did = executor2_kp.did.clone();

    println!("LOW_REP_TEST: Originator DID: {}", originator_did);
    println!("LOW_REP_TEST: Executor 1 DID: {}", executor1_did);
    println!("LOW_REP_TEST: Executor 2 DID: {}", executor2_did);

    // 2. Initialize MeshNodes and get command senders
    let (originator_node_instance, originator_internal_rx, originator_command_tx) = 
        setup_node(originator_kp.clone(), Some("/ip4/127.0.0.1/tcp/0".to_string()), None)
        .await.expect("LOW_REP_TEST: Failed to setup originator node");
    
    let (executor1_node_instance, executor1_internal_rx, executor1_command_tx) = 
        setup_node(executor1_kp.clone(), Some("/ip4/127.0.0.1/tcp/0".to_string()), None)
        .await.expect("LOW_REP_TEST: Failed to setup executor1 node");

    let (executor2_node_instance, executor2_internal_rx, executor2_command_tx) = 
        setup_node(executor2_kp.clone(), Some("/ip4/127.0.0.1/tcp/0".to_string()), None)
        .await.expect("LOW_REP_TEST: Failed to setup executor2 node");
    
    // Start event loops
    let originator_handle = tokio::spawn(async move { originator_node_instance.run_event_loop(originator_internal_rx).await });
    let executor1_handle = tokio::spawn(async move { executor1_node_instance.run_event_loop(executor1_internal_rx).await });
    let executor2_handle = tokio::spawn(async move { executor2_node_instance.run_event_loop(executor2_internal_rx).await });

    tokio::time::sleep(Duration::from_secs(5)).await; // Allow nodes to start and discover

    // 3. Define an ExecutionPolicy with a high min_reputation_score
    let job_execution_policy = ExecutionPolicy {
        min_reputation_score: Some(80.0), // Min reputation of 80
        max_price: Some(200),             // Max price (not the limiting factor here)
        weight_price: Some(0.5),
        weight_reputation: Some(0.5),
        preferred_regions: None,
        required_ccl_level: None,
        custom_policy_script: None,
    };

    // 4. Create and Announce Job by Originator
    let job_id: IcnJobId = format!("test-low-rep-reject-{}", Utc::now().timestamp_millis());
    let mesh_job_params = MeshJobParams {
        wasm_cid: "bafyreibmicpv3gzfxmlsx7qvyfigt765hsdgdnkrhdk2qdsdlvgnpvchuq".to_string(),
        description: Some("Test job: policy should reject all due to low reputation".to_string()),
        execution_policy: Some(job_execution_policy.clone()),
        required_resources_json: r#"{}"#.to_string(), // Minimal resources
        max_execution_time_secs: Some(60),
        output_location: None,
        is_interactive: false,
        stages: None,
        workflow_type: icn_types::mesh::WorkflowType::SingleWasmModule,
        ccl_cid: None,
        trust_requirements: None,
    };
    let job_to_announce = MeshJob {
        job_id: job_id.clone(),
        params: mesh_job_params,
        originator_did: originator_did.clone(),
        originator_org_scope: Some(OrganizationScopeIdentifier::Personal(originator_did.clone())),
        submission_timestamp: Utc::now().timestamp(),
    };

    // Clone Arcs for state checking
    let originator_assigned_by_originator = Arc::clone(&planetary_mesh::node::test_utils::get_assigned_by_originator_arc(&originator_node_instance));
    let executor1_available_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_available_jobs_on_mesh_arc(&executor1_node_instance));
    let executor1_assigned_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_assigned_jobs_arc(&executor1_node_instance));
    let executor2_available_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_available_jobs_on_mesh_arc(&executor2_node_instance));
    let executor2_assigned_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_assigned_jobs_arc(&executor2_node_instance));

    // 5. Set Mock Reputations on Originator Node (both below policy threshold)
    let mut mock_reputations = HashMap::new();
    mock_reputations.insert(executor1_did.clone(), 60.0); // Executor 1: Rep 60 (below 80)
    mock_reputations.insert(executor2_did.clone(), 75.0); // Executor 2: Rep 75 (below 80)
    
    println!("LOW_REP_TEST: Setting mock reputations on originator: {:?}", mock_reputations);
    test_utils::command_node_to_set_mock_reputations(&originator_command_tx, mock_reputations)
        .await
        .expect("LOW_REP_TEST: Failed to send SetMockReputations command");

    // Originator announces the job
    println!("LOW_REP_TEST: Announcing job {}: policy min_rep=80", job_id);
    test_utils::command_originator_to_announce_job(&originator_command_tx, job_to_announce.clone())
        .await
        .expect("LOW_REP_TEST: Failed to send AnnounceJob command");

    // 6. Executors Submit Bids (prices are within policy limits)
    // Executor 1 waits for job and submits bid
    println!("LOW_REP_TEST: Executor 1 waiting for job announcement...");
    timeout(Duration::from_secs(10), async {
        loop {
            if executor1_available_jobs.read().unwrap().contains_key(&job_id) { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("LOW_REP_TEST: Executor 1 timed out waiting for job announcement");
    let bid_by_executor1 = Bid { job_id: job_id.clone(), executor_did: executor1_did.clone(), price: 100, timestamp: Utc::now().timestamp() };
    println!("LOW_REP_TEST: Executor 1 (Rep 60) submitting bid for job {}", job_id);
    test_utils::command_executor_to_submit_bid(&executor1_command_tx, bid_by_executor1.clone()).await.expect("LOW_REP_TEST: Executor 1 failed to submit bid");

    // Executor 2 waits for job and submits bid
    println!("LOW_REP_TEST: Executor 2 waiting for job announcement...");
    timeout(Duration::from_secs(10), async {
        loop {
            if executor2_available_jobs.read().unwrap().contains_key(&job_id) { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("LOW_REP_TEST: Executor 2 timed out waiting for job announcement");
    let bid_by_executor2 = Bid { job_id: job_id.clone(), executor_did: executor2_did.clone(), price: 110, timestamp: Utc::now().timestamp() };
    println!("LOW_REP_TEST: Executor 2 (Rep 75) submitting bid for job {}", job_id);
    test_utils::command_executor_to_submit_bid(&executor2_command_tx, bid_by_executor2.clone()).await.expect("LOW_REP_TEST: Executor 2 failed to submit bid");

    // 7. Assertions: No assignment should occur
    println!("LOW_REP_TEST: Waiting to confirm NO job assignment occurs due to low reputation...");
    // Wait for a duration longer than the typical assignment interval to be reasonably sure.
    // The select_executor_for_originated_jobs interval in MeshNode is key here.
    // Assuming it's around 10-15 seconds, waiting for 20-25 seconds should be sufficient.
    tokio::time::sleep(Duration::from_secs(25)).await; 

    let is_assigned_by_originator = originator_assigned_by_originator.read().unwrap().contains(&job_id);
    assert!(!is_assigned_by_originator, "LOW_REP_TEST: Job {} was unexpectedly assigned by originator!", job_id);

    let executor1_has_job = executor1_assigned_jobs.read().unwrap().contains_key(&job_id);
    assert!(!executor1_has_job, "LOW_REP_TEST: Job {} was unexpectedly assigned to Executor 1!", job_id);

    let executor2_has_job = executor2_assigned_jobs.read().unwrap().contains_key(&job_id);
    assert!(!executor2_has_job, "LOW_REP_TEST: Job {} was unexpectedly assigned to Executor 2!", job_id);

    println!("LOW_REP_TEST: Confirmed: Job {} was NOT assigned, as expected due to low reputation of all bidders.", job_id);

    // 8. Teardown
    println!("LOW_REP_TEST: Test steps completed. Tearing down nodes.");
    originator_handle.abort();
    executor1_handle.abort();
    executor2_handle.abort();
    // Optional: await handles and check for errors, but for this test, primary check is no assignment.

    println!("LOW_REP_TEST: Finished.");
    assert!(true, "Test for policy rejecting all bidders due to low reputation completed."); 
}

// New test function for policy edge case: high price
#[tokio::test]
#[ignore] // Mark as ignored for now, can be run explicitly
async fn test_policy_rejects_all_bidders_due_to_high_price() {
    // 1. Setup: Keypairs and DIDs for originator and two executors
    let originator_kp = IcnKeyPair::generate();
    let originator_did = originator_kp.did.clone();
    let executor1_kp = IcnKeyPair::generate();
    let executor1_did = executor1_kp.did.clone();
    let executor2_kp = IcnKeyPair::generate();
    let executor2_did = executor2_kp.did.clone();

    println!("HIGH_PRICE_TEST: Originator DID: {}", originator_did);
    println!("HIGH_PRICE_TEST: Executor 1 DID: {}", executor1_did);
    println!("HIGH_PRICE_TEST: Executor 2 DID: {}", executor2_did);

    // 2. Initialize MeshNodes
    let (originator_node_instance, originator_internal_rx, originator_command_tx) = 
        setup_node(originator_kp.clone(), Some("/ip4/127.0.0.1/tcp/0".to_string()), None)
        .await.expect("HIGH_PRICE_TEST: Failed to setup originator node");
    
    let (executor1_node_instance, executor1_internal_rx, executor1_command_tx) = 
        setup_node(executor1_kp.clone(), Some("/ip4/127.0.0.1/tcp/0".to_string()), None)
        .await.expect("HIGH_PRICE_TEST: Failed to setup executor1 node");

    let (executor2_node_instance, executor2_internal_rx, executor2_command_tx) = 
        setup_node(executor2_kp.clone(), Some("/ip4/127.0.0.1/tcp/0".to_string()), None)
        .await.expect("HIGH_PRICE_TEST: Failed to setup executor2 node");
    
    // Start event loops
    let originator_handle = tokio::spawn(async move { originator_node_instance.run_event_loop(originator_internal_rx).await });
    let executor1_handle = tokio::spawn(async move { executor1_node_instance.run_event_loop(executor1_internal_rx).await });
    let executor2_handle = tokio::spawn(async move { executor2_node_instance.run_event_loop(executor2_internal_rx).await });

    tokio::time::sleep(Duration::from_secs(5)).await; // Allow nodes to start and discover

    // 3. Define an ExecutionPolicy with a low max_price
    let job_execution_policy = ExecutionPolicy {
        min_reputation_score: Some(70.0), // Min reputation (not the limiting factor here)
        max_price: Some(100),             // Max price of 100
        weight_price: Some(0.5),
        weight_reputation: Some(0.5),
        preferred_regions: None,
        required_ccl_level: None,
        custom_policy_script: None,
    };

    // 4. Create and Announce Job by Originator
    let job_id: IcnJobId = format!("test-high-price-reject-{}", Utc::now().timestamp_millis());
    let mesh_job_params = MeshJobParams {
        wasm_cid: "bafyreibmicpv3gzfxmlsx7qvyfigt765hsdgdnkrhdk2qdsdlvgnpvchuq".to_string(),
        description: Some("Test job: policy should reject all due to high price".to_string()),
        execution_policy: Some(job_execution_policy.clone()),
        required_resources_json: r#"{}"#.to_string(), // Minimal resources
        max_execution_time_secs: Some(60),
        output_location: None,
        is_interactive: false,
        stages: None,
        workflow_type: icn_types::mesh::WorkflowType::SingleWasmModule,
        ccl_cid: None,
        trust_requirements: None,
    };
    let job_to_announce = MeshJob {
        job_id: job_id.clone(),
        params: mesh_job_params,
        originator_did: originator_did.clone(),
        originator_org_scope: Some(OrganizationScopeIdentifier::Personal(originator_did.clone())),
        submission_timestamp: Utc::now().timestamp(),
    };

    // Clone Arcs for state checking
    let originator_assigned_by_originator = Arc::clone(&planetary_mesh::node::test_utils::get_assigned_by_originator_arc(&originator_node_instance));
    let executor1_available_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_available_jobs_on_mesh_arc(&executor1_node_instance));
    let executor1_assigned_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_assigned_jobs_arc(&executor1_node_instance));
    let executor2_available_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_available_jobs_on_mesh_arc(&executor2_node_instance));
    let executor2_assigned_jobs = Arc::clone(&planetary_mesh::node::test_utils::get_assigned_jobs_arc(&executor2_node_instance));

    // 5. Set Mock Reputations on Originator Node (both high, acceptable)
    let mut mock_reputations = HashMap::new();
    mock_reputations.insert(executor1_did.clone(), 80.0);
    mock_reputations.insert(executor2_did.clone(), 85.0);
    
    println!("HIGH_PRICE_TEST: Setting mock reputations on originator: {:?}", mock_reputations);
    test_utils::command_node_to_set_mock_reputations(&originator_command_tx, mock_reputations)
        .await
        .expect("HIGH_PRICE_TEST: Failed to send SetMockReputations command");

    // Originator announces the job
    println!("HIGH_PRICE_TEST: Announcing job {}: policy max_price=100", job_id);
    test_utils::command_originator_to_announce_job(&originator_command_tx, job_to_announce.clone())
        .await
        .expect("HIGH_PRICE_TEST: Failed to send AnnounceJob command");

    // 6. Executors Submit Bids (prices are all ABOVE policy limits)
    // Executor 1 waits for job and submits bid
    println!("HIGH_PRICE_TEST: Executor 1 waiting for job announcement...");
    timeout(Duration::from_secs(10), async {
        loop {
            if executor1_available_jobs.read().unwrap().contains_key(&job_id) { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("HIGH_PRICE_TEST: Executor 1 timed out waiting for job announcement");
    let bid_by_executor1 = Bid { job_id: job_id.clone(), executor_did: executor1_did.clone(), price: 110, timestamp: Utc::now().timestamp() }; // Price 110 > 100
    println!("HIGH_PRICE_TEST: Executor 1 (Price 110) submitting bid for job {}", job_id);
    test_utils::command_executor_to_submit_bid(&executor1_command_tx, bid_by_executor1.clone()).await.expect("HIGH_PRICE_TEST: Executor 1 failed to submit bid");

    // Executor 2 waits for job and submits bid
    println!("HIGH_PRICE_TEST: Executor 2 waiting for job announcement...");
    timeout(Duration::from_secs(10), async {
        loop {
            if executor2_available_jobs.read().unwrap().contains_key(&job_id) { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("HIGH_PRICE_TEST: Executor 2 timed out waiting for job announcement");
    let bid_by_executor2 = Bid { job_id: job_id.clone(), executor_did: executor2_did.clone(), price: 120, timestamp: Utc::now().timestamp() }; // Price 120 > 100
    println!("HIGH_PRICE_TEST: Executor 2 (Price 120) submitting bid for job {}", job_id);
    test_utils::command_executor_to_submit_bid(&executor2_command_tx, bid_by_executor2.clone()).await.expect("HIGH_PRICE_TEST: Executor 2 failed to submit bid");

    // 7. Assertions: No assignment should occur
    println!("HIGH_PRICE_TEST: Waiting to confirm NO job assignment occurs due to high prices...");
    tokio::time::sleep(Duration::from_secs(25)).await; 

    let is_assigned_by_originator = originator_assigned_by_originator.read().unwrap().contains(&job_id);
    assert!(!is_assigned_by_originator, "HIGH_PRICE_TEST: Job {} was unexpectedly assigned by originator!", job_id);

    let executor1_has_job = executor1_assigned_jobs.read().unwrap().contains_key(&job_id);
    assert!(!executor1_has_job, "HIGH_PRICE_TEST: Job {} was unexpectedly assigned to Executor 1!", job_id);

    let executor2_has_job = executor2_assigned_jobs.read().unwrap().contains_key(&job_id);
    assert!(!executor2_has_job, "HIGH_PRICE_TEST: Job {} was unexpectedly assigned to Executor 2!", job_id);

    println!("HIGH_PRICE_TEST: Confirmed: Job {} was NOT assigned, as expected due to high prices of all bidders.", job_id);

    // 8. Teardown
    println!("HIGH_PRICE_TEST: Test steps completed. Tearing down nodes.");
    originator_handle.abort();
    executor1_handle.abort();
    executor2_handle.abort();

    println!("HIGH_PRICE_TEST: Finished.");
    assert!(true, "Test for policy rejecting all bidders due to high price completed."); 
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

    /// Get the verified reputation records from a node for testing.
    pub fn get_verified_reputation_records_arc(
        mesh_node: &MeshNode,
    ) -> Arc<RwLock<HashMap<Cid, ReputationRecord>>> {
        mesh_node.verified_reputation_records.clone()
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

    // New command function to set mock reputations
    pub async fn command_node_to_set_mock_reputations(
        tx: &Sender<NodeCommand>,
        reputations: HashMap<Did, f64>,
    ) -> Result<(), String> {
        tx.send(NodeCommand::SetMockReputations(reputations))
            .await
            .map_err(|e| format!("Failed to send SetMockReputations command: {:?}", e))
    }
}
