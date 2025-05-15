use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response, Json as AxumJson}, // Renamed Json to AxumJson to avoid conflict
    routing::{get, post},
    Router,
};
use icn_identity::Did;
use icn_types::reputation::{ReputationProfile, ReputationRecord, ReputationUpdateEvent};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{net::SocketAddr, sync::Arc};
use tracing_subscriber;
use std::collections::HashMap;

// Declare the new reputation module
pub mod reputation;

// Assuming storage module is in the same directory or crate root
mod storage;
use storage::{InMemoryReputationStore, ReputationStore};

// Application error type for consistent JSON error responses
struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("Application error: {:#}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            AxumJson(json!({ "error": self.0.to_string() })),
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

#[derive(Debug, Deserialize)]
pub struct ReputationProfileParams {
    pub min_score: Option<f64>,
    pub max_score: Option<f64>,
    pub did: Option<String>,
    pub sort_by: Option<String>, // "score", "updated_at", etc.
}

#[derive(Debug, Serialize)]
pub struct ReputationProfileSummary {
    pub did: Did,
    pub score: f64,
    pub successful_jobs: usize,
    pub failed_jobs: usize,
    pub last_updated: Option<i64>, // Unix timestamp (assuming ReputationRecord.timestamp is this)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing (logging)
    tracing_subscriber::fmt::init();

    let store: Arc<dyn ReputationStore> = Arc::new(InMemoryReputationStore::new());

    let app = Router::new()
        .route("/reputation/records", post(submit_record_handler))
        .route("/reputation/profiles/:did", get(get_profile_handler))
        .route("/reputation/records/:did", get(get_records_handler))
        .route("/reputation/profiles", get(get_all_reputation_profiles_handler))
        .route("/reputation/profiles/:did/history", get(get_reputation_profile_history_handler))
        .layer(Extension(store));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8081)); // Using port 8081 as specified
    tracing::info!("🚀 ICN Reputation service running on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

async fn submit_record_handler(
    Extension(store): Extension<Arc<dyn ReputationStore>>,
    AxumJson(record): AxumJson<ReputationRecord>,
) -> Result<StatusCode, AppError> {
    store.submit_record(record).await?;
    Ok(StatusCode::CREATED)
}

async fn get_profile_handler(
    Extension(store): Extension<Arc<dyn ReputationStore>>,
    Path(did_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Assuming Did can be constructed directly from a String. 
    // If Did has a specific parser, that should be used.
    let did = Did(did_str); // Direct conversion from String to Did

    match store.get_profile(&did).await? {
        Some(profile) => Ok(AxumJson(profile).into_response()),
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

async fn get_records_handler(
    Extension(store): Extension<Arc<dyn ReputationStore>>,
    Path(did_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let did = Did(did_str); // Direct conversion from String to Did

    let records = store.list_records(&did).await?;
    Ok(AxumJson(records).into_response())
}

pub async fn get_all_reputation_profiles_handler(
    Extension(store): Extension<Arc<dyn ReputationStore + Send + Sync>>,
    Query(params): Query<ReputationProfileParams>,
) -> Result<AxumJson<Vec<ReputationProfileSummary>>, AppError> {
    let all_records = store.list_all_records().await?;
    
    let mut grouped: HashMap<Did, Vec<ReputationRecord>> = HashMap::new();
    for record in all_records {
        grouped.entry(record.subject.clone()).or_default().push(record);
    }

    let mut profiles: Vec<ReputationProfileSummary> = grouped.into_iter().map(|(did, records)| {
        let mut successful = 0;
        let mut failed = 0;
        let mut latest_ts = 0i64;

        for r in &records {
            latest_ts = latest_ts.max(r.issued_at.timestamp());
            
            match r.event {
                ReputationUpdateEvent::JobCompletedSuccessfully { .. } => successful += 1,
                ReputationUpdateEvent::JobFailed { .. } => failed += 1,
            }
        }

        let base = 50.0;
        let mut score = (base + (successful as f64 * 10.0) - (failed as f64 * 15.0));
        score = score.max(0.0).min(100.0);
        if successful == 0 && failed == 0 {
            score = 50.0;
        }

        ReputationProfileSummary {
            did,
            score,
            successful_jobs: successful,
            failed_jobs: failed,
            last_updated: if records.is_empty() { None } else { Some(latest_ts) },
        }
    }).collect();

    if let Some(min) = params.min_score {
        profiles.retain(|p| p.score >= min);
    }
    if let Some(max) = params.max_score {
        profiles.retain(|p| p.score <= max);
    }
    if let Some(ref did_filter_str) = params.did {
        let target_did = Did(did_filter_str.clone());
        profiles.retain(|p| p.did == target_did);
    }

    match params.sort_by.as_deref() {
        Some("updated_at") => profiles.sort_by_key(|p| std::cmp::Reverse(p.last_updated.unwrap_or(0))),
        Some("did") => profiles.sort_by(|a, b| a.did.cmp(&b.did)),
        _ => profiles.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)),
    }

    Ok(AxumJson(profiles))
}

// New handler function for reputation history
pub async fn get_reputation_profile_history_handler(
    Path(did_str): Path<String>, // Renamed to did_str to avoid conflict with Did type
    Extension(store): Extension<Arc<dyn ReputationStore>>,
) -> Result<AxumJson<Vec<(i64, f64)>>, AppError> {
    let did = Did(did_str); // Assuming Did is a tuple struct `Did(String)`

    // Use the existing list_records method from ReputationStore
    let records = store
        .list_records(&did)
        .await
        .map_err(AppError::internal)?; // Or just `?` if AppError implements From for anyhow::Error

    let mut sorted_records = records; // No need to clone if `records` is not used later
    // Assuming ReputationRecord has `issued_at` field that is a DateTime<Utc> or similar
    sorted_records.sort_by_key(|r| r.issued_at.timestamp());

    let mut score = 50.0; // Base score
    let mut points = Vec::new();

    if sorted_records.is_empty() {
        // If there are no records, we could return a single point representing the base score at the current time,
        // or an empty list. Returning an empty list seems more straightforward for charting.
        // Or, return the base score at the time of the DID's "creation" if known, but that's complex.
        // For now, an empty list if no records.
    } else {
        // It might be better to start with the score *before* the first record,
        // then show the score *after* each record.
        // Or, if the first point should be the score *after* the first record, this is fine.

        // The current logic calculates score *after* each event.
        for r_event in sorted_records { // Renamed `r` to `r_event` to avoid confusion if `r` is used for record itself
            let ts = r_event.issued_at.timestamp();
            match r_event.event { // Assuming ReputationRecord has an `event` field
                ReputationUpdateEvent::JobCompletedSuccessfully { .. } => score += 10.0,
                ReputationUpdateEvent::JobFailed { .. } => score -= 15.0,
                // Add other event types if they affect score, e.g.:
                // ReputationUpdateEvent::EndorsementReceived { .. } => score += 5.0,
                // ReputationUpdateEvent::Slashed { .. } => score -= 25.0,
                _ => {} // Ignore other event types or those that don't change score
            }
            score = score.clamp(0.0, 100.0); // Clamp score between 0 and 100
            points.push((ts, score));
        }
    }

    Ok(AxumJson(points))
} 