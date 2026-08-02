//! Axum route handlers for the IronPass REST API.

pub mod config;
pub mod hwid;
pub mod nodes;
pub mod proxy;
pub mod subscriptions;

use crate::error::ApiError;
use crate::models::HealthResponse;
use crate::state::AppState;
use axum::{
    extract::State,
    response::Json,
    routing::{get, post, put},
    Router,
};
use std::sync::Arc;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/config", get(config::get_config).put(config::put_config))
        .route("/api/v1/hwid", get(hwid::get_hwid))
        .route("/api/v1/hwid/regenerate", put(hwid::regenerate_hwid))
        .route(
            "/api/v1/subscriptions",
            get(subscriptions::list).post(subscriptions::add),
        )
        .route(
            "/api/v1/subscriptions/:id",
            get(subscriptions::get).delete(subscriptions::delete),
        )
        .route(
            "/api/v1/subscriptions/:id/fetch",
            put(subscriptions::fetch),
        )
        .route("/api/v1/nodes", get(nodes::list))
        .route("/api/v1/nodes/:id/select", put(nodes::select))
        .route("/api/v1/proxy/status", get(proxy::status))
        .route("/api/v1/proxy/start", post(proxy::start))
        .route("/api/v1/proxy/stop", post(proxy::stop))
        .with_state(state)
}

async fn health(State(state): State<Arc<AppState>>) -> Result<Json<HealthResponse>, ApiError> {
    let hwid = state.hwid_provider.generate()?;
    Ok(Json(HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        uptime_secs: state.start_time.elapsed().as_secs(),
        hwid,
    }))
}
