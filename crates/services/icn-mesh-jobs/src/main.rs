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
use icn_types::jobs::{Bid, JobRequest, JobStatus, ResourceEstimate, ResourceRequirements};
use icn_types::reputation::{ReputationRecord, ReputationUpdateEvent, ReputationProfile};
use icn_types::mesh::MeshJobParams;
use icn_types::JobFailureReason;
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
use dotenv::dotenv;
use std::ops::Deref;

// Import the unified AppError from error.rs
use crate::error::AppError;

// Added for SqliteStore integration
use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};

mod storage;
// Remove InMemoryStore if it's no longer the default and not used elsewhere, or keep if needed for tests/other configs
// For now, assuming SqliteStore becomes the primary store.
use storage::MeshJobStore; // MeshJobStore trait is still needed

mod sqlite_store; // Declare the new module
use sqlite_store::SqliteStore; // Import the SqliteStore struct

mod reputation_client;
mod reputation_cache; // Add reputation_cache module
mod metrics; // Add metrics module  
mod bid_logic;
mod job_assignment; // Added module
mod models; // Add models module

// Import our types
use crate::job_assignment::{DefaultExecutorSelector, ExecutorSelector, GovernedExecutorSelector, ExecutionPolicy}; // Updated import
use crate::models::{BidEvaluatorConfig, ScoreComponent, ReputationSummary, BidExplanation, BidsExplainResponse};

// ADDITION START
// Define a type alias for the shared P2P node state
pub type SharedP2pNode = Arc<tokio::sync::Mutex<PlanetaryMeshNode>>;
// ADDITION END

use planetary_mesh::node::MeshNode as PlanetaryMeshNode;
use libp2p::Multiaddr;
// ADDITION for the test listener channel type
use planetary_mesh::protocol::MeshProtocolMessage as PlanetaryMeshMessage;

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
    reason: String,
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

// Add bid extension trait
trait BidExtension {
    fn score_components(&self) -> Option<&Vec<ScoreComponent>>;
}

impl BidExtension for Bid {
    fn score_components(&self) -> Option<&Vec<ScoreComponent>> {
        // Default implementation returns None since standard Bid doesn't have this field
        None
    }
}

// Add an enhanced bid wrapper
struct EnhancedBidWrapper {
    inner: Bid,
    components: Option<Vec<ScoreComponent>>,
}

