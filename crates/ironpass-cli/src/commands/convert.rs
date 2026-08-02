use crate::args::OutputFormatArg;
use color_eyre::eyre;
use ironpass_core::models::OutputFormat;
use ironpass_core::traits::NodeExporter;
use ironpass_subscription::{NodeExporterImpl, SubscriptionParser};

pub async fn handle(
    input: Option<String>,
    to: OutputFormatArg,
    output_file: Option<String>,
) -> eyre::Result<()> {
    let raw = match input {
        Some(path) => std::fs::read_to_string(&path)?,
        None => {
            let mut buf = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
            buf
        }
    };

    let parser = SubscriptionParser::new();
    let nodes = parser.parse(&raw)?;
    eprintln!("Parsed {} nodes", nodes.len());

    let exporter = NodeExporterImpl::new();
    let core_fmt = match to {
        OutputFormatArg::Clash => OutputFormat::Clash,
        OutputFormatArg::SingBox => OutputFormat::SingBox,
        OutputFormatArg::V2Ray => OutputFormat::V2Ray,
        OutputFormatArg::Raw => OutputFormat::Raw,
        OutputFormatArg::Json => {
            println!("{}", serde_json::to_string_pretty(&nodes)?);
            return Ok(());
        }
        OutputFormatArg::Table => {
            for node in &nodes {
                println!(
                    "{:<20} {:<15}:{:<5} {:<10} {:<8}",
                    node.name,
                    node.server,
                    node.port,
                    format!("{:?}", node.protocol),
                    format!("{:?}", node.transport),
                );
            }
            return Ok(());
        }
    };

    let content = exporter.export(&nodes, &core_fmt)?;

    match output_file {
        Some(path) => {
            std::fs::write(&path, &content)?;
            println!("Written to {}", path);
        }
        None => println!("{}", content),
    }

    Ok(())
}
