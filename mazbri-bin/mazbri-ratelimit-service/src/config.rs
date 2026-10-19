use anyhow::Result;
use mazbri_core::config_loader::{ServiceConfig, ValidateConfig};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditConfig {
    #[serde(default = "default_audit_enabled")]
    pub enabled: bool,
}

fn default_audit_enabled() -> bool {
    true
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: default_audit_enabled(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RateLimitConfigSection {
    #[serde(default)]
    pub audit: AuditConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitServiceConfig {
    pub server: mazbri_core::ServerConfig,
    pub database: mazbri_core::DatabaseConfig,
    #[serde(default)]
    pub tls: Option<mazbri_tls::TlsConfig>,
    #[serde(default)]
    pub rate_limit: RateLimitConfigSection,
    #[serde(default)]
    pub admin: mazbri_admin::AdminConfig,
}

impl ServiceConfig for RateLimitServiceConfig {
    fn service_name() -> &'static str {
        "mazbri-ratelimit-service"
    }
    fn default_port() -> u16 {
        8082
    }
    fn server_config(&self) -> &mazbri_core::ServerConfig {
        &self.server
    }
}

impl ValidateConfig for RateLimitServiceConfig {
    fn validate(&self) -> Result<()> {
        if let Some(ref tls_config) = self.tls {
            tls_config.validate()?;
        }
        Ok(())
    }
}

impl RateLimitServiceConfig {
    pub fn is_audit_enabled(&self) -> bool {
        self.rate_limit.audit.enabled
    }
}
