use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ironpass_core::{Error, Result, models::*, traits::*};
use tracing::{info, warn};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Controls optional behaviour of the HTTP subscription fetcher.
///
/// Use this structure to enable or disable automatic HWID retry and to tune the
/// number of retries performed when a provider responds with placeholder nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchOptions {
    /// Automatically generate a HWID and retry when the server returns placeholders.
    pub auto_hwid_retry: bool,
    /// Maximum number of HWID retries. Defaults to 1.
    pub max_hwid_retries: usize,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            auto_hwid_retry: true,
            max_hwid_retries: 1,
        }
    }
}

/// HTTP implementation of [`SubscriptionFetcher`] with dependency-injected
/// [`reqwest::Client`] and [`HwidProvider`] for testability.
///
/// `HttpSubscriptionFetcher` is responsible for:
///
/// 1. Building the outbound HTTP request, optionally injecting HWID and device-info
///    headers.
/// 2. Executing the request and validating the HTTP status.
/// 3. Parsing the response body into proxy nodes and extracting traffic / metadata.
/// 4. Applying the HWID retry policy when placeholder-only responses are received.
pub struct HttpSubscriptionFetcher {
    client: reqwest::Client,
    hwid_provider: Box<dyn HwidProvider>,
    options: FetchOptions,
}

impl HttpSubscriptionFetcher {
    /// Create a fetcher using the default HTTP client and the system HWID provider.
    pub fn new() -> Self {
        let user_agent = format!("IronPass/{}", VERSION);
        let client = reqwest::Client::builder()
            .user_agent(&user_agent)
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("Failed to create HTTP client");

        Self::with_client(client, FetchOptions::default())
    }

    /// Create a fetcher with a fully custom HTTP client, HWID provider and options.
    pub fn with_client_and_provider(
        client: reqwest::Client,
        hwid_provider: Box<dyn HwidProvider>,
        options: FetchOptions,
    ) -> Self {
        Self {
            client,
            hwid_provider,
            options,
        }
    }

    /// Create a fetcher with a custom HTTP client and options, still using the system HWID provider.
    pub fn with_client(client: reqwest::Client, options: FetchOptions) -> Self {
        Self {
            client,
            hwid_provider: Box::new(ironpass_hwid::SystemHwidProvider::new()),
            options,
        }
    }

    /// Build the [`reqwest::RequestBuilder`] for a fetch attempt.
    fn build_request(&self, url: &str, hwid: Option<&str>) -> Result<reqwest::RequestBuilder> {
        info!("Building request for: {}", mask_url(url));

        let mut request = self.client.get(url);

        if let Some(id) = hwid {
            request = request.header("x-hwid", id);

            let info = self.hwid_provider.get_device_info().ok();

            if let Some(ref info) = info {
                let os_short = info
                    .os
                    .split('(')
                    .next()
                    .unwrap_or(&info.os)
                    .trim()
                    .to_string();
                let ua = format!("IronPass/{} ({})", VERSION, os_short);

                request = request.header("x-device-model", &info.device_model);
                request = request.header("x-device-os", &os_short);
                request = request.header("x-ver-os", &info.os);
                request = request.header("User-Agent", &ua);

                info!("Sending HWID: {}...", &id[..id.len().min(16)]);
                info!("Device: {}", info.device_model);
                info!("OS: {} (short: {})", info.os, os_short);
                info!("UA: {}", ua);
            } else {
                warn!("HWID provided but device info unavailable");
            }
        } else {
            warn!("No HWID provided — server may return placeholder nodes");
        }

        Ok(request)
    }

    /// Execute the HTTP request and materialise a raw response.
    async fn execute_request(&self, request: reqwest::RequestBuilder) -> Result<HttpResponse> {
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

        Ok(HttpResponse { headers, body })
    }

