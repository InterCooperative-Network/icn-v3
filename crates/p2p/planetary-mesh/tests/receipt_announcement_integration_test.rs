// crates/p2p/planetary-mesh/tests/receipt_announcement_integration_test.rs

use planetary_mesh::node::MeshNode;
use planetary_mesh::protocol::MeshProtocolMessage;
use icn_identity::{KeyPair, Did};
use icn_runtime::context::{RuntimeContextBuilder}; 
use icn_types::dag_store::MemoryDagStore; 
use icn_types::mesh::{MeshJob, MeshJobParams, JobId as IcnJobId};
use tokio::{sync::{broadcast}, time::{timeout, Duration}};
use std::{collections::{VecDeque, HashMap}, sync::{Arc, Mutex, RwLock}};
use cid::Cid;
use uuid::Uuid;

// === Add MockDagStore for testing anchoring failures ===
use icn_types::dag_store::{DagStore, DagStoreError, DagNode, SearchUniqueResult, ListResult};
use async_trait::async_trait;

#[derive(Debug, Clone)]
struct MockDagStore {
    fail_on_insert: bool,
    // We can add a simple in-memory store here if we want to test successful inserts too for other tests
    // For this specific test, we only care about failing.
    // data: Arc<RwLock<HashMap<Cid, DagNode>>> // Example if we needed to store
}

impl MockDagStore {
    fn new(fail_on_insert: bool) -> Self {
        Self { 
            fail_on_insert,
            // data: Arc::new(RwLock::new(HashMap::new())) 
        }
    }
}

#[async_trait]
impl DagStore for MockDagStore {
    async fn insert(&self, _node: DagNode) -> Result<Cid, DagStoreError> {
        if self.fail_on_insert {
            Err(DagStoreError::Other("Simulated DAG insert failure".to_string()))
        } else {
            // Return a dummy CID if we were to allow success
            Ok(Cid::try_from("bafybeiczssuxtmccagh2h5cy6kvhax2t5h5yaijhc4yrk4h6svoqy3zlce").unwrap())
        }
    }

    async fn get_unique(&self, _cid: &Cid) -> Result<SearchUniqueResult, DagStoreError> {
        Ok(SearchUniqueResult::NotFound)
    }

    async fn list_by_event_type(&self, _event_type: &str, _limit: Option<usize>, _offset: Option<usize>) -> Result<ListResult, DagStoreError> {
        Ok(ListResult{ nodes: vec![], total: 0, limit: 0, offset: 0 })
    }

    async fn list_by_scope_id(&self, _scope_id: &str, _limit: Option<usize>, _offset: Option<usize>) -> Result<ListResult, DagStoreError> {
        Ok(ListResult{ nodes: vec![], total: 0, limit: 0, offset: 0 })
    }
    // Implement other DagStore methods as needed, returning default/empty results
    async fn contains(&self, _cid: &Cid) -> Result<bool, DagStoreError> {
        Ok(false)
    }
    async fn count(&self) -> Result<u64, DagStoreError> {
        Ok(0)
    }
}
// === End of MockDagStore ===

