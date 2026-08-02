//! API error type and Axum response conversion.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use ironpass_core::Error as CoreError;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    Conflict(String),

    #[error(transparent)]
    Core(#[from] CoreError),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            ApiError::Anyhow(e) => {
                let msg = e.to_string();
                tracing::error!("Internal error: {:#}", e);
                if msg.contains("No node selected") {
                    (StatusCode::BAD_REQUEST, msg)
                } else {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Internal server error".into(),
                    )
                }
            }
            ApiError::Core(e) => {
                tracing::error!("Core error: {}", e);
                match e {
                    CoreError::Config(msg)
                    | CoreError::Parse(msg)
                    | CoreError::Hwid(msg)
                    | CoreError::UnsupportedProtocol(msg)
                    | CoreError::Custom(msg) => (StatusCode::BAD_REQUEST, msg),
                    CoreError::Network(_) => (
                        StatusCode::BAD_GATEWAY,
                        "Upstream network error".into(),
                    ),
                    CoreError::Url(_) => (StatusCode::BAD_REQUEST, "Invalid URL".into()),
                    CoreError::Serialization(_)
                    | CoreError::Yaml(_)
                    | CoreError::Base64(_)
                    | CoreError::Io(_)
                    | CoreError::Database(_) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Internal server error".into(),
                    ),
                    CoreError::SubscriptionExpired => {
                        (StatusCode::FORBIDDEN, "Subscription expired".into())
                    }
                    CoreError::DeviceLimitExceeded { current, limit } => (
                        StatusCode::FORBIDDEN,
                        format!("Device limit exceeded: {current}/{limit}"),
                    ),
                }
            }
        };

        let body = Json(json!({ "error": message }));
        (status, body).into_response()
    }
}
