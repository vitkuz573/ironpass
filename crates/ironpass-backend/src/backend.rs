//! Backend abstraction layer for proxy-core generators.
//!
//! This module defines a common [`Backend`] trait implemented by sing-box and
//! Xray-core generators, plus a registry that resolves [`BackendType`] choices
//! (including `Auto`) to a concrete backend instance.

use crate::assets::{GeoAssetStatus, detect_geo_assets, locate_core_binary};
use crate::core_process::CoreType;
use crate::singbox::generate_config as generate_singbox_config;
use crate::xray::generate_config as generate_xray_config;
use ironpass_core::models::{
    Protocol, ProxyNode, RoutingMode, Security, SplitTunnelRule, Transport,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Capabilities for a single proxy core backend.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackendCapability {
    pub available: bool,
    pub geo_assets_available: bool,
    pub version: Option<String>,
}

/// Capabilities reported by the daemon for the installed proxy cores.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackendCapabilities {
    pub xray: BackendCapability,
    pub sing_box: BackendCapability,
}
use std::path::PathBuf;
use std::sync::Arc;

/// User-selectable backend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(rename_all = "snake_case")]
pub enum BackendType {
    /// Automatically pick the best backend for the node.
    #[default]
    Auto,
    /// Use the sing-box generator.
    SingBox,
    /// Use the Xray-core generator.
    Xray,
}

impl BackendType {
    /// Return a snake_case identifier useful for CLI/API display.
    pub fn as_str(&self) -> &'static str {
        match self {
            BackendType::Auto => "auto",
            BackendType::SingBox => "sing-box",
            BackendType::Xray => "xray",
        }
    }
}

impl std::str::FromStr for BackendType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(BackendType::Auto),
            "sing-box" | "singbox" | "sb" => Ok(BackendType::SingBox),
            "xray" | "xray-core" => Ok(BackendType::Xray),
            _ => Err(format!("Unknown backend type: {s}")),
        }
    }
}

/// Port selection for proxy inbounds, shared across all backends.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProxyPorts {
    pub socks: Option<u16>,
    pub http: Option<u16>,
    pub mixed: Option<u16>,
}

/// A generated core configuration together with the local ports it exposes.
#[derive(Debug, Clone)]
pub struct GeneratedConfig {
    pub json: String,
    pub socks_port: Option<u16>,
    pub http_port: Option<u16>,
    pub mixed_port: Option<u16>,
}

/// A proxy-core backend capable of producing JSON configuration for a node.
pub trait Backend: Send + Sync {
    /// Generate a core JSON configuration for `node`.
    fn generate_config(
        &self,
        node: &ProxyNode,
        ports: ProxyPorts,
        rules: &[SplitTunnelRule],
        routing_mode: RoutingMode,
    ) -> anyhow::Result<GeneratedConfig>;

    /// Return the core process type associated with this backend.
    fn core_type(&self) -> CoreType;

    /// Return true if this backend can generate a working config for `node`.
    fn supports(&self, node: &ProxyNode) -> bool;

    /// Update the backend's view of available geo assets.
    fn set_geo_asset_status(&mut self, _status: GeoAssetStatus) {}

    /// Return the configured geo asset status, if any.
    fn geo_asset_status(&self) -> Option<GeoAssetStatus> {
        None
    }

    /// Return the explicit binary path, if set.
    fn binary_path(&self) -> Option<&PathBuf> {
        None
    }

    /// Set the explicit binary path.
    fn set_binary_path(&mut self, _path: Option<PathBuf>) {}
}

