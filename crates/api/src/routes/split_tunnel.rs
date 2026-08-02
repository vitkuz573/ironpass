use crate::error::ApiError;
use crate::models::{AddSplitTunnelRuleRequest, SplitTunnelRule, UpdateSplitTunnelRuleRequest};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Vec<SplitTunnelRule>>, ApiError> {
    let node_id = query
        .get("node")
        .and_then(|s| Uuid::parse_str(s).ok());
    Ok(Json(state.list_split_tunnel_rules(node_id).await?))
}

pub async fn add(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddSplitTunnelRuleRequest>,
) -> Result<Json<SplitTunnelRule>, ApiError> {
    let rule = state
        .add_split_tunnel_rule(req.target, req.value, req.action, req.node_id)
        .await?;
    Ok(Json(rule))
}

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

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !state.delete_split_tunnel_rule(id).await? {
        return Err(ApiError::NotFound(format!("Split tunnel rule {id} not found")));
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}
