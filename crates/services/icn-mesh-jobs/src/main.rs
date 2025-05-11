use axum::{
    extract::{Extension, Path, Query, State, WebSocketUpgrade, ws::{Message, WebSocket}},
    http::StatusCode,
    response::{IntoResponse, Response, Json as AxumJson},
    routing::{get, post},
    Router,
    headers::HeaderMap,
};
use cid::Cid;
use futures::{stream::StreamExt, SinkExt};
use icn_identity::Did;
use icn_types::jobs::{Bid, JobRequest, JobStatus, ResourceEstimate};
use icn_types::reputation::{ReputationRecord, ReputationUpdateEvent, ReputationProfile};
use icn_types::mesh::MeshJobParams;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tracing_subscriber;
use chrono::Utc;
use sha2::{Digest, Sha256};
use multihash::{Code, Multihash};

// Added for SqliteStore integration
use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};

mod storage;
// Remove InMemoryStore if it's no longer the default and not used elsewhere, or keep if needed for tests/other configs
// For now, assuming SqliteStore becomes the primary store.
use storage::MeshJobStore; // MeshJobStore trait is still needed

mod sqlite_store; // Declare the new module
use sqlite_store::SqliteStore; // Import the SqliteStore struct

mod reputation_client;
mod bid_logic;
mod job_assignment; // Added module
use crate::job_assignment::{DefaultExecutorSelector, ExecutorSelector, GovernedExecutorSelector, ExecutionPolicy}; // Updated import

// ADDITION START
// Define a type alias for the shared P2P node state
pub type SharedP2pNode = Arc<tokio::sync::Mutex<PlanetaryMeshNode>>;
// ADDITION END

use planetary_mesh::node::MeshNode as PlanetaryMeshNode;
use libp2p::Multiaddr;
// ADDITION for the test listener channel type
use planetary_mesh::protocol::MeshProtocolMessage as PlanetaryMeshMessage;

enum AppError {
    Internal(anyhow::Error),
    Forbidden(String),
    BadRequest(String),
    NotFound(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Internal(err) => {
                tracing::error!("Internal server error: {:#}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, AxumJson(json!({ "error": "Internal server error" }))).into_response()
            }
            AppError::Forbidden(msg) => {
                tracing::warn!("Forbidden access: {}", msg);
                (StatusCode::FORBIDDEN, AxumJson(json!({ "error": msg }))).into_response()
            }
            AppError::BadRequest(msg) => {
                tracing::warn!("Bad request: {}", msg);
                (StatusCode::BAD_REQUEST, AxumJson(json!({ "error": msg }))).into_response()
            }
            AppError::NotFound(msg) => {
                tracing::warn!("Not found: {}", msg);
                (StatusCode::NOT_FOUND, AxumJson(json!({ "error": msg }))).into_response()
            }
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err)
    }
}

#[derive(Deserialize)]
struct ListJobsQuery { status: Option<String> }
fn parse_job_status(s: Option<String>) -> Option<JobStatus> {
    s.and_then(|status_str| match status_str.to_lowercase().as_str() {
        "pending" => Some(JobStatus::Pending),
        "bidding" => Some(JobStatus::Bidding),
        _ => None,
    })
}

lazy_static::lazy_static! {
    static ref MESH_JOBS_SYSTEM_DID: Did = Did("did:icn:system:mesh-jobs".to_string());
}

#[derive(Deserialize, Debug)]
pub struct JobCompletionDetails {
    execution_duration_ms: u32,
    bid_accuracy: f32,
    on_time: bool,
    result_anchor_cid: Option<Cid>,
}

#[derive(Deserialize, Debug)]
pub struct JobFailureDetails {
    reason: String,
    failure_anchor_cid: Option<Cid>,
}

#[derive(serde::Serialize)]
struct AssignJobResponse {
    message: String,
    job_id: String,
    assigned_bidder_did: String,
    winning_bid_id: i64,
    winning_score: f64,
}

/// Payload expected for creating a new job.
#[derive(Deserialize)]
struct CreateJobApiPayload {
    params: MeshJobParams,
    originator_did: Did,
}

/// Internal struct for deterministic CID generation of a job.
#[derive(Serialize)]
struct JobCidInput<'a> {
    params: &'a MeshJobParams,
    originator_did: &'a Did,
}

