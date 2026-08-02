use color_eyre::eyre;
use ironpass_api_client::ApiClient;
use ironpass_subscription::is_placeholder_node;

pub async fn handle(api_url: &str, target: Option<String>, json: bool) -> eyre::Result<()> {
    let client = ApiClient::with_url(api_url.into());

    let id = match target {
        Some(t) if t.starts_with("http://") || t.starts_with("https://") => {
            // Direct URL fetch.
            let service = ironpass_subscription::SubscriptionService::new();
            let sub = service.fetch_and_parse(&t, None).await?;
            print_analysis(
                &sub.url,
                &sub.nodes,
                sub.traffic_used,
                sub.traffic_total,
                sub.fetched_at,
                json,
            )?;
            return Ok(());
        }
        Some(t) => resolve_subscription_id(&client, &t).await?,
        None => {
            let subs = client.list_subscriptions().await?;
            subs.into_iter()
                .find(|s| s.is_active)
                .map(|s| s.id)
                .ok_or_else(|| eyre::eyre!("No subscriptions saved."))?
        }
    };

    let detail = client.get_subscription(id).await?;
    print_analysis(
        &detail.subscription.url,
        &detail
            .nodes
            .iter()
            .map(|n| n.node.clone())
            .collect::<Vec<_>>(),
        detail.subscription.traffic_used,
        detail.subscription.traffic_total,
        detail
            .subscription
            .last_updated
            .unwrap_or(detail.subscription.added_at),
        json,
    )?;

    Ok(())
}

fn print_analysis(
    url: &str,
    nodes: &[ironpass_core::models::ProxyNode],
    traffic_used: Option<u64>,
    traffic_total: Option<u64>,
    fetched_at: chrono::DateTime<chrono::Utc>,
    json: bool,
) -> eyre::Result<()> {
    let real: Vec<_> = nodes.iter().filter(|n| !is_placeholder_node(n)).collect();
    let placeholders: Vec<_> = nodes.iter().filter(|n| is_placeholder_node(n)).collect();

    let mut protocols = std::collections::HashMap::new();
    let mut transports = std::collections::HashMap::new();
    let mut securities = std::collections::HashMap::new();

    for node in &real {
        *protocols.entry(format!("{:?}", node.protocol)).or_insert(0) += 1;
        *transports
            .entry(format!("{:?}", node.transport))
            .or_insert(0) += 1;
        *securities
            .entry(format!("{:?}", node.security))
            .or_insert(0) += 1;
    }

    if json {
        println!(
            "{}",
            serde_json::json!({
                "total_nodes": nodes.len(),
                "real_nodes": real.len(),
                "placeholder_nodes": placeholders.len(),
                "protocols": protocols,
                "transports": transports,
                "securities": securities,
                "traffic_used": traffic_used,
                "traffic_total": traffic_total,
                "fetched_at": fetched_at,
            })
        );
    } else {
        println!("=== Subscription Analysis ===");
        println!("URL:         {}", url);
        println!(
            "Fetched at:  {}",
            fetched_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
        if let Some(used) = traffic_used {
            println!("Traffic used: {} bytes", used);
        }
        if let Some(total) = traffic_total {
            println!("Traffic total: {} bytes", total);
        }
        println!("Total nodes: {}", nodes.len());
        println!("Real nodes: {}", real.len());
        println!("Placeholder nodes: {}", placeholders.len());
        println!("Protocols: {:?}", protocols);
        println!("Transports: {:?}", transports);
        println!("Securities: {:?}", securities);
    }

    Ok(())
}

async fn resolve_subscription_id(client: &ApiClient, target: &str) -> eyre::Result<uuid::Uuid> {
    if let Ok(id) = uuid::Uuid::parse_str(target) {
        return Ok(id);
    }
    let subs = client.list_subscriptions().await?;
    subs.into_iter()
        .find(|s| s.url == target || s.name.as_deref() == Some(target))
        .map(|s| s.id)
        .ok_or_else(|| eyre::eyre!("Subscription not found: {}", target))
}
