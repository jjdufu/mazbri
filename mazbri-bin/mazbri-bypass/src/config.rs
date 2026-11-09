use anyhow::Result;
use mazbri_core::config::{ServiceConfig, ValidateConfig};
use mazbri_upstream_runtime::RuntimeOptions;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BypassServiceConfig {
    pub server: mazbri_core::ServerConfig,
    pub database: mazbri_core::DatabaseConfig,
    pub upstream: UpstreamConfig,
    #[serde(default)]
    pub admin: mazbri_admin::AdminConfig,
    /// `[ssrf]` 出站 SSRF 策略；缺省即 `allow_private_network = false`。
    #[serde(default)]
    pub ssrf: mazbri_upstream_runtime::SsrfConfig,
    /// `[mgmt_probe]`：mgmt 工作台试调隔离 HTTP 口（与 8888 mTLS 数据面分离）。
    #[serde(default)]
    pub mgmt_probe: MgmtProbeConfig,
}

/// mgmt ↔ bypass 管理探针口；默认关闭，模板里开启并预置共享 token。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MgmtProbeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_mgmt_probe_host")]
    pub host: String,
    #[serde(default = "default_mgmt_probe_port")]
    pub port: u16,
    /// 与 mgmt bootstrap `mgmt_probe_token` 相同的共享令牌。
    #[serde(default)]
    pub mgmt_probe_token: String,
}

fn default_mgmt_probe_host() -> String {
    "0.0.0.0".to_string()
}

fn default_mgmt_probe_port() -> u16 {
    8890
}

impl Default for MgmtProbeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: default_mgmt_probe_host(),
            port: default_mgmt_probe_port(),
            mgmt_probe_token: String::new(),
        }
    }
}