/// Generates a deterministic CID for a job based on its parameters and originator.
fn generate_job_cid_from_payload(
    params: &MeshJobParams,
    originator_did: &Did,
) -> Result<Cid, AppError> {
    let cid_input = JobCidInput { params, originator_did };
    let bytes = serde_json::to_vec(&cid_input)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize job data for CID generation: {}", e)))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash_bytes = hasher.finalize();
    // Using SHA2-256 multihash code (0x12)
    let multihash = Multihash::new(Code::Sha2_256.into(), &hash_bytes)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to create multihash for CID generation: {}", e)))?;
    // Using DAG-CBOR codec (0x71) for the CID, common for IPLD structured data.
    Ok(Cid::new_v1(cid:: известных_кодеков::DAG_CBOR, multihash))
}

/// Start the ICN Mesh Jobs server with P2P integration.
pub async fn run_server(
    database_url: String,
    p2p_identity: IcnKeyPair,
    p2p_listen_address: Option<String>,
    reputation_service_url: String,
    http_listen_addr: SocketAddr,
    // ADDITION: Test listener sender parameter for P2P node
    test_listener_tx: Option<tokio::sync::broadcast::Sender<PlanetaryMeshMessage>>,
) -> anyhow::Result<(SocketAddr, Vec<Multiaddr>)> {
    tracing::info!("run_server: Initializing with DB URL: {}, P2P Listen: {:?}, HTTP Listen: {}", database_url, p2p_listen_address, http_listen_addr);

    // --- Database Setup ---
    tracing::info!("run_server: Setting up database at: {}", database_url);
    if !Sqlite::database_exists(&database_url).await.unwrap_or(false) {
        tracing::info!("run_server: Database not found, creating new one...");
        Sqlite::create_database(&database_url).await?;
        tracing::info!("run_server: Database created.");
    }

    let pool = Arc::new(SqlitePool::connect(&database_url).await
        .map_err(|e| anyhow::anyhow!("run_server: Failed to connect to database {}: {}", database_url, e))?);
    tracing::info!("run_server: Database connection pool established.");

    match sqlx::migrate!("./migrations").run(&*pool).await {
        Ok(_) => tracing::info!("run_server: Database migrations completed successfully."),
        Err(e) => {
            tracing::error!("run_server: Failed to run database migrations: {}", e);
            return Err(anyhow::anyhow!("run_server: Migration error: {}", e));
        }
    }
    let store: Arc<dyn MeshJobStore> = Arc::new(SqliteStore::new(pool.clone()));
    tracing::info!("run_server: Using SqliteStore for job and bid management.");

    let reputation_url = Arc::new(reputation_service_url);
    tracing::info!("run_server: Using reputation service at: {}", *reputation_url);

    // --- P2P Mesh Node Setup ---
    tracing::info!("run_server: P2P Node DID for icn-mesh-jobs service: {}", p2p_identity.did);
    let job_queue: Arc<Mutex<VecDeque<MeshJob>>> = Arc::new(Mutex::new(VecDeque::new()));
    let mut p2p_node = PlanetaryMeshNode::new(
        p2p_identity,
        p2p_listen_address.clone(),
        job_queue,
        None, // local_runtime_context
        // ADDITION: Pass the test listener to the P2P node
        test_listener_tx,
    )
    .await?;
    
    // Get P2P listen addresses. Note: `run` must be called for listening to truly start for external purposes,
    // but `new` usually sets up the listener internally. We'll get addresses before spawning run.
    // This might require PlanetaryMeshNode to expose listeners immediately after Swarm is created or a helper.
    // For now, assuming new starts listeners effectively enough to get addresses.
    // This part might need adjustment depending on PlanetaryMeshNode's internal listen logic.
    // A safer way is to get listeners *after* run() has been called and the node signals readiness,
    // or have PlanetaryMeshNode::new return them.
    // Assuming PlanetaryMeshNode::new makes listen_on calls that are effective immediately for address retrieval.
    let listen_addrs = p2p_node.swarm.listeners().cloned().collect::<Vec<_>>();
    if listen_addrs.is_empty() {
        tracing::warn!("run_server: P2P Node reported no listen addresses immediately after creation. This might be an issue for discovery.");
    } else {
        for addr in &listen_addrs {
            tracing::info!("run_server: P2P Node listening on: {}", addr);
        }
    }
    
    let shared_p2p = Arc::new(tokio::sync::Mutex::new(p2p_node));

    let runner = shared_p2p.clone();
    tokio::spawn(async move {
        tracing::info!("run_server: Starting P2P Mesh Node event loop...");
        // The lock guard is held for the duration of run(). Ensure run() doesn't deadlock.
        let mut node_guard = runner.lock().await;
        if let Err(e) = node_guard.run().await { 
             tracing::error!("run_server: P2P Mesh Node event loop failed: {}", e);
        }
    });
    tracing::info!("run_server: P2P Mesh Node task has been spawned.");

    // --- HTTP Server Setup ---
    // Bind a TCP listener to get the actual local address, especially if port 0 is used.
    let http_listener = tokio::net::TcpListener::bind(http_listen_addr).await
        .map_err(|e| anyhow::anyhow!("run_server: Failed to bind HTTP listener to {}: {}", http_listen_addr, e))?;
    let actual_http_listen_addr = http_listener.local_addr()
        .map_err(|e| anyhow::anyhow!("run_server: Failed to get local address from HTTP listener: {}", e))?;
    
    tracing::info!("run_server: HTTP server attempting to listen on {}", actual_http_listen_addr);

    let app = Router::new()
        // All original routes from main.rs
        .route("/jobs", post(create_job).get(list_jobs))
        .route("/jobs/by-worker/:worker_did", get(get_jobs_for_worker_handler))
        .route("/jobs/:job_id", get(get_job))
        .route("/jobs/:job_id/bids", post(submit_bid).get(ws_stream_bids_handler))
        .route("/jobs/:job_id/begin-bidding", post(begin_bidding_handler))
        .route("/jobs/:job_id/assign", post(assign_best_bid_handler))
        .route("/jobs/:job_id/start", post(start_job_handler))
        .route("/jobs/:job_id/complete", post(mark_job_completed_handler))
        .route("/jobs/:job_id/fail", post(mark_job_failed_handler))
        .layer(Extension(store.clone()))
        .layer(Extension(reputation_url.clone()))
        .layer(Extension(shared_p2p.clone()));

    tokio::spawn(async move {
        tracing::info!("run_server: ICN Mesh Jobs service HTTP server starting on {}", actual_http_listen_addr);
        if let Err(e) = axum::Server::from_tcp(http_listener)
            .map_err(|e| anyhow::anyhow!("Failed to create axum server from_tcp: {}",e ))? // map this error
            .serve(app.into_make_service())
            .await {
                tracing::error!("run_server: HTTP server failed: {}", e);
            }
        Ok::<(), anyhow::Error>(()) // Ensure the spawned task matches expected type if it needs to return Result
    });
    
    tracing::info!("run_server: HTTP server task has been spawned. Returning bound addresses.");
    Ok((actual_http_listen_addr, listen_addrs))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:icn_mesh_jobs.db?mode=rwc".to_string());
    
    // Generate or load P2P identity for the service
    // For production, this should be loaded from a secure config or persisted.
    // For now, generate a new one each time main() runs.
    let p2p_service_keypair = IcnKeyPair::generate();
    tracing::info!("main: Generated P2P DID for service: {}", p2p_service_keypair.did);

    let p2p_listen_addr_env = env::var("ICN_MESH_JOBS_P2P_LISTEN_ADDRESS").ok();
    let reputation_url_env = env::var("REPUTATION_SERVICE_URL").unwrap_or_else(|_| "http://localhost:8081".to_string());
    
    let http_listen_addr_str = env::var("HTTP_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let http_socket_addr: SocketAddr = http_listen_addr_str.parse()
        .map_err(|e| anyhow::anyhow!("Invalid HTTP_LISTEN_ADDR format '{}': {}", http_listen_addr_str, e))?;

    tracing::info!("main: Starting icn-mesh-jobs server...");
    // In main, we don't critically need the returned addresses, but run_server now provides them.
    // The server itself is spawned as a background task by run_server.
    // We need to keep main alive if run_server's HTTP and P2P parts are spawned.
    // The original axum::Server::bind(...).serve(...).await? would block main.
    // If run_server spawns its tasks and returns, main might exit.
    // For now, let run_server spawn tasks and return addresses.
    // The test will await these tasks or manage them. Main will just call and exit if tasks are spawned.
    // To make `main` behave like before (blocking until server stops), it should await a handle from `run_server`.
    // For simplicity in refactoring for tests, run_server will spawn and return.
    // If `main` needs to block, it should await a join handle from `run_server` for the http server task.
    
    // Create a Tokio channel to signal completion or error from the server tasks
    let (tx, mut rx) = tokio::sync::oneshot::channel::<anyhow::Result<()>>();

    tokio::spawn(async move {
        match run_server(
            database_url,
            p2p_service_keypair,
            p2p_listen_addr_env,
            reputation_url_env,
            http_socket_addr,
            None,
        ).await {
            Ok((http_addr, p2p_addrs)) => {
                tracing::info!("main: run_server started successfully. HTTP: {}, P2P: {:?}", http_addr, p2p_addrs);
                // If run_server's spawned tasks are the actual long-running server, 
                // main needs to wait for them or a signal.
                // The current run_server spawns tasks and returns Ok.
                // To keep main running like a typical server, we need it to await something.
                // The simplest is for run_server to *not* spawn the http server but return it.
                // Or, main just prints and exits, assuming spawned tasks continue (detached).
                // Let's adjust `run_server` so it doesn't spawn the HTTP server but returns it.
                // NO, the previous design was: axum::Server::bind(&addr).serve(app.into_make_service()).await? in main
                // which blocks.
                // The new run_server *also* spawns the http server.
                // This means `main` calling `run_server().await` will wait for `run_server` to finish setup,
                // but `run_server` itself returns after spawning the actual server tasks.
                // So `main` will exit unless we add a wait here.
                // For a typical server, we'd wait indefinitely.
                // Since `run_server` now returns quickly after spawning, add a ctrl-c handler in main.
                // This change makes `main` behave differently.
                // Let's stick to `run_server` returning the `JoinHandle` for the http server.
                // For now, this simpler version just logs. The test will manage its own spawned server.
                // The oneshot channel is a way for the spawned tasks to signal main.
                // However, run_server does not currently accept this channel.
                // Let's keep it simpler: main calls run_server. run_server returns addresses.
                // The spawned tasks within run_server keep it alive. If main exits, tasks might too if not detached.
                // Default for tokio::spawn is detached.
                // So main can exit.
                let _ = tx.send(Ok(())); // Signal successful startup
            }
            Err(e) => {
                tracing::error!("main: Failed to start server: {:?}", e);
                 let _ = tx.send(Err(e)); // Signal error
            }
        }
    });

    // Wait for the server to start (or fail)
    match rx.await {
        Ok(Ok(_)) => {
            tracing::info!("main: Server components initialized by run_server. Main will now wait for Ctrl-C.");
            // Keep main alive, typically a server would loop or await a shutdown signal
            tokio::signal::ctrl_c().await?;
            tracing::info!("main: Ctrl-C received, shutting down.");
        }
        Ok(Err(e)) => {
            tracing::error!("main: Server failed to initialize: {}", e);
            return Err(e);
        }
        Err(_) => {
             tracing::error!("main: Oneshot channel for server startup failed.");
             return Err(anyhow::anyhow!("Server startup signal channel failed"));
        }
    }
    
    Ok(())
}

async fn create_job(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    AxumJson(payload): AxumJson<CreateJobApiPayload>,
) -> Result<impl IntoResponse, AppError> {
    // 1. Generate the Job ID (CID)
    let job_id = generate_job_cid_from_payload(&payload.params, &payload.originator_did)?;

    // 2. Construct the JobRequest for storage
    let job_request_to_store = JobRequest {
        id: job_id,
        params: payload.params,
        originator_did: payload.originator_did,
    };

    // 3. Insert into the store
    match store.insert_job(job_request_to_store).await {
        Ok(returned_job_cid) => {
            // The store.insert_job now returns the CID that was part of the input JobRequest
            // So, returned_job_cid should be the same as job_id generated above.
            // We can add an assertion here for safety during development if desired.
            // assert_eq!(job_id, returned_job_cid, "Mismatch between generated CID and stored CID");
            Ok((StatusCode::CREATED, AxumJson(json!({ "job_id": returned_job_cid.to_string() }))))
        }
        Err(e) => {
            tracing::error!("Failed to create job: {}", e);
            Err(AppError::Internal(e)) // Assuming AppError has a From<anyhow::Error>
        }
    }
}

async fn get_job(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Path(job_id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let job_id = Cid::try_from(job_id_str.clone()).map_err(|e| AppError::BadRequest(format!("Invalid Job ID format: {} - {}", job_id_str, e)))?;
    match store.get_job(&job_id).await {
        Ok(Some((job_req, status))) => Ok(AxumJson(json!({ "request": job_req, "status": status })).into_response()),
        Ok(None) => Err(AppError::NotFound(format!("Job not found: {}", job_id))),
        Err(e) => Err(AppError::Internal(e)),
    }
}

async fn list_jobs(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Query(query): Query<ListJobsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let status_filter = parse_job_status(query.status);
    match store.list_jobs(status_filter).await {
        Ok(job_cids) => Ok(AxumJson(job_cids.into_iter().map(|cid| cid.to_string()).collect::<Vec<_>>()).into_response()),
        Err(e) => {
            tracing::error!("Failed to list jobs: {}", e);
            Err(AppError::Internal(e))
        }
    }
}

async fn get_jobs_for_worker_handler(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Path(worker_did_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let worker_did = Did(worker_did_str.clone());

    match store.list_jobs_for_worker(&worker_did).await {
        Ok(jobs_list) => {
            #[derive(Serialize)]
            struct WorkerJobEntry {
                job_id: String,
                request: JobRequest,
                status: JobStatus,
            }

            let response_data: Vec<WorkerJobEntry> = jobs_list
                .into_iter()
                .map(|(cid, request, status)| WorkerJobEntry {
                    job_id: cid.to_string(),
                    request,
                    status,
                })
                .collect();
            
            Ok(AxumJson(response_data).into_response())
        }
        Err(e) => {
            tracing::error!("Failed to list jobs for worker {}: {}", worker_did.0, e);
            Err(AppError::Internal(e))
        }
    }
}

async fn submit_bid(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Extension(reputation_url): Extension<Arc<String>>,
    Path(job_id_str): Path<String>,
    AxumJson(mut bid_req): AxumJson<Bid>,
) -> Result<impl IntoResponse, AppError> {
    let job_id = Cid::try_from(job_id_str.clone()).map_err(|e| AppError::BadRequest(format!("Invalid Job ID format: {} - {}", job_id_str, e)))?;
    if bid_req.job_id != job_id {
        return Err(AppError::BadRequest("Job ID in path does not match Job ID in bid payload".to_string()));
    }

    match reputation_client::get_reputation_score(&bid_req.bidder, &reputation_url).await {
        Ok(score_option) => {
            bid_req.reputation_score = score_option;
            tracing::info!(
                "Fetched reputation score for bidder {}: {:?}. Populating in bid for job {}",
                bid_req.bidder.0,
                score_option,
                job_id_str
            );
        }
        Err(e) => {
            tracing::warn!(
                "Failed to fetch reputation score for bidder {}: {}. Proceeding with no score.",
                bid_req.bidder.0, e
            );
            bid_req.reputation_score = None;
        }
    }

    store.insert_bid(&job_id, bid_req).await.map_err(AppError::Internal)?;
    Ok(StatusCode::ACCEPTED)
}

async fn assign_best_bid_handler(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Extension(p2p_node_state): Extension<SharedP2pNode>,
    Path(job_id_str): Path<String>,
) -> Result<AxumJson<AssignJobResponse>, AppError> {
    let job_id_cid = Cid::try_from(job_id_str.clone()).map_err(|e| {
        AppError::BadRequest(format!("Invalid job ID format: {} - {}", job_id_str, e))
    })?;

    tracing::info!(job_id = %job_id_str, "Attempting to assign best bid for job");

    // 1. Fetch job details and current status
    let (job_request, current_status) = store.get_job(&job_id_cid).await?
        .ok_or_else(|| AppError::NotFound(format!("Job with ID {} not found", job_id_str)))?;

    // 2. Ensure job is in Bidding state
    if !matches!(current_status, JobStatus::Bidding) {
        tracing::warn!(job_id = %job_id_str, current_status = ?current_status, "Job not in Bidding state, cannot assign");
        return Err(AppError::BadRequest(format!(
            "Job {} is not in Bidding state, currently: {:?}",
            job_id_str, current_status
        )));
    }

    // 3. Fetch all bids for the job
    let bids = store.list_bids(&job_id_cid).await?;
    if bids.is_empty() {
        tracing::warn!(job_id = %job_id_str, "No bids found for job, cannot assign");
        return Err(AppError::NotFound(format!("No bids found for job {}", job_id_str)));
    }

    // 4. Determine ExecutorSelector based on ExecutionPolicy in JobRequest.params
    let selector: Box<dyn ExecutorSelector> = {
        // Assumes `job_request` (of type icn_types::jobs::JobRequest) now has a `params` field
        // of type `icn_types::mesh::MeshJobParams` which in turn contains `execution_policy`.
        // This requires `icn_types::jobs::JobRequest` to be refactored.
        if let Some(policy) = &job_request.params.execution_policy {
            tracing::info!(policy = ?policy, "Using GovernedExecutorSelector for job {}", job_id_str);
            Box::new(GovernedExecutorSelector::new(policy.clone()))
        } else {
            tracing::info!("No execution policy found in job_request.params, using DefaultExecutorSelector for job {}", job_id_str);
            // Provide default weights for DefaultExecutorSelector if its constructor requires them.
            // These were 0.7 and 0.3 in the previous version.
            Box::new(DefaultExecutorSelector::new(0.7, 0.3))
        }
    };

    // 5. Select the best bid
    let winning_bid_tuple = selector.select(&job_request, &bids)?
        .ok_or_else(|| AppError::NotFound(format!("No acceptable bids found for job {} based on policy", job_id_str)))?;
    
    let (winning_bid, winning_score) = winning_bid_tuple;

    // Ensure winning_bid.id is present
    let winning_bid_id = winning_bid.id.ok_or_else(|| AppError::Internal(anyhow::anyhow!("Winning bid {} has no ID", winning_bid.bidder)))?;

    // 6. Update job status to Assigned in the local store
    store.assign_job(&job_id_cid, winning_bid.bidder.clone(), winning_bid_id).await?;

    tracing::info!(job_id = %job_id_str, bidder = %winning_bid.bidder, score = winning_score, "Job assigned to bidder in local store");

    // 7. Notify the winning bidder via P2P // ADDED SECTION START
    // Reconstruct the MeshJob details for the P2P message.
    // JobRequest (from store.get_job) contains params and originator_did.
    let mesh_job_details_for_p2p = MeshJob {
        job_id: job_request.id.to_string(), // Convert CID to string for P2P JobId
        params: job_request.params.clone(),
        originator_did: job_request.originator_did.clone(),
        // submission_timestamp: JobRequest doesn't seem to have submission_timestamp.
        // Using current time as a placeholder. Ideally, this would be the original job submission time.
        submission_timestamp: Utc::now().timestamp() as u64, 
        originator_org_scope: None, // Populate if available from JobRequest or context
    };

    let originator_did_for_p2p = job_request.originator_did.clone();
    let target_executor_did_for_p2p = winning_bid.bidder.clone();
    // Use the String version of job_id for the P2P call, as IcnJobId is String.
    let job_id_string_for_p2p = job_request.id.to_string(); 

    tracing::info!(
        job_id = %job_id_string_for_p2p,
        target_executor = %target_executor_did_for_p2p,
        originator = %originator_did_for_p2p,
        "Attempting to send AssignJobV1 P2P message."
    );
    
    let mut p2p_node_guard = p2p_node_state.lock().await;
    match p2p_node_guard.assign_job_to_executor(
        &job_id_string_for_p2p, // Pass as &String
        target_executor_did_for_p2p,
        mesh_job_details_for_p2p,
        originator_did_for_p2p
    ).await {
        Ok(_) => tracing::info!("Successfully published AssignJobV1 for job {}", job_id_str),
        Err(e) => {
            // Log the error. The HTTP request itself won't fail due to this,
            // as the primary action (DB update) succeeded.
            // Robust P2P messaging might require a retry queue or other out-of-band handling.
            tracing::error!("Failed to publish AssignJobV1 for job {}: {}. The job remains assigned in the database.", job_id_str, e);
        }
    }
    // ADDED SECTION END

    Ok(AxumJson(AssignJobResponse {
        message: "Job assigned successfully. P2P notification to executor initiated.".to_string(),
        job_id: job_id_str,
        assigned_bidder_did: winning_bid.bidder.0.clone(),
        winning_bid_id,
        winning_score,
    }))
}

async fn start_job_handler(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Path(job_id_str): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    let job_id = Cid::try_from(job_id_str.clone()).map_err(|e| AppError::BadRequest(format!("Invalid Job ID format: {} - {}", job_id_str, e)))?;

    let worker_did_header = headers
        .get("X-Worker-DID")
        .ok_or_else(|| AppError::BadRequest("Missing X-Worker-DID header".to_string()))?
        .to_str()
        .map_err(|_| AppError::BadRequest("Invalid X-Worker-DID header format".to_string()))?;
    let worker_did = Did(worker_did_header.to_string());

    let (_job_request, current_status) = store.get_job(&job_id).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id)))?;

    match current_status {
        JobStatus::Assigned { bidder } => {
            if bidder != worker_did {
                tracing::warn!(
                    "Authorization failed for starting job {}. Expected bidder {}, got worker {}.",
                    job_id, bidder.0, worker_did.0
                );
                return Err(AppError::Forbidden("Worker DID does not match assigned bidder DID".to_string()));
            }
            store.update_job_status(&job_id, JobStatus::Running { runner: bidder.clone() }).await.map_err(AppError::Internal)?;
            tracing::info!("Job {} has been started by bidder {}.", job_id, bidder.0);
            Ok(StatusCode::OK)
        }
        JobStatus::Running { runner } => {
            tracing::info!("Job {} is already running by {}.", job_id, runner.0);
            Err(AppError::BadRequest(format!("Job {} is already running.", job_id)))
        }
        _ => {
            tracing::error!("Job {} is in status {:?} and cannot be started.", job_id, current_status);
            Err(AppError::BadRequest(format!("Job {} cannot be started from its current state: {:?}.", job_id, current_status)))
        }
    }
}

