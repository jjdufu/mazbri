use crate::config::ValidateConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedisConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub db: Option<i64>,
    #[serde(default)]
    pub is_cluster: bool,
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default = "default_key_prefix")]
    pub key_prefix: String,
    #[serde(default = "default_connect_timeout_ms")]
    pub connect_timeout_ms: u64,
    #[serde(default = "default_command_timeout_ms")]
    pub command_timeout_ms: u64,
    #[serde(default = "default_pool_max_size")]
    pub pool_max_size: usize,
}

fn default_key_prefix() -> String {
    "mazbri".to_string()
}

fn default_connect_timeout_ms() -> u64 {
    500
}

fn default_command_timeout_ms() -> u64 {
    300
}

fn default_pool_max_size() -> usize {
    64
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            host: None,
            port: None,
            password: None,
            db: None,
            is_cluster: false,
            use_tls: false,
            key_prefix: default_key_prefix(),
            connect_timeout_ms: default_connect_timeout_ms(),
            command_timeout_ms: default_command_timeout_ms(),
            pool_max_size: default_pool_max_size(),
        }
    }
}

impl ValidateConfig for RedisConfig {
    fn validate(&self) -> Result<()> {
        if self.url.trim().is_empty() && self.host.is_none() {
            anyhow::bail!("Either redis.url or redis.host must not be empty");
        }
        if self.pool_max_size == 0 {
            anyhow::bail!("redis.pool_max_size must be greater than zero");
        }
        if self.connect_timeout_ms == 0 {
            anyhow::bail!("redis.connect_timeout_ms must be greater than zero");
        }
        if self.command_timeout_ms == 0 {
            anyhow::bail!("redis.command_timeout_ms must be greater than zero");
        }
        Ok(())
    }
}
