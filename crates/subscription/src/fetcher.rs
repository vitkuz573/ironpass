use ironpass_core::{Error, Result, models::*, traits::*};
use async_trait::async_trait;
use tracing::{info, warn};

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct HttpSubscriptionFetcher {
    client: reqwest::Client,
}

impl HttpSubscriptionFetcher {
    pub fn new() -> Self {
        let user_agent = format!("IronPass/{}", VERSION);
        let client = reqwest::Client::builder()
            .user_agent(&user_agent)
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }
}

impl Default for HttpSubscriptionFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SubscriptionFetcher for HttpSubscriptionFetcher {
    async fn fetch(&self, url: &str, hwid: Option<&str>) -> Result<Subscription> {
        info!("Fetching subscription from: {}", mask_url(url));

        let mut request = self.client.get(url);

        if let Some(id) = hwid {
            request = request.header("x-hwid", id);

            let provider = ironpass_hwid::SystemHwidProvider::new();
            let info = provider.get_device_info().ok();

            if let Some(ref info) = info {
                request = request.header("x-device-model", &info.device_model);

                let os_short = info.os.split('(').next().unwrap_or(&info.os).trim().to_string();
                let ua = format!("IronPass/{} ({})", VERSION, os_short);
                request = request.header("User-Agent", &ua);
                request = request.header("x-device-os", &os_short);
                request = request.header("x-ver-os", &info.os);

                info!("Sending HWID: {}...", &id[..id.len().min(16)]);
                info!("Device: {}", info.device_model);
                info!("OS: {} (short: {})", info.os, os_short);
                info!("UA: {}", ua);
            } else {
                request = request.header("x-hwid", id);
            }
        } else {
            warn!("No HWID provided — server may return placeholder nodes");
        }

        let response = request.send().await?;
        let status = response.status();

        if !status.is_success() {
            return Err(Error::Custom(format!(
                "HTTP {} from subscription endpoint",
                status
            )));
        }

        let headers = response.headers().clone();
        let body = response.text().await?;

        let traffic_used = headers
            .get("subscription-userinfo")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| parse_subscription_info(s).map(|i| i.0));

        let traffic_total = headers
            .get("subscription-userinfo")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| parse_subscription_info(s).map(|i| i.1));

        let parser = super::parser::SubscriptionParser::new();
        let format = parser.detect_format(&body);
        let nodes = parser.parse(&body)?;

        let placeholder_count = nodes.iter().filter(|n| is_placeholder_node(n)).count();

        if placeholder_count > 0 && placeholder_count == nodes.len() {
            warn!(
                "All {} nodes are placeholders — HWID likely required or device limit reached",
                nodes.len()
            );
        }

        info!(
            "Detected format: {}, found {} nodes ({} real, {} placeholder)",
            format,
            nodes.len(),
            nodes.len() - placeholder_count,
            placeholder_count
        );

        Ok(Subscription {
            id: uuid::Uuid::new_v4(),
            url: url.to_string(),
            name: None,
            nodes,
            fetched_at: chrono::Utc::now(),
            expires_at: None,
            traffic_used,
            traffic_total,
        })
    }
}

/// Placeholder detection via protocol-level signals only.
pub fn is_placeholder_node(node: &ProxyNode) -> bool {
    if is_dummy_address(&node.server) {
        return true;
    }

    if node.port == 0 || node.port == 1 {
        return true;
    }

    if let Some(ref uuid) = node.uuid {
        if uuid == "00000000-0000-0000-0000-000000000000" {
            return true;
        }
    }

    false
}

fn is_dummy_address(addr: &str) -> bool {
    matches!(
        addr,
        "0.0.0.0"
            | "127.0.0.1"
            | "::1"
            | "::"
            | "localhost"
            | "example.com"
            | "test.com"
    ) || addr.starts_with("0.")
}

pub fn placeholder_messages(nodes: &[ProxyNode]) -> Vec<String> {
    nodes
        .iter()
        .filter(|n| is_placeholder_node(n))
        .map(|n| n.name.clone())
        .collect()
}

fn mask_url(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        let path = parsed.path();
        if path.len() > 10 {
            format!("{}...{}", &path[..6], &path[path.len() - 4..])
        } else {
            url.to_string()
        }
    } else {
        "***".to_string()
    }
}

fn parse_subscription_info(info: &str) -> Option<(u64, u64)> {
    let upload: u64 = info
        .split(';')
        .find(|s| s.contains("upload="))
        .and_then(|s| s.split('=').last())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let download: u64 = info
        .split(';')
        .find(|s| s.contains("download="))
        .and_then(|s| s.split('=').last())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let total: u64 = info
        .split(';')
        .find(|s| s.contains("total="))
        .and_then(|s| s.split('=').last())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    Some((upload + download, total))
}
