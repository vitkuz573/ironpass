//! High-level subscription handling for IronPass.
//!
//! This crate provides the public API used by the CLI to fetch, parse and export
//! VPN subscription content. The main entry point is [`SubscriptionService`], which
//! composes a fetcher, parser and exporter behind a small, convenient facade.
//!
//! For low-level control over fetching (custom HTTP clients, mock HWID providers,
//! retry options, etc.), use [`HttpSubscriptionFetcher`] directly.

use ironpass_core::{Result, models::*, traits::*};

mod converters;
mod exporter;
mod fetcher;
mod parser;

pub use exporter::NodeExporterImpl;
pub use fetcher::{
    FetchOptions, HttpSubscriptionFetcher, PlaceholderPolicy, extract_inline_metadata,
    is_placeholder_node, placeholder_messages,
};
pub use parser::SubscriptionParser;

/// Convenience facade that combines subscription fetching, parsing and exporting.
///
/// [`SubscriptionService::new`] builds a service with sensible defaults: a 30-second
/// HTTP timeout, limited redirects and automatic HWID retry on placeholder responses.
/// Use [`SubscriptionService::with_fetch_options`] to customise retry behaviour.
pub struct SubscriptionService {
    fetcher: HttpSubscriptionFetcher,
    parser: SubscriptionParser,
    exporter: NodeExporterImpl,
}

impl SubscriptionService {
    /// Create a new service using the default [`FetchOptions`].
    pub fn new() -> Self {
        Self::with_fetch_options(FetchOptions::default())
    }

    /// Create a new service with custom fetcher options.
    ///
    /// This is the recommended way to disable automatic HWID retries or change the
    /// maximum number of retries.
    pub fn with_fetch_options(options: FetchOptions) -> Self {
        Self {
            fetcher: HttpSubscriptionFetcher::with_client(default_http_client(), options),
            parser: SubscriptionParser::new(),
            exporter: NodeExporterImpl::new(),
        }
    }

    /// Fetch a subscription from `url` and parse it into a structured object.
    ///
    /// `hwid` is an optional Hardware ID sent to the provider. If `None` and the
    /// response contains only placeholder nodes, the fetcher will generate a HWID
    /// and retry according to the configured [`FetchOptions`].
    pub async fn fetch_and_parse(&self, url: &str, hwid: Option<&str>) -> Result<Subscription> {
        self.fetcher.fetch(url, hwid).await
    }

    /// Parse raw subscription text into a list of proxy nodes.
    ///
    /// The format is auto-detected from the input (Base64 URI list, raw URI list,
    /// Clash YAML or sing-box JSON).
    pub fn parse_raw(&self, input: &str) -> Result<Vec<ProxyNode>> {
        self.parser.parse(input)
    }

    /// Detect the format of `input` without fully parsing it.
    pub fn detect_format(&self, input: &str) -> SubscriptionFormat {
        self.parser.detect_format(input)
    }

    /// Export a slice of nodes to the requested output format.
    pub fn export(&self, nodes: &[ProxyNode], format: &OutputFormat) -> Result<String> {
        self.exporter.export(nodes, format)
    }
}

impl Default for SubscriptionService {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the default [`reqwest::Client`] used by [`SubscriptionService`].
fn default_http_client() -> reqwest::Client {
    let user_agent = format!("IronPass/{}", env!("CARGO_PKG_VERSION"));
    reqwest::Client::builder()
        .user_agent(&user_agent)
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .expect("Failed to create HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use ironpass_core::models::{OutputFormat, Protocol, SubscriptionFormat};

    #[test]
    fn service_detect_format_base64() {
        let svc = SubscriptionService::new();
        let raw = "vless://uuid@example.com:443?encryption=none#T";
        let encoded = STANDARD.encode(raw.as_bytes());
        assert_eq!(
            svc.detect_format(&encoded),
            SubscriptionFormat::Base64VlessList
        );
    }

    #[test]
    fn service_detect_format_raw() {
        let svc = SubscriptionService::new();
        assert_eq!(
            svc.detect_format("vless://uuid@example.com:443?encryption=none#T"),
            SubscriptionFormat::RawUriList
        );
    }

    #[test]
    fn service_parse_raw_delegates() {
        let svc = SubscriptionService::new();
        let raw = "vless://550e8400-e29b-41d4-a716-446655440000@example.com:443?encryption=none#T";
        let nodes = svc.parse_raw(raw).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].protocol, Protocol::Vless);
    }

    #[test]
    fn service_export_raw() {
        let svc = SubscriptionService::new();
        let raw = "vless://550e8400-e29b-41d4-a716-446655440000@example.com:443?encryption=none#T";
        let nodes = svc.parse_raw(raw).unwrap();
        let out = svc.export(&nodes, &OutputFormat::Raw).unwrap();
        assert!(out.contains("vless://"));
    }
}
