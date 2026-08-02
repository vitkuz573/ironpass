use ironpass_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const APP_NAME: &str = "ironpass";
const CONFIG_FILE: &str = "config.toml";
const SUBSCRIPTIONS_FILE: &str = "subscriptions.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,

    #[serde(default)]
    pub subscription: SubscriptionConfig,

    #[serde(default)]
    pub hwid: HwidConfig,

    #[serde(default)]
    pub output: OutputConfig,

    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionConfig {
    #[serde(default)]
    pub default_url: Option<String>,

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
            default_url: None,
            auto_update: default_auto_update(),
            update_interval_hours: default_update_interval(),
            proxy: None,
            extra_headers: std::collections::HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_format")]
    pub format: String,

    #[serde(default)]
    pub output_file: Option<PathBuf>,

    #[serde(default = "default_pretty")]
    pub pretty: bool,

    #[serde(default)]
    pub sort_by: Option<String>,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: default_format(),
            output_file: None,
            pretty: default_pretty(),
            sort_by: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,

    #[serde(default = "default_log_file")]
    pub file: bool,

    #[serde(default)]
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

fn default_user_agent() -> String { "v2rayN/6.0".into() }
fn default_timeout() -> u64 { 30 }
fn default_max_retries() -> u32 { 3 }
fn default_auto_update() -> bool { true }
fn default_update_interval() -> u64 { 24 }
fn default_enabled() -> bool { true }
fn default_format() -> String { "clash".into() }
fn default_pretty() -> bool { true }
fn default_log_level() -> String { "info".into() }
fn default_log_file() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSubscription {
    pub url: String,
    pub name: Option<String>,
    pub added_at: chrono::DateTime<chrono::Utc>,
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
    pub hwid: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubscriptionsStore {
    pub subscriptions: Vec<StoredSubscription>,
}

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

        Self { config_dir, data_dir }
    }

    pub fn with_dirs(config_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self { config_dir, data_dir }
    }

    fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.config_dir)?;
        fs::create_dir_all(&self.data_dir)?;
        Ok(())
    }

    pub fn config_path(&self) -> PathBuf {
        self.config_dir.join(CONFIG_FILE)
    }

    pub fn subscriptions_path(&self) -> PathBuf {
        self.data_dir.join(SUBSCRIPTIONS_FILE)
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

    pub fn load_subscriptions(&self) -> Result<SubscriptionsStore> {
        let path = self.subscriptions_path();
        if !path.exists() {
            return Ok(SubscriptionsStore::default());
        }

        let content = fs::read_to_string(&path)?;
        let store: SubscriptionsStore = serde_json::from_str(&content)
            .map_err(|e| Error::Config(format!("Failed to parse subscriptions: {}", e)))?;
        Ok(store)
    }

    pub fn save_subscriptions(&self, store: &SubscriptionsStore) -> Result<()> {
        self.ensure_dirs()?;
        let content = serde_json::to_string_pretty(store)?;
        fs::write(self.subscriptions_path(), content)?;
        Ok(())
    }

    pub fn add_subscription(&self, url: &str, name: Option<String>, hwid: Option<String>) -> Result<StoredSubscription> {
        let mut store = self.load_subscriptions()?;

        if store.subscriptions.iter().any(|s| s.url == url) {
            return Err(Error::Custom("Subscription already exists".into()));
        }

        let sub = StoredSubscription {
            url: url.to_string(),
            name,
            added_at: chrono::Utc::now(),
            last_updated: None,
            hwid,
            is_active: true,
        };

        store.subscriptions.push(sub.clone());
        self.save_subscriptions(&store)?;

        Ok(sub)
    }

    pub fn remove_subscription(&self, url: &str) -> Result<()> {
        let mut store = self.load_subscriptions()?;
        store.subscriptions.retain(|s| s.url != url);
        self.save_subscriptions(&store)?;
        Ok(())
    }

    pub fn list_subscriptions(&self) -> Result<Vec<StoredSubscription>> {
        let store = self.load_subscriptions()?;
        Ok(store.subscriptions)
    }

    pub fn update_subscription_timestamp(&self, url: &str) -> Result<()> {
        let mut store = self.load_subscriptions()?;
        if let Some(sub) = store.subscriptions.iter_mut().find(|s| s.url == url) {
            sub.last_updated = Some(chrono::Utc::now());
            self.save_subscriptions(&store)?;
        }
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
