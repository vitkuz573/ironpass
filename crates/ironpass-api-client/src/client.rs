//! Thin HTTP client for the ironpassd REST API.

use crate::error::ApiClientError;
use crate::models::{
    AddSplitTunnelRuleRequest, AddSubscriptionRequest, BackendCapabilities, ConfigResponse,
    HealthResponse, HwidResponse, NodeWithSubscription, ProxyStatus, StartProxyRequest,
    StoredSubscription, SubscriptionDetail,
};
use ironpass_config::AppConfig;
use ironpass_core::models::{SplitTunnelAction, SplitTunnelRule, SplitTunnelTarget};
use reqwest::{Response, StatusCode};
use serde::de::DeserializeOwned;
use std::time::Duration;
use uuid::Uuid;

/// HTTP client for the IronPass API.
#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    client: reqwest::Client,
}

impl ApiClient {
    /// Create a new client pointing at `base_url`.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Create a new client pointing at `base_url`.
    pub fn with_url(base_url: String) -> Self {
        Self::new(base_url)
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Check daemon health.
    pub async fn health(&self) -> Result<HealthResponse, ApiClientError> {
        self.get("/api/v1/health").await
    }

    /// Get the current application config.
    pub async fn get_config(&self) -> Result<AppConfig, ApiClientError> {
        let resp: ConfigResponse = self.get("/api/v1/config").await?;
        Ok(resp.config)
    }

    /// Update the application config.
    pub async fn put_config(&self, config: &AppConfig) -> Result<AppConfig, ApiClientError> {
        let resp: ConfigResponse = self.put_json("/api/v1/config", config).await?;
        Ok(resp.config)
    }

    /// Get the current HWID.
    pub async fn get_hwid(&self) -> Result<HwidResponse, ApiClientError> {
        self.get("/api/v1/hwid").await
    }

    /// Regenerate the HWID.
    pub async fn regenerate_hwid(&self) -> Result<HwidResponse, ApiClientError> {
        self.put_empty("/api/v1/hwid/regenerate").await
    }

    /// List stored subscriptions.
    pub async fn list_subscriptions(&self) -> Result<Vec<StoredSubscription>, ApiClientError> {
        self.get("/api/v1/subscriptions").await
    }

    /// Add a subscription.
    pub async fn add_subscription(
        &self,
        url: String,
        name: Option<String>,
        hwid: Option<String>,
    ) -> Result<StoredSubscription, ApiClientError> {
        self.post_json(
            "/api/v1/subscriptions",
            &AddSubscriptionRequest { url, name, hwid },
        )
        .await
    }

    /// Get a subscription and its nodes.
    pub async fn get_subscription(&self, id: Uuid) -> Result<SubscriptionDetail, ApiClientError> {
        self.get(&format!("/api/v1/subscriptions/{id}")).await
    }

    /// Delete a subscription.
    pub async fn delete_subscription(&self, id: Uuid) -> Result<serde_json::Value, ApiClientError> {
        self.delete(&format!("/api/v1/subscriptions/{id}")).await
    }

    /// Re-fetch a subscription.
    pub async fn fetch_subscription(
        &self,
        id: Uuid,
        hwid: Option<String>,
    ) -> Result<SubscriptionDetail, ApiClientError> {
        let url = match hwid {
            Some(h) => format!("/api/v1/subscriptions/{id}/fetch?hwid={h}"),
            None => format!("/api/v1/subscriptions/{id}/fetch"),
        };
        self.put_empty(&url).await
    }

    /// List nodes, optionally filtered by subscription.
    pub async fn list_nodes(
        &self,
        subscription_id: Option<Uuid>,
    ) -> Result<Vec<NodeWithSubscription>, ApiClientError> {
        let path = match subscription_id {
            Some(id) => format!("/api/v1/nodes?subscription={id}"),
            None => "/api/v1/nodes".into(),
        };
        self.get(&path).await
    }

    /// Select a node for proxying.
    pub async fn select_node(&self, id: Uuid) -> Result<NodeWithSubscription, ApiClientError> {
        self.put_empty(&format!("/api/v1/nodes/{id}/select")).await
    }

    /// Get proxy status.
    pub async fn proxy_status(&self) -> Result<ProxyStatus, ApiClientError> {
        self.get("/api/v1/proxy/status").await
    }

    /// Start the proxy.
    pub async fn start_proxy(
        &self,
        req: &StartProxyRequest,
    ) -> Result<ProxyStatus, ApiClientError> {
        self.post_json("/api/v1/proxy/start", req).await
    }

    /// Stop the proxy.
    pub async fn stop_proxy(&self) -> Result<ProxyStatus, ApiClientError> {
        self.post_empty("/api/v1/proxy/stop").await
    }

    /// List split tunnel rules, optionally filtered by node.
    pub async fn list_split_tunnel_rules(
        &self,
        node_id: Option<Uuid>,
    ) -> Result<Vec<SplitTunnelRule>, ApiClientError> {
        let path = match node_id {
            Some(id) => format!("/api/v1/split-tunnel?node={id}"),
            None => "/api/v1/split-tunnel".into(),
        };
        self.get(&path).await
    }

    /// Add a split tunnel rule.
    pub async fn add_split_tunnel_rule(
        &self,
        target: SplitTunnelTarget,
        value: String,
        action: SplitTunnelAction,
        node_id: Option<Uuid>,
    ) -> Result<SplitTunnelRule, ApiClientError> {
        self.post_json(
            "/api/v1/split-tunnel",
            &AddSplitTunnelRuleRequest {
                target,
                value,
                action,
                node_id,
            },
        )
        .await
    }

    /// Update a split tunnel rule.
    pub async fn update_split_tunnel_rule(
        &self,
        id: Uuid,
        target: SplitTunnelTarget,
        value: String,
        action: SplitTunnelAction,
        node_id: Option<Uuid>,
    ) -> Result<SplitTunnelRule, ApiClientError> {
        self.put_json(
            &format!("/api/v1/split-tunnel/{id}"),
            &AddSplitTunnelRuleRequest {
                target,
                value,
                action,
                node_id,
            },
        )
        .await
    }

    /// Delete a split tunnel rule.
    pub async fn delete_split_tunnel_rule(
        &self,
        id: Uuid,
    ) -> Result<serde_json::Value, ApiClientError> {
        self.delete(&format!("/api/v1/split-tunnel/{id}")).await
    }

    /// Get backend capabilities.
    pub async fn backend_capabilities(&self) -> Result<BackendCapabilities, ApiClientError> {
        self.get("/api/v1/backend/capabilities").await
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiClientError> {
        self.parse_json(
            self.client
                .get(format!("{}{}", self.base_url, path))
                .send()
                .await?,
        )
        .await
    }

    async fn post_json<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiClientError> {
        self.parse_json(
            self.client
                .post(format!("{}{}", self.base_url, path))
                .json(body)
                .send()
                .await?,
        )
        .await
    }

    async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiClientError> {
        self.parse_json(
            self.client
                .post(format!("{}{}", self.base_url, path))
                .send()
                .await?,
        )
        .await
    }

    async fn put_json<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiClientError> {
        self.parse_json(
            self.client
                .put(format!("{}{}", self.base_url, path))
                .json(body)
                .send()
                .await?,
        )
        .await
    }

    async fn put_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiClientError> {
        self.parse_json(
            self.client
                .put(format!("{}{}", self.base_url, path))
                .send()
                .await?,
        )
        .await
    }

    async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiClientError> {
        self.parse_json(
            self.client
                .delete(format!("{}{}", self.base_url, path))
                .send()
                .await?,
        )
        .await
    }

    async fn parse_json<T: DeserializeOwned>(
        &self,
        response: Response,
    ) -> Result<T, ApiClientError> {
        let status = response.status();
        if status.is_success() {
            Ok(response.json::<T>().await?)
        } else {
            let message = response
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
                .unwrap_or_else(|| "Unknown API error".into());
            Err(map_error(status, message))
        }
    }
}

fn map_error(status: StatusCode, message: String) -> ApiClientError {
    match status {
        StatusCode::NOT_FOUND => ApiClientError::NotFound(message),
        StatusCode::BAD_REQUEST => ApiClientError::BadRequest(message),
        StatusCode::CONFLICT => ApiClientError::Conflict(message),
        _ => ApiClientError::Api { status, message },
    }
}
