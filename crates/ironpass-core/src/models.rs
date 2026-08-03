use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use strum::{Display, EnumString};
use utoipa::ToSchema;
use uuid::Uuid;

/// Network protocol used by a proxy node.
#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Display, EnumString, ToSchema,
)]
#[strum(serialize_all = "snake_case")]
#[schema(rename_all = "snake_case")]
pub enum Protocol {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
    Hysteria2,
    Tuic,
    WireGuard,
    AnyTls,
}

/// Transport layer used by a proxy node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Display, EnumString, ToSchema)]
#[strum(serialize_all = "snake_case")]
#[schema(rename_all = "snake_case")]
pub enum Transport {
    Tcp,
    Ws,
    Grpc,
    H2,
    Xhttp,
    Splithttp,
    Kcp,
}

/// Security layer used by a proxy node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Display, EnumString, ToSchema)]
#[strum(serialize_all = "snake_case")]
#[schema(rename_all = "snake_case")]
pub enum Security {
    None,
    Tls,
    Reality,
    RealityPsk,
}

/// Target type for a split tunnel rule.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(rename_all = "snake_case")]
pub enum SplitTunnelTarget {
    /// Exact domain name.
    #[default]
    Domain,
    /// IPv4 or IPv6 address.
    Ip,
    /// CIDR range.
    Cidr,
    /// Application name or path (not supported by all backends).
    App,
}

/// Action for a split tunnel rule.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(rename_all = "snake_case")]
pub enum SplitTunnelAction {
    /// Bypass the proxy and route directly.
    #[default]
    Direct,
    /// Force traffic through the proxy.
    Proxy,
}

impl SplitTunnelAction {
    /// Return the outbound tag corresponding to this action.
    pub fn outbound_tag(self) -> &'static str {
        match self {
            SplitTunnelAction::Direct => "direct",
            SplitTunnelAction::Proxy => "proxy",
        }
    }

    /// Return true if this rule action differs from the default outbound tag.
    ///
    /// In `ProxyAllExceptBypass` the default outbound is `proxy`, so only
    /// `direct` rules need to be emitted. In `ProxyOnlyListed` the default is
    /// `direct`, so only `proxy` rules need to be emitted.
    pub fn matches_routing_mode(self, default_tag: &str) -> bool {
        self.outbound_tag() != default_tag
    }
}

/// Global routing mode for split tunnel behavior.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(rename_all = "snake_case")]
pub enum RoutingMode {
    /// Route everything through the proxy except explicit `direct` rules.
    #[default]
    ProxyAllExceptBypass,
    /// Route everything directly except explicit `proxy` rules.
    ProxyOnlyListed,
}

/// A user-defined split tunnel (selective routing) rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct SplitTunnelRule {
    pub id: Uuid,
    pub target: SplitTunnelTarget,
    pub value: String,
    pub action: SplitTunnelAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SplitTunnelRule {
    pub fn new(
        target: SplitTunnelTarget,
        value: impl Into<String>,
        action: SplitTunnelAction,
        node_id: Option<Uuid>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            target,
            value: value.into(),
            action,
            node_id,
            created_at: now,
            updated_at: now,
        }
    }
}

/// A parsed proxy node from a subscription.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProxyNode {
    pub protocol: Protocol,
    pub name: String,
    pub server: String,
    pub port: u16,
    pub uuid: Option<String>,
    pub password: Option<String>,
    pub alter_id: Option<u32>,
    pub encryption: Option<String>,
    pub transport: Transport,
    pub security: Security,
    pub flow: Option<String>,
    pub sni: Option<String>,
    pub fingerprint: Option<String>,
    pub public_key: Option<String>,
    pub short_id: Option<String>,
    pub spider_x: Option<String>,
    pub path: Option<String>,
    pub host: Option<String>,
    pub service_name: Option<String>,
    pub alpn: Option<Vec<String>>,
    /// Parsed XHTTP extra configuration (the `extra` query parameter in VLESS URIs).
    #[serde(default)]
    pub extra: Option<XhttpExtra>,
    pub tags: Vec<String>,
    pub raw_uri: String,
}

