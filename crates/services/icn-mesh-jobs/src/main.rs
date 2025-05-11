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
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tracing_subscriber;
use chrono::Utc;

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    // --- Database Setup ---
    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:icn_mesh_jobs.db?mode=rwc".to_string());
    tracing::info!("Using database at: {}", database_url);

    if !Sqlite::database_exists(&database_url).await.unwrap_or(false) {
        tracing::info!("Database not found, creating new one...");
        Sqlite::create_database(&database_url).await?;
        tracing::info!("Database created.");
    }

    let pool = Arc::new(SqlitePool::connect(&database_url).await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database {}: {}", database_url, e))?);
    tracing::info!("Database connection pool established.");

    // Run migrations
    // The path is relative to CARGO_MANIFEST_DIR, which is the root of the current crate.
    // If main.rs is in src/, then ./migrations is correct if migrations/ is in the crate root.
    match sqlx::migrate!("./migrations").run(&*pool).await {
        Ok(_) => tracing::info!("Database migrations completed successfully."),
        Err(e) => {
            tracing::error!("Failed to run database migrations: {}", e);
            // Depending on the error, you might want to panic or handle differently.
            // For instance, if the migrations table is locked, it might be a transient issue.
            // If migrations are corrupt, it's a fatal error.
            return Err(anyhow::anyhow!("Migration error: {}", e));
        }
    }
    // --- End Database Setup ---

    // Replace InMemoryStore with SqliteStore
    let store: Arc<dyn MeshJobStore> = Arc::new(SqliteStore::new(pool.clone()));
    tracing::info!("Using SqliteStore for job and bid management.");

    let reputation_service_url = Arc::new(env::var("REPUTATION_SERVICE_URL").unwrap_or_else(|_| "http://localhost:8081".to_string()));
    tracing::info!("Using reputation service at: {}", *reputation_service_url);

    let app = Router::new()
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
        .layer(Extension(reputation_service_url));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    tracing::info!("ICN Mesh Jobs service listening on {}", addr);
    axum::Server::bind(&addr).serve(app.into_make_service()).await?;
    Ok(())
}

async fn create_job(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    AxumJson(req): AxumJson<JobRequest>,
) -> Result<impl IntoResponse, AppError> {
    match store.insert_job(req).await {
        Ok(job_cid) => Ok((StatusCode::CREATED, AxumJson(json!({ "job_id": job_cid.to_string() })))),
        Err(e) => {
            tracing::error!("Failed to create job: {}", e);
            Err(AppError::Internal(e))
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
    Path(job_id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let job_id = Cid::try_from(job_id_str.clone()).map_err(|e| AppError::BadRequest(format!("Invalid Job ID format: {} - {}", job_id_str, e)))?;

    let (job_request, current_status) = store.get_job(&job_id).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id)))?;
    
    match current_status {
        JobStatus::Bidding => { /* Proceed */ }
        _ => return Err(AppError::BadRequest(format!("Job {} is not in Bidding state and cannot have a bid assigned. Current status: {:?}", job_id, current_status))),
    }

    let bids = store.list_bids(&job_id).await.map_err(AppError::Internal)?;
    if bids.is_empty() {
        tracing::info!("No bids found for job {}. Cannot assign.", job_id);
        return Err(AppError::NotFound(format!("No bids available for job {}", job_id)));
    }

    let mut best_bid: Option<&Bid> = None;
    let mut highest_score = f64::NEG_INFINITY;

    for bid in &bids {
        let score = bid_logic::calculate_bid_selection_score(
            bid, 
            &job_request, 
            bid_logic::DEFAULT_REP_WEIGHT, 
            bid_logic::DEFAULT_PRICE_WEIGHT
        );
        tracing::debug!("Calculated score {} for bid from {} on job {}", score, bid.bidder.0, job_id);
        if score > highest_score {
            highest_score = score;
            best_bid = Some(bid);
        }
    }

    if let Some(winner) = best_bid {
        if highest_score < 0.0 {
            tracing::info!("No bids for job {} met the minimum score threshold (all scores <= 0 or disqualified).", job_id);
            return Err(AppError::NotFound(format!("No suitable bids found for job {} after scoring.", job_id)));
        }

        store.assign_job(&job_id, winner.bidder.clone()).await.map_err(AppError::Internal)?;
        tracing::info!(
            "Job {} assigned to bidder {} with score {}. Winning bid: {:?}",
            job_id, winner.bidder.0, highest_score, winner
        );
        Ok(AxumJson(json!({
            "message": "Job assigned successfully",
            "job_id": job_id.to_string(),
            "assigned_bidder": winner.bidder.0,
            "winning_score": highest_score,
            "winning_bid": winner
        })).into_response())
    } else {
        tracing::info!("No eligible bid found for job {} after selection process.", job_id);
        Err(AppError::NotFound(format!("No eligible bid found for assignment on job {}", job_id)))
    }
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