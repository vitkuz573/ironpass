use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("Config error: {0}")]
    Config(String),

    #[error("HWID error: {0}")]
    Hwid(String),

    #[error("Unsupported protocol: {0}")]
    UnsupportedProtocol(String),

    #[error("Subscription expired")]
    SubscriptionExpired,

    #[error("Device limit exceeded: {current}/{limit}")]
    DeviceLimitExceeded { current: usize, limit: usize },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SQLite error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("{0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, Error>;
