use anyhow::Result;
use mazbri_core::config_loader::{ServiceConfig, ValidateConfig};
use serde::Deserialize;
use std::time::Duration;
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CBProServiceConfig {
    pub server: mazbri_core::ServerConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub redis: mazbri_core::redis::RedisConfig,
    pub tls: Option<mazbri_tls::TlsConfig>,
    #[serde(default)]
    pub admin: mazbri_admin::AdminConfig,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    #[serde(default = "default_ttl_secs")]
    pub default_ttl_secs: u64,
    #[serde(default = "default_local_cache_ttl_ms")]
    pub local_cache_ttl_ms: u64,
}
fn default_max_entries() -> usize {
    10_000
}
fn default_ttl_secs() -> u64 {
    300
}
fn default_local_cache_ttl_ms() -> u64 {
    100
}
impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: default_max_entries(),
            default_ttl_secs: default_ttl_secs(),
            local_cache_ttl_ms: default_local_cache_ttl_ms(),
        }
    }
}
impl ServiceConfig for CBProServiceConfig {
    fn service_name() -> &'static str {
        "mazbri-cbpro-service"
    }
    fn default_port() -> u16 {
        8088
    }
    fn server_config(&self) -> &mazbri_core::ServerConfig {
        &self.server
    }
}
impl ValidateConfig for CBProServiceConfig {
    fn validate(&self) -> Result<()> {
        self.redis.validate()?;
        if let Some(ref tls) = self.tls {
            tls.validate()?;
        }
        Ok(())
    }
}
impl CacheConfig {
    pub fn default_ttl(&self) -> Duration {
        Duration::from_secs(self.default_ttl_secs)
    }
    pub fn local_cache_ttl(&self) -> Duration {
        Duration::from_millis(self.local_cache_ttl_ms)
    }
}
