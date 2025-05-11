"""use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::time::Duration;

use cid::Cid;
use icn_identity::Did;
use icn_types::jobs::{Bid, JobRequest, JobStatus, ResourceEstimate};
use icn_types::mesh::MeshJobParams;
use icn_types::jobs::policy::ExecutionPolicy; // Correct path for ExecutionPolicy

use icn_services_mesh_jobs::{
    storage::MeshJobStore, // Assuming AppError and other necessary components are pub from lib.rs or main.rs
    sqlite_store::SqliteStore,
    // If AppError is not public, tests might need to define their own error handling or use anyhow
};
use serde_json::json;
use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};
use reqwest::Client;

// Helper to spawn the app and return its address and a DB connection pool (for direct checks if needed)
async fn spawn_app() -> (String, Arc<SqlitePool>, Client) {
    // 0 will request a random available port from the OS
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();
    let address = format!("http://127.0.0.1:{}", port);

    // In-memory SQLite for testing
    let database_url = "sqlite::memory:";

    if !Sqlite::database_exists(&database_url).await.unwrap_or(false) {
        Sqlite::create_database(&database_url).await.expect("Failed to create in-memory DB");
    }

    let pool = Arc::new(SqlitePool::connect(&database_url).await.expect("Failed to connect to in-memory SQLite"));
    
    // Run migrations - ensure path is correct relative to where test is run from
    // or use an absolute path / embed migrations if sqlx supports it well for tests
    // For now, assuming migrations are found relative to crate root.
    // This path needs to be correct for where `cargo test` is executed or configured.
    // If tests are in `tests/` dir, `../migrations` might be needed if migrations are in crate root.
    // Or, if the binary runs migrations, we ensure our test setup does too.
    // The `main.rs` uses `sqlx::migrate!("./migrations")` which is relative to CARGO_MANIFEST_DIR.
    sqlx::migrate!("./migrations") // Adjusted path
        .run(&*pool)
        .await
        .expect("Failed to run database migrations for test");

    let store: Arc<dyn MeshJobStore> = Arc::new(SqliteStore::new(pool.clone()));
    
    // Mock reputation service URL or use a test double if needed
    let reputation_service_url = Arc::new("http://localhost:12345".to_string()); // Mocked, won't be called if reputation is manually set in bids

    let app_router = icn_services_mesh_jobs::app_router(store, reputation_service_url); // Assuming you have a function to build the router

    tokio::spawn(async move {
        axum::Server::from_tcp(listener)
            .unwrap()
            .serve(app_router.into_make_service())
            .await
            .unwrap();
    });
    
    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = Client::new();

    (address, pool, client)
}

#[tokio::test]
async fn sample_governed_assignment_test() {
    let (app_address, db_pool, client) = spawn_app().await;

    // 1. Define ExecutionPolicy
    let execution_policy = ExecutionPolicy {
        rep_weight: 0.5,
        price_weight: 0.5,
        region_filter: None,
        min_reputation: Some(0.7),
    };

    // 2. Define MeshJobParams
    let job_params = MeshJobParams {
        wasm_cid: "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string(), // Example CID
        description: "Test job with min reputation policy".to_string(),
        resources_required: vec![], // Simplified for this test
        qos_profile: icn_types::mesh::QoSProfile::BestEffort,
        deadline: None,
        input_data_cid: None,
        max_acceptable_bid_tokens: None,
        workflow_type: icn_types::mesh::WorkflowType::SingleWasmModule,
        stages: None,
        is_interactive: false,
        expected_output_schema_cid: None,
        execution_policy: Some(execution_policy.clone()),
    };

    // 3. Define Originator DID
    let originator_did = Did("did:key:z6MkrPhff2xRRBAbKz4p2iuaFfk8zKeVhdK8xPzS2J74KzRR".to_string());

    // 4. Create Job Submission Payload
    let create_job_payload = json!({
        "params": job_params,
        "originator_did": originator_did
    });

    // 5. POST to /jobs to create the job
    let response = client
        .post(format!("{}/jobs", app_address))
        .json(&create_job_payload)
        .send()
        .await
        .expect("Failed to execute request to create job.");

    assert_eq!(response.status().as_u16(), 201, "Failed to create job. Response: {:?}", response.text().await);
    let job_creation_response: serde_json::Value = response.json().await.expect("Failed to parse job creation response.");
    let job_id_str = job_creation_response["job_id"].as_str().expect("Job ID not found in creation response.");
    let job_id = Cid::try_from(job_id_str).expect("Failed to parse job_id_str as Cid");

    // Mark job for bidding
    let response_bidding = client
        .post(format!("{}/jobs/{}/begin-bidding", app_address, job_id_str))
        .send()
        .await
        .expect("Failed to mark job for bidding.");
    assert_eq!(response_bidding.status().as_u16(), 200, "Failed to mark job for bidding. Response: {:?}", response_bidding.text().await);

    // 6. Define Bidders and Bids
    let bidder_a_did = Did("did:key:zAaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()); // Low Rep
    let bidder_b_did = Did("did:key:zBbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()); // High Rep, High Price
    let bidder_c_did = Did("did:key:zCccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string()); // Med Rep, Med Price
    let bidder_d_did = Did("did:key:zDddddddddddddddddddddddddddddddddddddddddddddddddddddd".to_string()); // High Rep, Low Price (Expected Winner)

    // Bid A (Disqualified by min_reputation)
    let bid_a = Bid {
        id: None,
        job_id,
        bidder: bidder_a_did.clone(),
        bid_amount: 10, // u64
        resource_estimate: None,
        reputation_score: Some(0.6), // Below min_reputation of 0.7
        submitted_at: chrono::Utc::now(),
        node_metadata: None,
        verifiable_reputation_assertion: None,
    };

    // Bid B
    let bid_b = Bid {
        id: None,
        job_id,
        bidder: bidder_b_did.clone(),
        bid_amount: 100,
        resource_estimate: None,
        reputation_score: Some(0.9),
        submitted_at: chrono::Utc::now(),
        node_metadata: None,
        verifiable_reputation_assertion: None,
    };

    // Bid C
    let bid_c = Bid {
        id: None,
        job_id,
        bidder: bidder_c_did.clone(),
        bid_amount: 50,
        resource_estimate: None,
        reputation_score: Some(0.8),
        submitted_at: chrono::Utc::now(),
        node_metadata: None,
        verifiable_reputation_assertion: None,
    };

    // Bid D (Expected Winner)
    let bid_d = Bid {
        id: None,
        job_id,
        bidder: bidder_d_did.clone(),
        bid_amount: 20,
        resource_estimate: None,
        reputation_score: Some(0.95),
        submitted_at: chrono::Utc::now(),
        node_metadata: None,
        verifiable_reputation_assertion: None,
    };

    // 7. Submit Bids
    for bid_payload in [&bid_a, &bid_b, &bid_c, &bid_d] {
        let res = client
            .post(format!("{}/jobs/{}/bids", app_address, job_id_str))
            .json(bid_payload)
            .send()
            .await
            .expect("Failed to submit bid");
        assert_eq!(res.status().as_u16(), 202, "Failed to submit bid for {}. Response: {:?}", bid_payload.bidder.0, res.text().await);
    }

    // 8. Trigger Assignment
    let assign_response = client
        .post(format!("{}/jobs/{}/assign", app_address, job_id_str))
        .send()
        .await
        .expect("Failed to trigger assignment");

    assert_eq!(assign_response.status().as_u16(), 200, "Assignment failed. Response: {:?}", assign_response.text().await);

    #[derive(serde::Deserialize, Debug)] // Add Deserialize here
    struct AssignJobResponse {
        message: String,
        job_id: String,
        assigned_bidder_did: String,
        winning_bid_id: i64, 
        winning_score: f64,
    }

    let assignment_details: AssignJobResponse = assign_response.json().await.expect("Failed to parse assignment response");

    // 9. Assert Winning Bidder and Score
    assert_eq!(assignment_details.assigned_bidder_did, bidder_d_did.0, "Incorrect bidder assigned.");
    // Note: Floating point comparisons can be tricky. Assert within a small epsilon if exact match is problematic.
    // Expected score for Bidder D: (0.95 * 0.5) + ( (1.0 - 20.0/100.0) * 0.5) = 0.475 + (0.8 * 0.5) = 0.475 + 0.4 = 0.875
    let expected_score_bidder_d = 0.875;
    assert!((assignment_details.winning_score - expected_score_bidder_d).abs() < 0.0001, 
            "Winning score mismatch. Expected: {}, Got: {}", expected_score_bidder_d, assignment_details.winning_score);

    // 10. Verify Job Status is Assigned
    let job_status_response = client
        .get(format!("{}/jobs/{}", app_address, job_id_str))
        .send()
        .await
        .expect("Failed to get job details post-assignment.");
    
    assert_eq!(job_status_response.status().as_u16(), 200, "Failed to fetch job after assignment.");

    let job_details: serde_json::Value = job_status_response.json().await.expect("Failed to parse job details.");
    let status_obj = job_details["status"].as_object().expect("Status is not an object");

    // Check for "Assigned" status type by looking for the presence of the "bidder" field within the status object.
    // The JobStatus enum serializes `Assigned { bidder: Did }` into `{"Assigned": {"bidder": "did:key:..."}}`
    // or just `{"bidder": "did:key:..."}` if using `#[serde(tag = "status_type", content = "details")]` style not shown here.
    // Based on current sqlite_store.rs row mapping, it's likely a flat structure like `status_type: "Assigned", status_did: "bidder_did"`
    // However, `get_job` in `main.rs` returns `json!({ "request": job_req, "status": status })` where `status` is the `JobStatus` enum.
    // Let's assume JobStatus::Assigned { bidder } serializes to something like `{"Assigned": {"bidder": "did:xxx"}}` or `{"status_type": "Assigned", "bidder": "did:xxx"}`.
    // The current JobStatus enum definition doesn't specify serde representation details.
    // A robust way is to deserialize into the JobStatus enum itself if its definition is accessible and serde-compatible.
    
    // For simplicity, let's check the assigned_bidder_did from the status object, assuming a structure like `{"Assigned":{"bidder":"did:key:zD..."}}`
    if let Some(assigned_status) = status_obj.get("Assigned") {
        assert_eq!(assigned_status["bidder"].as_str().unwrap(), bidder_d_did.0, "Job status does not reflect correct assigned bidder.");
    } else {
        panic!("Job status is not 'Assigned'. Status: {:?}", job_details["status"]);
    }

    // Placeholder removed
    // assert!(true); 
}

// We would also need to make the app_router function in main.rs public or move it to lib.rs
// For example, in main.rs, change:
// fn app_router(store: Arc<dyn MeshJobStore>, reputation_service_url: Arc<String>) -> Router { ... }
// to:
// pub fn app_router(store: Arc<dyn MeshJobStore>, reputation_service_url: Arc<String>) -> Router { ... }
// And then in this test file: use icn_services_mesh_jobs::app_router;
// Or, if icn-mesh-jobs is a binary crate, tests are usually in the same crate (src/main.rs with #[cfg(test)])
// or the router logic is extracted into a library part of the crate.
// For `icn_services_mesh_jobs::app_router`, I'm assuming `icn-mesh-jobs` is structured as a library `icn_services_mesh_jobs`
// or that `main.rs` exposes this. If `icn-mesh-jobs` is a binary, test setup might need to be in `main.rs` or `lib.rs`.
"" 