    /// Parse the response body into a list of [`ProxyNode`]s and traffic metadata.
    fn parse_response(&self, url: &str, response: HttpResponse) -> Result<ParsedResponse> {
        let userinfo = response
            .headers
            .get("subscription-userinfo")
            .and_then(|v| v.to_str().ok());

        let (traffic_used, traffic_total, expires_at) =
            userinfo.map(parse_subscription_info).unwrap_or_default();

        let parser = super::parser::SubscriptionParser::new();
        let format = parser.detect_format(&response.body);
        let nodes = parser.parse(&response.body)?;

        let placeholder_count = nodes.iter().filter(|n| is_placeholder_node(n)).count();
        let all_placeholders = !nodes.is_empty() && placeholder_count == nodes.len();

        if all_placeholders {
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

        let header_metadata = extract_header_metadata(&response.headers);
        let inline_metadata = extract_inline_metadata(&response.body);
        let metadata = merge_metadata(inline_metadata, header_metadata);

        Ok(ParsedResponse {
            url: url.to_string(),
            nodes,
            all_placeholders,
            hwid_limit: is_hwid_limit(&response.headers),
            traffic_used,
            traffic_total,
            expires_at,
            metadata,
        })
    }

    /// Apply the HWID retry policy.
    ///
    /// When no explicit HWID was supplied, the response only contains placeholder nodes,
    /// and retrying is enabled, generate a HWID and retry up to `max_hwid_retries` times.
    async fn apply_retry_policy(
        &self,
        url: &str,
        supplied_hwid: Option<&str>,
        parsed: ParsedResponse,
    ) -> Result<Subscription> {
        if parsed.hwid_limit {
            warn!("Server reported HWID device limit");
            return Err(Error::DeviceLimitExceeded {
                current: parsed.nodes.len(),
                limit: 1,
            });
        }

        if supplied_hwid.is_none() && parsed.all_placeholders && self.options.auto_hwid_retry {
            let mut last_error: Option<Error> = None;

            for attempt in 1..=self.options.max_hwid_retries {
                info!(
                    "HWID retry attempt {}/{}",
                    attempt, self.options.max_hwid_retries
                );

                let generated = match self.hwid_provider.generate() {
                    Ok(id) => id,
                    Err(e) => {
                        warn!("Failed to generate HWID: {}", e);
                        last_error = Some(e);
                        continue;
                    }
                };

                let request = self.build_request(url, Some(&generated))?;
                let response = match self.execute_request(request).await {
                    Ok(r) => r,
                    Err(e) => {
                        warn!("HWID retry request failed: {}", e);
                        last_error = Some(e);
                        continue;
                    }
                };

                let retry_parsed = match self.parse_response(url, response) {
                    Ok(r) => r,
                    Err(e) => {
                        warn!("HWID retry response parsing failed: {}", e);
                        last_error = Some(e);
                        continue;
                    }
                };

                if retry_parsed.hwid_limit {
                    warn!("Server reported HWID device limit on retry");
                    return Err(Error::DeviceLimitExceeded {
                        current: retry_parsed.nodes.len(),
                        limit: 1,
                    });
                }

                if !retry_parsed.all_placeholders {
                    return Ok(self.into_subscription(retry_parsed));
                }

                warn!("Retry {} still returned placeholders", attempt);
                last_error = Some(Error::Custom(
                    "Server returned placeholder nodes after HWID retry".to_string(),
                ));
            }

            return Err(last_error.unwrap_or_else(|| {
                Error::Custom("HWID retry exhausted without success".to_string())
            }));
        }

        if parsed.all_placeholders {
            warn!("Placeholder response and HWID retry is disabled or already supplied");
            return Err(Error::Custom(
                "Subscription returned only placeholder nodes".to_string(),
            ));
        }

        Ok(self.into_subscription(parsed))
    }

    fn into_subscription(&self, parsed: ParsedResponse) -> Subscription {
        Subscription {
            id: uuid::Uuid::new_v4(),
            url: parsed.url,
            name: None,
            nodes: parsed.nodes,
            fetched_at: chrono::Utc::now(),
            expires_at: parsed.expires_at,
            traffic_used: parsed.traffic_used,
            traffic_total: parsed.traffic_total,
            metadata: parsed.metadata,
        }
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

        let request = self.build_request(url, hwid)?;
        let response = self.execute_request(request).await?;
        let parsed = self.parse_response(url, response)?;
        self.apply_retry_policy(url, hwid, parsed).await
    }
}

struct HttpResponse {
    headers: reqwest::header::HeaderMap,
    body: String,
}

struct ParsedResponse {
    url: String,
    nodes: Vec<ProxyNode>,
    all_placeholders: bool,
    hwid_limit: bool,
    traffic_used: Option<u64>,
    traffic_total: Option<u64>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    metadata: SubscriptionMetadata,
}

fn is_hwid_limit(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get("x-hwid-limit")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

use std::collections::HashSet;
use std::net::IpAddr;
use uuid::Uuid;

/// Configurable policy for detecting placeholder / sentinel proxy nodes.
///
/// A placeholder is a node returned by a provider that is not intended for actual
/// use — for example a node with address `0.0.0.0`, port `0` or the nil UUID.
/// Providers emit placeholders when a subscription requires HWID activation or when
/// the device limit has been reached.
///
/// The policy supports both hard sentinels (always treated as placeholders) and a
/// scoring system where a node must match at least `score_threshold` independent
/// criteria. Use [`PlaceholderPolicy::default`] for conservative detection that
/// matches the historical behaviour of [`is_placeholder_node`], or
/// [`PlaceholderPolicy::strict`] for enterprise environments where loopback and
/// common sentinel domains should also be rejected.
#[derive(Debug, Clone)]
pub struct PlaceholderPolicy {
    dummy_addresses: HashSet<String>,
    dummy_address_prefixes: Vec<String>,
    dummy_ports: HashSet<u16>,
    dummy_uuids: HashSet<Uuid>,
    sentinel_domains: HashSet<String>,
    /// Minimum number of independent criteria that must match for a node to be
    /// flagged as a placeholder, unless a hard sentinel is matched first.
    score_threshold: usize,
}

impl PlaceholderPolicy {
    /// Create an empty policy with a given scoring threshold.
    fn with_threshold(score_threshold: usize) -> Self {
        Self {
            dummy_addresses: HashSet::new(),
            dummy_address_prefixes: Vec::new(),
            dummy_ports: HashSet::new(),
            dummy_uuids: HashSet::new(),
            sentinel_domains: HashSet::new(),
            score_threshold,
        }
    }

    /// Conservative default matching the historical behavior of [`is_placeholder_node`].
    pub fn default() -> Self {
        let zero_uuid = Uuid::nil();
        let mut policy = Self::with_threshold(2);

        for addr in ["0.0.0.0"] {
            policy.dummy_addresses.insert(addr.to_string());
        }
        for prefix in ["0."] {
            policy.dummy_address_prefixes.push(prefix.to_string());
        }
        for port in [0u16, 1] {
            policy.dummy_ports.insert(port);
        }
        policy.dummy_uuids.insert(zero_uuid);

        policy
    }

    /// Strict enterprise policy that catches common provider sentinel values.
    pub fn strict() -> Self {
        let zero_uuid = Uuid::nil();
        let mut policy = Self::with_threshold(2);

        for addr in [
            "0.0.0.0",
            "127.0.0.1",
            "::1",
            "::",
            "localhost",
            "example.com",
            "test.com",
            "invalid",
        ] {
            policy.dummy_addresses.insert(addr.to_string());
        }
        for prefix in ["0."] {
            policy.dummy_address_prefixes.push(prefix.to_string());
        }
        for port in [0u16, 1, 2, 3, 80, 8080] {
            policy.dummy_ports.insert(port);
        }
        policy.dummy_uuids.insert(zero_uuid);
        for domain in ["example.com", "test.com", "invalid", "localhost"] {
            policy.sentinel_domains.insert(domain.to_string());
        }

        policy
    }

    /// Add a literal dummy address (e.g. `"0.0.0.0"`).
    pub fn add_dummy_address(&mut self, addr: &str) {
        self.dummy_addresses.insert(addr.to_lowercase());
    }

    /// Add a dummy UUID sentinel (in addition to the nil UUID).
    pub fn add_dummy_uuid(&mut self, uuid: Uuid) {
        self.dummy_uuids.insert(uuid);
    }

    /// Returns true if the node is considered a placeholder under this policy.
    pub fn is_placeholder(&self, node: &ProxyNode) -> bool {
        let hard_sentinel = self.is_hard_sentinel(node);
        if hard_sentinel {
            return true;
        }

        let score = self.score(node);
        score >= self.score_threshold
    }

    /// Hard sentinels always flag a node regardless of scoring.
    fn is_hard_sentinel(&self, node: &ProxyNode) -> bool {
        if self.is_zero_address(&node.server) {
            return true;
        }

        if self.dummy_ports.contains(&node.port) && (node.port == 0 || node.port == 1) {
            return true;
        }

        if let Some(uuid_str) = node.uuid.as_deref() {
            if let Ok(uuid) = Uuid::parse_str(uuid_str) {
                if self.dummy_uuids.contains(&uuid) {
                    return true;
                }
            }
        }

        if self.is_user_dummy_address(&node.server) {
            return true;
        }

        false
    }

    /// Addresses explicitly added via [`PlaceholderPolicy::add_dummy_address`].
    fn is_user_dummy_address(&self, addr: &str) -> bool {
        let lower = addr.to_lowercase();
        self.dummy_addresses.contains(&lower) && !self.is_built_in_dummy_address(addr)
    }

    fn is_built_in_dummy_address(&self, addr: &str) -> bool {
        matches!(addr, "127.0.0.1" | "::1" | "::" | "localhost")
    }

    /// Count independent criteria matched by the node.
    fn score(&self, node: &ProxyNode) -> usize {
        let mut score = 0;

        if self.is_dummy_address(&node.server) {
            score += 1;
        }

        if self.dummy_ports.contains(&node.port) {
            score += 1;
        }

        if let Some(uuid_str) = node.uuid.as_deref() {
            if let Ok(uuid) = Uuid::parse_str(uuid_str) {
                if self.dummy_uuids.contains(&uuid) {
                    score += 1;
                }
            }
        }

        if self.is_sentinel_domain(&node.server) {
            score += 1;
        }

        score
    }

    fn is_zero_address(&self, addr: &str) -> bool {
        addr == "0.0.0.0"
    }

    fn is_dummy_address(&self, addr: &str) -> bool {
        let lower = addr.to_lowercase();
        if self.dummy_addresses.contains(&lower) {
            return true;
        }
        if self
            .dummy_address_prefixes
            .iter()
            .any(|p| lower.starts_with(p))
        {
            return true;
        }
        if let Ok(ip) = addr.parse::<IpAddr>() {
            if ip.is_loopback() || ip.is_unspecified() {
                return true;
            }
        }
        false
    }

    fn is_sentinel_domain(&self, addr: &str) -> bool {
        let lower = addr.to_lowercase();
        self.sentinel_domains.contains(&lower)
    }
}

impl Default for PlaceholderPolicy {
    fn default() -> Self {
        Self::default()
    }
}

/// Placeholder detection using the default [`PlaceholderPolicy`].
///
/// This is the convenience entry point used by the CLI and exporters to filter out
/// sentinel nodes. For configurable detection (e.g. enterprise strict mode or custom
/// sentinel values), build a [`PlaceholderPolicy`] directly.
pub fn is_placeholder_node(node: &ProxyNode) -> bool {
    PlaceholderPolicy::default().is_placeholder(node)
}

/// Return the display names of all placeholder nodes in the slice.
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

/// Parse the `subscription-userinfo` header value.
///
/// The expected format is a semicolon-separated list of `key=value` pairs such as
/// `upload=0; download=205542220; total=322122547200; expire=1786355700`. The
/// function returns `(used_bytes, total_bytes, expires_at)` where `used` is the
/// sum of upload and download. Missing numeric fields default to zero; missing or
/// invalid `expire` returns `None`.
fn parse_subscription_info(
    info: &str,
) -> (
    Option<u64>,
    Option<u64>,
    Option<chrono::DateTime<chrono::Utc>>,
) {
    let mut upload: Option<u64> = None;
    let mut download: Option<u64> = None;
    let mut total: Option<u64> = None;
    let mut expire: Option<i64> = None;

    for part in info.split(';') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "upload" => upload = value.parse().ok(),
                "download" => download = value.parse().ok(),
                "total" => total = value.parse().ok(),
                "expire" => expire = value.parse().ok(),
                _ => {}
            }
        }
    }

    let used = upload
        .or(Some(0))
        .zip(download.or(Some(0)))
        .map(|(u, d)| u + d);
    let expires_at = expire.and_then(|ts| chrono::DateTime::from_timestamp(ts, 0));

    (used, total, expires_at)
}

