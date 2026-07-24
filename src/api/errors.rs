use axum::Json;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use uuid::Uuid;

use crate::storage::{StorageError, StorageErrorKind};
use crate::util::sensitive_url::redact_sensitive_text;

pub const ERROR_ID_HEADER: &str = "x-error-id";

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_id: Option<String>,
}

/// Application error type that converts to HTTP responses.
#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    NotFound(String),
    EventCursorGone(String),
    Forbidden(String),
    Conflict(String),
    ServiceUnavailable(String),
    Storage(StorageError),
    Internal(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::BadRequest(error) => {
                public_error(StatusCode::BAD_REQUEST, error, "bad_request")
            }
            AppError::NotFound(error) => public_error(StatusCode::NOT_FOUND, error, "not_found"),
            AppError::EventCursorGone(error) => {
                public_error(StatusCode::GONE, error, "event_cursor_gone")
            }
            AppError::Forbidden(error) => public_error(StatusCode::FORBIDDEN, error, "forbidden"),
            AppError::Conflict(error) => public_error(StatusCode::CONFLICT, error, "conflict"),
            AppError::ServiceUnavailable(error) => public_error(
                StatusCode::SERVICE_UNAVAILABLE,
                error,
                "service_unavailable",
            ),
            AppError::Storage(error) => match error.kind() {
                StorageErrorKind::InvalidInput => {
                    public_error(StatusCode::BAD_REQUEST, error.to_string(), "bad_request")
                }
                StorageErrorKind::NotFound => {
                    public_error(StatusCode::NOT_FOUND, error.to_string(), "not_found")
                }
                StorageErrorKind::Conflict => {
                    public_error(StatusCode::CONFLICT, error.to_string(), "conflict")
                }
                _ => internal_error(error.diagnostic()),
            },
            AppError::Internal(error) => internal_error(&format!("{error:#}")),
        }
    }
}

fn public_error(status: StatusCode, error: String, code: &'static str) -> Response {
    (
        status,
        Json(ErrorResponse {
            error,
            code,
            error_id: None,
        }),
    )
        .into_response()
}

fn internal_error(diagnostic: &str) -> Response {
    let error_id = Uuid::new_v4().to_string();
    let diagnostic = redact_sensitive_text(diagnostic);
    tracing::error!(
        %error_id,
        status = StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
        error = %diagnostic,
        "API request failed"
    );

    let mut response = (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: "Internal server error".to_string(),
            code: "internal_error",
            error_id: Some(error_id.clone()),
        }),
    )
        .into_response();
    response.headers_mut().insert(
        ERROR_ID_HEADER,
        HeaderValue::from_str(&error_id).expect("UUID error IDs are valid HTTP header values"),
    );
    response
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err)
    }
}

impl From<StorageError> for AppError {
    fn from(error: StorageError) -> Self {
        AppError::Storage(error)
    }
}
