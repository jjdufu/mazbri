use anyhow::Result;
use mazbri_core::config_loader::{ServiceConfig, ValidateConfig};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CBProExtServiceConfig {
    pub server: mazbri_core::ServerConfig,
    #[serde(default)]
    pub platform_database: Option<mazbri_core::DatabaseConfig>,
    #[serde(default)]
    pub greptime: GreptimeConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub validation: ValidationConfig,
    pub clients: ClientsConfig,
    #[serde(default)]
    pub tls: Option<mazbri_tls::TlsConfig>,
    #[serde(default)]
    pub admin: mazbri_admin::AdminConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GreptimeConfig {
    #[serde(default = "default_greptime_enabled")]
    pub enabled: bool,
    #[serde(default = "default_greptime_http_url")]
    pub http_url: String,
    #[serde(default = "default_greptime_username")]
    pub username: String,
    #[serde(default = "default_greptime_password")]
    pub password: String,
    #[serde(default = "default_greptime_query_interval_secs")]
    pub query_interval_secs: u64,
}

fn default_greptime_enabled() -> bool {
    true
}
fn default_greptime_http_url() -> String {
    "http://logs.greptimedb.local:4000".to_string()
}
fn default_greptime_username() -> String {
    "jwczjzl".to_string()
}
fn default_greptime_password() -> String {
    "P0stGre.s_X398".to_string()
}
fn default_greptime_query_interval_secs() -> u64 {
    30
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    #[serde(default = "default_runtime_decision_ttl_ms")]
    pub decision_ttl_ms: u64,
    #[serde(default = "default_max_runtime_entries")]
    pub max_runtime_entries: usize,
    #[serde(default = "default_latency_baseline_ms")]
    pub default_latency_baseline_ms: u64,
}

fn default_runtime_decision_ttl_ms() -> u64 {
    5_000
}
fn default_max_runtime_entries() -> usize {
    100_000
}
fn default_latency_baseline_ms() -> u64 {
    120
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationConfig {
    #[serde(default = "default_validation_enabled")]
    pub enabled: bool,
    #[serde(default = "default_worker_count")]
    pub worker_count: usize,
    #[serde(default = "default_max_inflight")]
    pub max_inflight: usize,
    #[serde(default = "default_job_poll_interval_ms")]
    pub job_poll_interval_ms: u64,
    #[serde(default = "default_lock_ttl_secs")]
    pub lock_ttl_secs: u64,
    #[serde(default = "default_validation_timeout_secs")]
    pub default_timeout_secs: u64,
}

fn default_validation_enabled() -> bool {
    true
}
fn default_worker_count() -> usize {
    8
}
fn default_max_inflight() -> usize {
    64
}
fn default_job_poll_interval_ms() -> u64 {
    500
}
fn default_lock_ttl_secs() -> u64 {
    60
}
fn default_validation_timeout_secs() -> u64 {
    10
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientsConfig {
    pub cbpro: ServiceEndpoint,
    #[serde(default)]
    pub gateway: Option<ServiceEndpoint>,
}

#[derive(Debug, Clone)]
pub struct ServiceEndpoint {
    pub url: String,
    pub server_name: Option<String>,
    pub timeout_ms: u64,
}

fn default_endpoint_timeout_ms() -> u64 {
    10
}

impl<'de> Deserialize<'de> for ServiceEndpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            String(String),
            Object {
                url: String,
                #[serde(default)]
                server_name: Option<String>,
                #[serde(default = "default_endpoint_timeout_ms")]
                timeout_ms: u64,
            },
        }

        match Helper::deserialize(deserializer)? {
            Helper::String(url) => Ok(Self {
                url,
                server_name: None,
                timeout_ms: default_endpoint_timeout_ms(),
            }),
            Helper::Object {
                url,
                server_name,
                timeout_ms,
            } => Ok(Self {
                url,
                server_name,
                timeout_ms,
            }),
        }
    }
}

impl ServiceEndpoint {
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    #[must_use]
    pub fn server_name(&self) -> Option<&str> {
        self.server_name.as_deref()
    }

    #[must_use]
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

impl ServiceConfig for CBProExtServiceConfig {
    fn service_name() -> &'static str {
        "mazbri-cbpro-ext"
    }

    fn default_port() -> u16 {
        8089
    }

    fn server_config(&self) -> &mazbri_core::ServerConfig {
        &self.server
    }
}

impl ValidateConfig for CBProExtServiceConfig {
    fn validate(&self) -> Result<()> {
        if let Some(ref tls) = self.tls {
            tls.validate()?;
        }
        if self.clients.cbpro.timeout().is_zero() {
            anyhow::bail!("clients.cbpro timeout_ms must be greater than 0");
        }
        if let Some(ref gateway) = self.clients.gateway
            && gateway.timeout().is_zero()
        {
            anyhow::bail!("clients.gateway timeout_ms must be greater than 0");
        }
        if let Some(ref db) = self.platform_database
            && db.credentials_file.is_none()
            && db.url.is_none()
            && (db.username.is_none() || db.password.is_none())
        {
            anyhow::bail!(
                "platform_database requires either 'url', 'username'+'password', or 'credentials_file' \
                 / platform_database 需要 'url'、'username'+'password' 或 'credentials_file'"
            );
        }
        if self.platform_database.is_none() {
            anyhow::bail!("platform_database is required");
        }
        if self.runtime.decision_ttl_ms == 0 {
            anyhow::bail!("runtime.decision_ttl_ms must be greater than 0");
        }
        if self.runtime.max_runtime_entries == 0 {
            anyhow::bail!("runtime.max_runtime_entries must be greater than 0");
        }
        if self.runtime.default_latency_baseline_ms == 0 {
            anyhow::bail!("runtime.default_latency_baseline_ms must be greater than 0");
        }
        if self.validation.worker_count == 0 {
            anyhow::bail!("validation.worker_count must be greater than 0");
        }
        if self.validation.max_inflight == 0 {
            anyhow::bail!("validation.max_inflight must be greater than 0");
        }
        if self.validation.job_poll_interval_ms == 0 {
            anyhow::bail!("validation.job_poll_interval_ms must be greater than 0");
        }
        if self.validation.lock_ttl_secs == 0 {
            anyhow::bail!("validation.lock_ttl_secs must be greater than 0");
        }
        if self.validation.default_timeout_secs == 0 {
            anyhow::bail!("validation.default_timeout_secs must be greater than 0");
        }
        if self.greptime.enabled && self.greptime.http_url.trim().is_empty() {
            anyhow::bail!("greptime.http_url must not be empty when greptime is enabled");
        }
        if self.clients.cbpro.url.trim().is_empty() || !self.clients.cbpro.url.contains(':') {
            anyhow::bail!("clients.cbpro must be in host:port format");
        }
        if let Some(ref gateway) = self.clients.gateway
            && (gateway.url.trim().is_empty() || !gateway.url.contains(':'))
        {
            anyhow::bail!("clients.gateway must be in host:port format");
        }
        Ok(())
    }
}

impl RuntimeConfig {
    #[must_use]
    pub fn decision_ttl(&self) -> Duration {
        Duration::from_millis(self.decision_ttl_ms)
    }
}
