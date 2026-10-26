use anyhow::Result;
use mazbri_core::config::{ServiceConfig, ValidateConfig};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditConfig {
    /// 限流审计总开关：`rate_limit_events` 用量审计 + 统一审计（控制命令确认）。
    #[serde(default = "default_audit_enabled")]
    pub enabled: bool,
    /// 统一审计写库（`mazbri_admin`，账号 `mazbri_audit_writer`）；缺省则不写统一审计。
    #[serde(default)]
    pub database: Option<mazbri_core::DatabaseConfig>,
    /// 统一审计落库失败时的本地 JSONL 兜底文件；缺省则输出到错误日志。
    #[serde(default)]
    pub fallback_file: Option<String>,
}

fn default_audit_enabled() -> bool {
    true
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: default_audit_enabled(),
            database: None,
            fallback_file: None,
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
    pub rate_limit: RateLimitConfigSection,
    #[serde(default)]
    pub admin: mazbri_admin::AdminConfig,
}

impl ServiceConfig for RateLimitServiceConfig {
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

impl ValidateConfig for RateLimitServiceConfig {
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl RateLimitServiceConfig {
    #[must_use]
    pub fn is_audit_enabled(&self) -> bool {
        self.rate_limit.audit.enabled
    }
}
