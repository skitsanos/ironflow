use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Application error type that converts to HTTP responses.
pub enum AppError {
    BadRequest(String),
    NotFound(String),
    Internal(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error, details) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg, None),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg, None),
            AppError::Internal(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
                Some(format!("{:#}", err)),
            ),
        };

        (status, Json(ErrorResponse { error, details })).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err)
    }
}
