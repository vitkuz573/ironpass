use ironpass_core::{Result, models::*, traits::*};

mod fetcher;
mod parser;
mod exporter;
mod converters;

pub use fetcher::{HttpSubscriptionFetcher, is_placeholder_node, placeholder_messages};
pub use parser::SubscriptionParser;
pub use exporter::NodeExporterImpl;

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
