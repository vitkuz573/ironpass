use clap::Parser;
use ironpass_api::{core_process::CoreType, default_state, serve};
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "ironpassd")]
#[command(about = "IronPass API daemon")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:8080")]
    bind: String,

    #[arg(long, help = "Path to sing-box binary")]
    sing_box: Option<PathBuf>,

    #[arg(long, help = "Path to Xray-core binary")]
    xray: Option<PathBuf>,

    #[arg(long, help = "Path to data directory")]
    data_dir: Option<PathBuf>,

    #[arg(long, help = "Path to config directory")]
    config_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let addr: SocketAddr = args.bind.parse()?;

    let state = default_state(args.xray.clone())?;

    if let Some(path) = args.sing_box {
        let mut manager = state.process_manager.write().await;
        manager.set_core_type(CoreType::SingBox);
        manager.set_path(path);
        drop(manager);
    }

    if let Some(path) = args.xray {
        let mut stored = state.xray_path.write().await;
        *stored = Some(path);
        drop(stored);
    }

    let migrated = state.migrate_legacy()?;
    if migrated > 0 {
        tracing::info!("Migrated {} legacy subscriptions to SQLite", migrated);
    }

    tracing::info!("ironpassd listening on http://{}", addr);
    serve(state, addr).await?;
    Ok(())
}
