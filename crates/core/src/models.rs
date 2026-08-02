use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use strum::{Display, EnumString};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Display, EnumString)]
#[strum(serialize_all = "snake_case")]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Display, EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum Transport {
    Tcp,
    Ws,
    Grpc,
    H2,
    Xhttp,
    Splithttp,
    Kcp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Display, EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum Security {
    None,
    Tls,
    Reality,
    RealityPsk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Display, EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum OutputFormat {
    Clash,
    SingBox,
    V2Ray,
    Surge,
    QuantumultX,
    Loon,
    Raw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub tags: Vec<String>,
    pub raw_uri: String,
}

impl ProxyNode {
    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.server, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionMetadata {
    pub profile_title: Option<String>,
    pub profile_update_interval_hours: Option<u64>,
    pub profile_web_page_url: Option<String>,
    pub announcement: Option<String>,
    pub headers: HashMap<String, String>,
}

impl Default for SubscriptionMetadata {
    fn default() -> Self {
        Self {
            profile_title: None,
            profile_update_interval_hours: None,
            profile_web_page_url: None,
            announcement: None,
            headers: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: uuid::Uuid,
    pub url: String,
    pub name: Option<String>,
    pub nodes: Vec<ProxyNode>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub traffic_used: Option<u64>,
    pub traffic_total: Option<u64>,
    pub metadata: SubscriptionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwidInfo {
    pub hwid: String,
    pub device_model: String,
    pub os: String,
    pub hostname: String,
    pub username: String,
    pub machine_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionResponse {
    pub raw: String,
    pub format: SubscriptionFormat,
    pub nodes: Vec<ProxyNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
