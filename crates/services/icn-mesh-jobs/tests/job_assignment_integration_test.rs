"""
// crates/services/icn-mesh-jobs/tests/job_assignment_integration_test.rs

use std::sync::Arc;
use std::net::SocketAddr;
use tokio::sync::oneshot; // For signaling server readiness

use cid::Cid;
use icn_identity::Did;
use icn_types::jobs::{JobRequest, Bid, ResourceRequirements, ResourceEstimate, JobStatus, TokenAmount};
use icn_mesh_jobs::{ /* AppError, (make AppError Cloneable or use anyhow for test results) */ AppError}; // Assuming AppError is accessible and usable in tests
use icn_mesh_jobs::sqlite_store::SqliteStore;
use icn_mesh_jobs::storage::MeshJobStore; // The trait
use reqwest::StatusCode;
use serde_json::json;
use sqlx::{SqlitePool, Row};

// Helper function to create a JobRequest (adapt as needed)
fn create_test_job_request(wasm_module_cid: &str, desc: &str) -> JobRequest {
    JobRequest {
        wasm_cid: Cid::try_from(wasm_module_cid).unwrap(),
        description: desc.to_string(),
        requirements: ResourceRequirements {
            cpu: 1,
            memory_mb: 1024,
            storage_mb: 500,
            bandwidth: 100,
        },
        deadline: None, // Add chrono::Utc::now() + chrono::Duration::days(1) if needed
    }
}

// Helper function to create a Bid (adapt as needed)
fn create_test_bid(job_id: Cid, bidder_did_str: &str, price: TokenAmount, est_cpu: u32) -> Bid {
    Bid {
        id: None, // Will be set by DB
        job_id,
        bidder: Did(bidder_did_str.to_string()),
        price,
        estimate: ResourceEstimate {
            cpu: est_cpu,
            memory_mb: 1000,
            storage_mb: 450,
            bandwidth: 90,
            estimated_duration_secs: Some(3600),
        },
        reputation_score: None, // Will be fetched by service; for test, can be None
    }
}


// This struct would be part of your main crate, made visible to tests, or redefined.
// For simplicity, let's assume the AssignJobResponse struct from main.rs is accessible.
// If not, you might need to redefine it here or deserialize into a generic serde_json::Value.
#[derive(serde::Deserialize, Debug, PartialEq)]
struct AssignJobResponseTest {
    message: String,
    job_id: String,
    assigned_bidder_did: String,
    winning_bid_id: i64,
    winning_score: f64,
}


async fn setup_test_db_and_server() -> (String, SqlitePool, oneshot::Sender<()>) {
    // 1. Setup in-memory SQLite database
    let pool = SqlitePool::connect("sqlite::memory:").await.expect("Failed to connect to in-memory SQLite DB");
    sqlx::migrate!("./migrations").run(&pool).await.expect("Failed to run migrations on in-memory DB");

    // 2. Setup and run the Axum server on a random available port
    //    This part is tricky as the main() function in your binary crate usually does this.
    //    For integration tests, you often extract the app logic into a library function.
    //    Let's assume you have a function `fn create_app(pool: Arc<SqlitePool>, rep_url: Arc<String>) -> axum::Router`
    //    in your `icn_mesh_jobs` crate (e.g. in lib.rs or a test_utils module).

    //    For now, this part is conceptual as I can't define `create_app` here.
    //    You would replace this with your actual server setup.
    
    let store = Arc::new(SqliteStore::new(Arc::new(pool.clone())));
    let reputation_service_url = Arc::new("http://localhost:12345".to_string()); // Mock or dummy URL for test

    // This is a simplified version of what would be in your main.rs's app creation
    let app = axum::Router::new()
        .route("/jobs", axum::routing::post(icn_mesh_jobs::create_job).get(icn_mesh_jobs::list_jobs))
        .route("/jobs/by-worker/:worker_did", axum::routing::get(icn_mesh_jobs::get_jobs_for_worker_handler))
        .route("/jobs/:job_id", axum::routing::get(icn_mesh_jobs::get_job))
        .route("/jobs/:job_id/bids", axum::routing::post(icn_mesh_jobs::submit_bid) /* .get(ws_stream_bids_handler) - WS needs more setup */)
        .route("/jobs/:job_id/begin-bidding", axum::routing::post(icn_mesh_jobs::begin_bidding_handler))
        .route("/jobs/:job_id/assign", axum::routing::post(icn_mesh_jobs::assign_best_bid_handler))
        // Add other routes if your test setup needs them (start, complete, fail)
        .layer(axum::Extension(store.clone() as Arc<dyn MeshJobStore>))
        .layer(axum::Extension(reputation_service_url.clone()));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service())
            .with_graceful_shutdown(async {
                rx.await.ok();
            })
            .await
            .unwrap();
    });
    
    (base_url, pool, tx)
}


#[tokio::test]
async fn test_job_assignment_and_state_changes() -> Result<(), anyhow::Error> {
    let (base_url, db_pool, server_shutdown_tx) = setup_test_db_and_server().await;
    let client = reqwest::Client::new();

    // --- Test Data ---
    let wasm_cid_str = "bafyreibmip333whnuyzebeu3ayes34sqzddsdlb6x6epjbmfddgvqfpliy"; // Example CID
    let job_request_payload = create_test_job_request(wasm_cid_str, "Test Assignment Job");
    
    // 1. Create Job
    let create_response = client.post(format!("{}/jobs", base_url))
        .json(&job_request_payload)
        .send()
        .await?;
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let job_response_json: serde_json::Value = create_response.json().await?;
    let job_id_str = job_response_json["job_id"].as_str().unwrap().to_string();
    let job_cid = Cid::try_from(job_id_str.clone())?;

    // 2. Transition Job to Bidding
    let begin_bidding_response = client.post(format!("{}/jobs/{}/begin-bidding", base_url, job_id_str))
        .send()
        .await?;
    assert_eq!(begin_bidding_response.status(), StatusCode::OK);

    // 3. Submit Bids
    // Bid 1 (higher price, expected to lose)
    let bid1_payload = create_test_bid(job_cid, "did:key:bidder1", 100, 1);
    // Bid 2 (lower price, expected to win)
    let bid2_payload = create_test_bid(job_cid, "did:key:bidder2", 50, 1);

    let submit_bid1_response = client.post(format!("{}/jobs/{}/bids", base_url, job_id_str))
        .json(&bid1_payload)
        .send()
        .await?;
    assert_eq!(submit_bid1_response.status(), StatusCode::ACCEPTED);

    let submit_bid2_response = client.post(format!("{}/jobs/{}/bids", base_url, job_id_str))
        .json(&bid2_payload)
        .send()
        .await?;
    assert_eq!(submit_bid2_response.status(), StatusCode::ACCEPTED);

    // Retrieve bid IDs from DB (since Bid struct doesn't get it back from API and POST /bids doesn't return body)
    // This part is important for knowing which bid ID won.
    let bids_from_db: Vec<(i64, String)> = sqlx::query_as("SELECT id, bidder_did FROM bids WHERE job_id = ? ORDER BY price ASC")
        .bind(job_id_str.clone())
        .fetch_all(&db_pool)
        .await?
        .into_iter()
        .map(|row: (i64, String)| (row.0, row.1))
        .collect();

    assert_eq!(bids_from_db.len(), 2, "Should have two bids in the database");
    let winning_bid_expected_id = bids_from_db.iter().find(|(_,did)| did == "did:key:bidder2").expect("Winning bidder's bid not found").0;
    let losing_bid_expected_id = bids_from_db.iter().find(|(_,did)| did == "did:key:bidder1").expect("Losing bidder's bid not found").0;


    // 4. Assign Job
    let assign_response = client.post(format!("{}/jobs/{}/assign", base_url, job_id_str))
        .send()
        .await?;
    assert_eq!(assign_response.status(), StatusCode::OK, "Assign endpoint failed. Body: {:?}", assign_response.text().await?);
    
    let assign_json: AssignJobResponseTest = assign_response.json().await?;
    assert_eq!(assign_json.job_id, job_id_str);
    assert_eq!(assign_json.assigned_bidder_did, "did:key:bidder2");
    assert_eq!(assign_json.winning_bid_id, winning_bid_expected_id);
    // We can't easily assert exact winning_score without knowing the scoring constants/logic precisely in test.
    // Asserting its presence or a general range might be enough if the logic is complex.
    assert!(assign_json.winning_score > 0.0, "Winning score should be positive");


    // 5. Verify Database State
    // Check jobs table
    let job_db_row = sqlx::query!(
        r#"SELECT status_type, status_did, winning_bid_id FROM jobs WHERE job_cid = $1"#,
        job_id_str
    )
    .fetch_one(&db_pool)
    .await?;

    assert_eq!(job_db_row.status_type.as_deref(), Some("Assigned"));
    assert_eq!(job_db_row.status_did.as_deref(), Some("did:key:bidder2"));
    assert_eq!(job_db_row.winning_bid_id, Some(winning_bid_expected_id));

    // Check bids table for statuses
    let winning_bid_status: Option<String> = sqlx::query_scalar("SELECT status FROM bids WHERE id = ?")
        .bind(winning_bid_expected_id)
        .fetch_one(&db_pool)
        .await?;
    assert_eq!(winning_bid_status.as_deref(), Some("Won"));

    let losing_bid_status: Option<String> = sqlx::query_scalar("SELECT status FROM bids WHERE id = ?")
        .bind(losing_bid_expected_id)
        .fetch_one(&db_pool)
        .await?;
    assert_eq!(losing_bid_status.as_deref(), Some("Lost"));

    // 6. Verify GET /jobs/{job_id} (Status Check)
    let get_job_response = client.get(format!("{}/jobs/{}", base_url, job_id_str)).send().await?;
    assert_eq!(get_job_response.status(), StatusCode::OK);
    let get_job_json: serde_json::Value = get_job_response.json().await?;
    
    let status_val = get_job_json.get("status").ok_or_else(|| anyhow::anyhow!("Status field missing from get_job response"))?;
    // Assuming JobStatus::Assigned serializes to an object like {"Assigned": {"bidder": "did:key:bidder2"}}
    // or a string "Assigned". This depends on your JobStatus serde implementation.
    // Let's be flexible for common patterns.
    if let Some(status_obj) = status_val.as_object() { // e.g. {"Assigned": {"bidder": "did:key:..."}}
        assert!(status_obj.contains_key("Assigned"));
        assert_eq!(status_obj["Assigned"]["bidder"].as_str(), Some("did:key:bidder2"));
    } else if let Some(status_str) = status_val.as_str() { // e.g. "Assigned" (if JobStatus serializes as simple string for this variant without data)
         // This path likely incorrect for Assigned which has data.
         // But for simple enums: assert_eq!(status_str, "Assigned");
         // For now, the object check is more robust for `Assigned { bidder: Did }`
         panic!("JobStatus::Assigned was expected to serialize as an object with bidder DID.");
    } else {
        panic!("JobStatus::Assigned did not serialize as expected object or string.");
    }


    // Clean up: Signal server to shutdown
    let _ = server_shutdown_tx.send(());

    Ok(())
}

// Note: You'll need to ensure that the main.rs handlers (create_job, assign_best_bid_handler, etc.)
// are publicly accessible from this test module (e.g., by putting them in lib.rs if main.rs calls lib.rs,
// or by making them `pub` and referencing the crate name if main.rs is the library root for the binary).
// The current setup in the user's files implies main.rs is the binary entry point.
// One way to make them callable is to move the router setup logic and handlers into a lib.rs
// and have main.rs call a function from lib.rs. `icn_mesh_jobs::create_job` etc. implies this structure.
"" 