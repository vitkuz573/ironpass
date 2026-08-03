pub mod assets;
pub mod backend;
pub mod core_process;
mod singbox;
mod xray;

pub use assets::{GeoAssetStatus, detect_geo_assets, locate_core_binary};
pub use backend::{
    Backend, BackendCapability, BackendCapabilities, BackendRegistry, BackendType, GeneratedConfig,
    ProxyPorts,
};
pub use core_process::{CoreProcessManager, CoreType};
