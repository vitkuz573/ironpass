use crate::error::ApiError;
use crate::models::{ProxyStatus, StartProxyRequest};
use crate::state::AppState;
use axum::{Json, extract::State};
use std::sync::Arc;

/// Get the current proxy process status.
#[utoipa::path(
    get,
    path = "/api/v1/proxy/status",
    tag = "Proxy",
    responses(
        (status = 200, description = "Proxy status", body = ProxyStatus),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn status(State(state): State<Arc<AppState>>) -> Result<Json<ProxyStatus>, ApiError> {
    Ok(Json(state.proxy_status().await?))
}

/// Start the proxy for a node.
#[utoipa::path(
    post,
    path = "/api/v1/proxy/start",
    tag = "Proxy",
    request_body = StartProxyRequest,
    responses(
        (status = 200, description = "Proxy started", body = ProxyStatus),
        (status = 400, description = "No node selected or unsupported node/backend"),
        (status = 404, description = "Node not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn start(
    State(state): State<Arc<AppState>>,
    Json(req): Json<StartProxyRequest>,
) -> Result<Json<ProxyStatus>, ApiError> {
    Ok(Json(state.start_proxy(req).await?))
}

/// Stop the proxy process.
#[utoipa::path(
    post,
    path = "/api/v1/proxy/stop",
    tag = "Proxy",
    responses(
        (status = 200, description = "Proxy stopped", body = ProxyStatus),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn stop(State(state): State<Arc<AppState>>) -> Result<Json<ProxyStatus>, ApiError> {
    Ok(Json(state.stop_proxy().await?))
}
