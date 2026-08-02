//! Thin HTTP client for the ironpassd REST API.

use ironpass_api::models::{
    AddSplitTunnelRuleRequest, AddSubscriptionRequest, ConfigResponse, HealthResponse, HwidResponse,
    NodeWithSubscription, ProxyStatus, StartProxyRequest, StoredSubscription,
};
use ironpass_core::models::{SplitTunnelAction, SplitTunnelRule, SplitTunnelTarget};
use ironpass_config::AppConfig;
use serde::de::DeserializeOwned;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    client: reqwest::Client,
}

#[allow(dead_code)]
impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    pub fn with_url(base_url: String) -> Self {
        Self::new(base_url)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn health(&self) -> reqwest::Result<HealthResponse> {
        self.get("/api/v1/health").await
    }

    pub async fn get_config(&self) -> reqwest::Result<AppConfig> {
        let resp: ConfigResponse = self.get("/api/v1/config").await?;
        Ok(resp.config)
    }

    pub async fn put_config(&self, config: &AppConfig) -> reqwest::Result<AppConfig> {
        let resp: ConfigResponse = self.put_json("/api/v1/config", config).await?;
        Ok(resp.config)
    }

    pub async fn get_hwid(&self) -> reqwest::Result<HwidResponse> {
        self.get("/api/v1/hwid").await
    }

    pub async fn regenerate_hwid(&self) -> reqwest::Result<HwidResponse> {
        self.put_empty("/api/v1/hwid/regenerate").await
    }

    pub async fn list_subscriptions(&self) -> reqwest::Result<Vec<StoredSubscription>> {
        self.get("/api/v1/subscriptions").await
    }

    pub async fn add_subscription(
        &self,
        url: String,
        name: Option<String>,
        hwid: Option<String>,
    ) -> reqwest::Result<StoredSubscription> {
        self.post_json(
            "/api/v1/subscriptions",
            &AddSubscriptionRequest { url, name, hwid },
        )
        .await
    }

    pub async fn get_subscription(
        &self,
        id: Uuid,
    ) -> reqwest::Result<SubscriptionDetail> {
        self.get(&format!("/api/v1/subscriptions/{id}")).await
    }

    pub async fn delete_subscription(&self, id: Uuid) -> reqwest::Result<serde_json::Value> {
        self.delete(&format!("/api/v1/subscriptions/{id}")).await
    }

    pub async fn fetch_subscription(&self, id: Uuid, hwid: Option<String>) -> reqwest::Result<SubscriptionDetail> {
        let url = match hwid {
            Some(h) => format!("/api/v1/subscriptions/{id}/fetch?hwid={h}"),
            None => format!("/api/v1/subscriptions/{id}/fetch"),
        };
        self.put_empty(&url).await
    }

    pub async fn list_nodes(&self, subscription_id: Option<Uuid>) -> reqwest::Result<Vec<NodeWithSubscription>> {
        let path = match subscription_id {
            Some(id) => format!("/api/v1/nodes?subscription={id}"),
            None => "/api/v1/nodes".into(),
        };
        self.get(&path).await
    }

    pub async fn select_node(&self, id: Uuid) -> reqwest::Result<NodeWithSubscription> {
        self.put_empty(&format!("/api/v1/nodes/{id}/select")).await
    }

    pub async fn proxy_status(&self) -> reqwest::Result<ProxyStatus> {
        self.get("/api/v1/proxy/status").await
    }

    pub async fn start_proxy(&self, req: &StartProxyRequest) -> reqwest::Result<ProxyStatus> {
        self.post_json("/api/v1/proxy/start", req).await
    }

    pub async fn stop_proxy(&self) -> reqwest::Result<ProxyStatus> {
        self.post_empty("/api/v1/proxy/stop").await
    }

    pub async fn list_split_tunnel_rules(
        &self,
        node_id: Option<Uuid>,
    ) -> reqwest::Result<Vec<SplitTunnelRule>> {
        let path = match node_id {
            Some(id) => format!("/api/v1/split-tunnel?node={id}"),
            None => "/api/v1/split-tunnel".into(),
        };
        self.get(&path).await
    }

    pub async fn add_split_tunnel_rule(
        &self,
        target: SplitTunnelTarget,
        value: String,
        action: SplitTunnelAction,
        node_id: Option<Uuid>,
    ) -> reqwest::Result<SplitTunnelRule> {
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

    pub async fn update_split_tunnel_rule(
        &self,
        id: Uuid,
        target: SplitTunnelTarget,
        value: String,
        action: SplitTunnelAction,
        node_id: Option<Uuid>,
    ) -> reqwest::Result<SplitTunnelRule> {
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

    pub async fn delete_split_tunnel_rule(&self, id: Uuid) -> reqwest::Result<serde_json::Value> {
        self.delete(&format!("/api/v1/split-tunnel/{id}")).await
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> reqwest::Result<T> {
        self.client
            .get(format!("{}{}", self.base_url, path))
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await
    }

    async fn post_json<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> reqwest::Result<T> {
        self.client
            .post(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await
    }

    async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> reqwest::Result<T> {
        self.client
            .post(format!("{}{}", self.base_url, path))
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await
    }

    async fn put_json<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> reqwest::Result<T> {
        self.client
            .put(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await
    }

    async fn put_empty<T: DeserializeOwned>(&self, path: &str) -> reqwest::Result<T> {
        self.client
            .put(format!("{}{}", self.base_url, path))
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await
    }

    async fn delete<T: DeserializeOwned>(&self, path: &str) -> reqwest::Result<T> {
        self.client
            .delete(format!("{}{}", self.base_url, path))
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await
    }
}

#[derive(serde::Deserialize)]
pub struct SubscriptionDetail {
    pub subscription: StoredSubscription,
    pub nodes: Vec<NodeWithSubscription>,
}
