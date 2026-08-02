use crate::args::{OutputFormatArg, SubAction};
use crate::output;
use color_eyre::eyre;
use ironpass_api_client::ApiClient;
use ironpass_core::models::OutputFormat;
use ironpass_core::traits::NodeExporter;
use ironpass_subscription::NodeExporterImpl;
use tracing::info;
use uuid::Uuid;

pub async fn fetch(
    api_url: &str,
    url: Option<String>,
    format: Option<OutputFormatArg>,
    output_file: Option<String>,
    hwid_override: Option<String>,
    include_placeholders: bool,
    sort: Option<String>,
) -> eyre::Result<()> {
    let client = ApiClient::with_url(api_url.into());

    // Resolve URL to a subscription. If a URL is provided, fetch it directly via parser.
    let fetch_url = match url {
        Some(u) if u.starts_with("http://") || u.starts_with("https://") => u,
        Some(_) | None => {
            let subs = client.list_subscriptions().await?;
            let first = subs
                .into_iter()
                .next()
                .ok_or_else(|| eyre::eyre!("No subscription URL provided and none saved."))?;
            first.url
        }
    };

    // Use the existing subscription service for direct URL fetches.
    let service = ironpass_subscription::SubscriptionService::new();
    let sub = service
        .fetch_and_parse(&fetch_url, hwid_override.as_deref())
        .await?;

    let url = sub.url.clone();
    let fetched_at = sub.fetched_at;
    let traffic_used = sub.traffic_used;
    let traffic_total = sub.traffic_total;

    let mut nodes: Vec<_> = sub.nodes;

    if !include_placeholders {
        let before = nodes.len();
        nodes.retain(|n| !ironpass_subscription::is_placeholder_node(n));
        let removed = before - nodes.len();
        if removed > 0 {
            info!("Filtered out {} placeholder nodes", removed);
        }
    }

    if let Some(ref sort_key) = sort {
        nodes.sort_by(|a, b| match sort_key.as_str() {
            "name" => a.name.cmp(&b.name),
            "server" => a.server.cmp(&b.server),
            "port" => a.port.cmp(&b.port),
            "protocol" => format!("{:?}", a.protocol).cmp(&format!("{:?}", b.protocol)),
            _ => std::cmp::Ordering::Equal,
        });
    }

    let out_fmt = format.unwrap_or(OutputFormatArg::Table);

    match out_fmt {
        OutputFormatArg::Table => {
            let sub_display = ironpass_core::models::Subscription {
                id: uuid::Uuid::new_v4(),
                url,
                name: None,
                nodes: nodes.clone(),
                fetched_at,
                expires_at: None,
                traffic_used,
                traffic_total,
                metadata: ironpass_core::models::SubscriptionMetadata::default(),
            };
            output::print_nodes_table(&nodes, &sub_display)?;
        }
        OutputFormatArg::Json => {
            output::print_nodes_json(&nodes)?;
        }
        _ => {
            let exporter = NodeExporterImpl::new();
            let core_fmt = arg_to_output_format(&out_fmt);
            let content = exporter.export(&nodes, &core_fmt)?;

            if let Some(path) = output_file {
                std::fs::write(&path, &content)?;
                println!("Written to {}", path);
            } else {
                println!("{}", content);
            }
        }
    }

    Ok(())
}

pub async fn handle(api_url: &str, action: SubAction, json: bool) -> eyre::Result<()> {
    let client = ApiClient::with_url(api_url.into());

    match action {
        SubAction::Add { url, name, hwid } => {
            let sub = client.add_subscription(url, name, hwid).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&sub)?);
            } else {
                println!("Added subscription: {}", sub.url);
                if let Some(n) = &sub.name {
                    println!("  Name: {}", n);
                }
            }
            Ok(())
        }
        SubAction::Remove { target } => {
            let id = resolve_subscription_id(&client, &target).await?;
            client.delete_subscription(id).await?;
            println!("Removed: {}", target);
            Ok(())
        }
        SubAction::List { detailed } => {
            let subs = client.list_subscriptions().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&subs)?);
            } else if subs.is_empty() {
                println!("No subscriptions saved.");
            } else {
                output::print_subscriptions_api(&subs, detailed);
            }
            Ok(())
        }
        SubAction::Update { target, hwid } => {
            let ids = match target {
                Some(t) => vec![resolve_subscription_id(&client, &t).await?],
                None => client
                    .list_subscriptions()
                    .await?
                    .into_iter()
                    .filter(|s| s.is_active)
                    .map(|s| s.id)
                    .collect(),
            };

            if ids.is_empty() {
                println!("No matching subscriptions found.");
                return Ok(());
            }

            for id in ids {
                match client.fetch_subscription(id, hwid.clone()).await {
                    Ok(detail) => {
                        let real = detail
                            .nodes
                            .iter()
                            .filter(|n| !ironpass_subscription::is_placeholder_node(&n.node))
                            .count();
                        println!(
                            "Updated {}: {} nodes ({} real)",
                            detail.subscription.url,
                            detail.nodes.len(),
                            real
                        );
                    }
                    Err(e) => {
                        eprintln!("Failed to update {}: {}", id, e);
                    }
                }
            }
            Ok(())
        }
    }
}

async fn resolve_subscription_id(client: &ApiClient, target: &str) -> eyre::Result<Uuid> {
    if let Ok(id) = Uuid::parse_str(target) {
        return Ok(id);
    }
    let subs = client.list_subscriptions().await?;
    subs.into_iter()
        .find(|s| s.url == target || s.name.as_deref() == Some(target))
        .map(|s| s.id)
        .ok_or_else(|| eyre::eyre!("Subscription not found: {}", target))
}

fn arg_to_output_format(arg: &OutputFormatArg) -> OutputFormat {
    match arg {
        OutputFormatArg::Clash => OutputFormat::Clash,
        OutputFormatArg::SingBox => OutputFormat::SingBox,
        OutputFormatArg::V2Ray => OutputFormat::V2Ray,
        OutputFormatArg::Raw => OutputFormat::Raw,
        OutputFormatArg::Json => OutputFormat::Raw,
        OutputFormatArg::Table => OutputFormat::Raw,
    }
}
