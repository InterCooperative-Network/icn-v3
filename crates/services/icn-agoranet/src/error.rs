use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use utoipa::ToSchema;
use icn_identity::DidError;
use cid::Error as CidLibError;

#[derive(Debug, ToSchema)]
pub enum ApiError {
    NotFound(String),
    InternalServerError(String),
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::InternalServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
        };

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}

// Helper for mapping any error to ApiError::InternalServerError
impl<E: std::error::Error> From<E> for ApiError {
    fn from(err: E) -> Self {
        ApiError::InternalServerError(err.to_string())
    }
}

// Added From impl for DidError
impl From<DidError> for ApiError {
    fn from(err: DidError) -> Self {
        tracing::warn!("DID Error: {}", err.to_string());
        ApiError::BadRequest(format!("Invalid DID format: {}", err))
    }
}

// Added From impl for cid::Error
impl From<CidLibError> for ApiError {
    fn from(err: CidLibError) -> Self {
        tracing::warn!("CID parsing error: {}", err.to_string());
        ApiError::BadRequest(format!("Invalid CID format provided: {}", err))
    }
}
