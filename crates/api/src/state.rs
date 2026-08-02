//! Global application state shared across API handlers.

use crate::db::{import_legacy_subscriptions, DbPool};
use crate::models::{NodeWithSubscription, ProxyStatus, StartProxyRequest, StoredSubscription};
use crate::singbox::{generate_config, requires_singbox, InboundPorts};
use crate::singbox_process::SingBoxProcessManager;
use ironpass_config::{AppConfig, ConfigManager};
use ironpass_core::models::Subscription;
use ironpass_core::traits::HwidProvider;
use ironpass_subscription::{FetchOptions, HttpSubscriptionFetcher, SubscriptionService};
use reqwest::Client;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct AppState {
    pub config_manager: ConfigManager,
    pub db: DbPool,
    pub hwid_provider: Arc<dyn HwidProvider + Send + Sync>,
    pub http_client: Client,
    pub process_manager: RwLock<SingBoxProcessManager>,
    pub selected_node: RwLock<Option<Uuid>>,
    pub proxy_ports: RwLock<Option<ProxyPorts>>,
    pub start_time: Instant,
}

#[derive(Debug, Clone, Copy)]
pub struct ProxyPorts {
    pub socks: Option<u16>,
    pub http: Option<u16>,
    pub mixed: Option<u16>,
}

