//! Typed errors for the IronPass API client.

use reqwest::StatusCode;

/// Errors returned by [`ApiClient`](crate::client::ApiClient).
#[derive(Debug, thiserror::Error)]
pub enum ApiClientError {
    /// The requested resource was not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// The request was invalid or malformed.
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// A conflict occurred, such as adding a duplicate subscription.
    #[error("Conflict: {0}")]
    Conflict(String),

    /// An upstream network or HTTP error.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization or deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// An unexpected error response was received from the API.
    #[error("API error {status}: {message}")]
    Api {
        /// HTTP status code returned by the server.
        status: StatusCode,
        /// Error message from the server.
        message: String,
    },
}
