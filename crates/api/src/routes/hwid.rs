use crate::error::ApiError;
use crate::models::HwidResponse;
use crate::state::AppState;
use axum::{extract::State, Json};
use std::sync::Arc;

pub async fn get_hwid(State(state): State<Arc<AppState>>) -> Result<Json<HwidResponse>, ApiError> {
    let hwid = state.hwid_provider.generate()?;
    let info = state.hwid_provider.get_device_info()?;
    Ok(Json(HwidResponse { hwid, info }))
}

pub async fn regenerate_hwid(
    State(state): State<Arc<AppState>>,
) -> Result<Json<HwidResponse>, ApiError> {
    let hwid = state.hwid_provider.generate()?;
    let info = state.hwid_provider.get_device_info()?;
    Ok(Json(HwidResponse { hwid, info }))
}
