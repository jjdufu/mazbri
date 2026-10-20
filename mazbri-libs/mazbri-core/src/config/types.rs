use serde::Deserialize;
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_shutdown_timeout")]
    pub graceful_shutdown_timeout_secs: u64,

    /// Max concurrent HTTP requests; over limit returns 503 JSON (gateway).
    #[serde(default = "default_max_server_connections")]
    pub max_connections: u32,

    /// Set TCP_NODELAY on accepted connections (reduces Nagle latency).
    #[serde(default = "default_true")]
    pub tcp_nodelay: bool,

    /// OS TCP keepalive idle time in seconds; `0` disables keepalive.
    #[serde(default = "default_tcp_keepalive_secs")]
    pub tcp_keepalive_secs: u64,

    /// HTTP/1 keep-alive (documented / applied when custom serve is used).
    /// axum 0.8 defaults keep-alive on; this flag is honored by gateway serve setup logs.
    #[serde(default = "default_true")]
    pub http1_keep_alive: bool,

    /// HTTP/1 header-read timeout seconds (hyper default 30). `0` keeps framework default.
    #[serde(default = "default_http1_header_read_timeout_secs")]
    pub http1_header_read_timeout_secs: u64,

    /// Response compression (gzip). Gateway may disable for proxy stability.
    #[serde(default = "default_true")]
    pub compression_enabled: bool,
}
fn default_shutdown_timeout() -> u64 {
    30
}
fn default_max_server_connections() -> u32 {
    10000
}
fn default_true() -> bool {
    true
}
fn default_tcp_keepalive_secs() -> u64 {
    60
}
fn default_http1_header_read_timeout_secs() -> u64 {
    30
}
impl ServerConfig {
    pub fn socket_addr(&self) -> Result<SocketAddr, std::net::AddrParseError> {
        format!("{}:{}", self.host, self.port).parse()
    }
    pub fn graceful_shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.graceful_shutdown_timeout_secs)
    }
    pub fn tcp_keepalive(&self) -> Option<Duration> {
        if self.tcp_keepalive_secs == 0 {
            None
        } else {
            Some(Duration::from_secs(self.tcp_keepalive_secs))
        }
    }
    pub fn http1_header_read_timeout(&self) -> Option<Duration> {
        if self.http1_header_read_timeout_secs == 0 {
            None
        } else {
            Some(Duration::from_secs(self.http1_header_read_timeout_secs))
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default = "default_database_port")]
    pub port: u16,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub credentials_file: Option<String>,
    #[serde(default)]
    pub ssl_mode: Option<String>,
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_max_db_connections")]
    pub max_connections: u32,
    #[serde(default = "default_acquire_timeout")]
    pub acquire_timeout_secs: u64,
    /// Optional millisecond precision for pool acquire timeout.
    /// When set (including `0`), overrides `acquire_timeout_secs`.
    /// Use for hot-path read pools that must fail fast instead of waiting tens of seconds.
    #[serde(default)]
    pub acquire_timeout_ms: Option<u64>,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
    #[serde(default = "default_max_lifetime")]
    pub max_lifetime_secs: u64,
    /// `pg_stat_activity.application_name`. Empty falls back to the database name.
    #[serde(default)]
    pub application_name: Option<String>,
}
fn default_database_port() -> u16 {
    5432
}
impl DatabaseConfig {
    /// Effective pool acquire timeout (prefers `acquire_timeout_ms` when present).
    #[must_use]
    pub fn acquire_timeout(&self) -> Duration {
        if let Some(ms) = self.acquire_timeout_ms {
            Duration::from_millis(ms)
        } else {
            Duration::from_secs(self.acquire_timeout_secs)
        }
    }
}
fn default_min_connections() -> u32 {
    1
}
fn default_max_db_connections() -> u32 {
    10
}
fn default_acquire_timeout() -> u64 {
    30
}
fn default_idle_timeout() -> u64 {
    120
}
fn default_max_lifetime() -> u64 {
    1800
}
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TracingConfig {
    #[serde(default = "default_tracing_level")]
    pub level: String,
    #[serde(default = "default_tracing_format")]
    pub format: String,
    /// OTLP HTTP base URL (SDK appends `/v1/traces`).
    /// Greptime example: `http://logs.greptimedb.local:4000/v1/otlp`.
    /// Empty / omitted = OTLP export disabled.
    pub otel_endpoint: Option<String>,
    /// Trace sampling ratio in `0.0..=1.0` when OTLP is enabled. Default `0.1`.
    #[serde(default = "default_otel_sample_ratio")]
    pub otel_sample_ratio: f64,
    /// Greptime database name sent as `x-greptime-db-name` (e.g. `mazbri_traces`).
    #[serde(default)]
    pub otel_db_name: Option<String>,
    /// Greptime pipeline name sent as `x-greptime-pipeline-name`.
    /// Greptime OTLP traces require a pipeline; default `greptime_trace_v1` when endpoint is set.
    #[serde(default)]
    pub otel_pipeline_name: Option<String>,
    /// Optional HTTP basic-auth username for the OTLP endpoint.
    #[serde(default)]
    pub otel_username: Option<String>,
    /// Optional HTTP basic-auth password for the OTLP endpoint.
    #[serde(default)]
    pub otel_password: Option<String>,
    #[serde(default = "default_log_dir")]
    pub log_dir: String,
    #[serde(default = "default_log_rotation_enabled")]
    pub rotation_enabled: bool,
}
fn default_otel_sample_ratio() -> f64 {
    0.1
}
fn default_log_dir() -> String {
    "logs".to_string()
}
fn default_log_rotation_enabled() -> bool {
    true
}
fn default_tracing_level() -> String {
    "info".to_string()
}
fn default_tracing_format() -> String {
    "json".to_string()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_pool_fields_use_small_defaults() {
        let config: DatabaseConfig = toml::from_str("").expect("empty database config");
        assert_eq!(config.min_connections, 1);
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.idle_timeout_secs, 120);
        assert!(config.application_name.is_none());
    }
}