impl Deref for EnhancedBidWrapper {
    type Target = Bid;
    
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl BidExtension for EnhancedBidWrapper {
    fn score_components(&self) -> Option<&Vec<ScoreComponent>> {
        self.components.as_ref()
    }
}

/// Start the ICN Mesh Jobs server with P2P integration.
pub async fn run_server(
    database_url: String,
    p2p_identity: IcnKeyPair,
    p2p_listen_address: Option<String>,
    reputation_service_url: String,
    http_listen_addr: SocketAddr,
    test_listener_tx: Option<tokio::sync::broadcast::Sender<PlanetaryMeshMessage>>,
) -> Result<(SocketAddr, Vec<Multiaddr>), AppError> {
    tracing::info!("run_server: Initializing with DB URL: {}, P2P Listen: {:?}, HTTP Listen: {}", database_url, p2p_listen_address, http_listen_addr);

    // --- Database Setup ---
    tracing::info!("run_server: Setting up database at: {}", database_url);
    if !Sqlite::database_exists(&database_url).await.unwrap_or(false) {
        tracing::info!("run_server: Database not found, creating new one...");
        Sqlite::create_database(&database_url).await.map_err(AppError::from)?;
        tracing::info!("run_server: Database created.");
    }

    let pool = Arc::new(SqlitePool::connect(&database_url).await
        .map_err(AppError::from)?);
    tracing::info!("run_server: Database connection pool established.");

    match sqlx::migrate!("./migrations").run(&*pool).await {
        Ok(_) => tracing::info!("run_server: Database migrations completed successfully."),
        Err(e) => {
            tracing::error!("run_server: Failed to run database migrations: {}", e);
            return Err(AppError::Internal(anyhow::anyhow!("Database migration error: {}", e)));
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
        test_listener_tx,
    )
    .await
    .map_err(|e_anyhow| {
        if let Some(icn_err) = e_anyhow.downcast_ref::<icn_types::IcnError>() {
            tracing::warn!("P2P Node setup failed with ICN Error: {:?}", icn_err);
            match icn_err {
                icn_types::IcnError::Identity(id_err) => AppError::Internal(anyhow::anyhow!("P2P Identity setup error: {}", id_err)),
                icn_types::IcnError::Config(conf_err) => AppError::Internal(anyhow::anyhow!("P2P Configuration error: {}", conf_err)),
                _ => AppError::Internal(anyhow::anyhow!("P2P Node setup failed: {}", icn_err)),
            }
        } else {
            AppError::Internal(e_anyhow)
        }
    })?;
    
    let p2p_listen_addrs = p2p_node.get_listen_addrs().map_err(AppError::Internal)?;

    let app_state = Arc::new(AppState {
        store: store.clone(),
        reputation_cache: reputation_cache::ReputationCache::new(reputation_url.clone()),
        metrics_registry: Arc::new(prometheus::Registry::new()),
        p2p_node_state: Arc::new(tokio::sync::Mutex::new(p2p_node)),
        bid_evaluator_config: BidEvaluatorConfig::load_from_env(),
        job_processor: job_assignment::JobProcessor::new(store.clone(), reputation_url.clone()),
    });
    
    let metrics_route = get(metrics::metrics_handler).layer(Extension(app_state.metrics_registry.clone()));

    let app = Router::new()
        .route("/jobs", post(create_job).get(list_jobs))
        .route("/jobs/:job_id", get(get_job))
        .route("/jobs/:job_id/bids", post(submit_bid))
        .route("/jobs/:job_id/bids/stream", get(ws_stream_bids_handler))
        .route("/jobs/:job_id/assign_bid", post(assign_best_bid_handler))
        .route("/jobs/:job_id/start", post(start_job_handler))
        .route("/jobs/:job_id/complete", post(mark_job_completed_handler))
        .route("/jobs/:job_id/fail", post(mark_job_failed_handler))
        .route("/jobs/:job_id/begin_bidding", post(begin_bidding_handler))
        .route("/jobs/:job_id/bids/explain", get(get_bids_explained_handler))
        .route("/worker/:worker_did/jobs", get(get_jobs_for_worker_handler))
        .route("/metrics", metrics_route)
        .layer(Extension(store))
        .layer(Extension(reputation_url))
        .layer(Extension(app_state.p2p_node_state.clone()))
        .layer(Extension(app_state.metrics_registry.clone()))
        .layer(Extension(app_state.bid_evaluator_config.clone()))
        .layer(Extension(app_state.job_processor.clone()))
        .with_state(app_state);

    tracing::info!("run_server: Starting HTTP server on {}", http_listen_addr);

    axum::Server::bind(&http_listen_addr)
        .serve(app.into_make_service())
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("HTTP server failed: {}", e)))?;

    Ok((http_listen_addr, p2p_listen_addrs))
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
    // generate_job_cid_from_payload in main.rs already returns Result<Cid, AppError>
    // It maps serde_json to AppError::Internal(anyhow!("Failed to serialize job data for CID generation: {}", e))
    // and multihash to AppError::Internal(anyhow!("Failed to create multihash for CID generation: {}", e))
    // This could be refined further if needed, e.g. map serde to AppError::Serialization or AppError::BadRequest.
    let job_id = generate_job_cid_from_payload(&payload.params, &payload.originator_did)?;

    let job_request = JobRequest {
        job_id: job_id.clone(),
        params: payload.params,
        originator: payload.originator_did,
        execution_policy: None, // TODO: Allow specifying execution_policy in CreateJobApiPayload
    };

    // store.insert_job now returns Result<Cid, AppError>.
    // If it returns AppError::Database, it will propagate correctly.
    store.insert_job(job_request.clone()).await?;

