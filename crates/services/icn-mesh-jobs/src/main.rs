use axum::{
    extract::{Extension, Path, Query, State, WebSocketUpgrade, ws::{Message, WebSocket}},
    http::StatusCode,
    response::{IntoResponse, Response, Json as AxumJson},
    routing::{get, post},
    Router,
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

mod storage;
use storage::{InMemoryStore, MeshJobStore};

mod reputation_client;

struct AppError(anyhow::Error);
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("Application error: {:#}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, AxumJson(json!({ "error": self.0.to_string() }))).into_response()
    }
}
impl<E: Into<anyhow::Error>> From<E> for AppError { fn from(err: E) -> Self { Self(err.into()) } }

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
    let store = Arc::new(InMemoryStore::new());
    let reputation_service_url = Arc::new(env::var("REPUTATION_SERVICE_URL").unwrap_or_else(|_| "http://localhost:8081".to_string()));
    tracing::info!("Using reputation service at: {}", *reputation_service_url);

    let app = Router::new()
        .route("/jobs", post(create_job).get(list_jobs))
        .route("/jobs/:job_id", get(get_job))
        .route("/jobs/:job_id/bids", post(submit_bid).get(ws_stream_bids_handler))
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
            Err(AppError(e))
        }
    }
}

async fn get_job(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Path(job_id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let job_id = Cid::try_from(job_id_str).map_err(|e| AppError(anyhow::anyhow!(e)))?;
    match store.get_job(&job_id).await? {
        Some((_req, status)) => Ok(AxumJson(status).into_response()),
        None => Err(AppError(anyhow::anyhow!("Job not found"))),
    }
}

async fn list_jobs(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Query(query): Query<ListJobsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let status_filter = parse_job_status(query.status);
    match store.list_jobs(status_filter).await? {
        job_cids => Ok(AxumJson(job_cids.into_iter().map(|cid| cid.to_string()).collect::<Vec<_>>()).into_response()),
    }
}

async fn submit_bid(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Extension(reputation_url): Extension<Arc<String>>,
    Path(job_id_str): Path<String>,
    AxumJson(mut bid_req): AxumJson<Bid>,
) -> Result<impl IntoResponse, AppError> {
    let job_id = Cid::try_from(job_id_str.clone()).map_err(|e| AppError(anyhow::anyhow!(e)))?;
    if bid_req.job_id != job_id {
        return Err(AppError(anyhow::anyhow!("Job ID in path does not match Job ID in bid payload")));
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

    store.insert_bid(&job_id, bid_req).await?;
    Ok(StatusCode::ACCEPTED)
}

async fn mark_job_completed_handler(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Extension(reputation_url): Extension<Arc<String>>,
    Path(job_id_str): Path<String>,
    AxumJson(details): AxumJson<JobCompletionDetails>,
) -> Result<StatusCode, AppError> {
    let job_id = Cid::try_from(job_id_str.clone())?;
    let (_, job_status) = store.get_job(&job_id).await?.ok_or_else(|| AppError(anyhow::anyhow!("Job not found")))?;

    let bidder_did = match job_status {
        JobStatus::Assigned { bidder } => bidder,
        JobStatus::Running => {
            tracing::warn!("Job {} is Running, assuming it was assigned. Need to confirm bidder.", job_id);
            return Err(AppError(anyhow::anyhow!("Job must be in Assigned state to get bidder for completion.")));
        }
        _ => return Err(AppError(anyhow::anyhow!("Job not in Assignable/Running state to be completed"))),
    };

    let record = ReputationRecord {
        timestamp: Utc::now(),
        issuer: MESH_JOBS_SYSTEM_DID.clone(),
        subject: bidder_did.clone(),
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

    store.update_job_status(&job_id, JobStatus::Completed).await?;
    tracing::info!("Marked job {} as Completed. Reputation record submitted for bidder {}.", job_id, bidder_did.0);
    Ok(StatusCode::OK)
}

async fn mark_job_failed_handler(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Extension(reputation_url): Extension<Arc<String>>,
    Path(job_id_str): Path<String>,
    AxumJson(details): AxumJson<JobFailureDetails>,
) -> Result<StatusCode, AppError> {
    let job_id = Cid::try_from(job_id_str.clone())?;
    let (_, job_status) = store.get_job(&job_id).await?.ok_or_else(|| AppError(anyhow::anyhow!("Job not found")))?;

    let bidder_did = match job_status {
        JobStatus::Assigned { bidder } => bidder,
        JobStatus::Running => {
            return Err(AppError(anyhow::anyhow!("Job must be in Assigned state to get bidder for failure reporting.")));
        }
        _ => return Err(AppError(anyhow::anyhow!("Job not in Assignable/Running state to be marked failed"))),
    };

    let record = ReputationRecord {
        timestamp: Utc::now(),
        issuer: MESH_JOBS_SYSTEM_DID.clone(),
        subject: bidder_did.clone(),
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

    store.update_job_status(&job_id, JobStatus::Failed { reason: details.reason }).await?;
    tracing::info!("Marked job {} as Failed. Reason: {}. Reputation record submitted for bidder {}.", job_id, store.get_job(&job_id).await.map(|j| format!("{:?}", j.map(|(_,s)|s))).unwrap_or_default(), bidder_did.0);
    Ok(StatusCode::OK)
}

async fn ws_stream_bids_handler(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Path(job_id_str): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    let job_id = Cid::try_from(job_id_str.clone()).map_err(|e| AppError(anyhow::anyhow!(e)))?;
    if store.get_job(&job_id).await?.is_none() {
        return Err(AppError(anyhow::anyhow!("Job not found: {}", job_id_str)));
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
            Ok(bid) = bid_receiver.recv() => {
                if let Ok(json_bid) = serde_json::to_string(&bid) {
                    if socket.send(Message::Text(json_bid)).await.is_err() {
                        tracing::warn!("Failed to send new bid to WebSocket client for job {}. Client disconnected?", job_id);
                        break; 
                    }
                } else {
                    tracing::warn!("Failed to serialize new bid for job {}: {:?}", job_id, bid);
                }
            }
            Some(Ok(msg)) = socket.next() => {
                match msg {
                    Message::Text(t) => { tracing::debug!("Received text message from client for job {}: {}", job_id, t); }
                    Message::Binary(b) => { tracing::debug!("Received binary message from client for job {}: {:?}", job_id, b); }
                    Message::Ping(p) => { if socket.send(Message::Pong(p)).await.is_err() { break; } }
                    Message::Pong(_) => { tracing::debug!("Pong received from client for job {}", job_id); }
                    Message::Close(c) => { tracing::info!("Client closed WebSocket connection for job {}: {:?}", job_id, c); break; }
                }
            }
            Err(RecvError::Closed) = bid_receiver.recv() => {
                tracing::warn!("Bid broadcast channel closed for job {}. No more live bids will be sent.", job_id);
                break;
            }
            Err(RecvError::Lagged(n)) = bid_receiver.recv() => {
                tracing::warn!("Bid broadcast channel lagged for job {} by {} messages. Client may have missed bids.", job_id, n);
            }
            else => {
                tracing::info!("WebSocket client for job {} disconnected or stream ended.", job_id);
                break;
            }
        }
    }
    tracing::info!("Stopped streaming bids for job {}", job_id);
} 