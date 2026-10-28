use crate::analytics_client::AnalyticsClientConfig;
use anyhow::Result;
use mazbri_core::config::ValidateConfig;
use mazbri_core::{CertServiceConfig, Identity, LogRouteConfig, TracingConfig};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct AuthConfig {
    pub jwt_expiration_secs: u64,
    pub refresh_token_expiration_secs: u64,
}
impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_expiration_secs: 86400,
            refresh_token_expiration_secs: 604_800,
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct MfaConfig {
    pub enabled: bool,
    pub issuer: String,
    pub digits: u32,
    pub period: u64,
    pub window: u64,
    pub max_attempts: i32,
    pub lockout_duration: i64,
    pub backup_codes_count: usize,
    /// 提权窗口固定时长（秒）。只来自 bootstrap，不进运行时 `system_settings`。
    pub step_up_ttl_secs: u64,
}
impl Default for MfaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            issuer: "Mazbri".to_string(),
            digits: 6,
            period: 30,
            window: 1,
            max_attempts: 5,
            lockout_duration: 900,
            backup_codes_count: 8,
            step_up_ttl_secs: 900,
        }
    }
}
/// Connector workbench probe limits and retention (FR-W11 / FR-W12).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct ConnectorProbeConfig {
    /// Max probe/live runs per process per minute (sliding window).
    pub max_runs_per_minute: u32,
    /// Max request body bytes accepted on probe (soft guard).
    pub max_body_bytes: usize,
    /// Default total timeout for probe runs when supplier has no `timeouts.total_ms`.
    pub default_timeout_ms: u64,
    /// Delete runs older than this many days (0 = disable cleanup).
    pub run_retention_days: u32,
    /// Background cleanup interval in seconds.
    pub cleanup_interval_secs: u64,
}

impl Default for ConnectorProbeConfig {
    fn default() -> Self {
        Self {
            max_runs_per_minute: 60,
            max_body_bytes: 256 * 1024,
            default_timeout_ms: 15_000,
            run_retention_days: 14,
            cleanup_interval_secs: 3600,
        }
    }
}

/// 用量对账（mgmt 管理面；静态 conf，非配置中心）。
/// 同一结构也是管理端运行时配置（`system_settings`）的形状。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct BillingReconciliationConfig {
    /// |OLTP−OLAP| / max(OLTP, 1) 超过该比例则 status=flagged。
    pub loss_ratio_threshold: f64,
    pub default_scope_type: String,
    pub default_scope_id: String,
    /// 出账前是否警告账期内存在未关闭差异（不阻断）。
    pub warn_on_open_discrepancies_before_bill: bool,
    /// `[billing_reconciliation.schedule]` 定时对账。
    pub schedule: ReconScheduleConfig,
    /// `[billing_reconciliation.alert]` flagged 告警。
    pub alert: ReconAlertConfig,
    /// `[billing_reconciliation.sources]` 对账数据维度。
    pub sources: ReconSourcesConfig,
}

/// mgmt 侧定时日对账 worker。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct ReconScheduleConfig {
    /// 是否启用。
    pub enabled: bool,
    /// 轮询间隔（秒，最小 60）；默认 86400=日级。
    pub interval_secs: u64,
    /// 对「昨天」做审计（true）或「今天」（false）。
    pub use_previous_day: bool,
}

/// flagged 时的 webhook 告警。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct ReconAlertConfig {
    /// flagged 时是否 POST webhook。
    pub on_flagged: bool,
    /// 告警 webhook URL（JSON body）；空则不发送。
    pub webhook_url: String,
}

/// 对账 details 额外数据维度。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct ReconSourcesConfig {
    /// 是否写入金额维（usage sum vs 当日 `bill_items` 合计）。
    pub include_amount_recon: bool,
}

impl Default for BillingReconciliationConfig {
    fn default() -> Self {
        Self {
            loss_ratio_threshold: 0.05,
            default_scope_type: "system".to_string(),
            default_scope_id: String::new(),
            warn_on_open_discrepancies_before_bill: true,
            schedule: ReconScheduleConfig::default(),
            alert: ReconAlertConfig::default(),
            sources: ReconSourcesConfig::default(),
        }
    }
}

impl Default for ReconScheduleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 86400,
            use_previous_day: true,
        }
    }
}

