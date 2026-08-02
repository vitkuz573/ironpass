use crate::models::{OutputFormat, ProxyNode, Subscription};
use async_trait::async_trait;

#[async_trait]
pub trait SubscriptionFetcher: Send + Sync {
    async fn fetch(&self, url: &str, hwid: Option<&str>) -> crate::Result<Subscription>;
}

pub trait NodeParser: Send + Sync {
    fn parse(&self, input: &str) -> crate::Result<Vec<ProxyNode>>;
    fn detect_format(&self, input: &str) -> crate::models::SubscriptionFormat;
}

pub trait NodeExporter: Send + Sync {
    fn export(&self, nodes: &[ProxyNode], format: &OutputFormat) -> crate::Result<String>;
}

pub trait HwidProvider: Send + Sync {
    fn generate(&self) -> crate::Result<String>;
    fn get_device_info(&self) -> crate::Result<crate::models::HwidInfo>;
}
