use crate::args::ExportTarget;
use color_eyre::eyre;
use ironpass_api_client::ApiClient;
use ironpass_core::models::OutputFormat;
use ironpass_core::traits::NodeExporter;
use ironpass_subscription::{NodeExporterImpl, SubscriptionService, is_placeholder_node};

pub async fn handle(
    api_url: &str,
    target: Option<String>,
    target_client: ExportTarget,
    output_file: Option<String>,
    hwid_override: Option<String>,
) -> eyre::Result<()> {
    let client = ApiClient::with_url(api_url.into());

    let nodes: Vec<ironpass_core::models::ProxyNode> = match target {
        Some(t) if t.starts_with("http://") || t.starts_with("https://") => {
            let service = SubscriptionService::new();
            let sub = service
                .fetch_and_parse(&t, hwid_override.as_deref())
                .await?;
            sub.nodes
                .into_iter()
                .filter(|n| !is_placeholder_node(n))
                .collect()
        }
        target => {
            let id = match target {
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
            detail
                .nodes
                .into_iter()
                .filter(|n| !is_placeholder_node(&n.node))
                .map(|n| n.node)
                .collect()
        }
    };

    if nodes.is_empty() {
        return Err(eyre::eyre!("No real nodes found in subscription"));
    }

    let exporter = NodeExporterImpl::new();

    let (fmt, extra_info) = match target_client {
        ExportTarget::Clash => (OutputFormat::Clash, Some("Clash Meta / mihomo")),
        ExportTarget::ClashMeta => (OutputFormat::Clash, Some("Clash Meta / mihomo")),
        ExportTarget::SingBox => (OutputFormat::SingBox, Some("sing-box")),
        ExportTarget::V2RayN => (OutputFormat::V2Ray, Some("V2RayN")),
        ExportTarget::V2RayNG => (OutputFormat::V2Ray, Some("V2RayNG")),
        ExportTarget::Hiddify => (OutputFormat::SingBox, Some("Hiddify")),
        ExportTarget::NekoRay => (OutputFormat::V2Ray, Some("NekoRay")),
        ExportTarget::Surge => (OutputFormat::Surge, Some("Surge")),
        ExportTarget::Shadowrocket => (OutputFormat::V2Ray, Some("Shadowrocket")),
        ExportTarget::QuantumultX => (OutputFormat::QuantumultX, Some("Quantumult X")),
        ExportTarget::Loon => (OutputFormat::Loon, Some("Loon")),
    };

    let content = exporter.export(&nodes, &fmt)?;

    if let Some(info) = extra_info {
        eprintln!("Exporting for: {} ({} nodes)", info, nodes.len());
    }

    match output_file {
        Some(path) => {
            std::fs::write(&path, &content)?;
            println!("Written to {}", path);
        }
        None => println!("{}", content),
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
