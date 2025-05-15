use thiserror::Error;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response, Json as AxumJson},
};
use serde_json::json; // For json! macro in IntoResponse
use crate::reputation_client::ReputationClientError; // Import the new error type
use crate::job_assignment::SelectionError; // Import the new error type

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Invalid CID: {0}")]
    InvalidCid(String),

    #[error("Invalid status transition: {0}")]
    InvalidStatusTransition(String),

    #[error("Reputation service error: {0}")]
    ReputationServiceError(#[from] ReputationClientError),

    #[error("Executor selection failed: {0}")]
    SelectionFailure(#[from] SelectionError),

    #[error("P2P interaction error: {0}")]
    P2pError(String),
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
            AppError::Database(err) => {
                tracing::error!("Database error: {:#}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, AxumJson(json!({ "error": "A database error occurred" }))).into_response()
            }
            AppError::InvalidInput(msg) => {
                tracing::warn!("Invalid input: {}", msg);
                (StatusCode::BAD_REQUEST, AxumJson(json!({ "error": msg }))).into_response()
            }
            AppError::Serialization(msg) => {
                tracing::error!("Serialization error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, AxumJson(json!({ "error": "Internal server error during data serialization" }))).into_response()
            }
            AppError::InvalidCid(msg) => {
                tracing::warn!("Invalid CID: {}", msg);
                (StatusCode::BAD_REQUEST, AxumJson(json!({ "error": format!("Invalid CID: {}", msg) }))).into_response()
            }
            AppError::InvalidStatusTransition(msg) => {
                tracing::warn!("Invalid status transition: {}", msg);
                (StatusCode::BAD_REQUEST, AxumJson(json!({ "error": msg }))).into_response()
            }
            AppError::ReputationServiceError(err) => {
                tracing::error!("Reputation service error: {:#}", err);
                (StatusCode::BAD_GATEWAY, AxumJson(json!({ "error": "Error communicating with reputation service" }))).into_response()
            }
            AppError::SelectionFailure(err) => {
                tracing::error!("Executor selection error: {:#}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, AxumJson(json!({ "error": format!("Failed to select executor: {}", err) }))).into_response()
            }
            AppError::P2pError(msg) => {
                tracing::error!("P2P interaction error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, AxumJson(json!({ "error": "P2P interaction failed" }))).into_response()
            }
        }
    }
}

// The From<anyhow::Error> for AppError is already handled by #[from] on the Internal variant.
// So, an explicit impl From<anyhow::Error> is not strictly needed if that's the only source for Internal.
// However, if other code relies on a general anyhow::Error to AppError conversion that isn't specifically for the Internal variant,
// or if we want to customize it, it could be kept. The #[from] is usually sufficient.
// For now, I will rely on the #[from] attribute for `Internal(#[from] anyhow::Error)`
// and not add an explicit `impl From<anyhow::Error> for AppError` here. 