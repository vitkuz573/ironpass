pub mod sub;
pub mod hwid;
pub mod convert;
pub mod analyze;
pub mod export;
pub mod config_cmd;
pub mod ping;
pub mod completions;
pub mod proxy;

use crate::args::{Cli, Commands};
use color_eyre::eyre;
use ironpass_config::ConfigManager;
use std::path::PathBuf;

fn build_config_manager(config_path: Option<&String>) -> ConfigManager {
    if let Some(path) = config_path {
        let dir = PathBuf::from(path);
        ConfigManager::with_dirs(dir.clone(), dir)
    } else {
        ConfigManager::new()
    }
}

pub async fn dispatch(cli: Cli) -> eyre::Result<()> {
    let manager = build_config_manager(cli.config.as_ref());

    match cli.command {
        Commands::Fetch { url, format, output, hwid, include_placeholders, sort } => {
            sub::fetch(
                &manager,
                url,
                format,
                output,
                hwid,
                include_placeholders,
                sort,
                cli.json,
            )
            .await
        }
        Commands::Sub { action } => sub::handle(&manager, action, cli.json).await,
        Commands::Hwid { action } => hwid::handle(action, cli.json),
        Commands::Convert { input, from, to, output } => {
            convert::handle(input, from, to, output).await
        }
        Commands::Analyze { url, probe, detailed } => {
            analyze::handle(&manager, url, probe, detailed, cli.json).await
        }
        Commands::Export { url, target, output, hwid } => {
            export::handle(&manager, url, target, output, hwid).await
        }
        Commands::Completions { shell } => completions::handle(shell),
        Commands::Config { action } => config_cmd::handle(&manager, action).await,
        Commands::Ping { url, timeout } => ping::handle(url, timeout).await,
        Commands::Proxy { url, node, socks_port, http_port, hwid } => {
            proxy::handle(&manager, url, node, socks_port, http_port, hwid).await
        }
    }
}
