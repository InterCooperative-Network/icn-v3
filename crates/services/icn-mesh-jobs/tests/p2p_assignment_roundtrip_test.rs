use icn_identity::KeyPair as IcnKeyPair;
use icn_mesh_jobs::run_server;
use icn_types::mesh::{MeshJob, MeshJobParams};
use planetary_mesh::node::MeshNode;
use tokio::sync::broadcast;
use planetary_mesh::protocol::MeshProtocolMessage as PlanetaryMeshMessage;
use icn_types::main::JobStatus as StandardJobStatus;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_assign_job_v1_roundtrip() {
    let _ = tracing_subscriber::fmt::try_init();

    let (status_tx, mut status_rx) = broadcast::channel::<PlanetaryMeshMessage>(32);

    let mesh_jobs_key = IcnKeyPair::generate();
    let mesh_jobs_db_url = "sqlite::memory:".to_string();
    let mesh_jobs_http_addr = "127.0.0.1:0".parse().unwrap();
    let mesh_jobs_reputation_url = "http://localhost:8081".to_string();

    info!("Starting icn-mesh-jobs service...");
    let (actual_http_addr, mesh_jobs_p2p_addrs) = run_server(
        mesh_jobs_db_url,
        mesh_jobs_key.clone(),
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        mesh_jobs_reputation_url,
        mesh_jobs_http_addr,
        Some(status_tx.clone()),
    )
    .await
    .expect("icn-mesh-jobs server failed to start");
    info!("icn-mesh-jobs service started. HTTP: {}, P2P Addrs: {:?}", actual_http_addr, mesh_jobs_p2p_addrs);

    let executor_keypair = IcnKeyPair::generate();
    let executor_did_str = executor_keypair.did.to_string();
    info!("Executor DID: {}", executor_did_str);
    let job_queue: Arc<std::sync::Mutex<VecDeque<MeshJob>>> = Arc::new(Default::default());
    let runtime_context = None;

    info!("Creating executor node...");
    let mut executor_node = MeshNode::new(
        executor_keypair.clone(),
        Some("/ip4/127.0.0.1/tcp/0".to_string()),
        job_queue.clone(),
        runtime_context,
        None,
    )
    .await
    .expect("failed to create executor node");

    let assigned_jobs = executor_node.assigned_jobs.clone();
    let executor_listen_addrs = executor_node.swarm.listeners().cloned().collect::<Vec<_>>();
    info!("Executor P2P addresses: {:?}", executor_listen_addrs);

    info!("Spawning executor node event loop...");
    tokio::spawn(async move {
        if let Err(e) = executor_node.run().await {
            tracing::error!("Executor node run loop failed: {}", e);
        }
    });

    info!("Pausing briefly for P2P nodes to discover each other (e.g., via mDNS)...");
    tokio::time::sleep(Duration::from_millis(1000)).await;

    info!("Submitting job to icn-mesh-jobs: {:#?}", create_job_payload);
    let job_resp = client
        .post(&format!("http://{}/jobs", actual_http_addr))
        .json(&create_job_payload)
        .send()
        .await
        .expect("Failed to submit job");
    assert_eq!(job_resp.status(), StatusCode::CREATED, "Job creation failed: {:?}", job_resp.text().await);
    let job_resp_json: serde_json::Value = job_resp.json().await.expect("Failed to parse job creation response");
    let http_created_job_id = job_resp_json["job_id"].as_str().expect("job_id not found in response").to_string();
    info!("Job submitted successfully. HTTP Job ID (CID): {}", http_created_job_id);

    info!("Submitting bid for job: {}", http_created_job_id);
    let bid_payload = json!({
        "job_id": http_created_job_id,
        "bidder": executor_did_str.clone(),
        "price_tokens": 5,
        "execution_estimate": {
            "cpu_seconds": 1.0,
            "memory_megabytes": 128,
            "disk_megabytes": 10
        },
        "metadata": {}
    });
    let bid_resp = client
        .post(&format!("http://{}/jobs/{}/bids", actual_http_addr, http_created_job_id))
        .json(&bid_payload)
        .send()
        .await
        .expect("Failed to submit bid");
    assert_eq!(bid_resp.status(), StatusCode::ACCEPTED, "Bid submission failed: {:?}", bid_resp.text().await);
    info!("Bid submitted successfully for job: {}", http_created_job_id);
    
    info!("Requesting to move job {} to Bidding state...", http_created_job_id);
    let begin_bidding_resp = client
        .post(&format!("http://{}/jobs/{}/begin-bidding", actual_http_addr, http_created_job_id))
        .send()
        .await
        .expect("Failed to call /begin-bidding endpoint");
    assert_eq!(begin_bidding_resp.status(), StatusCode::OK, "Failed to move job to Bidding state: {:?}", begin_bidding_resp.text().await);
    info!("Job {} successfully moved to Bidding state.", http_created_job_id);
    
    info!("Triggering assignment for job: {}", http_created_job_id);
    let assign_resp = client
        .post(&format!("http://{}/jobs/{}/assign", actual_http_addr, http_created_job_id))
        .send()
        .await
        .expect("Failed to assign job");
    assert_eq!(assign_resp.status(), StatusCode::OK, "Job assignment failed: {:?}", assign_resp.text().await);
    info!("Assignment triggered successfully for job: {}", http_created_job_id);

    let p2p_job_id_to_check = http_created_job_id.clone();
    info!("Waiting for executor to receive assignment for P2P Job ID: {}", p2p_job_id_to_check);
    let wait_executor_assigned_result = timeout(Duration::from_secs(10), async {
        loop {
            {
                let jobs_map = assigned_jobs.read().await;
                if jobs_map.contains_key(&p2p_job_id_to_check) {
                    info!("Executor received assignment for job P2P ID: {}", &p2p_job_id_to_check);
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await;
    assert!(wait_executor_assigned_result.is_ok(), "Executor timed out waiting for assignment of job P2P ID: {}", p2p_job_id_to_check);

    info!("Waiting for JobStatusUpdateV1 {Assigned} from icn-mesh-jobs node's P2P listener...");
    let status_update_timeout = Duration::from_secs(10);
    let status_update_result = timeout(status_update_timeout, status_rx.recv()).await;

    assert!(status_update_result.is_ok(), "Timed out waiting for JobStatusUpdateV1 on broadcast channel after {}s", status_update_timeout.as_secs());
    let received_message_result = status_update_result.unwrap();
    assert!(received_message_result.is_ok(), "Error receiving from broadcast channel: {:?}", received_message_result.err());
    let received_message = received_message_result.unwrap();

    match received_message {
        PlanetaryMeshMessage::JobStatusUpdateV1 { job_id: received_p2p_job_id, executor_did: received_executor_did, status: received_status } => {
            info!("Received JobStatusUpdateV1 via test listener: job_id={}, executor_did={}, status={:?}", received_p2p_job_id, received_executor_did, received_status);
            assert_eq!(received_p2p_job_id, p2p_job_id_to_check, "P2P Job ID in status update does not match expected");
            assert_eq!(received_executor_did.to_string(), executor_did_str, "Executor DID in status update does not match");
            
            match received_status {
                StandardJobStatus::Assigned { runner_did } => {
                    assert_eq!(runner_did.to_string(), executor_did_str, "Runner DID in Assigned status does not match executor DID");
                    info!("Successfully verified JobStatusUpdateV1 with Assigned status for P2P Job ID: {}", p2p_job_id_to_check);
                }
                _ => panic!("Received JobStatusUpdateV1 with unexpected status type: {:?}", received_status),
            }
        }
        other_message => {
            panic!("Received unexpected P2P message type on status channel: {:?}", other_message);
        }
    }
    info!("SUCCESS: Full P2P assignment and JobStatusUpdateV1 {Assigned} roundtrip verified.");
} 