impl AppState {
    pub fn new(
        config_manager: ConfigManager,
        db: DbPool,
        hwid_provider: Arc<dyn HwidProvider + Send + Sync>,
    ) -> Self {
        Self {
            http_client: Client::builder()
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::limited(10))
                .build()
                .expect("Failed to create HTTP client"),
            config_manager,
            db,
            hwid_provider,
            process_manager: RwLock::new(SingBoxProcessManager::new()),
            selected_node: RwLock::new(None),
            proxy_ports: RwLock::new(None),
            start_time: Instant::now(),
        }
    }

    /// Load or migrate legacy JSON state.
    pub fn migrate_legacy(&self) -> anyhow::Result<usize> {
        let legacy_path = self.config_manager.subscriptions_path();
        if !legacy_path.exists() {
            return Ok(0);
        }
        let content = std::fs::read_to_string(&legacy_path)?;
        let legacy: ironpass_config::SubscriptionsStore = serde_json::from_str(&content)?;
        let count = import_legacy_subscriptions(&self.db, &legacy)?;
        if count > 0 {
            let backup = legacy_path.with_extension("json.bak");
            std::fs::rename(&legacy_path, &backup)?;
        }
        Ok(count)
    }

    pub fn load_config(&self) -> anyhow::Result<AppConfig> {
        Ok(self.config_manager.load_config()?)
    }

    pub fn save_config(&self, config: &AppConfig) -> anyhow::Result<()> {
        Ok(self.config_manager.save_config(config)?)
    }

    pub async fn add_subscription(
        &self,
        url: String,
        name: Option<String>,
        hwid: Option<String>,
    ) -> anyhow::Result<StoredSubscription> {
        if self
            .db
            .list_subscriptions()?
            .into_iter()
            .any(|s| s.url == url)
        {
            anyhow::bail!("Subscription already exists");
        }
        let sub = StoredSubscription::new(url, name, hwid);
        self.db.insert_subscription(&sub)?;
        Ok(sub)
    }

    pub async fn get_subscription(&self, id: Uuid) -> anyhow::Result<Option<StoredSubscription>> {
        Ok(self.db.get_subscription(id)?)
    }

    pub async fn list_subscriptions(&self) -> anyhow::Result<Vec<StoredSubscription>> {
        Ok(self.db.list_subscriptions()?)
    }

    pub async fn delete_subscription(&self, id: Uuid) -> anyhow::Result<bool> {
        // Clear selected node if it belongs to this subscription.
        let mut selected = self.selected_node.write().await;
        if let Some(node_id) = *selected
            && let Ok(Some((_, sub_id, _))) = self.db.get_node(node_id)
            && sub_id == id
        {
            *selected = None;
        }
        drop(selected);
        Ok(self.db.delete_subscription(id)?)
    }

    pub async fn fetch_subscription(
        &self,
        id: Uuid,
        override_hwid: Option<String>,
    ) -> anyhow::Result<Subscription> {
        let sub = self
            .db
            .get_subscription(id)?
            .ok_or_else(|| anyhow::anyhow!("Subscription not found"))?;

        let hwid = override_hwid
            .or(sub.hwid.clone())
            .or_else(|| self.hwid_provider.generate().ok());

        let fetcher = HttpSubscriptionFetcher::with_client(
            self.http_client.clone(),
            FetchOptions::default(),
        );
        let service = SubscriptionService::with_fetcher(fetcher);

        let mut fetched = service.fetch_and_parse(&sub.url, hwid.as_deref()).await?;
        fetched.url = sub.url.clone();

        // Update stored metadata.
        let mut stored = sub;
        stored.last_updated = Some(chrono::Utc::now());
        stored.metadata = fetched.metadata.clone();
        stored.traffic_used = fetched.traffic_used;
        stored.traffic_total = fetched.traffic_total;
        stored.expires_at = fetched.expires_at;
        self.db.update_subscription(&stored)?;

        // Persist nodes.
        self.db.replace_nodes(id, &fetched.nodes)?;

        Ok(fetched)
    }

    pub async fn list_nodes(
        &self,
        subscription_id: Option<Uuid>,
    ) -> anyhow::Result<Vec<NodeWithSubscription>> {
        let subs = self.db.list_subscriptions()?;
        let sub_names: std::collections::HashMap<Uuid, Option<String>> = subs
            .into_iter()
            .map(|s| (s.id, s.name))
            .collect();

        let rows = self.db.list_nodes(subscription_id)?;
        let mut nodes = Vec::with_capacity(rows.len());
        for (id, sub_id, node) in rows {
            nodes.push(NodeWithSubscription {
                id,
                subscription_id: sub_id,
                subscription_name: sub_names.get(&sub_id).cloned().unwrap_or(None),
                node,
            });
        }
        Ok(nodes)
    }

    pub async fn get_node(&self, id: Uuid) -> anyhow::Result<Option<NodeWithSubscription>> {
        let row = self.db.get_node(id)?;
        Ok(row.map(|(id, sub_id, node)| {
            let name = self
                .db
                .get_subscription(sub_id)
                .ok()
                .flatten()
                .and_then(|s| s.name);
            NodeWithSubscription {
                id,
                subscription_id: sub_id,
                subscription_name: name,
                node,
            }
        }))
    }

    pub async fn select_node(&self, id: Uuid) -> anyhow::Result<NodeWithSubscription> {
        let node = self
            .get_node(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found"))?;
        let mut selected = self.selected_node.write().await;
        *selected = Some(id);
        Ok(node)
    }

    pub async fn selected_node(&self) -> anyhow::Result<Option<NodeWithSubscription>> {
        let selected = self.selected_node.read().await;
        match *selected {
            Some(id) => Ok(self.get_node(id).await?),
            None => Ok(None),
        }
    }

    pub async fn proxy_status(&self) -> anyhow::Result<ProxyStatus> {
        let manager = self.process_manager.read().await;
        let running = manager.is_running().await;
        let selected_node = self.selected_node().await?;
        let ports = self.proxy_ports.read().await;
        let (pid, uptime_secs, last_error) = if running {
            manager.status().await
        } else {
            (None, None, None)
        };
        Ok(ProxyStatus {
            running,
            selected_node,
            socks_port: ports.map(|p| p.socks).unwrap_or(None),
            http_port: ports.map(|p| p.http).unwrap_or(None),
            mixed_port: ports.map(|p| p.mixed).unwrap_or(None),
            pid,
            uptime_secs,
            last_error,
        })
    }

    pub async fn start_proxy(&self, req: StartProxyRequest) -> anyhow::Result<ProxyStatus> {
        let node = match req.node_id {
            Some(id) => {
                let n = self
                    .get_node(id)
                    .await?
                    .ok_or_else(|| crate::error::ApiError::NotFound(format!("Node {id} not found")))?;
                self.select_node(id).await?;
                n
            }
            None => match self.selected_node().await? {
                Some(n) => n,
                None => return Err(crate::error::ApiError::BadRequest("No node selected".into()).into()),
            },
        };

        // Default to sing-box for any advanced node; otherwise still prefer sing-box.
        let _ = requires_singbox(&node.node);

        let ports = ProxyPorts {
            socks: req.socks_port,
            http: req.http_port,
            mixed: req.mixed_port,
        };
        let singbox_ports = InboundPorts {
            socks_port: ports.socks,
            http_port: ports.http,
            mixed_port: ports.mixed,
        };
        let config = generate_config(&node.node, singbox_ports)?;

        let manager = self.process_manager.write().await;
        manager.stop().await.ok();
        manager.start(&config).await?;

        let mut stored_ports = self.proxy_ports.write().await;
        *stored_ports = Some(ProxyPorts {
            socks: config.socks_port,
            http: config.http_port,
            mixed: config.mixed_port,
        });

        drop(manager);
        drop(stored_ports);
        self.proxy_status().await
    }

    pub async fn stop_proxy(&self) -> anyhow::Result<ProxyStatus> {
        let manager = self.process_manager.write().await;
        manager.stop().await?;
        let mut ports = self.proxy_ports.write().await;
        *ports = None;
        drop(manager);
        drop(ports);
        self.proxy_status().await
    }
}
