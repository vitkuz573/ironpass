//! Global application state shared across API handlers.

use crate::db::DbPool;
use crate::models::{NodeWithSubscription, ProxyStatus, StartProxyRequest, StoredSubscription};
use ironpass_backend::{
    BackendCapabilities, BackendCapability, BackendRegistry, BackendType, CoreProcessManager,
    CoreType, GeneratedConfig, ProxyPorts, detect_geo_assets, locate_core_binary,
};
use ironpass_config::{AppConfig, ConfigManager};
use ironpass_core::models::{
    RoutingMode, SplitTunnelAction, SplitTunnelRule, SplitTunnelTarget, Subscription,
};
use ironpass_core::traits::HwidProvider;
use ironpass_subscription::{FetchOptions, HttpSubscriptionFetcher, SubscriptionService};
use reqwest::Client;
use std::path::PathBuf;
use std::process::Command;
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
    pub routing_mode: Arc<RwLock<RoutingMode>>,
    pub start_time: Instant,
    pub xray_path: Arc<RwLock<Option<PathBuf>>>,
    pub backend_registry: Arc<BackendRegistry>,
    pub preferred_backend: Arc<RwLock<BackendType>>,
}

impl AppState {
    pub fn new(
        config_manager: ConfigManager,
        db: DbPool,
        hwid_provider: Arc<dyn HwidProvider + Send + Sync>,
        xray_path: Option<PathBuf>,
    ) -> Self {
        let selected_node = db.get_selected_node_id().unwrap_or(None);
        let routing_mode = db.get_routing_mode().unwrap_or_default();
        let backend_registry = BackendRegistry::new();
        backend_registry.refresh_geo_assets();
        let _caps = backend_registry.xray_geo_status();
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
            selected_node: Arc::new(RwLock::new(selected_node)),
            proxy_ports: Arc::new(RwLock::new(None)),
            split_tunnel_rules: Arc::new(RwLock::new(Vec::new())),
            routing_mode: Arc::new(RwLock::new(routing_mode)),
            start_time: Instant::now(),
            xray_path: Arc::new(RwLock::new(xray_path)),
            backend_registry: Arc::new(backend_registry),
            preferred_backend: Arc::new(RwLock::new(BackendType::Auto)),
        }
    }

    /// Set the preferred backend type used for `Auto` resolution.
    pub async fn set_preferred_backend(&self, backend_type: BackendType) {
        let mut guard = self.preferred_backend.write().await;
        *guard = backend_type;
    }

    /// Refresh backend binary availability and geo asset status from disk.
    pub async fn refresh_backend_capabilities(&self) {
        self.backend_registry.refresh_geo_assets();
    }

    /// Return the current backend capabilities.
    pub async fn backend_capabilities(&self) -> BackendCapabilities {
        let xray_path = self.xray_path.read().await.clone();
        let xray_bin = locate_core_binary(&["xray", "xray.exe"], xray_path.as_deref());
        let xray_geo = detect_geo_assets(xray_bin.as_deref());
        let xray_version = xray_bin.as_deref().and_then(core_version);

        let sing_box_bin = locate_core_binary(&["sing-box", "sing-box.exe", "sb"], None);
        let sing_box_geo = detect_geo_assets(sing_box_bin.as_deref());
        let sing_box_version = sing_box_bin.as_deref().and_then(core_version);

        BackendCapabilities {
            xray: BackendCapability {
                available: xray_bin.is_some(),
                geo_assets_available: xray_geo.available,
                version: xray_version,
            },
            sing_box: BackendCapability {
                available: sing_box_bin.is_some(),
                geo_assets_available: sing_box_geo.available,
                version: sing_box_version,
            },
        }
    }

    /// Resolve a backend type to a concrete backend.
    ///
    /// `Auto` uses the state's preferred backend if set, otherwise selects the
    /// best backend for the node.
    pub async fn resolve_backend(
        &self,
        backend_type: BackendType,
        node: &ironpass_core::models::ProxyNode,
    ) -> anyhow::Result<(BackendType, Arc<dyn ironpass_backend::Backend>)> {
        let backend: Arc<dyn ironpass_backend::Backend> = match backend_type {
            BackendType::Auto => {
                let preferred = *self.preferred_backend.read().await;
                if preferred == BackendType::Auto {
                    self.backend_registry.resolve(BackendType::Auto, node)
                } else {
                    self.backend_registry.resolve(preferred, node)
                }
            }
            _ => self.backend_registry.resolve(backend_type, node),
        };
        Ok((backend_type, backend))
    }

    /// Load split tunnel rules from the database into memory.
    pub async fn load_split_tunnel_rules(&self) -> anyhow::Result<()> {
        let rules = self.db.list_split_tunnel_rules(None)?;
        let mut guard = self.split_tunnel_rules.write().await;
        *guard = rules;
        Ok(())
    }

    pub fn load_config(&self) -> anyhow::Result<AppConfig> {
        let mut config = self.config_manager.load_config()?;
        config.routing_mode = self.db.get_routing_mode().unwrap_or_default();
        Ok(config)
    }

    pub fn save_config(&self, config: &AppConfig) -> anyhow::Result<()> {
        self.db.set_routing_mode(config.routing_mode)?;
        let mut file_config = config.clone();
        file_config.routing_mode = RoutingMode::default();
        Ok(self.config_manager.save_config(&file_config)?)
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
        let should_clear = if let Some(node_id) = *selected {
            matches!(self.db.get_node(node_id), Ok(Some((_, sub_id, _))) if sub_id == id)
        } else {
            false
        };
        if should_clear {
            *selected = None;
            self.db.set_selected_node_id(None)?;
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

        let fetcher =
            HttpSubscriptionFetcher::with_client(self.http_client.clone(), FetchOptions::default());
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
        let sub_names: std::collections::HashMap<Uuid, Option<String>> =
            subs.into_iter().map(|s| (s.id, s.name)).collect();

        let selected = *self.selected_node.read().await;
        let rows = self.db.list_nodes(subscription_id)?;
        let mut nodes = Vec::with_capacity(rows.len());
        for (id, sub_id, node) in rows {
            nodes.push(NodeWithSubscription {
                id,
                subscription_id: sub_id,
                subscription_name: sub_names.get(&sub_id).cloned().unwrap_or(None),
                selected: selected == Some(id),
                node,
            });
        }
        Ok(nodes)
    }

    pub async fn get_node(&self, id: Uuid) -> anyhow::Result<Option<NodeWithSubscription>> {
        let row = self.db.get_node(id)?;
        let selected = *self.selected_node.read().await;
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
                selected: selected == Some(id),
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
        drop(selected);
        self.db.set_selected_node_id(Some(id))?;
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
        validate_split_tunnel_rule(target, &value)?;
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
        validate_split_tunnel_rule(target, &value)?;
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
        let backend = *self.preferred_backend.read().await;
        Ok(ProxyStatus {
            running,
            selected_node,
            socks_port: ports.map(|p| p.socks).unwrap_or(None),
            http_port: ports.map(|p| p.http).unwrap_or(None),
            mixed_port: ports.map(|p| p.mixed).unwrap_or(None),
            pid,
            uptime_secs,
            last_error,
            backend: Some(backend),
        })
    }

    pub async fn start_proxy(&self, req: StartProxyRequest) -> anyhow::Result<ProxyStatus> {
        let node = match req.node_id {
            Some(id) => {
                let n = self.get_node(id).await?.ok_or_else(|| {
                    crate::error::ApiError::NotFound(format!("Node {id} not found"))
                })?;
                self.select_node(id).await?;
                n
            }
            None => match self.selected_node().await? {
                Some(n) => n,
                None => {
                    self.db.set_selected_node_id(None).ok();
                    return Err(
                        crate::error::ApiError::BadRequest("No node selected".into()).into(),
                    );
                }
            },
        };

        // Ensure the selected node still exists in the database.
        if self.db.get_node(node.id)?.is_none() {
            let mut selected = self.selected_node.write().await;
            *selected = None;
            drop(selected);
            self.db.set_selected_node_id(None)?;
            return Err(
                crate::error::ApiError::NotFound(format!("Node {} not found", node.id)).into(),
            );
        }

        let ports = ProxyPorts {
            socks: req.socks_port,
            http: req.http_port,
            mixed: req.mixed_port,
        };

        let backend_type = req.backend.unwrap_or(BackendType::Auto);
        let (_resolved_type, backend) = self.resolve_backend(backend_type, &node.node).await?;
        if !backend.supports(&node.node) {
            return Err(crate::error::ApiError::BadRequest(
                "Selected backend does not support this node".into(),
            )
            .into());
        }

        let rules = self.split_tunnel_rules.read().await.clone();
        let routing_mode = *self.routing_mode.read().await;
        let config: GeneratedConfig =
            backend.generate_config(&node.node, ports, &rules, routing_mode)?;
        let core_type = backend.core_type();
        let actual_ports = ProxyPorts {
            socks: config.socks_port,
            http: config.http_port,
            mixed: config.mixed_port,
        };

        // Xray requires explicit configuration or a binary in PATH.
        if core_type == CoreType::Xray {
            let xray_path = self.xray_path.read().await.clone();
            if xray_path.is_none() && CoreProcessManager::which_xray_in_path().is_err() {
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
        manager.start(&config.json).await?;

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

fn core_version(path: &std::path::Path) -> Option<String> {
    let output = Command::new(path).arg("version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    Some(line.trim().to_string())
}

fn validate_split_tunnel_rule(target: SplitTunnelTarget, value: &str) -> anyhow::Result<()> {
    match target {
        SplitTunnelTarget::Ip if value.parse::<std::net::IpAddr>().is_err() => {
            anyhow::bail!("Invalid IP address: {value}");
        }
        SplitTunnelTarget::Cidr if value.parse::<ipnet::IpNet>().is_err() => {
            anyhow::bail!("Invalid CIDR: {value}");
        }
        _ => {}
    }
    Ok(())
}
