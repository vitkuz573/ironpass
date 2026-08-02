use ironpass_core::{Error, Result};
use reqwest::{Client, RequestBuilder};
use std::time::Duration;
use tracing::{info, warn, debug};

#[derive(Debug, Clone)]
pub struct TransportConfig {
    pub user_agent: String,
    pub timeout: Duration,
    pub max_retries: u32,
    pub proxy: Option<String>,
    pub extra_headers: Vec<(String, String)>,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            user_agent: "v2rayN/6.0".into(),
            timeout: Duration::from_secs(30),
            max_retries: 3,
            proxy: None,
            extra_headers: Vec::new(),
        }
    }
}

pub struct HttpTransport {
    client: Client,
    config: TransportConfig,
}

impl HttpTransport {
    pub fn new(config: TransportConfig) -> Result<Self> {
        let mut builder = Client::builder()
            .user_agent(&config.user_agent)
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::limited(10))
            .connect_timeout(Duration::from_secs(10));

        if let Some(ref proxy) = config.proxy {
            let proxy_req = reqwest::Proxy::all(proxy)
                .map_err(|e| Error::Config(format!("Invalid proxy URL: {}", e)))?;
            builder = builder.proxy(proxy_req);
        }

        let client = builder
            .build()
            .map_err(|e| Error::Config(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { client, config })
    }

    pub fn get(&self, url: &str) -> RequestBuilder {
        let mut request = self.client.get(url);

        for (key, value) in &self.config.extra_headers {
            request = request.header(key.as_str(), value.as_str());
        }

        request
    }

    pub async fn fetch_with_retry(&self, url: &str, headers: &[(&str, &str)]) -> Result<String> {
        let mut last_error = None;

        for attempt in 1..=self.config.max_retries {
            debug!("Attempt {}/{} for {}", attempt, self.config.max_retries, url);

            let mut request = self.client.get(url);

            for (key, value) in headers {
                request = request.header(*key, *value);
            }

            for (key, value) in &self.config.extra_headers {
                request = request.header(key.as_str(), value.as_str());
            }

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body = response.text().await?;
                        return Ok(body);
                    }

                    warn!(
                        "HTTP {} on attempt {}/{}",
                        status, attempt, self.config.max_retries
                    );

                    last_error = Some(Error::Custom(format!("HTTP {}", status)));
                }
                Err(e) => {
                    warn!(
                        "Request error on attempt {}/{}: {}",
                        attempt, self.config.max_retries, e
                    );
                    last_error = Some(Error::Network(e));
                }
            }

            if attempt < self.config.max_retries {
                let delay = Duration::from_secs(2u64.pow(attempt - 1));
                info!("Retrying in {}ms...", delay.as_millis());
                tokio::time::sleep(delay).await;
            }
        }

        Err(last_error.unwrap_or_else(|| Error::Custom("All retries exhausted".into())))
    }

    pub async fn fetch_with_hwid(&self, url: &str, hwid: &str) -> Result<String> {
        self.fetch_with_retry(
            url,
            &[
                ("x-hwid", hwid),
                ("x-device-model", "IronPass/CLI"),
            ],
        )
        .await
    }
}