async fn mark_job_completed_handler(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Extension(reputation_url): Extension<Arc<String>>,
    Path(job_id_str): Path<String>,
    headers: HeaderMap,
    AxumJson(details): AxumJson<JobCompletionDetails>,
) -> Result<StatusCode, AppError> {
    let job_id = Cid::try_from(job_id_str.clone()).map_err(|e| AppError::BadRequest(format!("Invalid Job ID format: {} - {}", job_id_str, e)))?;

    let worker_did_header = headers
        .get("X-Worker-DID")
        .ok_or_else(|| AppError::BadRequest("Missing X-Worker-DID header".to_string()))?
        .to_str()
        .map_err(|_| AppError::BadRequest("Invalid X-Worker-DID header format".to_string()))?;
    let worker_did = Did(worker_did_header.to_string());

    let (_, job_status) = store.get_job(&job_id).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id)))?;

    let runner_did = match job_status {
        JobStatus::Running { ref runner } => {
            if runner != &worker_did {
                tracing::warn!(
                    "Authorization failed for completing job {}. Expected runner {}, got worker {}.",
                    job_id, runner.0, worker_did.0
                );
                return Err(AppError::Forbidden("Worker DID does not match job runner DID".to_string()));
            }
            runner.clone()
        }
        _ => {
            tracing::error!("Job {} is in status {:?} and cannot be marked completed.", job_id, job_status);
            return Err(AppError::BadRequest(format!("Job {} not in Running state. Current state: {:?}.", job_id, job_status)));
        }
    };

    let record = ReputationRecord {
        timestamp: Utc::now(),
        issuer: MESH_JOBS_SYSTEM_DID.clone(),
        subject: runner_did.clone(),
        event: ReputationUpdateEvent::JobCompletedSuccessfully {
            job_id,
            execution_duration_ms: details.execution_duration_ms,
            bid_accuracy: details.bid_accuracy,
            on_time: details.on_time,
            anchor_cid: details.result_anchor_cid,
        },
        anchor: details.result_anchor_cid,
        signature: None,
    };

    if let Err(e) = reputation_client::submit_reputation_record(&record, &reputation_url).await {
        tracing::error!("Failed to submit reputation record for job {}: {}. Proceeding with job completion.", job_id, e);
    }

    store.update_job_status(&job_id, JobStatus::Completed).await.map_err(AppError::Internal)?;
    tracing::info!("Marked job {} as Completed. Reputation record submitted for runner {}.", job_id, runner_did.0);
    Ok(StatusCode::OK)
}

