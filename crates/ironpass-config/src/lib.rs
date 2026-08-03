use ironpass_core::models::RoutingMode;
use ironpass_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use utoipa::ToSchema;

const APP_NAME: &str = "ironpass";
const CONFIG_FILE: &str = "config.toml";

/// Top-level application configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,

    #[serde(default)]
    pub subscription: SubscriptionConfig,

    #[serde(default)]
    pub hwid: HwidConfig,

    #[serde(default)]
    pub logging: LoggingConfig,

    /// Global split-tunnel routing mode.
    #[serde(default)]
    pub routing_mode: RoutingMode,
}

/// General application settings.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GeneralConfig {
    #[serde(default = "default_user_agent")]
    pub user_agent: String,

    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            user_agent: default_user_agent(),
            timeout_secs: default_timeout(),
            max_retries: default_max_retries(),
        }
    }
}

/// Subscription fetching settings.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubscriptionConfig {
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,

    #[serde(default = "default_update_interval")]
    pub update_interval_hours: u64,

    #[serde(default)]
    pub proxy: Option<String>,

    #[serde(default)]
    pub extra_headers: std::collections::HashMap<String, String>,
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            auto_update: default_auto_update(),
            update_interval_hours: default_update_interval(),
            proxy: None,
            extra_headers: std::collections::HashMap::new(),
        }
    }
}

/// HWID generation settings.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HwidConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default)]
    pub custom_id: Option<String>,

    pub device_model_override: Option<String>,
}

impl Default for HwidConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            custom_id: None,
            device_model_override: None,
        }
    }
}

/// Logging settings.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,

    #[serde(default = "default_log_file")]
    pub file: bool,

    /// Path to the directory containing log files.
    #[serde(default)]
    #[schema(value_type = Option<String>, format = "path")]
    pub log_dir: Option<PathBuf>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: default_log_file(),
            log_dir: None,
        }
    }
}

fn default_user_agent() -> String {
    "v2rayN/6.0".into()
}
fn default_timeout() -> u64 {
    30
}
fn default_max_retries() -> u32 {
    3
}
fn default_auto_update() -> bool {
    true
}
fn default_update_interval() -> u64 {
    24
}
fn default_enabled() -> bool {
    true
}
fn default_log_level() -> String {
    "info".into()
}
fn default_log_file() -> bool {
    true
}

#[derive(Clone)]
pub struct ConfigManager {
    config_dir: PathBuf,
    data_dir: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_NAME);

        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_NAME);

        Self {
            config_dir,
            data_dir,
        }
    }

    pub fn with_dirs(config_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            config_dir,
            data_dir,
        }
    }

    pub(crate) fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.config_dir)?;
        fs::create_dir_all(&self.data_dir)?;
        Ok(())
    }

    pub fn config_path(&self) -> PathBuf {
        self.config_dir.join(CONFIG_FILE)
    }

    pub fn load_config(&self) -> Result<AppConfig> {
        let path = self.config_path();
        if !path.exists() {
            let config = AppConfig::default();
            self.save_config(&config)?;
            return Ok(config);
        }

        let content = fs::read_to_string(&path)?;
        let config: AppConfig = toml::from_str(&content)
            .map_err(|e| Error::Config(format!("Failed to parse config: {}", e)))?;
        Ok(config)
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<()> {
        self.ensure_dirs()?;
        let content = toml::to_string_pretty(config)
            .map_err(|e| Error::Config(format!("Failed to serialize config: {}", e)))?;
        fs::write(self.config_path(), content)?;
        Ok(())
    }

    pub fn config_dir_display(&self) -> String {
        self.config_dir.display().to_string()
    }

    pub fn data_dir_display(&self) -> String {
        self.data_dir.display().to_string()
    }
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new()
    }
}