/// Registry holding the available backend implementations.
pub struct BackendRegistry {
    sing_box: Arc<RwLock<SingBoxBackend>>,
    xray: Arc<RwLock<XrayBackend>>,
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BackendRegistry {
    /// Create a registry with the default sing-box and Xray backends.
    pub fn new() -> Self {
        Self {
            sing_box: Arc::new(RwLock::new(SingBoxBackend::new())),
            xray: Arc::new(RwLock::new(XrayBackend::new())),
        }
    }

    /// Return the current geo asset status for the Xray backend.
    pub fn xray_geo_status(&self) -> GeoAssetStatus {
        self.xray
            .read()
            .unwrap()
            .geo_asset_status()
            .unwrap_or(GeoAssetStatus::new(true))
    }

    /// Return the current geo asset status for the sing-box backend.
    pub fn sing_box_geo_status(&self) -> GeoAssetStatus {
        self.sing_box
            .read()
            .unwrap()
            .geo_asset_status()
            .unwrap_or(GeoAssetStatus::new(false))
    }

    /// Refresh geo asset availability for both backends from disk.
    pub fn refresh_geo_assets(&self) {
        let xray_path = self.xray.read().unwrap().binary_path().cloned();
        if let Some(path) = locate_core_binary(&["xray", "xray.exe"], xray_path.as_deref()) {
            let status = detect_geo_assets(Some(&path));
            self.xray.write().unwrap().set_geo_asset_status(status);
        }
        let sing_box_path = self.sing_box.read().unwrap().binary_path().cloned();
        if let Some(path) = locate_core_binary(
            &["sing-box", "sing-box.exe", "sb"],
            sing_box_path.as_deref(),
        ) {
            let status = detect_geo_assets(Some(&path));
            self.sing_box.write().unwrap().set_geo_asset_status(status);
        }
    }

    /// Resolve a [`BackendType`] to a concrete backend.
    ///
    /// `Auto` selects Xray-core for XHTTP/Splithttp transports and sing-box
    /// otherwise.
    pub fn resolve(&self, backend_type: BackendType, node: &ProxyNode) -> Arc<dyn Backend> {
        match backend_type {
            BackendType::SingBox => {
                Arc::new(BackendSnapshot::from(&*self.sing_box.read().unwrap()))
            }
            BackendType::Xray => Arc::new(BackendSnapshot::from(&*self.xray.read().unwrap())),
            BackendType::Auto => {
                if self.xray.read().unwrap().supports(node) {
                    Arc::new(BackendSnapshot::from(&*self.xray.read().unwrap()))
                } else {
                    Arc::new(BackendSnapshot::from(&*self.sing_box.read().unwrap()))
                }
            }
        }
    }
}

use std::sync::RwLock;

/// A serializable backend snapshot with current geo asset status.
#[derive(Clone)]
pub struct BackendSnapshot {
    core: CoreType,
    geo: GeoAssetStatus,
}

impl BackendSnapshot {
    pub fn geo_asset_status(&self) -> GeoAssetStatus {
        self.geo
    }
}

impl Backend for BackendSnapshot {
    fn generate_config(
        &self,
        node: &ProxyNode,
        ports: ProxyPorts,
        rules: &[SplitTunnelRule],
        routing_mode: RoutingMode,
    ) -> anyhow::Result<GeneratedConfig> {
        match self.core {
            CoreType::SingBox => {
                let cfg = generate_singbox_config(
                    node,
                    crate::singbox::InboundPorts {
                        socks_port: ports.socks,
                        http_port: ports.http,
                        mixed_port: ports.mixed,
                    },
                    rules,
                    self.geo,
                    routing_mode,
                )?;
                Ok(GeneratedConfig {
                    json: cfg.json,
                    socks_port: cfg.socks_port,
                    http_port: cfg.http_port,
                    mixed_port: cfg.mixed_port,
                })
            }
            CoreType::Xray => {
                let cfg = generate_xray_config(
                    node,
                    crate::xray::InboundPorts {
                        socks_port: ports.socks,
                        http_port: ports.http,
                        mixed_port: ports.mixed,
                    },
                    rules,
                    self.geo,
                    routing_mode,
                )?;
                Ok(GeneratedConfig {
                    json: cfg.json,
                    socks_port: cfg.socks_port,
                    http_port: cfg.http_port,
                    mixed_port: cfg.mixed_port,
                })
            }
        }
    }

    fn core_type(&self) -> CoreType {
        self.core
    }

    fn supports(&self, node: &ProxyNode) -> bool {
        match self.core {
            CoreType::SingBox => supports_singbox(node),
            CoreType::Xray => supports_xray(node),
        }
    }