/// Decode a value if it is prefixed with `base64:`, otherwise return it unchanged.
fn decode_metadata_value(value: &str) -> String {
    let value = value.trim();
    if let Some(encoded) = value.strip_prefix("base64:") {
        STANDARD
            .decode(encoded.trim())
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_else(|| value.to_string())
    } else {
        value.to_string()
    }
}

/// Extract interesting subscription metadata from HTTP response headers.
///
/// Base64-encoded header values (prefixed with `base64:`) are decoded automatically.
/// All interesting header names and their raw values are also preserved in
/// `metadata.headers`.
fn extract_header_metadata(headers: &reqwest::header::HeaderMap) -> SubscriptionMetadata {
    let mut metadata = SubscriptionMetadata::default();
    let interesting = [
        "profile-title",
        "profile-update-interval",
        "profile-web-page-url",
        "announce",
    ];

    for name in interesting {
        if let Some(value) = headers.get(name).and_then(|v| v.to_str().ok()) {
            let decoded = decode_metadata_value(value);
            metadata.headers.insert(name.to_string(), decoded.clone());
            match name {
                "profile-title" => metadata.profile_title = Some(decoded),
                "profile-update-interval" => {
                    metadata.profile_update_interval_hours = decoded.parse().ok();
                }
                "profile-web-page-url" => metadata.profile_web_page_url = Some(decoded),
                "announce" => metadata.announcement = Some(decoded),
                _ => {}
            }
        }
    }

    metadata
}

