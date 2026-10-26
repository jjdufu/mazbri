use crate::san::SanConfig;
use anyhow::Result;
use mazbri_core::config::ValidateConfig;
use mazbri_core::{
    CertServiceConfig, DatabaseConfig, Identity, LogRouteConfig, ServerConfig, TracingConfig,
};
use serde::Deserialize;

/// cert 自带配置（bootstrap 文件即完整配置，单次解析）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CertAppConfig {
    pub service_name: Identity,
    pub cert_service: CertServiceConfig,
    #[serde(default)]
    pub tracing: TracingConfig,
    #[serde(default)]
    pub log_routes: Vec<LogRouteConfig>,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub cert: CertConfig,
    pub san: SanConfig,
    #[serde(default)]
    pub auth: CertAuthConfig,
    pub admin: mazbri_admin::AdminConfig,
    /// 统一审计（可省略：旧配置照常启动，审计只写本地兜底文件）。
    #[serde(default)]
    pub audit: crate::audit::CertAuditConfig,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CertConfig {
    #[serde(default = "default_ca_validity_days")]
    pub ca_validity_days: i64,
    #[serde(default = "default_cert_validity_days")]
    pub cert_validity_days: i64,
    #[serde(default)]
    pub key_password: Option<String>,
}
impl CertConfig {
    #[must_use]
    pub fn key_password(&self) -> &str {
        self.key_password.as_deref().unwrap_or("")
    }
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CertAuthConfig {
    #[serde(default, deserialize_with = "deserialize_static_tokens")]
    pub static_tokens: Vec<String>,
}
fn deserialize_static_tokens<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        String(String),
        Vec(Vec<String>),
    }
    match StringOrVec::deserialize(deserializer)? {
        StringOrVec::String(s) if s.is_empty() => Ok(Vec::new()),
        StringOrVec::String(s) => Ok(s.split(',').map(|t| t.trim().to_string()).collect()),
        StringOrVec::Vec(v) => Ok(v),
    }
}
fn default_ca_validity_days() -> i64 {
    3650
}
fn default_cert_validity_days() -> i64 {
    365
}
impl ValidateConfig for CertAppConfig {
    fn validate(&self) -> Result<()> {
        self.cert_service.validate()?;
        self.audit.validate()?;
        if self
            .cert
            .key_password
            .as_ref()
            .is_none_or(std::string::String::is_empty)
        {
            tracing::info!(
                "KEY_PASSWORD will be provided by the commercial base authorization package"
            );
        }
        if self.server.port == 0 {
            anyhow::bail!("服务器端口不能为 0");
        }
        if self.cert.ca_validity_days <= 0 {
            anyhow::bail!("CA 有效期必须为正数（当前 {}）", self.cert.ca_validity_days);
        }
        if self.cert.cert_validity_days <= 0 {
            anyhow::bail!(
                "证书有效期必须为正数（当前 {}）",
                self.cert.cert_validity_days
            );
        }
        if self.cert.cert_validity_days >= self.cert.ca_validity_days {
            anyhow::bail!(
                "Certificate validity ({}) must be less than CA validity ({}) \
                 / 证书有效期（{}）必须小于 CA 有效期（{}）",
                self.cert.cert_validity_days,
                self.cert.ca_validity_days,
                self.cert.cert_validity_days,
                self.cert.ca_validity_days
            );
        }
        if self.server.graceful_shutdown_timeout_secs == 0 {
            anyhow::bail!(
                "Graceful shutdown timeout must be positive \
                 / 优雅关闭超时必须为正数"
            );
        }
        Ok(())
    }
}
