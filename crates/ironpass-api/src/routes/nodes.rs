use crate::error::ApiError;
use crate::models::NodeWithSubscription;
use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Query parameters for listing nodes.
#[derive(Deserialize, ToSchema)]
pub struct ListNodesQuery {
    /// Filter nodes by subscription ID.
    pub subscription: Option<Uuid>,
}

/// List all proxy nodes, optionally filtered by subscription.
#[utoipa::path(
    get,
    path = "/api/v1/nodes",
    tag = "Nodes",
    params(("subscription" = Option<Uuid>, Query, description = "Optional subscription ID filter")),
    responses(
        (status = 200, description = "List of nodes", body = [NodeWithSubscription]),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListNodesQuery>,
) -> Result<Json<Vec<NodeWithSubscription>>, ApiError> {
    Ok(Json(state.list_nodes(query.subscription).await?))
}

/// Mark a node as the selected/active node.
#[utoipa::path(
    put,
    path = "/api/v1/nodes/{id}/select",
    tag = "Nodes",
    params(("id" = Uuid, Path, description = "Node ID")),
    responses(
        (status = 200, description = "Node selected", body = NodeWithSubscription),
        (status = 404, description = "Node not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn select(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<NodeWithSubscription>, ApiError> {
    let node = state.select_node(id).await?;
    Ok(Json(node))
}
