//! Global application state shared across API handlers.

use crate::core_process::{CoreProcessManager, CoreType};
use crate::db::{import_legacy_subscriptions, DbPool};
use crate::models::{
    NodeWithSubscription, ProxyStatus, SplitTunnelAction, SplitTunnelRule, SplitTunnelTarget,
    StartProxyRequest, StoredSubscription,
};
use crate::singbox::{generate_config as generate_singbox_config, InboundPorts};
use crate::xray::{generate_config as generate_xray_config, requires_xray, InboundPorts as XrayInboundPorts};
use ironpass_config::{AppConfig, ConfigManager};
use ironpass_core::models::Subscription;
use ironpass_core::traits::HwidProvider;
use ironpass_subscription::{FetchOptions, HttpSubscriptionFetcher, SubscriptionService};
use reqwest::Client;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub config_manager: ConfigManager,
    pub db: DbPool,
    pub hwid_provider: Arc<dyn HwidProvider + Send + Sync>,
    pub http_client: Client,
    pub process_manager: Arc<RwLock<CoreProcessManager>>,
    pub selected_node: Arc<RwLock<Option<Uuid>>>,
    pub proxy_ports: Arc<RwLock<Option<ProxyPorts>>>,
    pub split_tunnel_rules: Arc<RwLock<Vec<SplitTunnelRule>>>,
    pub start_time: Instant,
    pub xray_path: Arc<RwLock<Option<PathBuf>>>,
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
        xray_path: Option<PathBuf>,
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
            process_manager: Arc::new(RwLock::new(CoreProcessManager::new())),
            selected_node: Arc::new(RwLock::new(None)),
            proxy_ports: Arc::new(RwLock::new(None)),
            split_tunnel_rules: Arc::new(RwLock::new(Vec::new())),
            start_time: Instant::now(),
            xray_path: Arc::new(RwLock::new(xray_path)),
        }
    }

    /// Load split tunnel rules from the database into memory.
    pub async fn load_split_tunnel_rules_async(&self) -> anyhow::Result<()> {
        let rules = self.db.list_split_tunnel_rules(None)?;
        let mut guard = self.split_tunnel_rules.write().await;
        *guard = rules;
        Ok(())
    }

    /// Load split tunnel rules from the database into memory.
    ///
    /// This synchronous variant is intended for use before an async runtime is running.
    pub fn load_split_tunnel_rules(&self) -> anyhow::Result<()> {
        let rules = self.db.list_split_tunnel_rules(None)?;
        // Best-effort: if the runtime is available, use an async write; otherwise
        // initialize the vector lazily on first async access.
        if let Ok(handle) = tokio::runtime::Handle::try_current()
            && handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread
        {
            let this = Arc::new(self.clone());
            handle.block_on(async move {
                let mut guard = this.split_tunnel_rules.write().await;
                *guard = rules;
            });
        }
        Ok(())
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

    pub async fn list_split_tunnel_rules(
        &self,
        node_id: Option<Uuid>,
    ) -> anyhow::Result<Vec<SplitTunnelRule>> {
        Ok(self.db.list_split_tunnel_rules(node_id)?)
    }

    pub async fn get_split_tunnel_rule(&self, id: Uuid) -> anyhow::Result<Option<SplitTunnelRule>> {
        Ok(self.db.get_split_tunnel_rule(id)?)
    }

    pub async fn add_split_tunnel_rule(
        &self,
        target: SplitTunnelTarget,
        value: String,
        action: SplitTunnelAction,
        node_id: Option<Uuid>,
    ) -> anyhow::Result<SplitTunnelRule> {
        if value.trim().is_empty() {
            anyhow::bail!("Rule value cannot be empty");
        }
        let rule = SplitTunnelRule::new(target, value, action, node_id);
        self.db.insert_split_tunnel_rule(&rule)?;
        let mut guard = self.split_tunnel_rules.write().await;
        guard.push(rule.clone());
        Ok(rule)
    }

    pub async fn update_split_tunnel_rule(
        &self,
        id: Uuid,
        target: SplitTunnelTarget,
        value: String,
        action: SplitTunnelAction,
        node_id: Option<Uuid>,
    ) -> anyhow::Result<SplitTunnelRule> {
        if value.trim().is_empty() {
            anyhow::bail!("Rule value cannot be empty");
        }
        let existing = self
            .db
            .get_split_tunnel_rule(id)?
            .ok_or_else(|| crate::error::ApiError::NotFound(format!("Rule {id} not found")))?;
        let mut updated = existing;
        updated.target = target;
        updated.value = value;
        updated.action = action;
        updated.node_id = node_id;
        updated.updated_at = chrono::Utc::now();
        if !self.db.update_split_tunnel_rule(&updated)? {
            anyhow::bail!("Rule {id} not found");
        }

        let mut guard = self.split_tunnel_rules.write().await;
        if let Some(pos) = guard.iter().position(|r| r.id == id) {
            guard[pos] = updated.clone();
        } else {
            guard.push(updated.clone());
        }
        Ok(updated)
    }

    pub async fn delete_split_tunnel_rule(&self, id: Uuid) -> anyhow::Result<bool> {
        let deleted = self.db.delete_split_tunnel_rule(id)?;
        if deleted {
            let mut guard = self.split_tunnel_rules.write().await;
            guard.retain(|r| r.id != id);
        }
        Ok(deleted)
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

        let ports = ProxyPorts {
            socks: req.socks_port,
            http: req.http_port,
            mixed: req.mixed_port,
        };

        let rules = self.split_tunnel_rules.read().await.clone();
        let (core_type, config_json, actual_ports) = if requires_xray(&node.node) {
            let xray_ports = XrayInboundPorts {
                socks_port: ports.socks,
                http_port: ports.http,
                mixed_port: ports.mixed,
            };
            let config = generate_xray_config(&node.node, xray_ports, &rules)?;
            (
                CoreType::Xray,
                config.json,
                ProxyPorts {
                    socks: config.socks_port,
                    http: config.http_port,
                    mixed: config.mixed_port,
                },
            )
        } else {
            let singbox_ports = InboundPorts {
                socks_port: ports.socks,
                http_port: ports.http,
                mixed_port: ports.mixed,
            };
            let config = generate_singbox_config(&node.node, singbox_ports, &rules)?;
            (
                CoreType::SingBox,
                config.json,
                ProxyPorts {
                    socks: config.socks_port,
                    http: config.http_port,
                    mixed: config.mixed_port,
                },
            )
        };

        // Xray requires explicit configuration or a binary in PATH.
        if core_type == CoreType::Xray {
            let xray_path = self.xray_path.read().await.clone();
            if xray_path.is_none() && which_xray_in_path().is_err() {
                anyhow::bail!(
                    "Xray-core is required for XHTTP/Splithttp nodes. \
                     Provide --xray <PATH> or ensure `xray`/`xray.exe` is in PATH."
                );
            }
        }

        let mut manager = self.process_manager.write().await;
        manager.set_core_type(core_type);
        if let Some(path) = self.xray_path.read().await.clone() {
            manager.set_path(path);
        }
        manager.stop().await.ok();
        manager.start(&config_json).await?;

        let mut stored_ports = self.proxy_ports.write().await;
        *stored_ports = Some(actual_ports);

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

fn which_xray_in_path() -> anyhow::Result<PathBuf> {
    let path_env = std::env::var_os("PATH").ok_or_else(|| anyhow::anyhow!("PATH not set"))?;
    for name in ["xray", "xray.exe"] {
        for dir in std::env::split_paths(&path_env) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    anyhow::bail!("xray not found in PATH")
}

