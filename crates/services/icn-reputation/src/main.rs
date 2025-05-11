use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::{IntoResponse, Response, Json as AxumJson}, // Renamed Json to AxumJson to avoid conflict
    routing::{get, post},
    Router,
};
use icn_identity::Did;
use icn_types::reputation::{ReputationProfile, ReputationRecord};
use serde_json::json;
use std::{net::SocketAddr, sync::Arc};
use tracing_subscriber;

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing (logging)
    tracing_subscriber::fmt::init();

    let store: Arc<dyn ReputationStore> = Arc::new(InMemoryReputationStore::new());

    let app = Router::new()
        .route("/reputation/records", post(submit_record_handler))
        .route("/reputation/profiles/:did", get(get_profile_handler))
        .route("/reputation/records/:did", get(get_records_handler))
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