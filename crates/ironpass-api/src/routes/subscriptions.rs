use crate::error::ApiError;
use crate::models::{AddSubscriptionRequest, NodeWithSubscription, StoredSubscription};
use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// List all stored subscriptions.
#[utoipa::path(
    get,
    path = "/api/v1/subscriptions",
    operation_id = "list_subscriptions",
    tag = "Subscriptions",
    responses(
        (status = 200, description = "List of subscriptions", body = [StoredSubscription]),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<StoredSubscription>>, ApiError> {
    Ok(Json(state.list_subscriptions().await?))
}

/// Add a new subscription.
#[utoipa::path(
    post,
    path = "/api/v1/subscriptions",
    operation_id = "create_subscription",
    tag = "Subscriptions",
    request_body = AddSubscriptionRequest,
    responses(
        (status = 200, description = "Subscription created", body = StoredSubscription),
        (status = 400, description = "Invalid request or duplicate subscription"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn add(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddSubscriptionRequest>,
) -> Result<Json<StoredSubscription>, ApiError> {
    let sub = state.add_subscription(req.url, req.name, req.hwid).await?;
    Ok(Json(sub))
}

/// Get a single subscription with its nodes.
#[utoipa::path(
    get,
    path = "/api/v1/subscriptions/{id}",
    operation_id = "get_subscription",
    tag = "Subscriptions",
    params(("id" = Uuid, Path, description = "Subscription ID")),
    responses(
        (status = 200, description = "Subscription details", body = SubscriptionDetail),
        (status = 404, description = "Subscription not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<SubscriptionDetail>, ApiError> {
    let sub = state
        .get_subscription(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Subscription {id} not found")))?;
    let nodes = state.list_nodes(Some(id)).await?;
    Ok(Json(SubscriptionDetail {
        subscription: sub,
        nodes,
    }))
}

/// Delete a subscription.
#[utoipa::path(
    delete,
    path = "/api/v1/subscriptions/{id}",
    operation_id = "delete_subscription",
    tag = "Subscriptions",
    params(("id" = Uuid, Path, description = "Subscription ID")),
    responses(
        (status = 200, description = "Subscription deleted", body = serde_json::Value),
        (status = 404, description = "Subscription not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !state.delete_subscription(id).await? {
        return Err(ApiError::NotFound(format!("Subscription {id} not found")));
    }
    Ok(Json(json!({ "deleted": true })))
}

/// Fetch/reload a subscription from its source URL.
#[utoipa::path(
    put,
    path = "/api/v1/subscriptions/{id}/fetch",
    operation_id = "fetch_subscription",
    tag = "Subscriptions",
    params(
        ("id" = Uuid, Path, description = "Subscription ID"),
        ("hwid" = Option<String>, Query, description = "Optional HWID override"),
    ),
    responses(
        (status = 200, description = "Subscription refreshed", body = SubscriptionDetail),
        (status = 404, description = "Subscription not found"),
        (status = 500, description = "Internal server error"),
    )
)]
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
    Ok(Json(SubscriptionDetail {
        subscription: sub,
        nodes,
    }))
}

/// Detailed subscription response including its nodes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct SubscriptionDetail {
    pub subscription: StoredSubscription,
    pub nodes: Vec<NodeWithSubscription>,
}