async fn mark_job_failed_handler(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Extension(reputation_url): Extension<Arc<String>>,
    Path(job_id_str): Path<String>,
    headers: HeaderMap,
    AxumJson(details): AxumJson<JobFailureDetails>,
) -> Result<StatusCode, AppError> {
    let job_id = Cid::try_from(job_id_str.clone()).map_err(|e| AppError::BadRequest(format!("Invalid Job ID format: {} - {}", job_id_str, e)))?;

    let worker_did_header = headers
        .get("X-Worker-DID")
        .ok_or_else(|| AppError::BadRequest("Missing X-Worker-DID header".to_string()))?
        .to_str()
        .map_err(|_| AppError::BadRequest("Invalid X-Worker-DID header format".to_string()))?;
    let worker_did = Did(worker_did_header.to_string());

    let (_, job_status) = store.get_job(&job_id).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id)))?;

    let runner_did = match job_status {
        JobStatus::Running { ref runner } => {
            if runner != &worker_did {
                tracing::warn!(
                    "Authorization failed for failing job {}. Expected runner {}, got worker {}.",
                    job_id, runner.0, worker_did.0
                );
                return Err(AppError::Forbidden("Worker DID does not match job runner DID".to_string()));
            }
            runner.clone()
        }
        _ => {
            tracing::error!("Job {} is in status {:?} and cannot be marked failed.", job_id, job_status);
            return Err(AppError::BadRequest(format!("Job {} not in Running state. Current state: {:?}.", job_id, job_status)));
        }
    };

    let record = ReputationRecord {
        timestamp: Utc::now(),
        issuer: MESH_JOBS_SYSTEM_DID.clone(),
        subject: runner_did.clone(),
        event: ReputationUpdateEvent::JobFailed {
            job_id,
            reason: details.reason.clone(),
            anchor_cid: details.failure_anchor_cid,
        },
        anchor: details.failure_anchor_cid,
        signature: None,
    };

    if let Err(e) = reputation_client::submit_reputation_record(&record, &reputation_url).await {
        tracing::error!("Failed to submit reputation record for failed job {}: {}. Proceeding.", job_id, e);
    }

    store.update_job_status(&job_id, JobStatus::Failed { reason: details.reason }).await.map_err(AppError::Internal)?;
    tracing::info!("Marked job {} as Failed. Reason: {}. Reputation record submitted for runner {}.", job_id, store.get_job(&job_id).await.map(|j| format!("{:?}", j.map(|(_,s)|s))).unwrap_or_default(), runner_did.0);
    Ok(StatusCode::OK)
}

