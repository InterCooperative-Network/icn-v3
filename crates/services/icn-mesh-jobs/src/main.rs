use axum::{
    extract::{Extension, Path, Query, State, WebSocketUpgrade, ws::{Message, WebSocket}},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json,
    Router,
};
use cid::Cid;
use futures::{stream::StreamExt, SinkExt};
use icn_types::jobs::{Bid, JobRequest, JobStatus};
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tracing_subscriber;

mod storage;
use storage::{InMemoryStore, MeshJobStore};

#[derive(Deserialize)]
struct ListJobsQuery {
    status: Option<String>, // We'll parse this into JobStatus
}

// Helper to convert String to JobStatus, very basic for now.
// In a real app, you'd use a more robust parsing, possibly with serde on JobStatus itself.
fn parse_job_status(s: Option<String>) -> Option<JobStatus> {
    s.and_then(|status_str| match status_str.to_lowercase().as_str() {
        "pending" => Some(JobStatus::Pending),
        "bidding" => Some(JobStatus::Bidding),
        // Add other statuses as needed for filtering
        _ => None,
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let store = Arc::new(InMemoryStore::new());

    let app = Router::new()
        .route("/jobs", post(create_job).get(list_jobs))
        .route("/jobs/:job_id", get(get_job))
        .route("/jobs/:job_id/bids", post(submit_bid).get(ws_stream_bids_handler)) // Changed stream_bids to ws_stream_bids_handler for clarity
        .layer(Extension(store.clone())); // Add store as a layer

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    tracing::info!("Listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

async fn create_job(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Json(req): Json<JobRequest>,
) -> Result<impl IntoResponse, AppError> {
    match store.insert_job(req).await {
        Ok(job_cid) => Ok((StatusCode::CREATED, Json(json!({ "job_id": job_cid.to_string() })))),
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
    match store.get_job(&job_id).await {
        Ok(Some((_req, status))) => Ok(Json(status)), // Consider returning the full JobRequest too
        Ok(None) => Err(AppError(anyhow::anyhow!("Job not found"))),
        Err(e) => {
            tracing::error!("Failed to get job: {}", e);
            Err(AppError(e))
        }
    }
}

async fn list_jobs(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Query(query): Query<ListJobsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let status_filter = parse_job_status(query.status);
    match store.list_jobs(status_filter).await {
        Ok(job_cids) => Ok(Json(job_cids.into_iter().map(|cid| cid.to_string()).collect::<Vec<_>>())),
        Err(e) => {
            tracing::error!("Failed to list jobs: {}", e);
            Err(AppError(e))
        }
    }
}

async fn submit_bid(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Path(job_id_str): Path<String>,
    Json(bid_req): Json<Bid>, // Assuming Bid type from icn_types is directly usable
) -> Result<impl IntoResponse, AppError> { // Changed name from bid to bid_req to avoid conflict with Bid type
    let job_id = Cid::try_from(job_id_str.clone()).map_err(|e| AppError(anyhow::anyhow!(e)))?;
    
    // Basic validation: job_id in path should match job_id in bid payload
    if bid_req.job_id != job_id {
        return Err(AppError(anyhow::anyhow!("Job ID in path does not match Job ID in bid payload")));
    }

    match store.insert_bid(&job_id, bid_req).await {
        Ok(_) => Ok(StatusCode::ACCEPTED),
        Err(e) => {
            tracing::error!("Failed to submit bid for job {}: {}", job_id_str, e);
            // Consider more specific error codes (e.g., 404 if job not found, 400/409 if not ready for bids)
            Err(AppError(e)) 
        }
    }
}

// Renamed from stream_bids and added WebSocketUpgrade extractor
async fn ws_stream_bids_handler(
    Extension(store): Extension<Arc<dyn MeshJobStore>>,
    Path(job_id_str): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    let job_id = Cid::try_from(job_id_str.clone()).map_err(|e| AppError(anyhow::anyhow!(e)))?;
    
    // Check if job exists before upgrading
    if store.get_job(&job_id).await?.is_none() {
        return Err(AppError(anyhow::anyhow!("Job not found: {}", job_id_str)));
    }

    Ok(ws.on_upgrade(move |socket| handle_bid_stream(socket, store, job_id)))
}

async fn handle_bid_stream(
    mut socket: WebSocket,
    store: Arc<dyn MeshJobStore>,
    job_id: Cid,
) {
    tracing::info!("WebSocket connection established for job bids: {}", job_id);

    // 1. Send existing bids first
    match store.list_bids(&job_id).await {
        Ok(bids) => {
            for bid in bids {
                if let Ok(json_bid) = serde_json::to_string(&bid) {
                    if socket.send(Message::Text(json_bid)).await.is_err() {
                        tracing::warn!("Failed to send existing bid to WebSocket client for job {}", job_id);
                        return; // Client disconnected or error
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to list existing bids for WebSocket stream for job {}: {}", job_id, e);
            // Optionally send an error message over WebSocket
            let _ = socket.send(Message::Text(serde_json::to_string(&json!({ "error": e.to_string() })).unwrap_or_default())).await;
            return;
        }
    }

    // 2. Subscribe to new bids
    let mut bid_receiver = match store.subscribe_to_bids(&job_id).await {
        Ok(Some(rx)) => rx,
        Ok(None) => {
            // This case might happen if the broadcaster was created *after* list_bids but before subscribe_to_bids,
            // or if the job was deleted. For InMemoryStore, get_or_create_broadcaster in insert_bid
            // and then subscribe_to_bids should generally find it if the job exists and has bids.
            // If no broadcaster exists (e.g., job has no bids yet, or never had one created by insert_bid)
            // we can try to create/get one. Or, if it implies an issue, log and close.
            tracing::warn!("No active bid broadcaster found for job {}, trying to create/get one.", job_id);
            // Attempt to get/create a receiver again, as insert_bid creates it.
            // This path implies the job exists (checked in ws_stream_bids_handler).
            // If a job can exist without a broadcaster (e.g. no bids submitted yet), then this is fine.
            // `subscribe_to_bids` from InMemoryStore uses `get_bid_receiver` which doesn't create.
            // To ensure a channel exists for a job that exists, we might need `get_or_create_receiver` logic.
            // For now, if it's None, it means no sender has been created (no bids submitted yet and thus no broadcaster).
            // The client will simply not receive live updates until the first bid is made.
            // Alternatively, create one here ensure it exists:
            // match store.get_or_create_broadcaster(&job_id).await { // this method isn't on the trait
            //    Ok(sender) => sender.subscribe(),
            //    Err(_) => { tracing::error!("Failed to ensure bid broadcaster for job {}", job_id); return; }
            // }
            // For now, let's stick to the logic that if subscribe_to_bids returns None, we can't stream.
            // The user's sketch returns early if subscribe is None, which is simple.
            tracing::info!("No bid broadcaster channel for job {}, will not stream live bids.", job_id);
            // We could simply return, or keep the socket open for other messages if planned.
            // For now, if no receiver, we close the stream for new bids from server side.
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
            //biased; // You might consider `biased` if you want to prioritize client messages

            // New bid received from the broadcast channel
            Ok(bid) = bid_receiver.recv() => {
                if let Ok(json_bid) = serde_json::to_string(&bid) {
                    if socket.send(Message::Text(json_bid)).await.is_err() {
                        tracing::warn!("Failed to send new bid to WebSocket client for job {}. Client disconnected?", job_id);
                        break; // Error sending, assume client disconnected
                    }
                } else {
                    tracing::warn!("Failed to serialize new bid for job {}: {:?}", job_id, bid);
                }
            }

            // Message received from the WebSocket client
            Some(Ok(msg)) = socket.next() => {
                match msg {
                    Message::Text(t) => {
                        tracing::debug!("Received text message from client for job {}: {}", job_id, t);
                        // Echo back or process if needed in future
                    }
                    Message::Binary(b) => {
                        tracing::debug!("Received binary message from client for job {}: {:?}", job_id, b);
                    }
                    Message::Ping(p) => {
                        if socket.send(Message::Pong(p)).await.is_err() {
                            break;
                        }
                    }
                    Message::Pong(_) => {
                        // Pong received, connection is alive
                        tracing::debug!("Pong received from client for job {}", job_id);
                    }
                    Message::Close(c) => {
                        tracing::info!("Client closed WebSocket connection for job {}: {:?}", job_id, c);
                        break;
                    }
                }
            }
            // Broadcast channel closed or lagged
            Err(RecvError::Closed) = bid_receiver.recv() => {
                tracing::warn!("Bid broadcast channel closed for job {}. No more live bids will be sent.", job_id);
                break;
            }
            Err(RecvError::Lagged(n)) = bid_receiver.recv() => {
                tracing::warn!("Bid broadcast channel lagged for job {} by {} messages. Client may have missed bids.", job_id, n);
                // Potentially resync or inform client, for now just continue listening
            }

            // Client disconnected
            else => {
                tracing::info!("WebSocket client for job {} disconnected or stream ended.", job_id);
                break;
            }
        }
    }
    tracing::info!("Stopped streaming bids for job {}", job_id);
}

// Custom error type for Axum handlers
struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("Application error: {:#}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": self.0.to_string() })),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
} 