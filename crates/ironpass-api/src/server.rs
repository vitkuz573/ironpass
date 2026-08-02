//! API server setup and application router.

use crate::state::AppState;
use axum::Router;
use ironpass_config::ConfigManager;
use ironpass_core::traits::HwidProvider;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

/// Build the API application router.
pub fn app(state: Arc<AppState>) -> Router {
    crate::routes::router(state)
}

/// Create default application state using XDG directories.
pub fn default_state(xray_path: Option<PathBuf>) -> anyhow::Result<Arc<AppState>> {
    let config_manager = ConfigManager::new();
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ironpass");
    std::fs::create_dir_all(&data_dir)?;
    let db_path = data_dir.join("ironpass.db");
    let db = crate::db::DbPool::open(db_path)?;
    let hwid: Arc<dyn HwidProvider + Send + Sync> =
        Arc::new(ironpass_hwid::SystemHwidProvider::new());
    let state = AppState::new(config_manager, db, hwid, xray_path);
    Ok(Arc::new(state))
}

/// Run the API server on the given address.
pub async fn serve(state: Arc<AppState>, addr: SocketAddr) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let app = app(state);
    axum::serve(listener, app).await?;
    Ok(())
}
