use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("dependency error: {0}")]
    Dependency(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("internal error")]
    Internal,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
    details: Vec<String>,
    trace_id: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, msg) = match self {
            AppError::Validation(m) => (StatusCode::BAD_REQUEST, "validation_error", m),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m),
            AppError::Conflict(m) => (StatusCode::CONFLICT, "conflict", m),
            AppError::Dependency(m) => (StatusCode::BAD_GATEWAY, "dependency_error", m),
            AppError::Config(m) => (StatusCode::INTERNAL_SERVER_ERROR, "config_error", m),
            AppError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "internal error".to_string(),
            ),
        };

        let body = ErrorBody {
            code,
            message: msg,
            details: Vec::new(),
            trace_id: format!("trace_{}", uuid::Uuid::new_v4().simple()),
        };

        (status, Json(body)).into_response()
    }
}
