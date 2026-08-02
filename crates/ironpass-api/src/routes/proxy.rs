use crate::error::ApiError;
use crate::models::{ProxyStatus, StartProxyRequest};
use crate::state::AppState;
use axum::{Json, extract::State};
use std::sync::Arc;

pub async fn status(State(state): State<Arc<AppState>>) -> Result<Json<ProxyStatus>, ApiError> {
    Ok(Json(state.proxy_status().await?))
}

pub async fn start(
    State(state): State<Arc<AppState>>,
    Json(req): Json<StartProxyRequest>,
) -> Result<Json<ProxyStatus>, ApiError> {
    Ok(Json(state.start_proxy(req).await?))
}

pub async fn stop(State(state): State<Arc<AppState>>) -> Result<Json<ProxyStatus>, ApiError> {
    Ok(Json(state.stop_proxy().await?))
}
