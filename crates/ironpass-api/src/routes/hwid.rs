use crate::error::ApiError;
use crate::models::HwidResponse;
use crate::state::AppState;
use axum::{Json, extract::State};
use std::sync::Arc;

/// Get the current device HWID and device information.
#[utoipa::path(
    get,
    path = "/api/v1/hwid",
    tag = "Auth",
    responses(
        (status = 200, description = "HWID and device info", body = HwidResponse),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn get_hwid(State(state): State<Arc<AppState>>) -> Result<Json<HwidResponse>, ApiError> {
    let hwid = state.hwid_provider.generate()?;
    let info = state.hwid_provider.get_device_info()?;
    Ok(Json(HwidResponse { hwid, info }))
}

/// Regenerate the device HWID.
#[utoipa::path(
    put,
    path = "/api/v1/hwid/regenerate",
    tag = "Auth",
    responses(
        (status = 200, description = "Regenerated HWID", body = HwidResponse),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn regenerate_hwid(
    State(state): State<Arc<AppState>>,
) -> Result<Json<HwidResponse>, ApiError> {
    let hwid = state.hwid_provider.generate()?;
    let info = state.hwid_provider.get_device_info()?;
    Ok(Json(HwidResponse { hwid, info }))
}
