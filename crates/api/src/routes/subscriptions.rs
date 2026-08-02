use crate::error::ApiError;
use crate::models::{AddSubscriptionRequest, StoredSubscription};
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
) -> Result<Json<Vec<StoredSubscription>>, ApiError> {
    Ok(Json(state.list_subscriptions().await?))
}

pub async fn add(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddSubscriptionRequest>,
) -> Result<Json<StoredSubscription>, ApiError> {
    let sub = state
        .add_subscription(req.url, req.name, req.hwid)
        .await?;
    Ok(Json(sub))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<SubscriptionDetail>, ApiError> {
    let sub = state
        .get_subscription(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Subscription {id} not found")))?;
    let nodes = state.list_nodes(Some(id)).await?;
    Ok(Json(SubscriptionDetail { subscription: sub, nodes }))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !state.delete_subscription(id).await? {
        return Err(ApiError::NotFound(format!("Subscription {id} not found")));
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}

pub async fn fetch(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<SubscriptionDetail>, ApiError> {
    let _ = state
        .get_subscription(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Subscription {id} not found")))?;
    let override_hwid = query.get("hwid").cloned();
    let _fetched = state.fetch_subscription(id, override_hwid).await?;
    let sub = state
        .get_subscription(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Subscription {id} not found")))?;
    let nodes = state.list_nodes(Some(id)).await?;
    Ok(Json(SubscriptionDetail { subscription: sub, nodes }))
}

#[derive(serde::Serialize)]
pub struct SubscriptionDetail {
    subscription: StoredSubscription,
    nodes: Vec<crate::models::NodeWithSubscription>,
}
