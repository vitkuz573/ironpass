//! Convenience facade that combines subscription fetching, parsing and exporting.

use ironpass_core::{Result, models::*, traits::*};

/// Convenience facade that combines subscription fetching, parsing and exporting.
///
/// [`SubscriptionService::new`] builds a service with sensible defaults: a 30-second
/// HTTP timeout, limited redirects and automatic HWID retry on placeholder responses.
/// Use [`SubscriptionService::with_fetch_options`] to customise retry behaviour.
pub struct SubscriptionService {
    fetcher: crate::fetcher::HttpSubscriptionFetcher,
    parser: crate::parser::SubscriptionParser,
    exporter: crate::exporter::NodeExporterImpl,
}

impl SubscriptionService {
    /// Create a new service using the default [`FetchOptions`].
    pub fn new() -> Self {
        Self::with_fetch_options(crate::fetcher::FetchOptions::default())
    }

    /// Create a new service with custom fetcher options.
    ///
    /// This is the recommended way to disable automatic HWID retries or change the
    /// maximum number of retries.
    pub fn with_fetch_options(options: crate::fetcher::FetchOptions) -> Self {
        Self {
            fetcher: crate::fetcher::HttpSubscriptionFetcher::with_client(
                default_http_client(),
                options,
            ),
            parser: crate::parser::SubscriptionParser::new(),
            exporter: crate::exporter::NodeExporterImpl::new(),
        }
    }

    /// Create a service from an existing HTTP client.
    pub fn with_fetcher(fetcher: crate::fetcher::HttpSubscriptionFetcher) -> Self {
        Self {
            fetcher,
            parser: crate::parser::SubscriptionParser::new(),
            exporter: crate::exporter::NodeExporterImpl::new(),
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
