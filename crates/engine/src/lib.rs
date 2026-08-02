pub mod vless;
pub mod trojan;
pub mod socks5;
pub mod http_proxy;
pub mod transport;
pub mod dispatcher;
pub mod proxy;

use ironpass_core::{Result, models::ProxyNode};

/// Install the ring crypto provider for rustls (must be called once before any TLS usage)
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub node: ProxyNode,
    pub local_socks_port: u16,
    pub local_http_port: u16,
    pub dns_port: u16,
}

pub struct ProxyEngine {
    config: ProxyConfig,
    shutdown: tokio::sync::broadcast::Sender<()>,
}

impl ProxyEngine {
    pub fn new(config: ProxyConfig) -> Self {
        let (shutdown, _) = tokio::sync::broadcast::channel(1);
        Self { config, shutdown }
    }

    pub async fn start(&self) -> Result<()> {
        tracing::info!("Starting proxy engine");
        tracing::info!("Node: {} -> {}:{}", self.config.node.name, self.config.node.server, self.config.node.port);
        tracing::info!("SOCKS5: 127.0.0.1:{}", self.config.local_socks_port);
        tracing::info!("HTTP:   127.0.0.1:{}", self.config.local_http_port);

        let node = self.config.node.clone();
        let socks_port = self.config.local_socks_port;
        let http_port = self.config.local_http_port;
        let shutdown_rx = self.shutdown.subscribe();

        let socks_handle = {
            let node = node.clone();
            tokio::spawn(async move {
                if let Err(e) = socks5::run_socks_server(node, socks_port, shutdown_rx).await {
                    tracing::error!("SOCKS5 server error: {}", e);
                }
            })
        };

        let http_handle = {
            let node = node.clone();
            let shutdown_rx = self.shutdown.subscribe();
            tokio::spawn(async move {
                if let Err(e) = http_proxy::run_http_server(node, http_port, shutdown_rx).await {
                    tracing::error!("HTTP proxy server error: {}", e);
                }
            })
        };

        tracing::info!("Proxy engine started");

        let mut shutdown_rx = self.shutdown.subscribe();
        let _ = shutdown_rx.recv().await;

        socks_handle.abort();
        http_handle.abort();
        tracing::info!("Proxy engine stopped");
        Ok(())
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown.send(());
    }
}
