use color_eyre::eyre;
use ironpass_config::ConfigManager;
use ironpass_core::traits::HwidProvider;
use ironpass_subscription::SubscriptionService;
use ironpass_subscription::is_placeholder_node;

pub async fn handle(
    url: Option<String>,
    _probe: bool,
    detailed: bool,
    json: bool,
) -> eyre::Result<()> {
    let config = ConfigManager::new().load_config()?;
    let fetch_url = url.or_else(|| config.subscription.default_url.clone())
        .ok_or_else(|| eyre::eyre!("No URL provided"))?;

    let hwid = if config.hwid.enabled {
        ironpass_hwid::SystemHwidProvider::new().generate().ok()
    } else {
        None
    };

    let service = SubscriptionService::new();
    let sub = service.fetch_and_parse(&fetch_url, hwid.as_deref()).await?;

    let nodes = &sub.nodes;
    let real: Vec<_> = nodes.iter()
        .filter(|n| !is_placeholder_node(n))
        .collect();
    let placeholders: Vec<_> = nodes.iter()
        .filter(|n| is_placeholder_node(n))
        .collect();

    let mut protocols = std::collections::HashMap::new();
    let mut transports = std::collections::HashMap::new();
    let mut securities = std::collections::HashMap::new();

    for node in &real {
        *protocols.entry(format!("{:?}", node.protocol)).or_insert(0) += 1;
        *transports.entry(format!("{:?}", node.transport)).or_insert(0) += 1;
        *securities.entry(format!("{:?}", node.security)).or_insert(0) += 1;
    }

    if json {
        println!("{}", serde_json::json!({
            "total_nodes": nodes.len(),
            "real_nodes": real.len(),
            "placeholder_nodes": placeholders.len(),
            "protocols": protocols,
            "transports": transports,
            "securities": securities,
            "traffic_used": sub.traffic_used,
            "traffic_total": sub.traffic_total,
            "fetched_at": sub.fetched_at,
        }));
    } else {
        println!("=== Subscription Analysis ===");
        println!("URL:         {}", sub.url);
        println!("Fetched at:  {}", sub.fetched_at.format("%Y-%m-%d %H:%M:%S UTC"));
        println!();
        println!("Nodes:       {} total ({} real, {} placeholder)", nodes.len(), real.len(), placeholders.len());

        if let Some(used) = sub.traffic_used {
            println!("Traffic:     {} used", bytesize::to_string(used, true));
        }
        if let Some(total) = sub.traffic_total {
            println!("  Total:     {}", bytesize::to_string(total, true));
        }

        if !protocols.is_empty() {
            println!();
            println!("Protocols:");
            for (proto, count) in &protocols {
                println!("  {:<15} {}", proto, count);
            }
        }

        if !transports.is_empty() {
            println!();
            println!("Transports:");
            for (tr, count) in &transports {
                println!("  {:<15} {}", tr, count);
            }
        }

        if !securities.is_empty() {
            println!();
            println!("Security:");
            for (sec, count) in &securities {
                println!("  {:<15} {}", sec, count);
            }
        }

        if detailed && !real.is_empty() {
            println!();
            println!("=== Real Nodes ===");
            for node in &real {
                println!();
                println!("  Name:       {}", node.name);
                println!("  Server:     {}:{}", node.server, node.port);
                println!("  Protocol:   {:?}", node.protocol);
                println!("  Transport:  {:?}", node.transport);
                println!("  Security:   {:?}", node.security);
                if let Some(ref sni) = node.sni {
                    println!("  SNI:        {}", sni);
                }
                if let Some(ref fp) = node.fingerprint {
                    println!("  FP:         {}", fp);
                }
                if let Some(ref flow) = node.flow {
                    println!("  Flow:       {}", flow);
                }
            }
        }
    }

    Ok(())
}