    let response = json!({ "message": "Job created successfully", "job_id": job_id.to_string() });
    Ok((StatusCode::CREATED, AxumJson(response)))
}

async fn get_job(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Path(job_id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // TODO: Validate job_id_str format if necessary before parsing as CID
    let job_id_cid = Cid::try_from(job_id_str.clone()).map_err(|e| {
        AppError::InvalidCid(format!(
            "Invalid Job ID format for get_job: '{}'. Error: {}",
            job_id_str, e
        ))
    })?;

    match store.get_job(&job_id_cid).await? {
        Some((job_request, job_status)) => Ok(AxumJson(json!({
            "job_id": job_id_cid.to_string(),
            "request": job_request,
            "status": job_status,
        }))),
        None => Err(AppError::NotFound(format!(
            "Job with ID {} not found",
            job_id_cid.to_string()
        ))),
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
    let job_id = Cid::try_from(job_id_str.clone())
        .map_err(|e| AppError::InvalidCid(format!("Invalid Job ID format for submit_bid: {} - {}", job_id_str, e)))?;
    
    if bid_req.job_id != job_id {
        return Err(AppError::BadRequest("Job ID in path does not match Job ID in bid payload".to_string()));
    }

    // get_reputation_score now returns Result<Option<f64>, ReputationClientError>
    match reputation_client::get_reputation_score(&bid_req.bidder, &reputation_url).await {
        Ok(score_option) => {
            bid_req.reputation_score = score_option;
            if score_option.is_some() {
                tracing::info!(
                    "Fetched reputation score for bidder {}: {:?}. Populating in bid for job {}",
                    bid_req.bidder,
                    score_option,
                    job_id_str
                );
            } else {
                tracing::info!("No reputation score found for bidder {} (or service returned 404), proceeding without score.", bid_req.bidder);
            }
        }
        Err(rep_err) => { // rep_err is ReputationClientError
            tracing::warn!(
                "Failed to fetch reputation score for bidder {}: {}. Proceeding with no score.",
                bid_req.bidder, rep_err // rep_err has Display via thiserror
            );
            bid_req.reputation_score = None;
        }
    }

    store.insert_bid(&job_id, bid_req).await?;
    
    Ok(StatusCode::ACCEPTED)
}

async fn assign_best_bid_handler(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Extension(p2p_node_state): Extension<SharedP2pNode>,
    Extension(reputation_url): Extension<Arc<String>>,
    Path(job_id_str): Path<String>,
) -> Result<AxumJson<AssignJobResponse>, AppError> {
    let job_id_cid = Cid::try_from(job_id_str.clone())
        .map_err(|e| AppError::InvalidCid(format!("Invalid job ID format for assign_best_bid: {} - {}", job_id_str, e)))?;

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

    // 4. Create a reputation client
    let reputation_client = Arc::new(reputation_cache::CachingReputationClient::with_defaults(reputation_url));
    
    // 5. Create the bid evaluator config (should be loaded from governance/CCL)
    let config = BidEvaluatorConfig {
        weight_price: 0.4,
        weight_resources: 0.2,
        weight_reputation: 0.3,
        weight_timeliness: 0.1,
    };

    // 6. Determine ExecutorSelector based on ExecutionPolicy in JobRequest.params
    let mut policy = ExecutionPolicy::default();
    
    // If the job has a policy defined, use it
    if let Some(exec_policy) = job_request.execution_policy.as_ref() {
        policy = exec_policy.clone();
    }

    let selector = match policy.selection_strategy {
        SelectionStrategy::LowestPrice => {
            tracing::info!(job_id = %job_id_str, "Using LowestPriceExecutorSelector");
            Box::new(LowestPriceExecutorSelector {}) as Box<dyn ExecutorSelector>
        }
        SelectionStrategy::Reputation => {
            tracing::info!(job_id = %job_id_str, "Using ReputationExecutorSelector with weights");
            Box::new(ReputationExecutorSelector {
                config: config.clone(),
                reputation_client: reputation_client.clone(),
            }) as Box<dyn ExecutorSelector>
        }
        SelectionStrategy::Hybrid => {
            tracing::info!(job_id = %job_id_str, "Using HybridExecutorSelector with policy");
            Box::new(HybridExecutorSelector {
                policy,
                reputation_client: reputation_client.clone(),
            }) as Box<dyn ExecutorSelector>
        }
    };

    // 7. Select the winning bid
    let selection_result = selector.select(&job_request, &bids, job_id_cid).await?;
    
    let (winning_bid, winning_score, selection_reason) = match selection_result {
        Some((bid, score, reason)) => (bid, score, reason),
        None => {
            tracing::warn!(job_id = %job_id_str, "No acceptable bid found for job");
            return Err(AppError::NotFound(format!("No acceptable bid found for job {}", job_id_str)));
        }
    };

    // 8. Record metrics for the winning bid
    metrics::record_bid_evaluation(&selection_reason);
    
    // Record component scores if we have them (from the ReputationExecutorSelector)
    if let Some(components) = winning_bid.score_components() {
        for component in components {
            metrics::record_bid_component_score(
                &component.name, 
                &winning_bid.bidder.0,
                component.value
            );
        }
    }

    let winning_bid_id = winning_bid.id.ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!("Winning bid has no ID"))
    })?;

    // 9. Assign the job in the store
    tracing::info!(
        job_id = %job_id_str,
        bid_id = winning_bid_id,
        bidder = %winning_bid.bidder.0,
        score = winning_score,
        "Assigning job to winning bidder"
    );
    
    store.assign_job(&job_id_cid, winning_bid.bidder.clone()).await?;

    // 10. Notify the P2P mesh that this node is assigning the job (if we're in mesh mode)
    // This is a local, synchronous message, not a P2P message yet
    if let Some(p2p_state) = p2p_node_state.as_ref() {
        let mut p2p_lock = p2p_state.lock().await;
        p2p_lock.assign_job(job_id_cid, winning_bid.bidder.clone())
            .map_err(|e| AppError::P2pError(format!("Failed to notify P2P mesh for job assignment {}: {}", job_id_cid, e)))?;
    }

    Ok(AxumJson(AssignJobResponse {
        message: "Job assigned successfully. P2P notification to executor initiated.".to_string(),
        job_id: job_id_str,
        assigned_bidder_did: winning_bid.bidder.0.clone(),
        winning_bid_id,
        winning_score,
        reason: selection_reason,
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
    let job_id = Cid::try_from(job_id_str.clone())
        .map_err(|e| AppError::InvalidCid(format!("Invalid Job ID format for mark_job_completed: {} - {}", job_id_str, e)))?;

    let worker_did_header = headers
        .get("X-Worker-DID")
        .ok_or_else(|| AppError::BadRequest("Missing X-Worker-DID header".to_string()))?
        .to_str()
        .map_err(|_| AppError::BadRequest("Invalid X-Worker-DID header format".to_string()))?;
    let worker_did = Did(worker_did_header.to_string());

    let (_, job_status) = store.get_job(&job_id).await? 
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
            return Err(AppError::InvalidStatusTransition(format!("Job {} not in Running state, cannot mark completed. Current state: {:?}.", job_id, job_status)));
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

    reputation_client::submit_reputation_record(&record, &reputation_url).await?;

    store.update_job_status(&job_id, JobStatus::Completed).await?; 
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
    let job_id = Cid::try_from(job_id_str.clone())
        .map_err(|e| AppError::InvalidCid(format!("Invalid Job ID format for mark_job_failed: {} - {}", job_id_str, e)))?;

    let worker_did_header = headers
        .get("X-Worker-DID")
        .ok_or_else(|| AppError::BadRequest("Missing X-Worker-DID header".to_string()))?
        .to_str()
        .map_err(|_| AppError::BadRequest("Invalid X-Worker-DID header format".to_string()))?;
    let worker_did = Did(worker_did_header.to_string());

    let (_, job_status) = store.get_job(&job_id).await?
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
            return Err(AppError::InvalidStatusTransition(format!("Job {} not in Running state, cannot mark failed. Current state: {:?}.", job_id, job_status)));
        }
    };

    let failure_reason_obj = JobFailureReason::Unknown(details.reason.clone());

    let record = ReputationRecord {
        timestamp: Utc::now(),
        issuer: MESH_JOBS_SYSTEM_DID.clone(),
        subject: runner_did.clone(),
        event: ReputationUpdateEvent::JobFailed {
            job_id,
            reason: failure_reason_obj.clone(),
            anchor_cid: details.failure_anchor_cid,
        },
        anchor: details.failure_anchor_cid,
        signature: None,
    };

    reputation_client::submit_reputation_record(&record, &reputation_url).await?;
    
    store.update_job_status(&job_id, JobStatus::Failed { reason: failure_reason_obj }).await?;
    tracing::info!("Marked job {} as Failed. Reason: {}. Reputation record submitted for runner {}.", job_id, details.reason, runner_did.0);
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
    let job_id = Cid::try_from(job_id_str.clone())
        .map_err(|e| AppError::InvalidCid(format!("Invalid Job ID format for begin_bidding: {} - {}", job_id_str, e)))?;

    let (_, current_status) = store.get_job(&job_id).await?
        .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id)))?;

    match current_status {
        JobStatus::Pending => {
            store.update_job_status(&job_id, JobStatus::Bidding).await?;
            tracing::info!("Job {} has been moved to Bidding state.", job_id);
            Ok(StatusCode::OK)
        }
        JobStatus::Bidding => {
            tracing::info!("Job {} is already in Bidding state.", job_id);
            Ok(StatusCode::OK) // Or potentially a BadRequest/Conflict if re-triggering is an issue
        }
        _ => {
            tracing::warn!("Job {} is in status {:?} and cannot be moved to Bidding state.", job_id, current_status);
            Err(AppError::InvalidStatusTransition(format!("Job {} is in status {:?} and cannot be moved to Bidding state.", job_id, current_status)))
        }
    }
}

