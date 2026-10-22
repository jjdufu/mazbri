use anyhow::Result;
use mazbri_core::config::{ServiceConfig, ValidateConfig};
use serde::Deserialize;
use std::collections::HashMap;
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheServiceConfig {
    pub server: mazbri_core::ServerConfig,
    pub database: mazbri_core::DatabaseConfig,
    pub redis: mazbri_core::redis::RedisConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub admin: mazbri_admin::AdminConfig,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    #[serde(default = "default_default_ttl")]
    pub default_ttl: u64,
    #[serde(default = "default_max_cache_size")]
    pub max_cache_size: u64,
    #[serde(default)]
    pub api_ttl: HashMap<String, u64>,
    #[serde(default = "default_true")]
    pub tenant_isolated: bool,
}
fn default_default_ttl() -> u64 {
    300
}
fn default_max_cache_size() -> u64 {
    1024 * 1024 * 1024
}
fn default_true() -> bool {
    true
}
impl Default for CacheConfig {
    fn default() -> Self {
        let mut api_ttl = HashMap::new();
        api_ttl.insert("/api/v1/*".to_string(), 300);
        Self {
            default_ttl: 300,
            max_cache_size: 1024 * 1024 * 1024,
            api_ttl,
            tenant_isolated: true,
        }
    }
}
impl ServiceConfig for CacheServiceConfig {
    fn admin_health(&self) -> Option<mazbri_core::config::AdminHealthEndpoint<'_>> {
        self.admin
            .enabled
            .then(|| mazbri_core::config::AdminHealthEndpoint {
                port: self.admin.port,
                path: &self.admin.health_path,
            })
    }
    fn server_config(&self) -> &mazbri_core::ServerConfig {
        &self.server
    }
}
impl ValidateConfig for CacheServiceConfig {
    fn validate(&self) -> Result<()> {
        self.redis.validate()?;
        Ok(())
    }
}
