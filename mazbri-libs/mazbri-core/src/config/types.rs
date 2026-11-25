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

    /// Set `TCP_NODELAY` on accepted connections (reduces Nagle latency).
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
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn socket_addr(&self) -> Result<SocketAddr, std::net::AddrParseError> {
        format!("{}:{}", self.host, self.port).parse()
    }
    #[must_use]
    pub fn graceful_shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.graceful_shutdown_timeout_secs)
    }
    #[must_use]
    pub fn tcp_keepalive(&self) -> Option<Duration> {
        if self.tcp_keepalive_secs == 0 {
            None
        } else {
            Some(Duration::from_secs(self.tcp_keepalive_secs))
        }
    }
    #[must_use]
    pub fn http1_header_read_timeout(&self) -> Option<Duration> {
        if self.http1_header_read_timeout_secs == 0 {
            None
        } else {
            Some(Duration::from_secs(self.http1_header_read_timeout_secs))
        }
    }
}
#[derive(Clone, Deserialize)]
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

impl std::fmt::Debug for DatabaseConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let masked_password = self
            .password
            .as_deref()
            .map(|p| if p.is_empty() { "" } else { "[REDACTED]" });
        f.debug_struct("DatabaseConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("name", &self.name)
            .field("username", &self.username)
            .field("password", &masked_password)
            .field("credentials_file", &self.credentials_file)
            .field("ssl_mode", &self.ssl_mode)
            .field("min_connections", &self.min_connections)
            .field("max_connections", &self.max_connections)
            .field("acquire_timeout_secs", &self.acquire_timeout_secs)
            .field("acquire_timeout_ms", &self.acquire_timeout_ms)
            .field("idle_timeout_secs", &self.idle_timeout_secs)
            .field("max_lifetime_secs", &self.max_lifetime_secs)
            .field("application_name", &self.application_name)
            .finish()
    }
}

/// Maximum character count accepted for `ILIKE` search inputs before truncation.
pub const MAX_ILIKE_QUERY_LEN: usize = 128;

/// Truncates `raw.trim()` to [`MAX_ILIKE_QUERY_LEN`] Unicode scalar values and escapes
/// `\`, `%`, and `_` for PostgreSQL `ILIKE ... ESCAPE '\\'`.
#[must_use]
pub fn escape_ilike_pattern(raw: &str) -> String {
    let trimmed = raw.trim();
    let mut out = String::with_capacity(trimmed.len().min(MAX_ILIKE_QUERY_LEN * 2));
    for ch in trimmed.chars().take(MAX_ILIKE_QUERY_LEN) {
        match ch {
            '\\' => out.push_str("\\\\"),
            '%' => out.push_str("\\%"),
            '_' => out.push_str("\\_"),
            other => out.push(other),
        }
    }
    out
}

/// Builds `%{escaped}%` for PostgreSQL `ILIKE ... ESCAPE '\\'`.
#[must_use]
pub fn contains_ilike_pattern(raw: &str) -> String {
    format!("%{}%", escape_ilike_pattern(raw))
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
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TracingConfig {
    #[serde(default = "default_tracing_level")]
    pub level: String,
    #[serde(default = "default_tracing_format")]
    pub format: String,
    /// OTLP/HTTP receiver base URL (exporter appends `/v1/traces`), e.g. the
    /// Vector receiver `http://vector:4318`. Empty / omitted = OTLP export disabled.
    pub otel_endpoint: Option<String>,
    /// Trace sampling ratio in `0.0..=1.0` when OTLP is enabled (root spans only;
    /// child spans follow the parent's sampled flag). Default `1.0` (full capture).
    #[serde(default = "default_otel_sample_ratio")]
    pub otel_sample_ratio: f64,
    /// Span export queue capacity; spans beyond it are dropped and counted in
    /// `mazbri_otlp_spans_dropped_total`. Default `8192`.
    #[serde(default = "default_otel_max_queue_size")]
    pub otel_max_queue_size: usize,
    /// Spans per OTLP export request. Default `512`.
    #[serde(default = "default_otel_max_export_batch_size")]
    pub otel_max_export_batch_size: usize,
    /// Export interval in milliseconds. Default `1000`.
    #[serde(default = "default_otel_scheduled_delay_ms")]
    pub otel_scheduled_delay_ms: u64,
    #[serde(default = "default_log_dir")]
    pub log_dir: String,
    #[serde(default = "default_log_rotation_enabled")]
    pub rotation_enabled: bool,
}
impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            level: default_tracing_level(),
            format: default_tracing_format(),
            otel_endpoint: None,
            otel_sample_ratio: default_otel_sample_ratio(),
            otel_max_queue_size: default_otel_max_queue_size(),
            otel_max_export_batch_size: default_otel_max_export_batch_size(),
            otel_scheduled_delay_ms: default_otel_scheduled_delay_ms(),
            log_dir: default_log_dir(),
            rotation_enabled: default_log_rotation_enabled(),
        }
    }
}
fn default_otel_sample_ratio() -> f64 {
    1.0
}
fn default_otel_max_queue_size() -> usize {
    8192
}
fn default_otel_max_export_batch_size() -> usize {
    512
}
fn default_otel_scheduled_delay_ms() -> u64 {
    1000
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

    #[test]
    fn debug_redacts_database_and_pg_credentials() {
        let config: DatabaseConfig = toml::from_str(
            r#"
            host = "127.0.0.1"
            username = "mazbri"
            password = "super-secret-db-password"
            "#,
        )
        .expect("parse db config");
        let rendered = format!("{config:?}");
        assert!(!rendered.contains("super-secret-db-password"));
        assert!(rendered.contains("[REDACTED]"));

        let cred = crate::config::PgCredential {
            username: "mazbri".into(),
            password: "raw-pg-password".into(),
        };
        let cred_dbg = format!("{cred:?}");
        assert!(!cred_dbg.contains("raw-pg-password"));
        assert!(cred_dbg.contains("[REDACTED]"));
    }

    #[test]
    fn escape_ilike_escapes_wildcards_and_caps_length() {
        assert_eq!(escape_ilike_pattern("  a%b_c\\d  "), "a\\%b\\_c\\\\d");
        assert_eq!(contains_ilike_pattern("100%_ok"), "%100\\%\\_ok%");
        let long = "x".repeat(200);
        assert_eq!(escape_ilike_pattern(&long).chars().count(), MAX_ILIKE_QUERY_LEN);
    }
}
