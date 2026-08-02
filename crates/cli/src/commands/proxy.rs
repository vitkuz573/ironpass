use crate::api_client::ApiClient;
use crate::args::OutputFormatArg;
use color_eyre::eyre;
use ironpass_api::models::StartProxyRequest;
use uuid::Uuid;

pub async fn handle(
    api_url: &str,
    node: Option<String>,
    socks_port: u16,
    http_port: u16,
    mixed_port: Option<u16>,
) -> eyre::Result<()> {
    let client = ApiClient::with_url(api_url.into());

    let node_id = match node {
        Some(n) => Some(resolve_node_or_subscription(&client, &n).await?),
        None => None,
    };

    let req = StartProxyRequest {
        node_id,
        socks_port: Some(socks_port),
        http_port: Some(http_port),
        mixed_port,
    };

    let status = client.start_proxy(&req).await?;

    println!("Proxy started");
    if let Some(node) = &status.selected_node {
        println!("  Node:      {}", node.node.name);
        println!("  Endpoint:  {}:{}", node.node.server, node.node.port);
        println!("  Protocol:  {:?}", node.node.protocol);
        println!("  Transport: {:?}", node.node.transport);
    }
    if let Some(port) = status.socks_port {
        println!("  SOCKS5:    127.0.0.1:{}", port);
    }
    if let Some(port) = status.http_port {
        println!("  HTTP:      127.0.0.1:{}", port);
    }
    if let Some(port) = status.mixed_port {
        println!("  Mixed:     127.0.0.1:{}", port);
    }

    Ok(())
}

async fn resolve_node_or_subscription(
    client: &ApiClient,
    target: &str,
) -> eyre::Result<Uuid> {
    if let Ok(id) = Uuid::parse_str(target) {
        return Ok(id);
    }

    // Try matching subscription by URL/name, then pick first node.
    let subs = client.list_subscriptions().await?;
    if let Some(sub) = subs
        .into_iter()
        .find(|s| s.url == target || s.name.as_deref() == Some(target))
    {
        let nodes = client.list_nodes(Some(sub.id)).await?;
        return nodes
            .into_iter()
            .next()
            .map(|n| n.id)
            .ok_or_else(|| eyre::eyre!("Subscription has no nodes"));
    }

    Err(eyre::eyre!(
        "Node or subscription not found: {}. Provide a node UUID.",
        target
    ))
}

#[allow(dead_code)]
fn arg_to_output_format(_arg: &OutputFormatArg) -> ironpass_core::models::OutputFormat {
    ironpass_core::models::OutputFormat::Raw
}