/// Extract subscription metadata written as `key=value` lines in the response body.
///
/// Recognised keys are `profile-title`, `profile-update-interval`,
/// `profile-web-page-url`, `announce`, and `announces`. Values prefixed with
/// `base64:` are decoded automatically. The body is base64-decoded first if the
/// entire payload is a valid Base64 string, which is common for subscription
/// providers that encode the whole node list.
///
/// HTTP response headers take precedence over inline body values; callers should
/// merge the result with header-derived metadata, with headers winning.
pub fn extract_inline_metadata(body: &str) -> SubscriptionMetadata {
    let decoded_body = if let Ok(bytes) = STANDARD.decode(body.trim()) {
        String::from_utf8(bytes).unwrap_or_else(|_| body.to_string())
    } else {
        body.to_string()
    };

    let mut metadata = SubscriptionMetadata::default();

    for line in decoded_body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = decode_metadata_value(value);

        match key {
            "profile-title" if metadata.profile_title.is_none() => {
                metadata.profile_title = Some(value);
            }
            "profile-update-interval" if metadata.profile_update_interval_hours.is_none() => {
                metadata.profile_update_interval_hours = value.parse().ok();
            }
            "profile-web-page-url" if metadata.profile_web_page_url.is_none() => {
                metadata.profile_web_page_url = Some(value);
            }
            "announce" | "announces" if metadata.announcement.is_none() => {
                metadata.announcement = Some(value);
            }
            _ => {}
        }
    }

    metadata
}

