use crate::error::ApiError;
use crate::models::NodeWithSubscription;
use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ListNodesQuery {
    subscription: Option<Uuid>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListNodesQuery>,
) -> Result<Json<Vec<NodeWithSubscription>>, ApiError> {
    Ok(Json(state.list_nodes(query.subscription).await?))
}

pub async fn select(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<NodeWithSubscription>, ApiError> {
    let node = state.select_node(id).await?;
    Ok(Json(node))
}
