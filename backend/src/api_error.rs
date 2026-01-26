use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Authentication failed: {0}")]
    Authentication(String),

    #[error("Authorization failed: {0}")]
    Authorization(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal server error")]
    InternalServerError,

    #[error("Database error: {0}")]
    Database(#[from] deadpool_postgres::tokio_postgres::Error),

    #[error("Pool error: {0}")]
    Pool(#[from] deadpool_postgres::PoolError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("Stellar error: {0}")]
    Stellar(String),

    #[error("Compliance violation: {0}")]
    Compliance(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
    code: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::Authentication(_) => (StatusCode::UNAUTHORIZED, "AUTHENTICATION_FAILED"),
            ApiError::Authorization(_) => (StatusCode::FORBIDDEN, "AUTHORIZATION_FAILED"),
            ApiError::Validation(_) => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR"),
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "CONFLICT"),
            ApiError::InternalServerError => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
            ApiError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR"),
            ApiError::Pool(_) => (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR"),
            ApiError::Json(_) => (StatusCode::BAD_REQUEST, "INVALID_JSON"),
            ApiError::Jwt(_) => (StatusCode::UNAUTHORIZED, "INVALID_TOKEN"),
            ApiError::Stellar(_) => (StatusCode::BAD_REQUEST, "STELLAR_ERROR"),
            ApiError::Compliance(_) => (StatusCode::FORBIDDEN, "COMPLIANCE_VIOLATION"),
            ApiError::RateLimit(_) => (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMIT_EXCEEDED"),
        };

        let error_response = ErrorResponse {
            error: code.to_string(),
            message: self.to_string(),
            code: code.to_string(),
        };

        (status, Json(json!(error_response))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(_err: anyhow::Error) -> Self {
        ApiError::InternalServerError
    }
}