/// Get all bids for a job with detailed explanation of scoring
async fn get_bids_explained_handler(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Extension(reputation_url): Extension<Arc<String>>,
    Path(job_id_str): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<AxumJson<BidsExplainResponse>, AppError> {
    let job_id = Cid::try_from(job_id_str.clone()).map_err(|e| 
        AppError::InvalidCid(format!("Invalid Job ID format: {} - {}", job_id_str, e))
    )?;
    
    // Get job and bids
    let (job_request, _) = store.get_job(&job_id).await?
        .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id)))?;
    
    let bids = store.list_bids(&job_id).await?;
    if bids.is_empty() {
        return Err(AppError::NotFound(format!("No bids found for job: {}", job_id)));
    }
    
    // Create reputation client with caching
    let client = reputation_cache::CachingReputationClient::with_defaults(reputation_url);
    
    // Default bid evaluation config (could be loaded from CCL policy or DB in future)
    let config = BidEvaluatorConfig {
        weight_price: 0.4,
        weight_resources: 0.2,
        weight_reputation: 0.3,
        weight_timeliness: 0.1,
    };
    
    // Generate explanations for each bid
    let mut explanations = Vec::with_capacity(bids.len());
    
    for bid in &bids {
        // Fetch profile (will use cache if available)
        let profile = match client.fetch_profile(&bid.bidder.0).await {
            Ok(profile) => profile,
            Err(e) => {
                tracing::warn!("Failed to fetch reputation profile for {}: {}", bid.bidder.0, e);
                // Generate a default profile
                ReputationProfile {
                    node_id: bid.bidder.0.clone(),
                    total_jobs: 0,
                    successful_jobs: 0,
                    failed_jobs: 0,
                    jobs_on_time: 0,
                    jobs_late: 0,
                    average_execution_ms: None,
                    average_bid_accuracy: None,
                    dishonesty_events: 0,
                    endorsements: Vec::new(),
                    computed_score: 50.0, // Default score
                }
            }
        };
        
        // Calculate normalized price (0-1 where 0 is lowest price)
        let max_price = bids.iter().map(|b| b.price).max().unwrap_or(1);
        let normalized_price = if max_price > 0 {
            bid.price as f64 / max_price as f64
        } else {
            0.0
        };
        
        // Calculate resource match (0-1 where 1 is perfect match)
        let resource_match = calculate_resource_match(&bid.estimate, &job_request.requirements);
        
        // Calculate individual score components
        let price_component = config.weight_price * (1.0 - normalized_price);
        let resources_component = config.weight_resources * resource_match;
        
        // Reputation components
        let reputation_score = profile.computed_score / 100.0;
        let reputation_component = config.weight_reputation * reputation_score;
        
        // Timeliness component
        let timeliness_score = if profile.successful_jobs > 0 {
            profile.jobs_on_time as f64 / profile.successful_jobs as f64
        } else {
            0.5 // Default
        };
        let timeliness_component = config.weight_timeliness * timeliness_score;
        
        // Calculate total score
        let total_score = price_component + resources_component + reputation_component + timeliness_component;
        
        // Create component breakdown
        let components = vec![
            ScoreComponent {
                name: "price".to_string(),
                value: price_component,
                weight: config.weight_price,
            },
            ScoreComponent {
                name: "resources".to_string(),
                value: resources_component,
                weight: config.weight_resources,
            },
            ScoreComponent {
                name: "reputation".to_string(),
                value: reputation_component,
                weight: config.weight_reputation,
            },
            ScoreComponent {
                name: "timeliness".to_string(),
                value: timeliness_component,
                weight: config.weight_timeliness,
            },
        ];
        
        // Create reputation summary
        let reputation_summary = ReputationSummary {
            score: profile.computed_score,
            jobs_count: profile.total_jobs,
            on_time_ratio: if profile.successful_jobs > 0 {
                profile.jobs_on_time as f64 / profile.successful_jobs as f64
            } else {
                0.0
            },
        };
        
        // Add explanation for this bid
        explanations.push(BidExplanation {
            bid_id: bid.id,
            node_did: bid.bidder.0.clone(),
            total_score,
            components,
            reputation_summary,
        });
    }
    
    // Sort explanations by score (highest first)
    explanations.sort_by(|a, b| b.total_score.partial_cmp(&a.total_score).unwrap_or(std::cmp::Ordering::Equal));
    
    Ok(AxumJson(BidsExplainResponse {
        bids: bids.clone(),
        explanations,
        config,
    }))
}

