use crate::args::ExportTarget;
use color_eyre::eyre;
use ironpass_config::ConfigManager;
use ironpass_core::models::OutputFormat;
use ironpass_core::traits::{HwidProvider, NodeExporter};
use ironpass_subscription::{NodeExporterImpl, SubscriptionService, is_placeholder_node};

pub async fn handle(
    manager: &ConfigManager,
    url: Option<String>,
    target: ExportTarget,
    output_file: Option<String>,
    hwid_override: Option<String>,
) -> eyre::Result<()> {
    let config = manager.load_config()?;
    let fetch_url = url.or_else(|| config.subscription.default_url.clone())
        .ok_or_else(|| eyre::eyre!("No URL provided"))?;

    let hwid = hwid_override.or_else(|| {
        if config.hwid.enabled {
            ironpass_hwid::SystemHwidProvider::new().generate().ok()
        } else {
            None
        }
    });

    let service = SubscriptionService::new();
    let sub = service.fetch_and_parse(&fetch_url, hwid.as_deref()).await?;

    let nodes: Vec<_> = sub.nodes.into_iter()
        .filter(|n| !is_placeholder_node(n))
        .collect();

    if nodes.is_empty() {
        return Err(eyre::eyre!("No real nodes found in subscription"));
    }

    let exporter = NodeExporterImpl::new();

    let (fmt, extra_info) = match target {
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
