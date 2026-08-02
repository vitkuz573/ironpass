//! API-specific domain models and request/response DTOs.

use chrono::{DateTime, Utc};
use ironpass_backend::backend::BackendType;
use ironpass_core::models::{
    HwidInfo, ProxyNode, SplitTunnelAction, SplitTunnelTarget, SubscriptionMetadata,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request body for adding a split tunnel rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddSplitTunnelRuleRequest {
    pub target: SplitTunnelTarget,
    pub value: String,
    pub action: SplitTunnelAction,
    #[serde(default)]
    pub node_id: Option<Uuid>,
}

/// Request body for updating a split tunnel rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSplitTunnelRuleRequest {
    pub target: SplitTunnelTarget,
    pub value: String,
    pub action: SplitTunnelAction,
    #[serde(default)]
    pub node_id: Option<Uuid>,
}

/// Request body for adding a subscription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddSubscriptionRequest {
    pub url: String,
    pub name: Option<String>,
    pub hwid: Option<String>,
}

/// Stored subscription record with cached nodes and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSubscription {
    pub id: Uuid,
    pub url: String,
    pub name: Option<String>,
    pub hwid: Option<String>,
    pub added_at: DateTime<Utc>,
    pub last_updated: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub metadata: SubscriptionMetadata,
    #[serde(default)]
    pub traffic_used: Option<u64>,
    #[serde(default)]
    pub traffic_total: Option<u64>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

/// A node with its owning subscription context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeWithSubscription {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub subscription_name: Option<String>,
    pub node: ProxyNode,
}

/// Request body for starting the proxy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartProxyRequest {
    pub node_id: Option<Uuid>,
    pub socks_port: Option<u16>,
    pub http_port: Option<u16>,
    pub mixed_port: Option<u16>,
    #[serde(default)]
    pub backend: Option<BackendType>,
}

/// Response body for proxy status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub selected_node: Option<NodeWithSubscription>,
    pub socks_port: Option<u16>,
    pub http_port: Option<u16>,
    pub mixed_port: Option<u16>,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendType>,
}

/// Aggregate health/status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
    pub hwid: String,
}

/// Config response wrapping the existing application config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigResponse {
    pub config: ironpass_config::AppConfig,
}

/// HWID response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwidResponse {
    pub hwid: String,
    pub info: HwidInfo,
}

impl StoredSubscription {
    pub fn new(url: String, name: Option<String>, hwid: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            url,
            name,
            hwid,
            added_at: Utc::now(),
            last_updated: None,
            is_active: true,
            metadata: SubscriptionMetadata::default(),
            traffic_used: None,
            traffic_total: None,
            expires_at: None,
        }
    }
}
