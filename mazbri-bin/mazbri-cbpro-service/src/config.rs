use anyhow::Result;
use mazbri_core::config::{ServiceConfig, ValidateConfig};
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
    fn admin_health(&self) -> Option<mazbri_core::config::AdminHealthEndpoint<'_>> {
        self.admin.enabled.then(|| mazbri_core::config::AdminHealthEndpoint {
            port: self.admin.port,
            path: &self.admin.health_path,
        })
    }
    fn server_config(&self) -> &mazbri_core::ServerConfig {
        &self.server
    }
}
impl ValidateConfig for CBProServiceConfig {
    fn validate(&self) -> Result<()> {
        self.redis.validate()?;
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