impl MgmtProbeConfig {
    pub fn validate(&self, data_plane_port: u16, admin_port: u16) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.host.trim().is_empty() {
            anyhow::bail!("mgmt_probe.host must not be empty when enabled");
        }
        if self.port == 0 {
            anyhow::bail!("mgmt_probe.port must be greater than 0 when enabled");
        }
        if self.port == data_plane_port {
            anyhow::bail!(
                "mgmt_probe.port ({}) must differ from data-plane server.port ({})",
                self.port,
                data_plane_port
            );
        }
        if admin_port != 0 && self.port == admin_port {
            anyhow::bail!(
                "mgmt_probe.port ({}) must differ from admin.port ({})",
                self.port,
                admin_port
            );
        }
        if self.mgmt_probe_token.trim().is_empty() {
            anyhow::bail!("mgmt_probe.mgmt_probe_token must not be empty when enabled");
        }
        if self.mgmt_probe_token.len() < 32 {
            anyhow::bail!("mgmt_probe.mgmt_probe_token must be at least 32 characters");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamConfig {
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_ws_handshake_timeout_secs")]
    pub ws_handshake_timeout_secs: u64,
    /// 全局 `ws_unary` 并发上限；超限失败快（无排队、无短连回退）。
    #[serde(default = "default_ws_max_concurrent")]
    pub ws_max_concurrent: u32,
    /// 每池键（`base_url` + `ws.path` + auth 指纹）并发上限。
    #[serde(default = "default_ws_max_concurrent_per_key")]
    pub ws_max_concurrent_per_key: u32,
    /// 每键空闲连接上限。
    #[serde(default = "default_ws_pool_max_idle_per_key")]
    pub ws_pool_max_idle_per_key: u32,
    /// 空闲连接 TTL（秒）；借出时丢弃过期连接。
    #[serde(default = "default_ws_pool_idle_ttl_secs")]
    pub ws_pool_idle_ttl_secs: u64,
    #[serde(default = "default_grpc_max_concurrency")]
    pub grpc_max_concurrency: u32,
    /// 换票响应既无配置的过期字段、也无 `expires_in` 时的令牌缓存秒数。
    #[serde(default = "default_token_default_ttl_secs")]
    pub token_default_ttl_secs: u64,
    /// 令牌提前视为过期的余量（秒），必须小于 `token_default_ttl_secs`。
    #[serde(default = "default_token_refresh_skew_secs")]
    pub token_refresh_skew_secs: u64,
    /// 认证被拒后作废令牌、换票并重试一次。
    #[serde(default = "default_auth_retry_enabled")]
    pub auth_retry_enabled: bool,
    /// 复用的 WS 连接在回复前断开时，换新连接重试一次。
    #[serde(default = "default_ws_stale_retry_enabled")]
    pub ws_stale_retry_enabled: bool,
    /// 借出空闲 WS 连接前做非阻塞存活检查。
    #[serde(default = "default_ws_liveness_check_on_checkout")]
    pub ws_liveness_check_on_checkout: bool,
    /// 重试预算：窗口内重试数 / 首次请求数上限，取值 (0, 1]。
    #[serde(default = "default_retry_budget_ratio")]
    pub retry_budget_ratio: f64,
    /// 低流量时窗口内保底允许的重试次数。
    #[serde(default = "default_retry_budget_min_per_window")]
    pub retry_budget_min_per_window: u32,
    /// 重试预算统计窗口（秒）。
    #[serde(default = "default_retry_budget_window_secs")]
    pub retry_budget_window_secs: u64,
}

fn default_timeout_secs() -> u64 {
    15
}

fn default_connect_timeout_secs() -> u64 {
    3
}

fn default_user_agent() -> String {
    "mazbri-bypass/0.1".to_string()
}

fn default_ws_handshake_timeout_secs() -> u64 {
    10
}

fn default_ws_max_concurrent() -> u32 {
    32
}

fn default_ws_max_concurrent_per_key() -> u32 {
    8
}

fn default_ws_pool_max_idle_per_key() -> u32 {
    4
}

fn default_ws_pool_idle_ttl_secs() -> u64 {
    60
}

fn default_grpc_max_concurrency() -> u32 {
    64
}

fn default_token_default_ttl_secs() -> u64 {
    RuntimeOptions::default().token_default_ttl.as_secs()
}

fn default_token_refresh_skew_secs() -> u64 {
    RuntimeOptions::default().token_refresh_skew.as_secs()
}

fn default_auth_retry_enabled() -> bool {
    RuntimeOptions::default().auth_retry_enabled
}

fn default_ws_stale_retry_enabled() -> bool {
    RuntimeOptions::default().ws_stale_retry_enabled
}

fn default_ws_liveness_check_on_checkout() -> bool {
    RuntimeOptions::default().ws_liveness_check_on_checkout
}

fn default_retry_budget_ratio() -> f64 {
    RuntimeOptions::default().retry_budget_ratio
}

fn default_retry_budget_min_per_window() -> u32 {
    RuntimeOptions::default().retry_budget_min_per_window
}

fn default_retry_budget_window_secs() -> u64 {
    RuntimeOptions::default().retry_budget_window.as_secs()
}

impl From<&UpstreamConfig> for RuntimeOptions {
    fn from(c: &UpstreamConfig) -> Self {
        Self {
            ws_handshake_timeout: Duration::from_secs(c.ws_handshake_timeout_secs),
            ws_max_concurrent: c.ws_max_concurrent,
            ws_max_concurrent_per_key: c.ws_max_concurrent_per_key,
            ws_pool_max_idle_per_key: c.ws_pool_max_idle_per_key,
            ws_pool_idle_ttl: Duration::from_secs(c.ws_pool_idle_ttl_secs),
            token_default_ttl: Duration::from_secs(c.token_default_ttl_secs),
            token_refresh_skew: Duration::from_secs(c.token_refresh_skew_secs),
            auth_retry_enabled: c.auth_retry_enabled,
            ws_stale_retry_enabled: c.ws_stale_retry_enabled,
            ws_liveness_check_on_checkout: c.ws_liveness_check_on_checkout,
            retry_budget_ratio: c.retry_budget_ratio,
            retry_budget_min_per_window: c.retry_budget_min_per_window,
            retry_budget_window: Duration::from_secs(c.retry_budget_window_secs),
        }
    }
}

impl UpstreamConfig {
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_secs)
    }
    pub fn validate(&self) -> Result<()> {
        if self.timeout_secs == 0 {
            anyhow::bail!("timeout_secs must be greater than 0");
        }
        if self.connect_timeout_secs == 0 {
            anyhow::bail!("connect_timeout_secs must be greater than 0");
        }
        if self.grpc_max_concurrency == 0 {
            anyhow::bail!("grpc_max_concurrency must be greater than 0");
        }
        RuntimeOptions::from(self)
            .validate()
            .map_err(|e| anyhow::anyhow!("upstream: {e}"))
    }
}

