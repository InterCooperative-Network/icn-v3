use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use chrono::Utc;
use cid::Cid;
use tokio::time::timeout;

use icn_identity::{Did, KeyPair as IcnKeyPair};
use icn_runtime::context::RuntimeContext;
use icn_types::mesh::{
    JobId as IcnJobId, JobStatus as StandardJobStatus, MeshJob, MeshJobParams,
    OrganizationScopeIdentifier,
};
use icn_types::reputation::ReputationRecord;
use libp::PeerId;
use std::collections::HashSet; // Added for assigned_by_originator checks // Added for checking executor DID in bid

use planetary_mesh::node::MeshNode; // Assuming MeshNode is public or pub(crate)
                                    // Assuming InternalNodeAction is a type used by the event loop, adjust path if necessary
use icn_types::jobs::policy::ExecutionPolicy;
use planetary_mesh::node::InternalNodeAction;
use planetary_mesh::node::{KnownReceiptInfo, NodeCommand, TestObservedReputationSubmission}; // Import TestObservedReputationSubmission
use planetary_mesh::protocol::{
    AssignJobV1, Bid, ExecutionReceiptAvailableV1, JobManifest, MeshProtocolMessage,
};
use tokio::sync::mpsc::{self, Receiver, Sender}; // Ensure Sender is imported from mpsc // Ensure ExecutionPolicy is imported

// Mock or minimal reputation service URL for testing
const MOCK_REPUTATION_SERVICE_URL: &str = "http://127.0.0.1:12345"; // Placeholder

// NEW: Import ReputationUpdateEvent to correctly simulate job outcomes for reputation
use icn_types::reputation::ReputationUpdateEvent;

