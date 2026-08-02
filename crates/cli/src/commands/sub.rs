use crate::args::{OutputFormatArg, SubAction};
use crate::output;
use color_eyre::eyre;
use ironpass_config::ConfigManager;
use ironpass_core::traits::{HwidProvider, NodeExporter};
use ironpass_core::models::OutputFormat;
use ironpass_subscription::{NodeExporterImpl, SubscriptionService, is_placeholder_node};
use tracing::info;

pub async fn fetch(
    url: Option<String>,
    format: Option<OutputFormatArg>,
    output_file: Option<String>,
    hwid_override: Option<String>,
    include_placeholders: bool,
    sort: Option<String>,
    _json_output: bool,
) -> eyre::Result<()> {
    let config = ConfigManager::new().load_config()?;
    let fetch_url = resolve_url(url, &config)?;

    let hwid = hwid_override.or_else(|| {
        if config.hwid.enabled {
            ironpass_hwid::SystemHwidProvider::new().generate().ok()
        } else {
            None
        }
    });

    let service = SubscriptionService::new();
    let sub = service.fetch_and_parse(&fetch_url, hwid.as_deref()).await?;

    let url = sub.url.clone();
    let fetched_at = sub.fetched_at;
    let traffic_used = sub.traffic_used;
    let traffic_total = sub.traffic_total;

    let mut nodes: Vec<_> = sub.nodes;

    if !include_placeholders {
        let before = nodes.len();
        nodes.retain(|n| !is_placeholder_node(n));
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

pub async fn handle(action: SubAction, json: bool) -> eyre::Result<()> {
    let manager = ConfigManager::new();

    match action {
        SubAction::Add { url, name, hwid } => {
            let sub = manager.add_subscription(&url, name.clone(), hwid)?;
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
            manager.remove_subscription(&target)?;
            println!("Removed: {}", target);
            Ok(())
        }
        SubAction::List { detailed } => {
            let subs = manager.list_subscriptions()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&subs)?);
            } else if subs.is_empty() {
                println!("No subscriptions saved.");
            } else {
                output::print_subscriptions(&subs, detailed);
            }
            Ok(())
        }
        SubAction::Update { target, hwid } => {
            let subs = manager.list_subscriptions()?;
            let to_update: Vec<_> = match &target {
                Some(t) => subs.into_iter().filter(|s| s.url == *t || s.name.as_deref() == Some(t.as_str())).collect(),
                None => subs.into_iter().filter(|s| s.is_active).collect(),
            };

            if to_update.is_empty() {
                println!("No matching subscriptions found.");
                return Ok(());
            }

            let service = SubscriptionService::new();
            for sub in &to_update {
                let hwid_val = hwid.clone().or_else(|| sub.hwid.clone());
                match service.fetch_and_parse(&sub.url, hwid_val.as_deref()).await {
                    Ok(fetched) => {
                        let real = fetched.nodes.iter()
                            .filter(|n| !is_placeholder_node(n))
                            .count();
                        println!("Updated {}: {} nodes ({} real)", sub.url, fetched.nodes.len(), real);
                        manager.update_subscription_timestamp(&sub.url)?;
                    }
                    Err(e) => {
                        eprintln!("Failed to update {}: {}", sub.url, e);
                    }
                }
            }
            Ok(())
        }
    }
}

fn resolve_url(url: Option<String>, config: &ironpass_config::AppConfig) -> eyre::Result<String> {
    if let Some(u) = url {
        return Ok(u);
    }
    if let Some(ref default) = config.subscription.default_url {
        return Ok(default.clone());
    }
    Err(eyre::eyre!("No subscription URL provided. Pass a URL or set default_url in config."))
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
