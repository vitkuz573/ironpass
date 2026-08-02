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

pub async fn dispatch(cli: Cli) -> eyre::Result<()> {
    match cli.command {
        Commands::Fetch { url, format, output, hwid, include_placeholders, sort } => {
            sub::fetch(url, format, output, hwid, include_placeholders, sort, cli.json).await
        }
        Commands::Sub { action } => sub::handle(action, cli.json).await,
        Commands::Hwid { action } => hwid::handle(action, cli.json),
        Commands::Convert { input, from, to, output } => {
            convert::handle(input, from, to, output).await
        }
        Commands::Analyze { url, probe, detailed } => {
            analyze::handle(url, probe, detailed, cli.json).await
        }
        Commands::Export { url, target, output, hwid } => {
            export::handle(url, target, output, hwid).await
        }
        Commands::Completions { shell } => completions::handle(shell),
        Commands::Config { action } => config_cmd::handle(action).await,
        Commands::Ping { url, timeout } => ping::handle(url, timeout).await,
        Commands::Proxy { url, node, socks_port, http_port, hwid } => {
            proxy::handle(url, node, socks_port, http_port, hwid).await
        }
    }
}