/// Merge body-derived metadata with header-derived metadata.
///
/// Header values take precedence over inline body values, matching the order
/// used by many providers where headers carry authoritative metadata. Values
/// already decoded in `header_metadata` are kept and any overlapping fields in
/// `body_metadata` are ignored.
fn merge_metadata(
    body_metadata: SubscriptionMetadata,
    header_metadata: SubscriptionMetadata,
) -> SubscriptionMetadata {
    SubscriptionMetadata {
        profile_title: header_metadata
            .profile_title
            .or(body_metadata.profile_title),
        profile_update_interval_hours: header_metadata
            .profile_update_interval_hours
            .or(body_metadata.profile_update_interval_hours),
        profile_web_page_url: header_metadata
            .profile_web_page_url
            .or(body_metadata.profile_web_page_url),
        announcement: header_metadata.announcement.or(body_metadata.announcement),
        headers: header_metadata.headers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_subscription_info_full() {
        let (used, total, expire) = parse_subscription_info(
            "upload=1073741824; download=2147483648; total=10737418240; expire=1893456000",
        );
        assert_eq!(used, Some(3221225472));
        assert_eq!(total, Some(10737418240));
        assert!(expire.is_some());
    }

    #[test]
    fn parse_subscription_info_missing_fields_defaults_to_zero_used() {
        let (used, total, expire) = parse_subscription_info("total=1000");
        assert_eq!(used, Some(0));
        assert_eq!(total, Some(1000));
        assert_eq!(expire, None);
    }

    #[test]
    fn parse_subscription_info_invalid_returns_none() {
        let (used, total, expire) = parse_subscription_info("not-a-valid-header");
        assert_eq!(used, Some(0));
        assert_eq!(total, None);
        assert_eq!(expire, None);
    }

    #[test]
    fn fetch_options_default() {
        let opts = FetchOptions::default();
        assert!(opts.auto_hwid_retry);
        assert_eq!(opts.max_hwid_retries, 1);
    }

    #[test]
    fn placeholder_node_detected_by_uuid() {
        let node = ProxyNode {
            protocol: Protocol::Vless,
            name: "P".into(),
            server: "example.com".into(),
            port: 443,
            uuid: Some("00000000-0000-0000-0000-000000000000".into()),
            ..DefaultPlaceholder::placeholder()
        };
        assert!(is_placeholder_node(&node));
    }

    #[test]
    fn placeholder_node_detected_by_address_and_port() {
        let node = ProxyNode {
            protocol: Protocol::Vless,
            name: "P".into(),
            server: "0.0.0.0".into(),
            port: 1,
            uuid: None,
            ..DefaultPlaceholder::placeholder()
        };
        assert!(is_placeholder_node(&node));
    }

    #[test]
    fn real_node_is_not_placeholder() {
        let node = ProxyNode {
            protocol: Protocol::Vless,
            name: "R".into(),
            server: "example.org".into(),
            port: 443,
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            ..DefaultPlaceholder::placeholder()
        };
        assert!(!is_placeholder_node(&node));
    }

    struct DefaultPlaceholder;

    impl DefaultPlaceholder {
        fn placeholder() -> ProxyNode {
            ProxyNode {
                protocol: Protocol::Vless,
                name: String::new(),
                server: String::new(),
                port: 0,
                uuid: None,
                password: None,
                alter_id: None,
                encryption: None,
                transport: Transport::Tcp,
                security: Security::None,
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
                tags: Vec::new(),
                raw_uri: String::new(),
            }
        }
    }

    #[test]
    fn default_policy_matches_old_behavior_zero_uuid() {
        let node = ProxyNode {
            protocol: Protocol::Vless,
            name: "Z".into(),
            server: "example.com".into(),
            port: 443,
            uuid: Some("00000000-0000-0000-0000-000000000000".into()),
            ..DefaultPlaceholder::placeholder()
        };
        assert!(PlaceholderPolicy::default().is_placeholder(&node));
        assert!(is_placeholder_node(&node));
    }

    #[test]
    fn default_policy_matches_old_behavior_zero_address_and_port() {
        let node = ProxyNode {
            protocol: Protocol::Vless,
            name: "Z".into(),
            server: "0.0.0.0".into(),
            port: 1,
            uuid: None,
            ..DefaultPlaceholder::placeholder()
        };
        assert!(PlaceholderPolicy::default().is_placeholder(&node));
        assert!(is_placeholder_node(&node));
    }

    #[test]
    fn default_policy_allows_real_node() {
        let node = ProxyNode {
            protocol: Protocol::Vless,
            name: "R".into(),
            server: "example.org".into(),
            port: 443,
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            ..DefaultPlaceholder::placeholder()
        };
        assert!(!PlaceholderPolicy::default().is_placeholder(&node));
        assert!(!is_placeholder_node(&node));
    }

    #[test]
    fn strict_policy_catches_localhost_and_low_ports() {
        let node = ProxyNode {
            protocol: Protocol::Vless,
            name: "P".into(),
            server: "127.0.0.1".into(),
            port: 1,
            uuid: None,
            ..DefaultPlaceholder::placeholder()
        };
        assert!(PlaceholderPolicy::strict().is_placeholder(&node));
    }

    #[test]
    fn strict_policy_catches_sentinel_domain() {
        let node = ProxyNode {
            protocol: Protocol::Vless,
            name: "P".into(),
            server: "test.com".into(),
            port: 443,
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            ..DefaultPlaceholder::placeholder()
        };
        assert!(PlaceholderPolicy::strict().is_placeholder(&node));
    }

    #[test]
    fn scoring_avoids_false_positives_for_localhost_with_real_uuid() {
        let node = ProxyNode {
            protocol: Protocol::Vless,
            name: "L".into(),
            server: "127.0.0.1".into(),
            port: 8080,
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            ..DefaultPlaceholder::placeholder()
        };
        assert!(!PlaceholderPolicy::default().is_placeholder(&node));
        assert!(PlaceholderPolicy::strict().is_placeholder(&node));
    }

    #[test]
    fn zero_uuid_is_always_placeholder() {
        let node = ProxyNode {
            protocol: Protocol::Vless,
            name: "Z".into(),
            server: "real-provider.example.org".into(),
            port: 443,
            uuid: Some("00000000-0000-0000-0000-000000000000".into()),
            ..DefaultPlaceholder::placeholder()
        };
        assert!(PlaceholderPolicy::default().is_placeholder(&node));
        assert!(PlaceholderPolicy::strict().is_placeholder(&node));
    }

    #[test]
    fn zero_address_is_always_placeholder() {
        let node = ProxyNode {
            protocol: Protocol::Vless,
            name: "Z".into(),
            server: "0.0.0.0".into(),
            port: 443,
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            ..DefaultPlaceholder::placeholder()
        };
        assert!(PlaceholderPolicy::default().is_placeholder(&node));
        assert!(PlaceholderPolicy::strict().is_placeholder(&node));
    }

    #[test]
    fn custom_policy_additions_work() {
        let mut policy = PlaceholderPolicy::default();
        let custom_uuid = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        policy.add_dummy_uuid(custom_uuid);
        policy.add_dummy_address("placeholder.invalid");

        let node_with_uuid = ProxyNode {
            protocol: Protocol::Vless,
            name: "C".into(),
            server: "example.org".into(),
            port: 443,
            uuid: Some("11111111-1111-1111-1111-111111111111".into()),
            ..DefaultPlaceholder::placeholder()
        };
        let node_with_addr = ProxyNode {
            protocol: Protocol::Vless,
            name: "C".into(),
            server: "placeholder.invalid".into(),
            port: 443,
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            ..DefaultPlaceholder::placeholder()
        };

        assert!(policy.is_placeholder(&node_with_uuid));
        assert!(policy.is_placeholder(&node_with_addr));
        assert!(!PlaceholderPolicy::default().is_placeholder(&node_with_uuid));
        assert!(!PlaceholderPolicy::default().is_placeholder(&node_with_addr));
    }

    #[test]
    fn port_zero_or_one_are_always_placeholders() {
        let node_port_zero = ProxyNode {
            protocol: Protocol::Vless,
            name: "Z".into(),
            server: "example.org".into(),
            port: 0,
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            ..DefaultPlaceholder::placeholder()
        };
        let node_port_one = ProxyNode {
            protocol: Protocol::Vless,
            name: "Z".into(),
            server: "example.org".into(),
            port: 1,
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            ..DefaultPlaceholder::placeholder()
        };
        assert!(PlaceholderPolicy::default().is_placeholder(&node_port_zero));
        assert!(PlaceholderPolicy::default().is_placeholder(&node_port_one));
    }

    #[test]
    fn single_criterion_is_not_placeholder_by_default() {
        let node = ProxyNode {
            protocol: Protocol::Vless,
            name: "B".into(),
            server: "example.com".into(),
            port: 443,
            uuid: Some("00000000-0000-0000-0000-000000000001".into()),
            ..DefaultPlaceholder::placeholder()
        };
        assert!(!PlaceholderPolicy::default().is_placeholder(&node));
    }
}
