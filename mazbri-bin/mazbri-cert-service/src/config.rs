use anyhow::{Context, Result};
use mazbri_core::config_loader::{ServiceConfig, ValidateConfig};
use serde::Deserialize;
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CertServiceConfig {
    #[serde(default)]
    pub service_name: Option<String>,
    pub server: mazbri_core::ServerConfig,
    pub database: mazbri_core::DatabaseConfig,
    pub cert: CertConfig,
    #[serde(default)]
    pub auth: CertAuthConfig,
    #[serde(default)]
    pub admin: mazbri_admin::AdminConfig,
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
impl CertServiceConfig {
    pub fn service_name_or(&self, default: &str) -> String {
        self.service_name
            .clone()
            .unwrap_or_else(|| default.to_string())
    }
    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            anyhow::bail!(
                "Config file not found: {}\n配置文件不存在: {}",
                path.display(),
                path.display()
            );
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let mut value: toml::Value = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file as TOML: {}", path.display()))?;
        if let Some(table) = value.as_table_mut() {
            table.remove("env");
            table.remove("tracing");
            table.remove("log_routes");
        }
        let config: Self = value
            .try_into()
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
        tracing::info!(path = % path.display(), "从配置文件加载配置");
        Ok(config)
    }
}
impl ServiceConfig for CertServiceConfig {
    fn service_name() -> &'static str {
        "mazbri-cert-service"
    }
    fn default_port() -> u16 {
        8086
    }
    fn server_config(&self) -> &mazbri_core::ServerConfig {
        &self.server
    }
}
impl ValidateConfig for CertServiceConfig {
    fn validate(&self) -> Result<()> {
        if self.cert.key_password.as_ref().is_none_or(|p| p.is_empty()) {
            tracing::info!(
                "KEY_PASSWORD will be provided by the commercial base authorization package"
            );
        }
        if self.auth.static_tokens.is_empty() {
            anyhow::bail!(
                "static_tokens must be configured via MAZBRI_CERT_SERVICE_TOKEN environment variable before startup"
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
