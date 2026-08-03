use crate::error::ApiError;
use crate::models::{AddSplitTunnelRuleRequest, UpdateSplitTunnelRuleRequest};
use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use ironpass_core::models::SplitTunnelRule;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// List split tunnel rules, optionally filtered by node.
#[utoipa::path(
    get,
    path = "/api/v1/split-tunnel",
    tag = "Split Tunnel",
    params(("node" = Option<Uuid>, Query, description = "Optional node ID filter")),
    responses(
        (status = 200, description = "List of rules", body = [SplitTunnelRule]),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Vec<SplitTunnelRule>>, ApiError> {
    let node_id = query.get("node").and_then(|s| Uuid::parse_str(s).ok());
    Ok(Json(state.list_split_tunnel_rules(node_id).await?))
}

/// Add a new split tunnel rule.
#[utoipa::path(
    post,
    path = "/api/v1/split-tunnel",
    tag = "Split Tunnel",
    request_body = AddSplitTunnelRuleRequest,
    responses(
        (status = 200, description = "Rule created", body = SplitTunnelRule),
        (status = 400, description = "Invalid rule"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn add(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddSplitTunnelRuleRequest>,
) -> Result<Json<SplitTunnelRule>, ApiError> {
    let rule = state
        .add_split_tunnel_rule(req.target, req.value, req.action, req.node_id)
        .await?;
    Ok(Json(rule))
}

/// Get a single split tunnel rule.
#[utoipa::path(
    get,
    path = "/api/v1/split-tunnel/{id}",
    tag = "Split Tunnel",
    params(("id" = Uuid, Path, description = "Rule ID")),
    responses(
        (status = 200, description = "Rule details", body = SplitTunnelRule),
        (status = 404, description = "Rule not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<SplitTunnelRule>, ApiError> {
    let rule = state
        .get_split_tunnel_rule(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Split tunnel rule {id} not found")))?;
    Ok(Json(rule))
}

/// Update a split tunnel rule.
#[utoipa::path(
    put,
    path = "/api/v1/split-tunnel/{id}",
    tag = "Split Tunnel",
    params(("id" = Uuid, Path, description = "Rule ID")),
    request_body = UpdateSplitTunnelRuleRequest,
    responses(
        (status = 200, description = "Rule updated", body = SplitTunnelRule),
        (status = 400, description = "Invalid rule"),
        (status = 404, description = "Rule not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateSplitTunnelRuleRequest>,
) -> Result<Json<SplitTunnelRule>, ApiError> {
    let rule = state
        .update_split_tunnel_rule(id, req.target, req.value, req.action, req.node_id)
        .await?;
    Ok(Json(rule))
}

/// Delete a split tunnel rule.
#[utoipa::path(
    delete,
    path = "/api/v1/split-tunnel/{id}",
    tag = "Split Tunnel",
    params(("id" = Uuid, Path, description = "Rule ID")),
    responses(
        (status = 200, description = "Rule deleted", body = serde_json::Value),
        (status = 404, description = "Rule not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !state.delete_split_tunnel_rule(id).await? {
        return Err(ApiError::NotFound(format!(
            "Split tunnel rule {id} not found"
        )));
    }
    Ok(Json(json!({ "deleted": true })))
}