    fn geo_asset_status(&self) -> Option<GeoAssetStatus> {
        Some(self.geo)
    }
}

impl From<&SingBoxBackend> for BackendSnapshot {
    fn from(backend: &SingBoxBackend) -> Self {
        Self {
            core: CoreType::SingBox,
            geo: backend
                .geo_asset_status()
                .unwrap_or(GeoAssetStatus::new(false)),
        }
    }
}

impl From<&XrayBackend> for BackendSnapshot {
    fn from(backend: &XrayBackend) -> Self {
        Self {
            core: CoreType::Xray,
            geo: backend
                .geo_asset_status()
                .unwrap_or(GeoAssetStatus::new(true)),
        }
    }
}

/// Sing-box backend wrapper.
pub struct SingBoxBackend {
    geo: GeoAssetStatus,
    path: Option<PathBuf>,
}

impl SingBoxBackend {
    pub fn new() -> Self {
        Self {
            geo: GeoAssetStatus::new(false),
            path: None,
        }
    }
}

impl Default for SingBoxBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for SingBoxBackend {
    fn generate_config(
        &self,
        node: &ProxyNode,
        ports: ProxyPorts,
        rules: &[SplitTunnelRule],
        routing_mode: RoutingMode,
    ) -> anyhow::Result<GeneratedConfig> {
        let cfg = generate_singbox_config(
            node,
            crate::singbox::InboundPorts {
                socks_port: ports.socks,
                http_port: ports.http,
                mixed_port: ports.mixed,
            },
            rules,
            self.geo,
            routing_mode,
        )?;
        Ok(GeneratedConfig {
            json: cfg.json,
            socks_port: cfg.socks_port,
            http_port: cfg.http_port,
            mixed_port: cfg.mixed_port,
        })
    }

    fn core_type(&self) -> CoreType {
        CoreType::SingBox
    }

    fn supports(&self, node: &ProxyNode) -> bool {
        supports_singbox(node)
    }

    fn set_geo_asset_status(&mut self, status: GeoAssetStatus) {
        self.geo = status;
    }

    fn geo_asset_status(&self) -> Option<GeoAssetStatus> {
        Some(self.geo)
    }

    fn binary_path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    fn set_binary_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }
}

/// Xray-core backend wrapper.
pub struct XrayBackend {
    geo: GeoAssetStatus,
    path: Option<PathBuf>,
}

impl XrayBackend {
    pub fn new() -> Self {
        Self {
            geo: GeoAssetStatus::new(true),
            path: None,
        }
    }
}

impl Default for XrayBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for XrayBackend {
    fn generate_config(
        &self,
        node: &ProxyNode,
        ports: ProxyPorts,
        rules: &[SplitTunnelRule],
        routing_mode: RoutingMode,
    ) -> anyhow::Result<GeneratedConfig> {
        let cfg = generate_xray_config(
            node,
            crate::xray::InboundPorts {
                socks_port: ports.socks,
                http_port: ports.http,
                mixed_port: ports.mixed,
            },
            rules,
            self.geo,
            routing_mode,
        )?;
        Ok(GeneratedConfig {
            json: cfg.json,
            socks_port: cfg.socks_port,
            http_port: cfg.http_port,
            mixed_port: cfg.mixed_port,
        })
    }

    fn core_type(&self) -> CoreType {
        CoreType::Xray
    }

    fn supports(&self, node: &ProxyNode) -> bool {
        supports_xray(node)
    }

    fn set_geo_asset_status(&mut self, status: GeoAssetStatus) {
        self.geo = status;
    }

    fn geo_asset_status(&self) -> Option<GeoAssetStatus> {
        Some(self.geo)
    }

    fn binary_path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    fn set_binary_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }
}

/// Returns true if sing-box can generate a working config for this node.
pub fn supports_singbox(node: &ProxyNode) -> bool {
    matches!(node.security, Security::Reality | Security::RealityPsk)
        || matches!(
            node.transport,
            Transport::Xhttp
                | Transport::Splithttp
                | Transport::Grpc
                | Transport::H2
                | Transport::Kcp
        )
        || matches!(
            node.protocol,
            Protocol::Hysteria2 | Protocol::Tuic | Protocol::WireGuard | Protocol::AnyTls
        )
}

