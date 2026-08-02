mod analyze;
mod backend;
mod completions;
mod config_cmd;
mod convert;
mod daemon;
mod export;
mod hwid;
mod ping;
mod proxy;
mod split_tunnel;
mod sub;

use crate::args::{Cli, Commands};
use color_eyre::eyre;
use ironpass_api_client::ApiClient;

pub fn api_url(cli: &Cli) -> String {
    cli.api_url
        .clone()
        .unwrap_or_else(|| "http://127.0.0.1:8080".into())
}

#[allow(dead_code)]
pub async fn ensure_daemon(_cli: &Cli, url: &str) -> eyre::Result<()> {
    let client = ApiClient::with_url(url.into());
    if client.health().await.is_err() {
        return Err(eyre::eyre!(
            "ironpassd is not running at {}. Run `ironpass daemon start` first.",
            url
        ));
    }
    Ok(())
}

pub async fn dispatch(cli: Cli) -> eyre::Result<()> {
    let url = api_url(&cli);

    match cli.command {
        Commands::Daemon { action } => daemon::handle(action).await,
        Commands::Fetch {
            url: fetch_url,
            format,
            output,
            hwid,
            include_placeholders,
            sort,
        } => {
            sub::fetch(
                &url,
                fetch_url,
                format,
                output,
                hwid,
                include_placeholders,
                sort,
                cli.json,
            )
            .await
        }
        Commands::Sub { action } => sub::handle(&url, action, cli.json).await,
        Commands::Hwid { action } => hwid::handle(&url, action, cli.json).await,
        Commands::Convert {
            input,
            from,
            to,
            output,
        } => convert::handle(input, from, to, output).await,
        Commands::Analyze {
            target,
            probe,
            detailed,
        } => analyze::handle(&url, target, probe, detailed, cli.json).await,
        Commands::Export {
            target,
            target_client,
            output,
            hwid,
        } => export::handle(&url, target, target_client, output, hwid).await,
        Commands::Completions { shell } => completions::handle(shell),
        Commands::Config { action } => config_cmd::handle(&url, action).await,
        Commands::Ping { url, timeout } => ping::handle(url, timeout).await,
        Commands::Proxy {
            node,
            socks_port,
            http_port,
            mixed_port,
            backend,
        } => {
            proxy::handle(
                &url,
                node,
                socks_port,
                http_port,
                mixed_port,
                backend.as_backend_type(),
            )
            .await
        }
        Commands::Backend { action } => backend::handle(action, cli.json).await,
        Commands::SplitTunnel { action } => split_tunnel::handle(&url, action, cli.json).await,
    }
}