async fn ws_stream_bids_handler(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Path(job_id_str): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    let job_id = Cid::try_from(job_id_str.clone()).map_err(|e| AppError::BadRequest(format!("Invalid Job ID format {} - {}", job_id_str, e)))?;
    if store.get_job(&job_id).await.map_err(AppError::Internal)?.is_none() {
        return Err(AppError::NotFound(format!("Job not found: {}", job_id_str)));
    }
    Ok(ws.on_upgrade(move |socket| handle_bid_stream(socket, store, job_id)))
}

async fn handle_bid_stream(mut socket: WebSocket, store: Arc<dyn MeshJobStore>, job_id: Cid) {
    tracing::info!("WebSocket connection established for job bids: {}", job_id);
    match store.list_bids(&job_id).await {
        Ok(bids) => {
            for bid in bids {
                if let Ok(json_bid) = serde_json::to_string(&bid) {
                    if socket.send(Message::Text(json_bid)).await.is_err() {
                        tracing::warn!("Failed to send existing bid to WebSocket client for job {}", job_id);
                        return; 
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to list existing bids for WebSocket stream for job {}: {}", job_id, e);
            let _ = socket.send(Message::Text(serde_json::to_string(&json!({ "error": e.to_string() })).unwrap_or_default())).await;
            return;
        }
    }
    let mut bid_receiver = match store.subscribe_to_bids(&job_id).await {
        Ok(Some(rx)) => rx,
        Ok(None) => {
            tracing::info!("No bid broadcaster channel for job {}, will not stream live bids.", job_id);
            return;
        }
        Err(e) => {
            tracing::error!("Error subscribing to bids for job {}: {}", job_id, e);
            return;
        }
    };
    tracing::info!("Subscribed to new bids for job {}", job_id);

    loop {
        tokio::select! {
            received_bid = bid_receiver.recv() => {
                match received_bid {
                    Ok(bid) => {
                        if let Ok(json_bid) = serde_json::to_string(&bid) {
                            if socket.send(Message::Text(json_bid)).await.is_err() {
                                tracing::warn!("WebSocket send error for job {}, client disconnected?", job_id);
                                break;
                            }
                        } else {
                            tracing::error!("Failed to serialize bid for WebSocket broadcast on job {}", job_id);
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!("WebSocket bid stream for job {} lagged by {} messages.", job_id, n);
                        // Optionally, you could send an error to the client or just continue
                    }
                    Err(RecvError::Closed) => {
                        tracing::info!("Bid broadcast channel closed for job {}. WebSocket stream ending.", job_id);
                        break;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("WebSocket connection closed by client for job bids: {}", job_id);
                        break;
                    }
                    Some(Ok(_)) => { /* We don't expect messages from client on this WebSocket */ }
                    Some(Err(e)) => {
                        tracing::warn!("WebSocket receive error for job {}: {}", job_id, e);
                        break;
                    }
                }
            }
        }
    }
}

async fn begin_bidding_handler(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Path(job_id_str): Path<String>,
) -> Result<StatusCode, AppError> {
    let job_id = Cid::try_from(job_id_str.clone()).map_err(|e| AppError::BadRequest(format!("Invalid Job ID format: {} - {}", job_id_str, e)))?;

    let (_, current_status) = store.get_job(&job_id).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id)))?;

    match current_status {
        JobStatus::Pending => {
            store.update_job_status(&job_id, JobStatus::Bidding).await.map_err(AppError::Internal)?;
            tracing::info!("Job {} has been moved to Bidding state.", job_id);
            Ok(StatusCode::OK)
        }
        JobStatus::Bidding => {
            tracing::info!("Job {} is already in Bidding state.", job_id);
            Ok(StatusCode::OK) // Or potentially a BadRequest/Conflict if re-triggering is an issue
        }
        _ => {
            tracing::warn!("Job {} is in status {:?} and cannot be moved to Bidding state.", job_id, current_status);
            Err(AppError::BadRequest(format!("Job {} is in status {:?} and cannot be moved to Bidding state.", job_id, current_status)))
        }
    }
} 