// Helper function to calculate resource match score
fn calculate_resource_match(estimate: &ResourceEstimate, requirements: &ResourceRequirements) -> f64 {
    // Calculate match as a value from 0 to 1 where 1 is a perfect match
    // This is a simple implementation - could be enhanced with more sophisticated matching
    
    // CPU match - estimate should be >= requirement
    let cpu_match = if estimate.cpu >= requirements.cpu {
        1.0
    } else {
        estimate.cpu as f64 / requirements.cpu as f64
    };
    
    // Memory match
    let memory_match = if estimate.memory_mb >= requirements.memory_mb {
        1.0
    } else {
        estimate.memory_mb as f64 / requirements.memory_mb as f64
    };
    
    // Storage match
    let storage_match = if estimate.storage_mb >= requirements.storage_mb {
        1.0
    } else {
        estimate.storage_mb as f64 / requirements.storage_mb as f64
    };
    
    // Average the match scores
    (cpu_match + memory_match + storage_match) / 3.0
}

/// Handler for Prometheus metrics endpoint
async fn metrics_handler() -> impl IntoResponse {
    let encoder = prometheus::TextEncoder::new();
    let registry = metrics::get_registry();
    
    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&registry.gather(), &mut buffer) {
        tracing::error!("Failed to encode Prometheus metrics: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to encode metrics").into_response();
    }
    
    match String::from_utf8(buffer) {
        Ok(metrics_text) => {
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/plain")],
                metrics_text
            ).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to convert metrics to UTF-8: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to format metrics").into_response()
        }
    }
} 