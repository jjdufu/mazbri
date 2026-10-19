use anyhow::Result;
use mazbri_core::config_loader::{ServiceConfig, ValidateConfig};
use serde::Deserialize;
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthServiceConfig {
    pub server: mazbri_core::ServerConfig,
    pub database: mazbri_core::DatabaseConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub tls: Option<mazbri_tls::TlsConfig>,
    #[serde(default)]
    pub admin: mazbri_admin::AdminConfig,
    /// 白名单表在平台库。不配置时仍用认证库，监听不会启动。
    #[serde(default)]
    pub platform_database: Option<mazbri_core::DatabaseConfig>,
    /// IP 白名单缓存：数据库通知叫醒，轮询保底。
    #[serde(default)]
    pub config_watch: mazbri_core::config_watch::ConfigWatchConfig,
}
impl ServiceConfig for AuthServiceConfig {
    fn service_name() -> &'static str {
        "mazbri-auth-service"
    }
    fn default_port() -> u16 {
        8081
    }
    fn server_config(&self) -> &mazbri_core::ServerConfig {
        &self.server
    }
}
impl ValidateConfig for AuthServiceConfig {
    fn validate(&self) -> Result<()> {
        if self.auth.api_key_prefix_length < 4 || self.auth.api_key_prefix_length > 16 {
            anyhow::bail!(
                "auth.api_key_prefix_length must be between 4 and 16, got {}",
                self.auth.api_key_prefix_length
            );
        }
        if self.auth.validation_cache_ttl_secs > 300 {
            anyhow::bail!(
                "auth.validation_cache_ttl_secs must be <= 300, got {}",
                self.auth.validation_cache_ttl_secs
            );
        }
        if self.auth.validation_cache_max_entries > 1_000_000 {
            anyhow::bail!(
                "auth.validation_cache_max_entries must be <= 1000000, got {}",
                self.auth.validation_cache_max_entries
            );
        }
        if let Some(ref tls_config) = self.tls {
            tls_config.validate()?;
        }
        self.config_watch.validate()?;
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    #[serde(default = "default_api_key_prefix_length")]
    pub api_key_prefix_length: usize,
    #[serde(default = "default_enable_ip_whitelist")]
    pub enable_ip_whitelist: bool,
    #[serde(default = "default_validation_cache_ttl_secs")]
    pub validation_cache_ttl_secs: u64,
    /// Max in-memory validation-cache entries (hashed keys). 0 = library default.
    #[serde(default = "default_validation_cache_max_entries")]
    pub validation_cache_max_entries: usize,
}
fn default_api_key_prefix_length() -> usize {
    8
}
fn default_enable_ip_whitelist() -> bool {
    true
}
fn default_validation_cache_ttl_secs() -> u64 {
    5
}
fn default_validation_cache_max_entries() -> usize {
    10_000
}
