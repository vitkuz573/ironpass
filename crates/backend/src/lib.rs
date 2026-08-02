pub mod backend;
pub mod core_process;
pub mod singbox;
pub mod xray;

pub use backend::{Backend, BackendRegistry, BackendType, GeneratedConfig, ProxyPorts};
pub use core_process::{CoreProcessManager, CoreType};