#[tokio::test(flavor = "multi_thread")] // multi-thread to allow independent task execution
async fn test_execution_receipt_announcement_roundtrip() {
    // === Channel for observing announcements ===
    // This channel will receive MeshProtocolMessage, specifically ExecutionReceiptAvailableV1
    let (receipt_event_tx, mut receipt_event_rx) = broadcast::channel::<MeshProtocolMessage>(16);

    // === Setup executor node ===
    let executor_keypair = KeyPair::generate();
    let executor_did = executor_keypair.did.clone();

    // Create a RuntimeContext with an in-memory DAG store for receipts
    let dag_store = MemoryDagStore::new();
    let runtime_ctx = Arc::new(
        RuntimeContextBuilder::new()
            .with_receipt_store(Arc::new(dag_store)) // Ensure receipt_store is set
            .with_executor_id(executor_did.to_string()) // Set executor ID
            .build()
            .expect("Failed to build runtime context"),
    );

    let executor_job_queue: Arc<Mutex<VecDeque<MeshJob>>> = Arc::new(Mutex::new(VecDeque::new()));

    // Pass the receipt_event_tx as the test_job_status_listener_tx
    let (mut executor_node, executor_internal_rx) = MeshNode::new(
        executor_keypair.clone(),
        Some("/ip4/127.0.0.1/tcp/0".to_string()), // Listen on an available port
        executor_job_queue,
        Some(runtime_ctx.clone()),
        Some(receipt_event_tx.clone()), // This is the key for the test shortcut
    )
    .await
    .expect("Failed to create executor node");

    let completed_receipt_cids_store = executor_node.completed_job_receipt_cids.clone();

    // === Start executor node's event loop ===
    tokio::spawn(async move {
        if let Err(e) = executor_node.run_event_loop(executor_internal_rx).await {
            eprintln!("Executor node event loop failed: {:?}", e);
        }
    });

    // Allow some time for the node to start up its listeners
    tokio::time::sleep(Duration::from_millis(500)).await;

    // === Prepare a fake job ===
    let job_id_str: IcnJobId = format!("job-{}", Uuid::new_v4());
    let mesh_job = MeshJob {
        job_id: job_id_str.clone(), 
        params: MeshJobParams {
            wasm_cid: "bafybeigdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string(),
            ..Default::default()
        },
        originator_did: executor_did.clone(),
        submission_timestamp: chrono::Utc::now().timestamp() as u64,
        originator_org_scope: None,
    };

    // === Simulate assignment by inserting job ===
    executor_node
        .assigned_jobs
        .write()
        .expect("Failed to lock assigned_jobs for write")
        .insert(job_id_str.clone(), mesh_job.clone());

    // === Trigger execution ===
    let trigger_result = executor_node.trigger_execution_for_job(&job_id_str).await;
    assert!(trigger_result.is_ok(), "Execution trigger failed: {:?}", trigger_result.err());

    // === Await receipt announcement from the broadcast channel ===
    let announce_timeout = Duration::from_secs(10);
    let announce_result = timeout(announce_timeout, receipt_event_rx.recv()).await;

    assert!(
        announce_result.is_ok(),
        "Timeout: Did not receive receipt announcement within {:?} via broadcast channel",
        announce_timeout
    );
    let event_result = announce_result.unwrap(); 
    assert!(
        event_result.is_ok(),
        "Broadcast channel recv error: {:?}",
        event_result.err()
    );

    let received_message = event_result.unwrap();
    if let MeshProtocolMessage::ExecutionReceiptAvailableV1 {
        job_id: received_job_id,
        receipt_cid: received_receipt_cid_str,
        executor_did: announcer_did,
    } = received_message
    {
        assert_eq!(received_job_id, job_id_str, "Job ID mismatch in announcement");
        assert_eq!(announcer_did, executor_did, "Executor DID mismatch in announcement");
        
        let announced_cid_res: Result<Cid, _> = received_receipt_cid_str.parse();
        assert!(
            announced_cid_res.is_ok(),
            "CID string in announcement is not parseable: {}",
            received_receipt_cid_str
        );
        let announced_cid = announced_cid_res.unwrap();

        let completed_cids_guard = completed_receipt_cids_store
            .read()
            .expect("Failed to lock completed_job_receipt_cids for read");
        let stored_cid_opt = completed_cids_guard.get(&job_id_str);
        assert!(
            stored_cid_opt.is_some(),
            "Receipt CID not found in executor_node.completed_job_receipt_cids for job_id: {}",
            job_id_str
        );
        assert_eq!(
            stored_cid_opt.unwrap(),
            &announced_cid,
            "Stored CID does not match announced CID"
        );
    } else {
        panic!("Unexpected message type received via broadcast channel: {:?}", received_message);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_receipt_announcement_no_anchoring_if_no_runtime_context() {
    // ... existing test_receipt_announcement_no_anchoring_if_no_runtime_context code ...
}

#[tokio::test(flavor = "multi_thread")]
async fn test_receipt_announcement_anchoring_failure() {
    let (receipt_event_tx, mut receipt_event_rx) = broadcast::channel::<MeshProtocolMessage>(16);

    // === Setup Executor Node ===
    let executor_keypair = KeyPair::generate();
    let executor_did = executor_keypair.did.clone();
    
    // Use MockDagStore configured to fail insertions
    let mock_receipt_store = Arc::new(MockDagStore::new(true)); 

    let runtime_ctx = Arc::new(
        RuntimeContextBuilder::new()
            .with_receipt_store(mock_receipt_store.clone()) // Use the mock store
            .with_executor_id(executor_did.to_string())
            .build()
            .expect("Failed to build runtime context"),
    );
    let executor_job_queue: Arc<Mutex<VecDeque<MeshJob>>> = Arc::new(Mutex::new(VecDeque::new()));
    
    let (mut executor_node, executor_internal_rx) = MeshNode::new(
        executor_keypair.clone(),
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        executor_job_queue,
        Some(runtime_ctx.clone()),
        Some(receipt_event_tx.clone()), // For test observation of announcement attempt
    ).await.expect("Failed to create executor node");

    let completed_receipt_cids_store = executor_node.completed_job_receipt_cids.clone();

    tokio::spawn(async move {
        if let Err(e) = executor_node.run_event_loop(executor_internal_rx).await {
            eprintln!("Executor node event loop failed: {:?}", e);
        }
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    // === Prepare and Execute Job ===
    let job_id_str: IcnJobId = format!("job-anchor-fail-{}", Uuid::new_v4());
    let mesh_job = MeshJob {
        job_id: job_id_str.clone(),
        params: MeshJobParams {
            wasm_cid: "bafybeigdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string(),
            ..Default::default()
        },
        originator_did: executor_did.clone(),
        submission_timestamp: chrono::Utc::now().timestamp() as u64,
        originator_org_scope: None,
    };

    executor_node
        .assigned_jobs
        .write()
        .expect("Failed to lock assigned_jobs")
        .insert(job_id_str.clone(), mesh_job.clone());

    let trigger_result = executor_node.trigger_execution_for_job(&job_id_str).await;
    // Execution itself should succeed, even if anchoring later fails within trigger_execution_for_job
    assert!(trigger_result.is_ok(), "Execution trigger should succeed even if anchoring fails internally: {:?}", trigger_result.err());

    // === Await potential receipt announcement (current logic announces even on anchor fail) ===
    let announce_timeout = Duration::from_secs(10);
    let announce_result = timeout(announce_timeout, receipt_event_rx.recv()).await;

    assert!(announce_result.is_ok(), "Timeout waiting for potential receipt announcement");
    let received_message_res = announce_result.unwrap();
    assert!(received_message_res.is_ok(), "Broadcast receive failed while expecting announcement");

    if let MeshProtocolMessage::ExecutionReceiptAvailableV1 {
        job_id: received_job_id,
        receipt_cid: received_receipt_cid_str, // This CID was generated before anchoring attempt
        executor_did: announcer_did,
    } = received_message_res.unwrap()
    {
        assert_eq!(received_job_id, job_id_str, "Job ID mismatch in announcement (anchor fail)");
        assert_eq!(announcer_did, executor_did, "Executor DID mismatch in announcement (anchor fail)");
        assert!(!received_receipt_cid_str.is_empty(), "Received receipt CID should not be empty (anchor fail)");
    } else {
        panic!("Unexpected message type received (anchor fail): {:?}", received_message_res);
    }

    // === CRUCIAL ASSERTION: CID should NOT be in completed_job_receipt_cids if anchoring failed ===
    // This depends on refining trigger_execution_for_job to *not* store the CID if anchor_receipt returns Err.
    // As of the last update to trigger_execution_for_job, it *does* store the CID if receipt.cid() succeeds,
    // *before* attempting to anchor. So this assertion as-is might reflect a need for refinement.
    let completed_cids_guard = completed_receipt_cids_store.read().expect("Failed to lock completed_job_receipt_cids");
    assert!(
        !completed_cids_guard.contains_key(&job_id_str),
        "Receipt CID for job {} SHOULD NOT be stored if anchoring failed, but it was. Current CID: {:?}",
        job_id_str,
        completed_cids_guard.get(&job_id_str)
    );
} 