impl Default for ReconSourcesConfig {
    fn default() -> Self {
        Self {
            include_amount_recon: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MgmtApiConfig {
    pub service_name: Identity,
    pub cert_service: CertServiceConfig,
    #[serde(default)]
    pub tracing: TracingConfig,
    #[serde(default)]
    pub log_routes: Vec<LogRouteConfig>,
    pub server: mazbri_core::ServerConfig,
    pub database: mazbri_core::DatabaseConfig,
    pub redis: mazbri_core::redis::RedisConfig,
    pub auth_database: mazbri_core::DatabaseConfig,
    pub ratelimit_database: mazbri_core::DatabaseConfig,
    pub cache_database: mazbri_core::DatabaseConfig,
    pub billing_database: mazbri_core::DatabaseConfig,
    pub admin_database: mazbri_core::DatabaseConfig,
    #[serde(default)]
    pub analytics: AnalyticsClientConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub mfa: MfaConfig,
    pub admin: mazbri_admin::AdminConfig,
    /// Upstream connector probe / live-run limits (optional section).
    #[serde(default)]
    pub connector_probe: ConnectorProbeConfig,
    /// 用量对账阈值与默认 scope（mgmt 管理面）。
    #[serde(default)]
    pub billing_reconciliation: BillingReconciliationConfig,
    /// 产品详情缓存：数据库通知叫醒，轮询保底。
    #[serde(default)]
    pub config_watch: mazbri_core::config_watch::ConfigWatchConfig,
    /// `[ssrf]` 上游 URL 预检与 connector 试跑出站策略（与 bypass 同语义）；
    /// 缺省即 `allow_private_network = false`。
    #[serde(default)]
    pub ssrf: mazbri_upstream_runtime::SsrfConfig,
    /// `[audit]` 统一审计中间件：可信代理、异步写入队列与兜底文件；缺省即不信任任何转发头。
    #[serde(default)]
    pub audit: crate::audit::AuditConfig,
}
impl ValidateConfig for MgmtApiConfig {
    fn validate(&self) -> Result<()> {
        if self.server.port == 0 {
            anyhow::bail!("server.port must be greater than 0");
        }
        if self.mfa.digits < 6 || self.mfa.digits > 8 {
            anyhow::bail!(
                "mfa.digits must be between 6 and 8, got {}",
                self.mfa.digits
            );
        }
        if self.mfa.period == 0 {
            anyhow::bail!("mfa.period must be greater than 0");
        }
        if self.mfa.max_attempts < 1 {
            anyhow::bail!("mfa.max_attempts must be at least 1");
        }
        if self.mfa.lockout_duration < 1 {
            anyhow::bail!(
                "mfa.lockout_duration must be at least 1, got {}",
                self.mfa.lockout_duration
            );
        }
        if self.mfa.backup_codes_count < 1 {
            anyhow::bail!("mfa.backup_codes_count must be at least 1");
        }
        if !(60..=3600).contains(&self.mfa.step_up_ttl_secs) {
            anyhow::bail!(
                "mfa.step_up_ttl_secs must be between 60 and 3600, got {}",
                self.mfa.step_up_ttl_secs
            );
        }
        if self.auth.jwt_expiration_secs == 0 {
            anyhow::bail!("auth.jwt_expiration_secs must be greater than 0");
        }
        if self.auth.refresh_token_expiration_secs == 0 {
            anyhow::bail!("auth.refresh_token_expiration_secs must be greater than 0");
        }
        if self.connector_probe.max_runs_per_minute == 0 {
            anyhow::bail!("connector_probe.max_runs_per_minute must be greater than 0");
        }
        if self.connector_probe.default_timeout_ms == 0 {
            anyhow::bail!("connector_probe.default_timeout_ms must be greater than 0");
        }
        if !self.billing_reconciliation.loss_ratio_threshold.is_finite()
            || self.billing_reconciliation.loss_ratio_threshold < 0.0
            || self.billing_reconciliation.loss_ratio_threshold > 1.0
        {
            anyhow::bail!(
                "billing_reconciliation.loss_ratio_threshold must be in [0.0, 1.0], got {}",
                self.billing_reconciliation.loss_ratio_threshold
            );
        }
        if self
            .billing_reconciliation
            .default_scope_type
            .trim()
            .is_empty()
        {
            anyhow::bail!("billing_reconciliation.default_scope_type must not be empty");
        }
        if self.billing_reconciliation.schedule.enabled
            && self.billing_reconciliation.schedule.interval_secs < 60
        {
            anyhow::bail!(
                "billing_reconciliation.schedule.interval_secs must be >= 60 when schedule.enabled"
            );
        }
        if self.billing_reconciliation.alert.on_flagged
            && !self
                .billing_reconciliation
                .alert
                .webhook_url
                .trim()
                .is_empty()
            && url::Url::parse(self.billing_reconciliation.alert.webhook_url.trim()).is_err()
        {
            anyhow::bail!("billing_reconciliation.alert.webhook_url is not a valid URL");
        }
        self.redis.validate()?;
        self.cert_service.validate()?;
        if self.analytics.url.is_empty() {
            anyhow::bail!(
                "analytics.url is required and must not be empty\n\
                 analytics.url 是必填项，不能为空"
            );
        }
        if let Err(e) = url::Url::parse(&self.analytics.url) {
            anyhow::bail!(
                "analytics.url is not a valid URL: {e}\n\
                 analytics.url 格式非法: {e}"
            );
        }
        self.config_watch.validate()?;
        self.audit.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod ssrf_config_tests {
    use super::MgmtApiConfig;

    const BASE: &str = r#"
service_name = "mazbri-mgmt"

[server]
host = "0.0.0.0"
port = 8888

[database]
name = "mazbri_platform"

[auth_database]
name = "mazbri_auth"

[ratelimit_database]
name = "mazbri_ratelimit"

[cache_database]
name = "mazbri_cache"

[billing_database]
name = "mazbri_billing"

[admin_database]
name = "mazbri_admin"

[redis]
url = "redis://127.0.0.1:6379"

[cert_service]
url = "http://mazbri-cert:8888"

[admin]
port = 9000
"#;

    fn parse(extra: &str) -> Result<MgmtApiConfig, toml::de::Error> {
        toml::from_str(&format!("{BASE}\n{extra}"))
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
    fn ssrf_unknown_key_rejected() {
        assert!(parse("[ssrf]\nallow_private = true\n").is_err());
    }

    #[test]
    fn audit_section_absent_trusts_no_proxy() {
        use mazbri_core::config::ValidateConfig;
        let cfg = parse("").expect("parse");
        assert_eq!(cfg.audit.trusted_proxies, Vec::<String>::new());
        assert!(cfg.audit.queue_capacity > 0);
        cfg.audit.validate().expect("default audit config valid");
    }

    #[test]
    fn audit_trusted_proxies_parse_and_validate() {
        use mazbri_core::config::ValidateConfig;
        let cfg = parse("[audit]\ntrusted_proxies = [\"10.0.0.1\", \"172.16.0.0/12\", \"::1\"]\n")
            .expect("parse");
        assert_eq!(cfg.audit.trusted_proxies.len(), 3);
        cfg.audit.validate().expect("valid proxies");
        let bad = parse("[audit]\ntrusted_proxies = [\"not-an-ip\"]\n").expect("parse");
        assert!(bad.audit.validate().is_err());
    }

    #[test]
    fn audit_unknown_key_rejected() {
        assert!(parse("[audit]\ntrusted_proxy = []\n").is_err());
    }

    #[test]
    fn mfa_step_up_ttl_defaults_and_bounds() {
        use mazbri_core::config::ValidateConfig;
        let mut cfg = parse("").expect("parse");
        assert_eq!(cfg.mfa.step_up_ttl_secs, 900);
        assert_eq!(cfg.mfa.lockout_duration, 900);
        cfg.analytics.url = "http://127.0.0.1:9".to_string();
        cfg.validate().expect("default mfa config valid");

        let mut zero = parse("[mfa]\nlockout_duration = 0\n").expect("parse");
        zero.analytics.url = "http://127.0.0.1:9".to_string();
        assert!(zero.validate().is_err());
        let mut short = parse("[mfa]\nstep_up_ttl_secs = 59\n").expect("parse");
        short.analytics.url = "http://127.0.0.1:9".to_string();
        assert!(short.validate().is_err());
        let mut long = parse("[mfa]\nstep_up_ttl_secs = 3601\n").expect("parse");
        long.analytics.url = "http://127.0.0.1:9".to_string();
        assert!(long.validate().is_err());
        let mut edge = parse("[mfa]\nstep_up_ttl_secs = 60\n").expect("parse");
        edge.analytics.url = "http://127.0.0.1:9".to_string();
        edge.validate().expect("lower bound");
    }
}