/// XHTTP transport extra settings carried in the `extra` query parameter.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(rename_all = "camelCase")]
pub struct XhttpExtra {
    /// XHTTP operating mode, e.g. `stream-up` or `packet-up`.
    pub mode: Option<String>,
    /// HTTP/2 ` SETTINGS_MAX_CONCURRENT_STREAMS` hint (used for HTTP/2 mode).
    pub max_concurrent_uploads: Option<u64>,
    /// Maximum number of upstream sessions kept open (HTTP/2 mode).
    pub max_connections: Option<u64>,
    /// `stream-up` submode: `"raw"` or `"packet-up"`.
    pub no_grpc_header: Option<bool>,
    /// Optional padding range for the HTTP request body, e.g. `"100-1000"`.
    pub x_padding_bytes: Option<String>,
    /// Optional request headers merged into the HTTP upgrade request.
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Additional XHTTP tuning fields not modelled explicitly (e.g. `xmux`).
    #[serde(flatten, default)]
    pub other: HashMap<String, serde_json::Value>,
}

impl XhttpExtra {
    /// Return true if the extra settings request an HTTP/2 based XHTTP mode.
    pub fn prefers_h2(&self) -> bool {
        matches!(self.mode.as_deref(), Some("stream-up") | Some("packet-up"))
    }

    /// Parse the raw `extra` query string value into structured settings.
    pub fn parse(raw: &str) -> serde_json::Result<Self> {
        serde_json::from_str(raw)
    }

    /// Return the number of initial padding bytes to generate, if configured.
    pub fn padding_len(&self) -> Option<usize> {
        let s = self.x_padding_bytes.as_deref()?;
        // Accept exact values or "min-max" ranges (pick max).
        if let Some((_, max)) = s.split_once('-') {
            max.trim().parse().ok()
        } else {
            s.parse().ok()
        }
    }
}

impl ProxyNode {
    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.server, self.port)
    }
}

/// Provider-specific metadata extracted from subscription headers or inline body lines.
///
/// The fetcher recognises the de-facto standard keys used by many providers:
/// `profile-title`, `profile-update-interval`, `profile-web-page-url` and
/// `announce`/`announces`. Values prefixed with `base64:` are decoded
/// automatically. HTTP response headers take precedence over inline body values.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct SubscriptionMetadata {
    /// Human-readable profile title, if provided.
    pub profile_title: Option<String>,
    /// Recommended update interval in hours.
    pub profile_update_interval_hours: Option<u64>,
    /// Provider web page or support URL.
    pub profile_web_page_url: Option<String>,
    /// Provider announcement or status message.
    pub announcement: Option<String>,
    /// Raw map of all recognised header keys and their (decoded) values.
    pub headers: HashMap<String, String>,
}

/// A fully parsed subscription, including nodes, traffic accounting, expiry and metadata.
///
/// This is the output of [`crate::traits::SubscriptionFetcher::fetch`] and the main
/// data structure consumed by the CLI for display and analysis.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Subscription {
    /// Unique identifier assigned locally when the subscription is fetched.
    pub id: uuid::Uuid,
    /// Source URL from which the subscription was fetched.
    pub url: String,
    /// Optional user-defined display name.
    pub name: Option<String>,
    /// Parsed proxy nodes (real + placeholder, unless filtered by the caller).
    pub nodes: Vec<ProxyNode>,
    /// Timestamp when the subscription was fetched.
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    /// Optional account expiry parsed from `subscription-userinfo`.
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Combined upload + download bytes used, if reported by the provider.
    pub traffic_used: Option<u64>,
    /// Total allowed bytes, if reported by the provider.
    pub traffic_total: Option<u64>,
    /// Provider metadata such as title, update interval and announcements.
    pub metadata: SubscriptionMetadata,
}

/// Hardware/device identifier information.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HwidInfo {
    pub hwid: String,
    pub device_model: String,
    pub os: String,
    pub hostname: String,
    pub username: String,
    pub machine_id: String,
}

/// Recognised subscription payload formats.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub enum SubscriptionFormat {
    Base64VlessList,
    ClashYaml,
    SingBoxJson,
    RawUriList,
    Unknown,
}

impl fmt::Display for SubscriptionFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Base64VlessList => write!(f, "Base64 URI List"),
            Self::ClashYaml => write!(f, "Clash YAML"),
            Self::SingBoxJson => write!(f, "sing-box JSON"),
            Self::RawUriList => write!(f, "Raw URI List"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}