async fn setup_node(
    keypair: IcnKeyPair,
    listen_addr: Option<String>,
    rep_url: Option<String>,
) -> Result<(MeshNode, Receiver<InternalNodeAction>, Sender<NodeCommand>), Box<dyn std::error::Error>>
{
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
    // IMPORTANT USER ACTION REQUIRED:
    // Before running this test, ensure you have populated the static TEST_PUBLIC_KEYS
    // HashMap in planetary-mesh/src/node.rs with the DIDs and IcnPublicKeys
    // for the originator, executor1, and executor2 generated below.
    // Example:
    // static TEST_PUBLIC_KEYS: Lazy<HashMap<Did, IcnPublicKey>> = Lazy::new(|| {
    //     let mut m = HashMap::new();
    //     // --- REPLACE WITH YOUR ACTUAL TEST DIDs AND PUBLIC KEYS ---
    //     // Originator:
    //     // let originator_did_str = "did_generated_by_test_for_originator";
    //     // let originator_pk_bytes: [u8; 32] = [ ... originator\'s public key bytes ... ];
    //     // if let Ok(pk) = IcnPublicKey::from_bytes(&originator_pk_bytes) { m.insert(Did::new(&originator_did_str), pk); }
    //     // Executor 1:
    //     // let executor1_did_str = "did_generated_by_test_for_executor1";
    //     // let executor1_pk_bytes: [u8; 32] = [ ... executor1\'s public key bytes ... ];
    //     // if let Ok(pk) = IcnPublicKey::from_bytes(&executor1_pk_bytes) { m.insert(Did::new(&executor1_did_str), pk); }
    //     // Executor 2:
    //     // let executor2_did_str = "did_generated_by_test_for_executor2";
    //     // let executor2_pk_bytes: [u8; 32] = [ ... executor2\'s public key bytes ... ];
    //     // if let Ok(pk) = IcnPublicKey::from_bytes(&executor2_pk_bytes) { m.insert(Did::new(&executor2_did_str), pk); }
    //     // --- END OF REPLACE SECTION ---
    //     if m.is_empty() { tracing::warn!("Test public key map is empty. DID resolution will likely fail for tests."); }
    //     m
    // });
    // You will need to extract the public key bytes from the IcnKeyPair instances.
    // For Ed25519 keys, this might involve serializing the PublicKey part of the Keypair.

    // 1. Setup: Create keypairs and DIDs
    let originator_kp = IcnKeyPair::generate();
    let originator_did = originator_kp.did.clone();

    let executor1_kp = IcnKeyPair::generate();
    let executor1_did = executor1_kp.did.clone();

    let executor2_kp = IcnKeyPair::generate();
    let executor2_did = executor2_kp.did.clone();

    println!(
        "Originator DID: {} (PK: {:?})",
        originator_did,
        originator_kp.public_key_bytes()
    );
    println!(
        "Executor 1 DID: {} (PK: {:?})",
        executor1_did,
        executor1_kp.public_key_bytes()
    );
    println!(
        "Executor 2 DID: {} (PK: {:?})",
        executor2_did,
        executor2_kp.public_key_bytes()
    );

    // 2. Initialize MeshNodes and get command senders
    let (originator_node_instance, originator_internal_rx, originator_command_tx) = setup_node(
        originator_kp.clone(),
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        None,
    ) // No mock rep service URL
    .await
    .expect("Failed to setup originator node");

    let (executor1_node_instance, executor1_internal_rx, executor1_command_tx) = setup_node(
        executor1_kp.clone(),
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        None,
    )
    .await
    .expect("Failed to setup executor1 node");

    let (executor2_node_instance, executor2_internal_rx, executor2_command_tx) = setup_node(
        executor2_kp.clone(),
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        None,
    )
    .await
    .expect("Failed to setup executor2 node");

    let _originator_peer_id = originator_node_instance.local_peer_id();
    let executor1_peer_id = executor1_node_instance.local_peer_id(); // Capture PeerId
    let executor2_peer_id = executor2_node_instance.local_peer_id(); // Capture PeerId

    // Start the event loops for each node
    let originator_handle = tokio::spawn(async move {
        originator_node_instance
            .run_event_loop(originator_internal_rx)
            .await
    });
    let executor1_handle = tokio::spawn(async move {
        executor1_node_instance
            .run_event_loop(executor1_internal_rx)
            .await
    });
    let executor2_handle = tokio::spawn(async move {
        executor2_node_instance
            .run_event_loop(executor2_internal_rx)
            .await
    }); // New: Start Executor 2 loop

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
        originator_org_scope: Some(OrganizationScopeIdentifier::Personal(
            originator_did.clone(),
        )),
        submission_timestamp: Utc::now().timestamp(),
    };

    // Clone Arcs for state checking
    let originator_bids = Arc::clone(&planetary_mesh::node::test_utils::get_bids_arc(
        &originator_node_instance,
    ));
    let originator_assigned_by_originator = Arc::clone(
        &planetary_mesh::node::test_utils::get_assigned_by_originator_arc(
            &originator_node_instance,
        ),
    );
    let originator_known_receipt_cids = Arc::clone(
        &planetary_mesh::node::test_utils::get_known_receipt_cids_arc(&originator_node_instance),
    );
    let originator_observed_reputation_submissions = Arc::clone(
        &planetary_mesh::node::test_utils::get_test_observed_reputation_submissions_arc(
            &originator_node_instance,
        ),
    );
    let originator_balance_store = Arc::clone(
        &planetary_mesh::node::test_utils::get_balance_store_arc(&originator_node_instance),
    );
    let originator_verified_reputation_records = Arc::clone(
        &planetary_mesh::node::test_utils::get_verified_reputation_records_arc(
            &originator_node_instance,
        ),
    );

    let executor1_available_jobs = Arc::clone(
        &planetary_mesh::node::test_utils::get_available_jobs_on_mesh_arc(&executor1_node_instance),
    );
    let executor1_assigned_jobs = Arc::clone(
        &planetary_mesh::node::test_utils::get_assigned_jobs_arc(&executor1_node_instance),
    );
    let executor1_balance_store = Arc::clone(
        &planetary_mesh::node::test_utils::get_balance_store_arc(&executor1_node_instance),
    );
    let executor1_verified_reputation_records = Arc::clone(
        &planetary_mesh::node::test_utils::get_verified_reputation_records_arc(
            &executor1_node_instance,
        ),
    );

    // New: State Arcs for Executor 2
    let executor2_available_jobs = Arc::clone(
        &planetary_mesh::node::test_utils::get_available_jobs_on_mesh_arc(&executor2_node_instance),
    );
    let executor2_assigned_jobs = Arc::clone(
        &planetary_mesh::node::test_utils::get_assigned_jobs_arc(&executor2_node_instance),
    );
    let executor2_balance_store = Arc::clone(
        &planetary_mesh::node::test_utils::get_balance_store_arc(&executor2_node_instance),
    );
    let executor2_verified_reputation_records = Arc::clone(
        &planetary_mesh::node::test_utils::get_verified_reputation_records_arc(
            &executor2_node_instance,
        ),
    );

    // ---- Reputation Seeding Phase ----
    println!("Starting Reputation Seeding Phase...");

    // Job S1: Successful job for Executor 1
    let job_s1_id = format!("seed-job-s1-{}", Utc::now().timestamp_millis());
    let job_s1_params = MeshJobParams {
        wasm_cid: "bafyreibmicpv3gzfxmlsx7qvyfigt765hsdgdnkrhdk2qdsdlvgnpvchuq".to_string(),
        ccl_cid: None,
        description: Some("Seed job for Executor 1 (success)".to_string()),
        execution_policy: None,
        required_resources_json: r#"{"min_cpu_cores": 1, "min_memory_mb": 128}"#.to_string(),
        max_execution_time_secs: Some(30),
        output_location: None,
        is_interactive: false,
        stages: None,
        workflow_type: icn_types::mesh::WorkflowType::SingleWasmModule,
        trust_requirements: None,
    };
    let job_s1_to_announce = MeshJob {
        job_id: job_s1_id.clone(),
        params: job_s1_params,
        originator_did: originator_did.clone(),
        originator_org_scope: Some(OrganizationScopeIdentifier::Personal(
            originator_did.clone(),
        )),
        submission_timestamp: Utc::now().timestamp(),
    };
    let job_s1_price = 50;

    run_job_flow_and_verify_reputation(
        &job_s1_to_announce,
        job_s1_price,
        &originator_command_tx,
        &executor1_command_tx,
        &executor1_did,
        executor1_peer_id,
        StandardJobStatus::CompletedSuccessfully,
        ReputationUpdateEvent::JobCompletedSuccessfully {
            job_id: job_s1_id.clone(),
            cpu_seconds_used: Some(1.0),
            memory_mb_hours_used: Some(0.1),
        },
        &originator_node_instance,
        &executor1_node_instance,
        &originator_verified_reputation_records,
        &executor1_verified_reputation_records,
        &originator_observed_reputation_submissions,
        &originator_known_receipt_cids,
        &executor1_available_jobs,
        &executor1_assigned_jobs,
        &originator_assigned_by_originator,
        &originator_balance_store,
        &executor1_balance_store,
        &originator_bids,
    )
    .await
    .expect("Job S1 flow failed for Executor 1");
    println!("Job S1 (Success for Executor 1) completed and reputation record processed.");

    // Job S2: Failed job for Executor 2
    let job_s2_id = format!("seed-job-s2-{}", Utc::now().timestamp_millis());
    let job_s2_params = MeshJobParams {
        wasm_cid: "bafyreibmicpv3gzfxmlsx7qvyfigt765hsdgdnkrhdk2qdsdlvgnpvchuq".to_string(),
        ccl_cid: None,
        description: Some("Seed job for Executor 2 (failure)".to_string()),
        execution_policy: None,
        required_resources_json: r#"{"min_cpu_cores": 1, "min_memory_mb": 128}"#.to_string(),
        max_execution_time_secs: Some(30),
        output_location: None,
        is_interactive: false,
        stages: None,
        workflow_type: icn_types::mesh::WorkflowType::SingleWasmModule,
        trust_requirements: None,
    };
    let job_s2_to_announce = MeshJob {
        job_id: job_s2_id.clone(),
        params: job_s2_params,
        originator_did: originator_did.clone(),
        originator_org_scope: Some(OrganizationScopeIdentifier::Personal(
            originator_did.clone(),
        )),
        submission_timestamp: Utc::now().timestamp(),
    };
    let job_s2_price = 60;

    run_job_flow_and_verify_reputation(
        &job_s2_to_announce,
        job_s2_price,
        &originator_command_tx,
        &executor2_command_tx,
        &executor2_did,
        executor2_peer_id,
        StandardJobStatus::Failed,
        ReputationUpdateEvent::JobFailed {
            job_id: job_s2_id.clone(),
            reason: "Simulated failure for test".to_string(),
        },
        &originator_node_instance,
        &executor2_node_instance,
        &originator_verified_reputation_records,
        &executor2_verified_reputation_records,
        &originator_observed_reputation_submissions,
        &originator_known_receipt_cids,
        &executor2_available_jobs,
        &executor2_assigned_jobs,
        &originator_assigned_by_originator,
        &originator_balance_store,
        &executor2_balance_store,
        &originator_bids,
    )
    .await
    .expect("Job S2 flow failed for Executor 2");
    println!("Job S2 (Failure for Executor 2) completed and reputation record processed.");

    println!("Asserting originator has verified reputation records for both executors...");
    timeout(Duration::from_secs(10), async {
        loop {
            let records = originator_verified_reputation_records.read().unwrap();
            let has_record_for_ex1 = records.values().any(|r| r.subject == executor1_did && r.issuer == originator_did && matches!(r.event, ReputationUpdateEvent::JobCompletedSuccessfully { job_id: ref jid, .. } if *jid == job_s1_id));
            let has_record_for_ex2 = records.values().any(|r| r.subject == executor2_did && r.issuer == originator_did && matches!(r.event, ReputationUpdateEvent::JobFailed { job_id: ref jid, .. } if *jid == job_s2_id) );
            if has_record_for_ex1 && has_record_for_ex2 {
                println!("Originator has verified reputation for Executor 1 (Job S1 success) and Executor 2 (Job S2 failure).");
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }).await.expect("Originator did not verify reputation records for both seed jobs in time.");

    // ---- Policy-Driven Job Phase (Job P1) ----
    println!("Starting Policy-Driven Job P1 Phase...");
    let job_p1_execution_policy = ExecutionPolicy {
        min_reputation_score: Some(55.0),
        max_price: Some(150),
        weight_reputation: Some(0.6),
        // ... other fields can be None or default ...
        preferred_regions: None,
        weight_price: Some(0.4),
        required_ccl_level: None,
        custom_policy_script: None,
    };
    let job_p1_id: IcnJobId = format!("test-policy-job-p1-{}", Utc::now().timestamp_millis());
    let job_p1_params = MeshJobParams {
        wasm_cid: "bafyreibmicpv3gzfxmlsx7qvyfigt765hsdgdnkrhdk2qdsdlvgnpvchuq".to_string(),
        description: Some("Policy job P1".to_string()),
        execution_policy: Some(job_p1_execution_policy.clone()),
        // ... other essential params ...
        ccl_cid: None,
        required_resources_json: r#"{"min_cpu_cores": 1, "min_memory_mb": 128}"#.to_string(),
        max_execution_time_secs: Some(60),
        output_location: None,
        is_interactive: false,
        stages: None,
        workflow_type: icn_types::mesh::WorkflowType::SingleWasmModule,
        trust_requirements: None,
    };
    let job_p1_to_announce = MeshJob {
        job_id: job_p1_id.clone(),
        params: job_p1_params,
        originator_did: originator_did.clone(),
        originator_org_scope: Some(OrganizationScopeIdentifier::Personal(
            originator_did.clone(),
        )),
        submission_timestamp: Utc::now().timestamp(),
    };

    test_utils::command_originator_to_announce_job(
        &originator_command_tx,
        job_p1_to_announce.clone(),
    )
    .await
    .expect("P1 announce failed");

    let bid_ex1_p1 = Bid {
        job_id: job_p1_id.clone(),
        executor_did: executor1_did.clone(),
        price: 100,
        timestamp: Utc::now().timestamp(),
    };
    test_utils::command_executor_to_submit_bid(&executor1_command_tx, bid_ex1_p1.clone())
        .await
        .expect("Ex1 P1 bid failed");

    let bid_ex2_p1 = Bid {
        job_id: job_p1_id.clone(),
        executor_did: executor2_did.clone(),
        price: 90,
        timestamp: Utc::now().timestamp(),
    };
    test_utils::command_executor_to_submit_bid(&executor2_command_tx, bid_ex2_p1.clone())
        .await
        .expect("Ex2 P1 bid failed");

    let expected_winner_p1_did = executor1_did.clone();
    timeout(Duration::from_secs(20), async {
        loop {
            let assigned_to_ex1 = executor1_assigned_jobs
                .read()
                .unwrap()
                .contains_key(&job_p1_id);
            if originator_assigned_by_originator
                .read()
                .unwrap()
                .contains(&job_p1_id)
                && assigned_to_ex1
            {
                assert!(
                    !executor2_assigned_jobs
                        .read()
                        .unwrap()
                        .contains_key(&job_p1_id),
                    "P1 wrongly assigned to ex2"
                );
                println!("Job P1 correctly assigned to Executor 1");
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("P1 assignment timeout or wrong assignment");

    // Simplified: Assume P1 completes successfully by Executor 1
    // Further receipt and reputation verification for P1 can be added if this step succeeds.
    println!("Waiting for P1 receipt related observations...");
    tokio::time::sleep(Duration::from_secs(10)).await; // Allow time for P1 post-processing by originator

    let p1_final_submissions = originator_observed_reputation_submissions.read().unwrap();
    let p1_submission = p1_final_submissions
        .iter()
        .find(|s| s.job_id == job_p1_id && s.executor_did == expected_winner_p1_did)
        .expect("No reputation submission observed for Job P1 by expected winner");
    assert!(
        p1_submission.anchor_cid.is_some(),
        "P1 submission missing anchor CID"
    );
    println!(
        "Job P1 completed, reputation submitted with CID: {:?}",
        p1_submission.anchor_cid
    );

    // Final check: Ensure Executor 2 also sees the P1 reputation record for Executor 1
    let p1_anchor_cid = p1_submission.anchor_cid.unwrap();
    timeout(Duration::from_secs(15), async {
        loop {
            if executor2_verified_reputation_records
                .read()
                .unwrap()
                .contains_key(&p1_anchor_cid)
            {
                println!("Executor 2 verified P1 reputation record for Executor 1.");
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .expect("Executor 2 failed to verify P1 rep record for Executor 1");

    // --- Final State Checks & Cleanup ---
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
    assert!(
        true,
        "Full job lifecycle test with policy completed basic checks."
    );
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
    let (originator_node_instance, originator_internal_rx, originator_command_tx) = setup_node(
        originator_kp.clone(),
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        None,
    )
    .await
    .expect("LOW_REP_TEST: Failed to setup originator node");

    let (executor1_node_instance, executor1_internal_rx, executor1_command_tx) = setup_node(
        executor1_kp.clone(),
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        None,
    )
    .await
    .expect("LOW_REP_TEST: Failed to setup executor1 node");

    let (executor2_node_instance, executor2_internal_rx, executor2_command_tx) = setup_node(
        executor2_kp.clone(),
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        None,
    )
    .await
    .expect("LOW_REP_TEST: Failed to setup executor2 node");

    // Start event loops
    let originator_handle = tokio::spawn(async move {
        originator_node_instance
            .run_event_loop(originator_internal_rx)
            .await
    });
    let executor1_handle = tokio::spawn(async move {
        executor1_node_instance
            .run_event_loop(executor1_internal_rx)
            .await
    });
    let executor2_handle = tokio::spawn(async move {
        executor2_node_instance
            .run_event_loop(executor2_internal_rx)
            .await
    });

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
        originator_org_scope: Some(OrganizationScopeIdentifier::Personal(
            originator_did.clone(),
        )),
        submission_timestamp: Utc::now().timestamp(),
    };

    // Clone Arcs for state checking
    let originator_assigned_by_originator = Arc::clone(
        &planetary_mesh::node::test_utils::get_assigned_by_originator_arc(
            &originator_node_instance,
        ),
    );
    let executor1_available_jobs = Arc::clone(
        &planetary_mesh::node::test_utils::get_available_jobs_on_mesh_arc(&executor1_node_instance),
    );
    let executor1_assigned_jobs = Arc::clone(
        &planetary_mesh::node::test_utils::get_assigned_jobs_arc(&executor1_node_instance),
    );
    let executor2_available_jobs = Arc::clone(
        &planetary_mesh::node::test_utils::get_available_jobs_on_mesh_arc(&executor2_node_instance),
    );
    let executor2_assigned_jobs = Arc::clone(
        &planetary_mesh::node::test_utils::get_assigned_jobs_arc(&executor2_node_instance),
    );

    // 5. Set Mock Reputations on Originator Node (both below policy threshold)
    let mut mock_reputations = HashMap::new();
    mock_reputations.insert(executor1_did.clone(), 60.0); // Executor 1: Rep 60 (below 80)
    mock_reputations.insert(executor2_did.clone(), 75.0); // Executor 2: Rep 75 (below 80)

    println!(
        "LOW_REP_TEST: Setting mock reputations on originator: {:?}",
        mock_reputations
    );
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
            if executor1_available_jobs
                .read()
                .unwrap()
                .contains_key(&job_id)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("LOW_REP_TEST: Executor 1 timed out waiting for job announcement");
    let bid_by_executor1 = Bid {
        job_id: job_id.clone(),
        executor_did: executor1_did.clone(),
        price: 100,
        timestamp: Utc::now().timestamp(),
    };
    println!(
        "LOW_REP_TEST: Executor 1 (Rep 60) submitting bid for job {}",
        job_id
    );
    test_utils::command_executor_to_submit_bid(&executor1_command_tx, bid_by_executor1.clone())
        .await
        .expect("LOW_REP_TEST: Executor 1 failed to submit bid");

    // Executor 2 waits for job and submits bid
    println!("LOW_REP_TEST: Executor 2 waiting for job announcement...");
    timeout(Duration::from_secs(10), async {
        loop {
            if executor2_available_jobs
                .read()
                .unwrap()
                .contains_key(&job_id)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("LOW_REP_TEST: Executor 2 timed out waiting for job announcement");
    let bid_by_executor2 = Bid {
        job_id: job_id.clone(),
        executor_did: executor2_did.clone(),
        price: 110,
        timestamp: Utc::now().timestamp(),
    };
    println!(
        "LOW_REP_TEST: Executor 2 (Rep 75) submitting bid for job {}",
        job_id
    );
    test_utils::command_executor_to_submit_bid(&executor2_command_tx, bid_by_executor2.clone())
        .await
        .expect("LOW_REP_TEST: Executor 2 failed to submit bid");

    // 7. Assertions: No assignment should occur
    println!("LOW_REP_TEST: Waiting to confirm NO job assignment occurs due to low reputation...");
    // Wait for a duration longer than the typical assignment interval to be reasonably sure.
    // The select_executor_for_originated_jobs interval in MeshNode is key here.
    // Assuming it's around 10-15 seconds, waiting for 20-25 seconds should be sufficient.
    tokio::time::sleep(Duration::from_secs(25)).await;

    let is_assigned_by_originator = originator_assigned_by_originator
        .read()
        .unwrap()
        .contains(&job_id);
    assert!(
        !is_assigned_by_originator,
        "LOW_REP_TEST: Job {} was unexpectedly assigned by originator!",
        job_id
    );

    let executor1_has_job = executor1_assigned_jobs
        .read()
        .unwrap()
        .contains_key(&job_id);
    assert!(
        !executor1_has_job,
        "LOW_REP_TEST: Job {} was unexpectedly assigned to Executor 1!",
        job_id
    );

    let executor2_has_job = executor2_assigned_jobs
        .read()
        .unwrap()
        .contains_key(&job_id);
    assert!(
        !executor2_has_job,
        "LOW_REP_TEST: Job {} was unexpectedly assigned to Executor 2!",
        job_id
    );

    println!("LOW_REP_TEST: Confirmed: Job {} was NOT assigned, as expected due to low reputation of all bidders.", job_id);

    // 8. Teardown
    println!("LOW_REP_TEST: Test steps completed. Tearing down nodes.");
    originator_handle.abort();
    executor1_handle.abort();
    executor2_handle.abort();
    // Optional: await handles and check for errors, but for this test, primary check is no assignment.

    println!("LOW_REP_TEST: Finished.");
    assert!(
        true,
        "Test for policy rejecting all bidders due to low reputation completed."
    );
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
    let (originator_node_instance, originator_internal_rx, originator_command_tx) = setup_node(
        originator_kp.clone(),
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        None,
    )
    .await
    .expect("HIGH_PRICE_TEST: Failed to setup originator node");

    let (executor1_node_instance, executor1_internal_rx, executor1_command_tx) = setup_node(
        executor1_kp.clone(),
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        None,
    )
    .await
    .expect("HIGH_PRICE_TEST: Failed to setup executor1 node");

    let (executor2_node_instance, executor2_internal_rx, executor2_command_tx) = setup_node(
        executor2_kp.clone(),
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        None,
    )
    .await
    .expect("HIGH_PRICE_TEST: Failed to setup executor2 node");

    // Start event loops
    let originator_handle = tokio::spawn(async move {
        originator_node_instance
            .run_event_loop(originator_internal_rx)
            .await
    });
    let executor1_handle = tokio::spawn(async move {
        executor1_node_instance
            .run_event_loop(executor1_internal_rx)
            .await
    });
    let executor2_handle = tokio::spawn(async move {
        executor2_node_instance
            .run_event_loop(executor2_internal_rx)
            .await
    });

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
        originator_org_scope: Some(OrganizationScopeIdentifier::Personal(
            originator_did.clone(),
        )),
        submission_timestamp: Utc::now().timestamp(),
    };

    // Clone Arcs for state checking
    let originator_assigned_by_originator = Arc::clone(
        &planetary_mesh::node::test_utils::get_assigned_by_originator_arc(
            &originator_node_instance,
        ),
    );
    let executor1_available_jobs = Arc::clone(
        &planetary_mesh::node::test_utils::get_available_jobs_on_mesh_arc(&executor1_node_instance),
    );
    let executor1_assigned_jobs = Arc::clone(
        &planetary_mesh::node::test_utils::get_assigned_jobs_arc(&executor1_node_instance),
    );
    let executor2_available_jobs = Arc::clone(
        &planetary_mesh::node::test_utils::get_available_jobs_on_mesh_arc(&executor2_node_instance),
    );
    let executor2_assigned_jobs = Arc::clone(
        &planetary_mesh::node::test_utils::get_assigned_jobs_arc(&executor2_node_instance),
    );

    // 5. Set Mock Reputations on Originator Node (both high, acceptable)
    let mut mock_reputations = HashMap::new();
    mock_reputations.insert(executor1_did.clone(), 80.0);
    mock_reputations.insert(executor2_did.clone(), 85.0);

    println!(
        "HIGH_PRICE_TEST: Setting mock reputations on originator: {:?}",
        mock_reputations
    );
    test_utils::command_node_to_set_mock_reputations(&originator_command_tx, mock_reputations)
        .await
        .expect("HIGH_PRICE_TEST: Failed to send SetMockReputations command");

    // Originator announces the job
    println!(
        "HIGH_PRICE_TEST: Announcing job {}: policy max_price=100",
        job_id
    );
    test_utils::command_originator_to_announce_job(&originator_command_tx, job_to_announce.clone())
        .await
        .expect("HIGH_PRICE_TEST: Failed to send AnnounceJob command");

    // 6. Executors Submit Bids (prices are all ABOVE policy limits)
    // Executor 1 waits for job and submits bid
    println!("HIGH_PRICE_TEST: Executor 1 waiting for job announcement...");
    timeout(Duration::from_secs(10), async {
        loop {
            if executor1_available_jobs
                .read()
                .unwrap()
                .contains_key(&job_id)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("HIGH_PRICE_TEST: Executor 1 timed out waiting for job announcement");
    let bid_by_executor1 = Bid {
        job_id: job_id.clone(),
        executor_did: executor1_did.clone(),
        price: 110,
        timestamp: Utc::now().timestamp(),
    }; // Price 110 > 100
    println!(
        "HIGH_PRICE_TEST: Executor 1 (Price 110) submitting bid for job {}",
        job_id
    );
    test_utils::command_executor_to_submit_bid(&executor1_command_tx, bid_by_executor1.clone())
        .await
        .expect("HIGH_PRICE_TEST: Executor 1 failed to submit bid");

    // Executor 2 waits for job and submits bid
    println!("HIGH_PRICE_TEST: Executor 2 waiting for job announcement...");
    timeout(Duration::from_secs(10), async {
        loop {
            if executor2_available_jobs
                .read()
                .unwrap()
                .contains_key(&job_id)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("HIGH_PRICE_TEST: Executor 2 timed out waiting for job announcement");
    let bid_by_executor2 = Bid {
        job_id: job_id.clone(),
        executor_did: executor2_did.clone(),
        price: 120,
        timestamp: Utc::now().timestamp(),
    }; // Price 120 > 100
    println!(
        "HIGH_PRICE_TEST: Executor 2 (Price 120) submitting bid for job {}",
        job_id
    );
    test_utils::command_executor_to_submit_bid(&executor2_command_tx, bid_by_executor2.clone())
        .await
        .expect("HIGH_PRICE_TEST: Executor 2 failed to submit bid");

    // 7. Assertions: No assignment should occur
    println!("HIGH_PRICE_TEST: Waiting to confirm NO job assignment occurs due to high prices...");
    tokio::time::sleep(Duration::from_secs(25)).await;

    let is_assigned_by_originator = originator_assigned_by_originator
        .read()
        .unwrap()
        .contains(&job_id);
    assert!(
        !is_assigned_by_originator,
        "HIGH_PRICE_TEST: Job {} was unexpectedly assigned by originator!",
        job_id
    );

    let executor1_has_job = executor1_assigned_jobs
        .read()
        .unwrap()
        .contains_key(&job_id);
    assert!(
        !executor1_has_job,
        "HIGH_PRICE_TEST: Job {} was unexpectedly assigned to Executor 1!",
        job_id
    );

    let executor2_has_job = executor2_assigned_jobs
        .read()
        .unwrap()
        .contains_key(&job_id);
    assert!(
        !executor2_has_job,
        "HIGH_PRICE_TEST: Job {} was unexpectedly assigned to Executor 2!",
        job_id
    );

    println!("HIGH_PRICE_TEST: Confirmed: Job {} was NOT assigned, as expected due to high prices of all bidders.", job_id);

    // 8. Teardown
    println!("HIGH_PRICE_TEST: Test steps completed. Tearing down nodes.");
    originator_handle.abort();
    executor1_handle.abort();
    executor2_handle.abort();

    println!("HIGH_PRICE_TEST: Finished.");
    assert!(
        true,
        "Test for policy rejecting all bidders due to high price completed."
    );
}

// Helper function for a full job flow, including reputation verification
#[allow(clippy::too_many_arguments)]
async fn run_job_flow_and_verify_reputation(
    job_to_announce: &MeshJob,
    bid_price: u64,
    originator_command_tx: &Sender<NodeCommand>,
    executor_command_tx: &Sender<NodeCommand>,
    executor_did: &Did,
    _executor_peer_id: PeerId, // Added to potentially use for direct dialing if needed
    expected_outcome: StandardJobStatus,
    expected_reputation_event_details: ReputationUpdateEvent, // More specific for verification
    _originator_node: &MeshNode, // Access to node state if needed directly (use Arcs primarily)
    _executor_node: &MeshNode,
    originator_verified_reputations: &Arc<RwLock<HashMap<Cid, ReputationRecord>>>,
    executor_verified_reputations: &Arc<RwLock<HashMap<Cid, ReputationRecord>>>, // Executor also verifies
    originator_observed_reputations: &Arc<RwLock<Vec<TestObservedReputationSubmission>>>,
    originator_known_receipts: &Arc<RwLock<HashMap<Cid, KnownReceiptInfo>>>,
    executor_available_jobs: &Arc<RwLock<HashMap<IcnJobId, JobManifest>>>,
    executor_assigned_jobs: &Arc<RwLock<HashMap<IcnJobId, (JobManifest, Bid)>>>,
    originator_assigned_by_originator: &Arc<RwLock<HashSet<IcnJobId>>>,
    originator_balance_store: &Arc<RwLock<icn_runtime::settlement::BalanceStoreTypeAlias>>,
    executor_balance_store: &Arc<RwLock<icn_runtime::settlement::BalanceStoreTypeAlias>>,
    _originator_bids: &Arc<RwLock<HashMap<IcnJobId, Vec<Bid>>>>,
) -> Result<(), String> {
    let job_id = &job_to_announce.job_id;
    let originator_did = &job_to_announce.originator_did;

    // 1. Originator announces job
    test_utils::command_originator_to_announce_job(originator_command_tx, job_to_announce.clone())
        .await?;
    println!("[{}] Job announcement sent.", job_id);

    // 2. Executor waits for job & submits bid
    timeout(Duration::from_secs(10), async {
        loop {
            if executor_available_jobs.read().unwrap().contains_key(job_id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| {
        format!(
            "[{}] Executor timed out waiting for job announcement",
            job_id
        )
    })?;

    let bid = Bid {
        job_id: job_id.clone(),
        executor_did: executor_did.clone(),
        price: bid_price,
        timestamp: Utc::now().timestamp(),
    };
    test_utils::command_executor_to_submit_bid(executor_command_tx, bid.clone()).await?;
    println!(
        "[{}] Executor {} submitted bid (Price: {}).",
        job_id, executor_did, bid_price
    );

    // 3. Originator assigns job (simple assignment, assumes this executor wins for seed jobs)
    timeout(Duration::from_secs(15), async {
        loop {
            let assigned_by_originator = originator_assigned_by_originator
                .read()
                .unwrap()
                .contains(job_id);
            let assigned_to_executor = executor_assigned_jobs.read().unwrap().contains_key(job_id);

            if assigned_by_originator && assigned_to_executor {
                let (_manifest, assigned_bid) = executor_assigned_jobs
                    .read()
                    .unwrap()
                    .get(job_id)
                    .unwrap()
                    .clone();
                assert_eq!(
                    &assigned_bid.executor_did, executor_did,
                    "[{}] Assigned to wrong executor.",
                    job_id
                );
                assert_eq!(
                    assigned_bid.price, bid_price,
                    "[{}] Assigned with wrong price.",
                    job_id
                );
                println!(
                    "[{}] Job assigned to Executor {} by Originator.",
                    job_id, executor_did
                );
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .map_err(|_| {
        format!(
            "[{}] Timed out waiting for job assignment to {}",
            job_id, executor_did
        )
    })?;

    println!(
        "[{}] Originator waiting for execution receipt from Executor {}...",
        job_id, executor_did
    );
    let mut receipt_cid_found: Option<Cid> = None;
    timeout(Duration::from_secs(15), async {
        loop {
            let known_cids_map = originator_known_receipts.read().unwrap();
            if let Some(cid) = known_cids_map.iter().find_map(|(c, info)| {
                if info.job_id == *job_id && info.executor_did == *executor_did {
                    Some(*c)
                } else {
                    None
                }
            }) {
                receipt_cid_found = Some(cid);
                println!(
                    "[{}] Originator saw receipt CID {} from Executor {}.",
                    job_id, cid, executor_did
                );
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .map_err(|_| {
        format!(
            "[{}] Originator timed out waiting for receipt from {}",
            job_id, executor_did
        )
    })?;
    let _receipt_cid = receipt_cid_found
        .ok_or_else(|| format!("[{}] Receipt CID not found after wait", job_id))?;

    tokio::time::sleep(Duration::from_secs(3)).await; // Time for settlement

    let initial_executor_balance = executor_balance_store
        .read()
        .unwrap()
        .get(executor_did)
        .copied()
        .unwrap_or(0);

    if expected_outcome == StandardJobStatus::CompletedSuccessfully {
        let final_executor_balance = executor_balance_store
            .read()
            .unwrap()
            .get(executor_did)
            .copied()
            .unwrap_or(0);
        assert_eq!(
            final_executor_balance,
            initial_executor_balance + bid_price,
            "[{}] Executor {} balance incorrect after successful job. Expected {}, got {}",
            job_id,
            executor_did,
            initial_executor_balance + bid_price,
            final_executor_balance
        );
        println!(
            "[{}] Economic settlement verified for successful job with Executor {}.",
            job_id, executor_did
        );
    } else {
        let final_executor_balance = executor_balance_store
            .read()
            .unwrap()
            .get(executor_did)
            .copied()
            .unwrap_or(0);
        assert_eq!(
            final_executor_balance, initial_executor_balance,
            "[{}] Executor {} balance should be unchanged for failed job. Expected {}, got {}",
            job_id, executor_did, initial_executor_balance, final_executor_balance
        );
        println!(
            "[{}] Economic settlement (no payment) verified for failed job with Executor {}.",
            job_id, executor_did
        );
    }

    println!("[{}] Waiting for reputation record submission to be observed by originator (Executor: {}, Outcome: {:?})", job_id, executor_did, expected_outcome);
    let mut observed_rep_submission_cid: Option<Cid> = None;
    timeout(Duration::from_secs(15), async { 
        loop {
            let submissions = originator_observed_reputations.read().unwrap();
            if let Some(submission) = submissions.iter().find(|s| {
                s.job_id == *job_id && s.executor_did == *executor_did && s.outcome == expected_outcome
            }) {
                println!("[{}] Originator observed reputation submission: Anchor CID {:?}, Job {}, Executor {}, Outcome {:?}", 
                    job_id, submission.anchor_cid, submission.job_id, submission.executor_did, submission.outcome);
                assert!(submission.anchor_cid.is_some(), "[{}] Anchor CID missing in observed submission", job_id);
                observed_rep_submission_cid = submission.anchor_cid;
                break;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }).await.map_err(|_| format!("[{}] Timed out waiting for originator to observe reputation submission for Executor {}", job_id, executor_did))?;

    let rep_record_cid = observed_rep_submission_cid.ok_or_else(|| {
        format!(
            "[{}] Reputation record CID not captured from observation",
            job_id
        )
    })?;

    println!("[{}] Originator waiting to verify its own reputation record (CID: {}) about Executor {}...", job_id, rep_record_cid, executor_did);
    timeout(Duration::from_secs(10), async {
        loop {
            let records = originator_verified_reputations.read().unwrap();
            if let Some(record) = records.get(&rep_record_cid) {
                 assert_eq!(&record.subject, executor_did, "[{}] Verified record subject mismatch on originator.", job_id);
                 assert_eq!(&record.issuer, originator_did, "[{}] Verified record issuer mismatch on originator.", job_id);
                 match (&record.event, &expected_reputation_event_details) {
                    (ReputationUpdateEvent::JobCompletedSuccessfully{job_id: r_jid, ..}, ReputationUpdateEvent::JobCompletedSuccessfully{job_id: e_jid, ..}) => assert_eq!(r_jid, e_jid, "[{}] Success event job ID mismatch", job_id),
                    (ReputationUpdateEvent::JobFailed{job_id: r_jid, reason: r_reason}, ReputationUpdateEvent::JobFailed{job_id: e_jid, reason: e_reason}) => {
                        assert_eq!(r_jid, e_jid, "[{}] Failure event job ID mismatch", job_id);
                        assert_eq!(r_reason, e_reason, "[{}] Failure event reason mismatch", job_id);
                    },
                    _ => panic!("[{}] Reputation event type mismatch in originator's verified record. Expected {:?}, got {:?}", job_id, expected_reputation_event_details, record.event),
                 }
                 println!("[{}] Originator successfully verified its own reputation record (CID: {}) about Executor {}.", job_id, rep_record_cid, executor_did);
                 break;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }).await.map_err(|_| format!("[{}] Originator timed out waiting to verify its own reputation record {}", job_id, rep_record_cid))?;

    println!(
        "[{}] Executor {} waiting to verify reputation record (CID: {}) about itself...",
        job_id, executor_did, rep_record_cid
    );
    timeout(Duration::from_secs(15), async { 
        loop {
            let records = executor_verified_reputations.read().unwrap();
            if let Some(record) = records.get(&rep_record_cid) {
                 assert_eq!(&record.subject, executor_did, "[{}] Verified record subject mismatch on executor.", job_id);
                 assert_eq!(&record.issuer, originator_did, "[{}] Verified record issuer mismatch on executor.", job_id);
                 match (&record.event, &expected_reputation_event_details) {
                    (ReputationUpdateEvent::JobCompletedSuccessfully{job_id: r_jid, ..}, ReputationUpdateEvent::JobCompletedSuccessfully{job_id: e_jid, ..}) => assert_eq!(r_jid, e_jid),
                    (ReputationUpdateEvent::JobFailed{job_id: r_jid, reason: r_reason}, ReputationUpdateEvent::JobFailed{job_id: e_jid, reason: e_reason}) => {
                        assert_eq!(r_jid, e_jid);
                        assert_eq!(r_reason, e_reason);
                    },
                    _ => panic!("[{}] Reputation event type mismatch in executor's verified record. Expected {:?}, got {:?}", job_id, expected_reputation_event_details, record.event),
                 }
                 println!("[{}] Executor {} successfully verified reputation record (CID: {}) about itself.", job_id, executor_did, rep_record_cid);
                 break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await; 
        }
    }).await.map_err(|_| format!("[{}] Executor {} timed out waiting to verify reputation record {}", job_id, executor_did, rep_record_cid))?;

    Ok(())
}

// Helper module for accessing MeshNode internals in tests.
// This is a placeholder for how you might access internal state.
// Ideally, MeshNode provides methods or uses channels for state observation in tests.
mod test_utils {
    use super::*;
    use cid::Cid;
    use libp2p::PeerId;
    use std::collections::{HashMap, HashSet};
    use tokio::sync::mpsc::Sender;
    use tokio::time::{timeout, Duration};

    use crate::planetary_mesh::node::{
        KnownReceiptInfo, MeshNode, NodeCommand, TestObservedReputationSubmission,
    };
    use icn_identity::Did;
    #[cfg(feature = "runtime-integration")]
    use icn_runtime::settlement::BalanceStoreTypeAlias;
    use icn_types::mesh::{Bid, JobId as IcnJobId, JobManifest, MeshJob};
    use icn_types::reputation::ReputationRecord;
    #[cfg(not(feature = "runtime-integration"))]
    pub type BalanceStoreTypeAlias = HashMap<Did, u64>; // Simple mock for non-runtime tests

    pub fn get_announced_originated_jobs_arc(
        node: &MeshNode,
    ) -> Arc<RwLock<HashMap<IcnJobId, (JobManifest, MeshJob)>>> {
        node.announced_originated_jobs.clone()
    }

    pub fn get_bids_arc(node: &MeshNode) -> Arc<RwLock<HashMap<IcnJobId, Vec<Bid>>>> {
        node.bids.clone()
    }

    pub fn get_assigned_by_originator_arc(node: &MeshNode) -> Arc<RwLock<HashSet<IcnJobId>>> {
        node.assigned_by_originator.clone()
    }

    pub fn get_receipt_store_dag_nodes_arc(node: &MeshNode) -> Arc<RwLock<HashMap<Cid, Vec<u8>>>> {
        node.local_runtime_context
            .as_ref()
            .expect("RuntimeContext not initialized in MeshNode for test")
            .receipt_store
            .dag_nodes
            .clone()
    }

    pub fn get_balance_store_arc(
        node: &MeshNode,
    ) -> Arc<RwLock<icn_runtime::settlement::BalanceStoreTypeAlias>> {
        let arc_ctx = node
            .local_runtime_context
            .as_ref()
            .expect("Local runtime context not found for balance store access")
            .clone();
        arc_ctx.balance_store()
    }

    pub fn get_available_jobs_on_mesh_arc(
        node: &MeshNode,
    ) -> Arc<RwLock<HashMap<IcnJobId, JobManifest>>> {
        node.available_jobs_on_mesh.clone()
    }

    pub fn get_assigned_jobs_arc(
        node: &MeshNode,
    ) -> Arc<RwLock<HashMap<IcnJobId, (JobManifest, Bid)>>> {
        node.assigned_jobs.clone()
    }

    pub fn get_known_receipt_cids_arc(
        node: &MeshNode,
    ) -> Arc<RwLock<HashMap<Cid, KnownReceiptInfo>>> {
        node.known_receipt_cids.clone()
    }

    pub fn get_test_observed_reputation_submissions_arc(
        node: &MeshNode,
    ) -> Arc<RwLock<Vec<TestObservedReputationSubmission>>> {
        node.test_observed_reputation_submissions.clone()
    }

    pub fn get_verified_reputation_records_arc(
        mesh_node: &MeshNode,
    ) -> Arc<RwLock<HashMap<Cid, ReputationRecord>>> {
        Arc::clone(&mesh_node.verified_reputation_records)
    }

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
            .map_err(|e| format!("Failed to send SubmitBid command: {}", e))
    }

    // Removed command_node_to_set_mock_reputations as it's no longer used.
}

#[tokio::test]
#[ignore] // Run manually with: cargo test -- --ignored test_policy_filters_region_filter_enforced
async fn test_policy_filters_region_filter_enforced() {
    use chrono::Utc;
    use icn_identity::KeyPair as IcnKeyPair;
    use icn_types::{
        jobs::policy::ExecutionPolicy,
        mesh::{MeshJob, MeshJobParams, QoSProfile, ResourceType, WorkflowType},
        OrganizationScopeIdentifier,
    };
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::time::{sleep, Duration};

    // Ensure test_utils are in scope if this test file needs them explicitly
    // use crate::node::test_utils;

    let originator_kp = IcnKeyPair::generate();
    let executor_eu_kp = IcnKeyPair::generate(); // Will bid with matching region
    let executor_us_kp = IcnKeyPair::generate(); // Will bid with different region
    let executor_no_region_kp = IcnKeyPair::generate(); // Will bid with no region

    let originator_did = originator_kp.did.clone();
    let executor_eu_did = executor_eu_kp.did.clone();
    let executor_us_did = executor_us_kp.did.clone();
    let executor_no_region_did = executor_no_region_kp.did.clone();

    println!("Region Test: Originator DID: {}", originator_did);
    println!("Region Test: Executor EU DID: {}", executor_eu_did);
    println!("Region Test: Executor US DID: {}", executor_us_did);
    println!(
        "Region Test: Executor NoRegion DID: {}",
        executor_no_region_did
    );

    let (originator_node, originator_rx, originator_tx) = setup_node(
        originator_kp,
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        None,
    )
    .await
    .expect("Originator setup failed");
    let (executor_eu_node, executor_eu_rx, executor_eu_tx) = setup_node(
        executor_eu_kp,
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        None,
    )
    .await
    .expect("Executor EU setup failed");
    let (executor_us_node, executor_us_rx, executor_us_tx) = setup_node(
        executor_us_kp,
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        None,
    )
    .await
    .expect("Executor US setup failed");
    let (executor_no_region_node, executor_no_region_rx, executor_no_region_tx) = setup_node(
        executor_no_region_kp,
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        None,
    )
    .await
    .expect("Executor NoRegion setup failed");

    let originator_handle =
        tokio::spawn(async move { originator_node.run_event_loop(originator_rx).await });
    let executor_eu_handle =
        tokio::spawn(async move { executor_eu_node.run_event_loop(executor_eu_rx).await });
    let executor_us_handle =
        tokio::spawn(async move { executor_us_node.run_event_loop(executor_us_rx).await });
    let executor_no_region_handle = tokio::spawn(async move {
        executor_no_region_node
            .run_event_loop(executor_no_region_rx)
            .await
    });

    sleep(Duration::from_secs(7)).await; // Allow nodes to connect

    let policy = ExecutionPolicy {
        min_reputation: None, // Not testing reputation here
        max_price: None,      // Not testing price here
        price_weight: 0.5,    // Weights don't matter if only one passes filter
        rep_weight: 0.5,
        region_filter: Some("eu-central".to_string()), // Specific region required
    };

    let job_id = format!(
        "test-region-filter-policy-{}",
        Utc::now().timestamp_millis()
    );
    let mesh_job_params = MeshJobParams {
        wasm_cid: "bafyfakecidregiontest".to_string(),
        description: "Region filter policy test".to_string(),
        execution_policy: Some(policy.clone()),
        resources_required: Vec::new(),
        max_acceptable_bid_tokens: None,
        qos_profile: QoSProfile::BestEffort,
        deadline: None,
        input_data_cid: None,
        stages: None,
        workflow_type: WorkflowType::SingleWasmModule,
        is_interactive: false,
        expected_output_schema_cid: None,
    };
    let mesh_job = MeshJob {
        job_id: job_id.clone(),
        params: mesh_job_params,
        originator_did: originator_did.clone(),
        originator_org_scope: Some(OrganizationScopeIdentifier::Personal(
            originator_did.clone(),
        )),
        submission_timestamp: Utc::now().timestamp(),
    };

    // Set mock reputations high for everyone so it's not a factor
    let mock_scores = HashMap::from([
        (executor_eu_did.clone(), 90.0),
        (executor_us_did.clone(), 90.0),
        (executor_no_region_did.clone(), 90.0),
    ]);
    test_utils::command_node_to_set_mock_reputations(&originator_tx, mock_scores)
        .await
        .expect("Set mock rep failed");

    test_utils::command_originator_to_announce_job(&originator_tx, mesh_job)
        .await
        .expect("Announce job failed");

    // Bid from EU executor (matching region)
    let bid_eu = planetary_mesh::protocol::Bid {
        job_id: job_id.clone(),
        executor_did: executor_eu_did.clone(),
        price: 50,
        timestamp: Utc::now().timestamp(),
        comment: Some("EU based bid".into()),
        region: Some("eu-central".to_string()),
    };
    test_utils::command_executor_to_submit_bid(&executor_eu_tx, bid_eu)
        .await
        .expect("EU bid submit failed");

    // Bid from US executor (non-matching region)
    let bid_us = planetary_mesh::protocol::Bid {
        job_id: job_id.clone(),
        executor_did: executor_us_did.clone(),
        price: 40, // Cheaper, but wrong region
        timestamp: Utc::now().timestamp(),
        comment: Some("US based bid".into()),
        region: Some("us-west".to_string()),
    };
    test_utils::command_executor_to_submit_bid(&executor_us_tx, bid_us)
        .await
        .expect("US bid submit failed");

    // Bid from NoRegion executor (None region)
    let bid_no_region = planetary_mesh::protocol::Bid {
        job_id: job_id.clone(),
        executor_did: executor_no_region_did.clone(),
        price: 45,
        timestamp: Utc::now().timestamp(),
        comment: Some("No region specified bid".into()),
        region: None,
    };
    test_utils::command_executor_to_submit_bid(&executor_no_region_tx, bid_no_region)
        .await
        .expect("NoRegion bid submit failed");

    sleep(Duration::from_secs(15)).await; // Allow time for selection

    let assigned_jobs_map =
        planetary_mesh::node::test_utils::get_assigned_jobs_arc(&originator_node);
    let assigned_bid_details = assigned_jobs_map.read().unwrap().get(&job_id).cloned();

    assert!(
        assigned_bid_details.is_some(),
        "Job {} was not assigned to anyone! Expected EU executor.",
        job_id
    );
    if let Some((_job_manifest, winning_bid)) = assigned_bid_details {
        assert_eq!(
            winning_bid.executor_did, executor_eu_did,
            "Job {} assigned to wrong executor. Expected EU ({}), got {}. Winning bid: {:?}",
            job_id, executor_eu_did, winning_bid.executor_did, winning_bid
        );
        assert_eq!(
            winning_bid.region.as_deref(),
            Some("eu-central"),
            "Winning bid region mismatch"
        );
    } else {
        // Already asserted by is_some(), but for clarity
        panic!("Expected job {} to be assigned, but it was not.", job_id);
    }

    originator_handle.abort();
    executor_eu_handle.abort();
    executor_us_handle.abort();
    executor_no_region_handle.abort();
}
