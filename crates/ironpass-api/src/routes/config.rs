use crate::error::ApiError;
use crate::models::ConfigResponse;
use crate::state::AppState;
use axum::{Json, extract::State};
use ironpass_config::AppConfig;
use std::sync::Arc;

/// Get the current application configuration.
#[utoipa::path(
    get,
    path = "/api/v1/config",
    tag = "System",
    responses(
        (status = 200, description = "Current config", body = ConfigResponse),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn get_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ConfigResponse>, ApiError> {
    let config = state.load_config()?;
    Ok(Json(ConfigResponse { config }))
}

/// Update the application configuration.
#[utoipa::path(
    put,
    path = "/api/v1/config",
    tag = "System",
    request_body = AppConfig,
    responses(
        (status = 200, description = "Updated config", body = ConfigResponse),
        (status = 400, description = "Invalid configuration"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn put_config(
    State(state): State<Arc<AppState>>,
    Json(config): Json<AppConfig>,
) -> Result<Json<ConfigResponse>, ApiError> {
    state.save_config(&config)?;
    Ok(Json(ConfigResponse { config }))
}
