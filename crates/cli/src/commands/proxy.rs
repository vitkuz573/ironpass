use color_eyre::eyre;
use ironpass_config::ConfigManager;
use ironpass_core::traits::HwidProvider;
use ironpass_subscription::SubscriptionService;
use ironpass_subscription::is_placeholder_node;
use ironpass_engine::{ProxyConfig, ProxyEngine};

pub async fn handle(
    manager: &ConfigManager,
    url: Option<String>,
    node_index: Option<usize>,
    socks_port: u16,
    http_port: u16,
    hwid_override: Option<String>,
) -> eyre::Result<()> {
    // Install ring crypto provider for rustls
    ironpass_engine::install_crypto_provider();

    let config = manager.load_config()?;
    let fetch_url = url.or_else(|| config.subscription.default_url.clone())
        .ok_or_else(|| eyre::eyre!("No URL provided"))?;

    println!("Fetching subscription...");

    let hwid = hwid_override.or_else(|| {
        if config.hwid.enabled {
            ironpass_hwid::SystemHwidProvider::new().generate().ok()
        } else {
            None
        }
    });

    let service = SubscriptionService::new();
    let sub = service.fetch_and_parse(&fetch_url, hwid.as_deref()).await?;

    let real_nodes: Vec<_> = sub.nodes.into_iter()
        .filter(|n| !is_placeholder_node(n))
        .collect();

    if real_nodes.is_empty() {
        return Err(eyre::eyre!("No real nodes available"));
    }

    let idx = node_index.unwrap_or(0);
    if idx >= real_nodes.len() {
        return Err(eyre::eyre!(
            "Node index {} out of range (0..{})",
            idx, real_nodes.len()
        ));
    }

    let selected = &real_nodes[idx];

    println!("Subscription parsed successfully: {} real node(s)", real_nodes.len());
    println!();
    println!("Selected node (#{}):", idx);
    println!("  Name:      {}", selected.name);
    println!("  Endpoint:  {}:{}", selected.server, selected.port);
    println!("  Protocol:  {:?}", selected.protocol);
    println!("  Transport: {:?}", selected.transport);
    println!("  Security:  {:?}", selected.security);
    if let Some(ref sni) = selected.sni {
        println!("  SNI:       {}", sni);
    }
    if let Some(ref host) = selected.host {
        println!("  Host:      {}", host);
    }
    if let Some(ref path) = selected.path {
        println!("  Path:      {}", path);
    }
    if let Some(ref flow) = selected.flow {
        println!("  Flow:      {}", flow);
    }
    println!();
    println!("Starting proxy engine...");
    println!("  SOCKS5:  127.0.0.1:{}", socks_port);
    println!("  HTTP:    127.0.0.1:{}", http_port);
    println!();
    println!("Usage:");
    println!("  curl -x socks5h://127.0.0.1:{} https://httpbin.org/ip", socks_port);
    println!("  curl -x http://127.0.0.1:{} https://httpbin.org/ip", http_port);
    println!();
    println!("Press Ctrl+C to stop.");

    let proxy_config = ProxyConfig {
        node: selected.clone(),
        local_socks_port: socks_port,
        local_http_port: http_port,
        dns_port: 5353,
    };

    let engine = ProxyEngine::new(proxy_config);
    engine.start().await?;

    Ok(())
}