/// Returns true if Xray-core can generate a working config for this node.
pub fn supports_xray(node: &ProxyNode) -> bool {
    matches!(node.transport, Transport::Xhttp | Transport::Splithttp)
        && matches!(node.protocol, Protocol::Vless | Protocol::Trojan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ironpass_core::models::{Protocol, Security, Transport};

    fn sample_vless(transport: Transport, security: Security) -> ProxyNode {
        ProxyNode {
            protocol: Protocol::Vless,
            name: "test".into(),
            server: "example.com".into(),
            port: 443,
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            password: None,
            alter_id: None,
            encryption: Some("none".into()),
            transport,
            security,
            flow: None,
            sni: None,
            fingerprint: None,
            public_key: None,
            short_id: None,
            spider_x: None,
            path: None,
            host: None,
            service_name: None,
            alpn: None,
            extra: None,
            tags: Vec::new(),
            raw_uri: String::new(),
        }
    }

    #[test]
    fn auto_selects_xray_for_xhttp() {
        let node = sample_vless(Transport::Xhttp, Security::Tls);
        let registry = BackendRegistry::new();
        let backend = registry.resolve(BackendType::Auto, &node);
        assert_eq!(backend.core_type(), CoreType::Xray);
    }

    #[test]
    fn auto_selects_xray_for_splithttp() {
        let node = sample_vless(Transport::Splithttp, Security::Tls);
        let registry = BackendRegistry::new();
        let backend = registry.resolve(BackendType::Auto, &node);
        assert_eq!(backend.core_type(), CoreType::Xray);
    }

    #[test]
    fn auto_selects_singbox_for_tcp() {
        let node = sample_vless(Transport::Tcp, Security::Tls);
        let registry = BackendRegistry::new();
        let backend = registry.resolve(BackendType::Auto, &node);
        assert_eq!(backend.core_type(), CoreType::SingBox);
    }

    #[test]
    fn auto_selects_singbox_for_reality() {
        let mut node = sample_vless(Transport::Tcp, Security::Reality);
        node.public_key = Some("pbk".into());
        node.short_id = Some("0123456789abcdef".into());
        let registry = BackendRegistry::new();
        let backend = registry.resolve(BackendType::Auto, &node);
        assert_eq!(backend.core_type(), CoreType::SingBox);
    }

    #[test]
    fn explicit_singbox_overrides_auto() {
        let node = sample_vless(Transport::Xhttp, Security::Tls);
        let registry = BackendRegistry::new();
        let backend = registry.resolve(BackendType::SingBox, &node);
        assert_eq!(backend.core_type(), CoreType::SingBox);
    }

    #[test]
    fn explicit_xray_overrides_auto() {
        let node = sample_vless(Transport::Tcp, Security::Tls);
        let registry = BackendRegistry::new();
        let backend = registry.resolve(BackendType::Xray, &node);
        assert_eq!(backend.core_type(), CoreType::Xray);
    }

    #[test]
    fn xray_does_not_support_ws() {
        let node = sample_vless(Transport::Ws, Security::Tls);
        assert!(!supports_xray(&node));
    }

    #[test]
    fn xray_does_not_support_vmess() {
        let mut node = sample_vless(Transport::Xhttp, Security::Tls);
        node.protocol = Protocol::Vmess;
        assert!(!supports_xray(&node));
    }

    #[test]
    fn backend_type_from_str_accepts_aliases() {
        assert_eq!("auto".parse::<BackendType>().unwrap(), BackendType::Auto);
        assert_eq!(
            "sing-box".parse::<BackendType>().unwrap(),
            BackendType::SingBox
        );
        assert_eq!(
            "singbox".parse::<BackendType>().unwrap(),
            BackendType::SingBox
        );
        assert_eq!("sb".parse::<BackendType>().unwrap(), BackendType::SingBox);
        assert_eq!("xray".parse::<BackendType>().unwrap(), BackendType::Xray);
        assert!("unknown".parse::<BackendType>().is_err());
    }
}
