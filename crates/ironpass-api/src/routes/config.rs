use crate::error::ApiError;
use crate::models::ConfigResponse;
use crate::state::AppState;
use axum::{Json, extract::State};
use ironpass_config::AppConfig;
use std::sync::Arc;

pub async fn get_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ConfigResponse>, ApiError> {
    let config = state.load_config()?;
    Ok(Json(ConfigResponse { config }))
}

pub async fn put_config(
    State(state): State<Arc<AppState>>,
    Json(config): Json<AppConfig>,
) -> Result<Json<ConfigResponse>, ApiError> {
    state.save_config(&config)?;
    Ok(Json(ConfigResponse { config }))
}