impl ServiceConfig for BypassServiceConfig {
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

impl ValidateConfig for BypassServiceConfig {
    fn validate(&self) -> Result<()> {
        self.upstream.validate()?;
        self.mgmt_probe.validate(self.server.port, self.admin.port)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"
[server]
host = "0.0.0.0"
port = 8888

[database]
host = "127.0.0.1"
port = 5432
name = "mazbri_platform"
username = "u"
password = "p"

[upstream]
"#;

    fn parse(extra: &str) -> Result<BypassServiceConfig, toml::de::Error> {
        toml::from_str(&format!("{BASE}\n{extra}"))
    }

    #[test]
    fn upstream_runtime_keys_default_when_omitted() {
        let cfg = parse("").expect("parse");
        let u = &cfg.upstream;
        assert_eq!(u.token_default_ttl_secs, 3600);
        assert_eq!(u.token_refresh_skew_secs, 30);
        assert!(u.auth_retry_enabled);
        assert!(u.ws_stale_retry_enabled);
        assert!(u.ws_liveness_check_on_checkout);
        assert!((u.retry_budget_ratio - 0.2).abs() < f64::EPSILON);
        assert_eq!(u.retry_budget_min_per_window, 3);
        assert_eq!(u.retry_budget_window_secs, 10);
        assert!(u.validate().is_ok());
    }

    #[test]
    fn upstream_runtime_options_map_field_by_field() {
        let cfg = parse(
            "token_default_ttl_secs = 1800\ntoken_refresh_skew_secs = 20\nauth_retry_enabled = false\nws_stale_retry_enabled = false\nws_liveness_check_on_checkout = false\nretry_budget_ratio = 0.5\nretry_budget_min_per_window = 1\nretry_budget_window_secs = 5\nws_pool_idle_ttl_secs = 45\nws_max_concurrent = 7",
        )
        .expect("parse");
        let o = RuntimeOptions::from(&cfg.upstream);
        assert_eq!(o.token_default_ttl, Duration::from_mins(30));
        assert_eq!(o.token_refresh_skew, Duration::from_secs(20));
        assert!(!o.auth_retry_enabled);
        assert!(!o.ws_stale_retry_enabled);
        assert!(!o.ws_liveness_check_on_checkout);
        assert!((o.retry_budget_ratio - 0.5).abs() < f64::EPSILON);
        assert_eq!(o.retry_budget_min_per_window, 1);
        assert_eq!(o.retry_budget_window, Duration::from_secs(5));
        assert_eq!(o.ws_pool_idle_ttl, Duration::from_secs(45));
        assert_eq!(o.ws_max_concurrent, 7);
    }

    #[test]
    fn upstream_runtime_invalid_values_rejected() {
        for extra in [
            "retry_budget_ratio = 0.0",
            "retry_budget_ratio = 1.5",
            "token_refresh_skew_secs = 3600",
            "retry_budget_window_secs = 0",
            "token_default_ttl_secs = 0",
            "ws_pool_idle_ttl_secs = 0",
        ] {
            let cfg = parse(extra).expect("parse");
            assert!(cfg.upstream.validate().is_err(), "{extra}");
        }
    }

    #[test]
    fn upstream_unknown_key_rejected() {
        assert!(parse("auth_retry = true").is_err());
    }

    #[test]
    fn ssrf_section_absent_defaults_to_deny_private() {
        let cfg = parse("").expect("parse");
        assert!(!cfg.ssrf.allow_private_network);
        assert!(!cfg.ssrf.checker().allow_private());
    }

    #[test]
    fn ssrf_allow_private_network_true() {
        let cfg = parse("[ssrf]\nallow_private_network = true\n").expect("parse");
        assert!(cfg.ssrf.allow_private_network);
        assert!(cfg.ssrf.checker().allow_private());
    }

    #[test]
    fn ssrf_explicit_false_and_unknown_key_rejected() {
        let cfg = parse("[ssrf]\nallow_private_network = false\n").expect("parse");
        assert!(!cfg.ssrf.allow_private_network);
        assert!(parse("[ssrf]\nallow_private = true\n").is_err());
    }

    #[test]
    fn mgmt_probe_defaults_disabled() {
        let cfg = parse("").expect("parse");
        assert!(!cfg.mgmt_probe.enabled);
        assert_eq!(cfg.mgmt_probe.port, 8890);
    }

    #[test]
    fn mgmt_probe_enabled_requires_token() {
        let cfg = parse(
            r#"[mgmt_probe]
enabled = true
host = "0.0.0.0"
port = 8890
mgmt_probe_token = "1a71e6a9d100abcb09537e333051dd63e102f39491df18588f8c6bb5a35ed256"
"#,
        )
        .expect("parse");
        assert!(cfg.validate().is_ok());
        let bad = parse(
            r#"[mgmt_probe]
enabled = true
port = 8890
mgmt_probe_token = ""
"#,
        )
        .expect("parse");
        assert!(bad.validate().is_err());
    }
}
