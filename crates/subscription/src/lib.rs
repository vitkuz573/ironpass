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

pub struct SubscriptionService {
    fetcher: HttpSubscriptionFetcher,
    parser: SubscriptionParser,
    exporter: NodeExporterImpl,
}

impl SubscriptionService {
    pub fn new() -> Self {
        Self {
            fetcher: HttpSubscriptionFetcher::new(),
            parser: SubscriptionParser::new(),
            exporter: NodeExporterImpl::new(),
        }
    }

    pub async fn fetch_and_parse(&self, url: &str, hwid: Option<&str>) -> Result<Subscription> {
        self.fetcher.fetch(url, hwid).await
    }

    pub fn parse_raw(&self, input: &str) -> Result<Vec<ProxyNode>> {
        self.parser.parse(input)
    }

    pub fn detect_format(&self, input: &str) -> SubscriptionFormat {
        self.parser.detect_format(input)
    }

    pub fn export(&self, nodes: &[ProxyNode], format: &OutputFormat) -> Result<String> {
        self.exporter.export(nodes, format)
    }
}

impl Default for SubscriptionService {
    fn default() -> Self {
        Self::new()
    }
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
