use anyhow::{Context, Result};
use mazbri_core::config_loader::{ServiceConfig, ValidateConfig};
use serde::Deserialize;
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsConfig {
    #[serde(default)]
    pub service_name: Option<String>,
    pub server: mazbri_core::ServerConfig,
    #[serde(default)]
    pub greptimedb: Option<GreptimeDBConfig>,
    pub clickhouse: ClickHouseConfig,
    #[serde(default)]
    pub admin: mazbri_admin::AdminConfig,
    #[serde(default)]
    pub metrics_targets: Vec<ServiceMetricTargetConfig>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClickHouseConfig {
    #[serde(default = "default_clickhouse_database")]
    pub database: String,
    pub http_url: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_clickhouse_timeout")]
    pub timeout_secs: u64,
}
fn default_clickhouse_database() -> String {
    "mazbri_logs".to_string()
}
fn default_clickhouse_timeout() -> u64 {
    30
}
impl ClickHouseConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.http_url.is_empty() {
            return Err("clickhouse.http_url 是必需的".to_string());
        }
        if !self.http_url.starts_with("http://") && !self.http_url.starts_with("https://") {
            return Err(format!(
                "clickhouse.http_url must start with http:// or https://, got: {}",
                self.http_url
            ));
        }
        if self.database.is_empty() {
            return Err("clickhouse.database is required".to_string());
        }
        if self.timeout_secs == 0 {
            return Err("clickhouse.timeout_secs must be greater than 0".to_string());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GreptimeDBConfig {
    #[serde(default = "default_greptimedb_database")]
    pub database: String,
    pub http_url: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub credentials_file: Option<String>,
    #[serde(default = "default_greptimedb_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_greptimedb_compression")]
    pub compression: bool,
    #[serde(default = "default_greptimedb_query_timeout")]
    pub query_timeout_secs: u64,
}
impl Default for GreptimeDBConfig {
    fn default() -> Self {
        Self {
            database: default_greptimedb_database(),
            http_url: String::new(),
            user: None,
            password: None,
            credentials_file: None,
            timeout_secs: default_greptimedb_timeout(),
            compression: default_greptimedb_compression(),
            query_timeout_secs: default_greptimedb_query_timeout(),
        }
    }
}
fn default_greptimedb_database() -> String {
    "mazbri_logs".to_string()
}
fn default_greptimedb_timeout() -> u64 {
    30
}
fn default_greptimedb_compression() -> bool {
    true
}
fn default_greptimedb_query_timeout() -> u64 {
    30
}
impl GreptimeDBConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.http_url.is_empty() {
            return Err("greptimedb.http_url 是必需的".to_string());
        }
        if !self.http_url.starts_with("http://") && !self.http_url.starts_with("https://") {
            return Err(format!(
                "greptimedb.http_url must start with http:// or https://, got: {} / \
                 greptimedb.http_url 必须以 http:// 或 https:// 开头，实际值: {}",
                self.http_url, self.http_url
            ));
        }
        if self.database.is_empty() {
            return Err("greptimedb.database is required".to_string());
        }
        if self.timeout_secs == 0 {
            return Err("greptimedb.timeout_secs must be greater than 0".to_string());
        }
        if self.query_timeout_secs == 0 {
            return Err("greptimedb.query_timeout_secs must be greater than 0".to_string());
        }
        if self.user.as_deref() == Some("default") && self.password.as_deref() == Some("") {
            tracing::warn!(
                "GreptimeDB is configured with default credentials (user='default', password=''). \
                 Consider using secure credentials in production! / \
                 GreptimeDB 配置了默认凭据（user='default', password=''）。\
                 生产环境请考虑使用安全的凭据！"
            );
        }
        Ok(())
    }
}
impl AnalyticsConfig {
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
impl ServiceConfig for AnalyticsConfig {
    fn service_name() -> &'static str {
        "mazbri-analytics"
    }
    fn default_port() -> u16 {
        8445
    }
    fn server_config(&self) -> &mazbri_core::ServerConfig {
        &self.server
    }
}
impl ValidateConfig for AnalyticsConfig {
    fn validate(&self) -> Result<()> {
        if let Some(ref greptimedb) = self.greptimedb {
            greptimedb
                .validate()
                .map_err(|e| anyhow::anyhow!("GreptimeDB 配置无效: {}", e))?;
        }
        self.clickhouse
            .validate()
            .map_err(|e| anyhow::anyhow!("ClickHouse 配置无效: {}", e))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ServiceMetricTargetConfig {
    pub key: String,
    pub name: String,
    pub port: u16,
    #[serde(default)]
    pub endpoint: Option<String>,
}
