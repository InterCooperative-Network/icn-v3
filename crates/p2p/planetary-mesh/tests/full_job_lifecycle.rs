"""use std::sync::{Arc, Mutex, RwLock};
use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use cid::Cid;
use tokio::time::timeout;

use icn_identity::{Did, KeyPair as IcnKeyPair};
use icn_types::mesh::{MeshJob, MeshJobParams, JobId as IcnJobId, JobStatus as StandardJobStatus, OrganizationScopeIdentifier};
use icn_types::reputation::ReputationRecord;
use icn_runtime::context::RuntimeContext;

use planetary_mesh::node::MeshNode; // Assuming MeshNode is public or pub(crate)
use planetary_mesh::protocol::{MeshProtocolMessage, JobManifest, Bid}; // Assuming these are needed and accessible

// Mock or minimal reputation service URL for testing
const MOCK_REPUTATION_SERVICE_URL: &str = "http://127.0.0.1:12345"; // Placeholder

async fn setup_node(
    keypair: IcnKeyPair,
    listen_addr: Option<String>,
    rep_url: Option<String>,
) -> Result<MeshNode, Box<dyn std::error::Error>> {
    let runtime_job_queue = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let local_runtime_context = Some(Arc::new(RuntimeContext::new())); // Basic context

    // The new method now returns a tuple (MeshNode, Receiver)
    let (node, _internal_action_rx) = MeshNode::new(
        keypair,
        listen_addr,
        runtime_job_queue,
        local_runtime_context,
        None, // test_job_status_listener_tx
        rep_url,
    )
    .await?;
    Ok(node)
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

    // 2. Initialize MeshNodes
    //    Need to ensure they can discover each other (e.g., through mDNS or explicit peering if mDNS is slow/flaky in tests)
    //    Assign distinct listen addresses if running locally.
    let mut originator_node = setup_node(originator_kp, Some("/ip4/127.0.0.1/tcp/0".to_string()), Some(MOCK_REPUTATION_SERVICE_URL.to_string())).await.expect("Failed to setup originator node");
    let mut executor1_node = setup_node(executor1_kp, Some("/ip4/127.0.0.1/tcp/0".to_string()), Some(MOCK_REPUTATION_SERVICE_URL.to_string())).await.expect("Failed to setup executor1 node");
    
    // TODO: Start the event loops for each node in separate tokio tasks
    // let originator_handle = tokio::spawn(async move { originator_node.run_event_loop(todo!()).await });
    // let executor1_handle = tokio::spawn(async move { executor1_node.run_event_loop(todo!()).await });


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

    // originator_node.announce_job(job_to_announce.clone()).await.expect("Failed to announce job");
    // println!("Job {} announced by originator", job_id);

    // TODO: Step 4: Executor Nodes Submit Bids
    // - Wait for JobAnnouncementV1 to be received by executors (check available_jobs_on_mesh)
    // - Executors construct and send JobBidV1. (This might need a helper `submit_bid` on MeshNode or direct gossipsub publish from test)

    // TODO: Step 5: Originator Selects Bid and Assigns Job
    // - Wait for JobBidV1 to be received by originator (check `bids` map)
    // - Wait for executor_selection_interval to trigger assignment (or trigger manually for test reliability)
    // - Verify AssignJobV1 is sent / executor's assigned_jobs is populated.

    // TODO: Step 6: Executor Executes Job and Announces Receipt
    // - Executor processes assignment.
    // - Simulates execution (e.g., `trigger_execution_for_job` or similar).
    // - Verifies ExecutionReceiptAvailableV1 is announced.

    // TODO: Step 7: Originator Fetches, Verifies, Anchors Receipt, and Settles
    // - Originator receives ExecutionReceiptAvailableV1.
    // - Verifies receipt is fetched, signature-verified, and anchored.
    // - Verifies economic settlement occurs with correct bid price.
    // - Verifies reputation record is constructed and "submitted".

    // TODO: Step 8: Assertions
    // - Check originator's/executor's balances (if mock ledger is used).
    // - Check "submitted" reputation records (if mock reputation service is used).
    // - Check job statuses, completed_job_receipt_cids etc.

    // TODO: Teardown: Shutdown nodes gracefully
    // originator_handle.abort();
    // executor1_handle.abort();

    // For now, a simple assertion to ensure the test runs
    assert!(true, "Basic test structure ran."); 
    // Add a small delay to allow nodes to discover each other if mDNS is used, though explicit peering is better for tests.
    // tokio::time::sleep(Duration::from_secs(5)).await; 
}
"" 