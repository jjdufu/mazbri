use crate::analytics_client::AnalyticsClientConfig;
use anyhow::{Context, Result};
use mazbri_core::config_loader::{ServiceConfig, ValidateConfig};
use serde::Deserialize;
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct CertServiceConfig {
    pub url: String,
    #[serde(alias = "cert_service_token")]
    pub token: Option<String>,
}
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
            refresh_token_expiration_secs: 604800,
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
    /// Default total timeout for probe runs when supplier has no timeouts.total_ms.
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
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct BillingReconciliationConfig {
    /// |OLTP−OLAP| / max(OLTP, 1) 超过该比例则 status=flagged。
    pub loss_ratio_threshold: f64,
    pub default_scope_type: String,
    pub default_scope_id: String,
    /// 出账前是否警告账期内存在未关闭差异（不阻断）。
    pub warn_on_open_discrepancies_before_bill: bool,
    /// 是否启用 mgmt 侧定时日对账 worker。
    pub scheduled_enabled: bool,
    /// 定时对账轮询间隔（秒）；默认 86400=日级。
    pub scheduled_interval_secs: u64,
    /// 定时跑批时对「昨天」做审计（true）或「今天」（false）。
    pub scheduled_use_previous_day: bool,
    /// flagged 时是否 POST webhook。
    pub alert_on_flagged: bool,
    /// 告警 webhook URL（JSON body）；空则不发送。
    pub alert_webhook_url: String,
    /// 对账 details 是否写入金额维（usage sum vs 当日 bill_items 合计）。
    pub include_amount_recon: bool,
    /// 是否从 analytics request_logs 聚合拉取 raw_log_call_count（真·第三方）。
    pub include_raw_log_source: bool,
}

impl Default for BillingReconciliationConfig {
    fn default() -> Self {
        Self {
            loss_ratio_threshold: 0.05,
            default_scope_type: "system".to_string(),
            default_scope_id: String::new(),
            warn_on_open_discrepancies_before_bill: true,
            scheduled_enabled: false,
            scheduled_interval_secs: 86400,
            scheduled_use_previous_day: true,
            alert_on_flagged: false,
            alert_webhook_url: String::new(),
            include_amount_recon: true,
            include_raw_log_source: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MgmtApiConfig {
    #[serde(default)]
    pub service_name: Option<String>,
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
    pub cert_service: CertServiceConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub mfa: MfaConfig,
    #[serde(default)]
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
}
impl MgmtApiConfig {
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
impl ServiceConfig for MgmtApiConfig {
    fn service_name() -> &'static str {
        "mazbri-mgmt-api-v2"
    }
    fn default_port() -> u16 {
        8085
    }
    fn server_config(&self) -> &mazbri_core::ServerConfig {
        &self.server
    }
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
        if self.mfa.lockout_duration < 0 {
            anyhow::bail!("mfa.lockout_duration must be non-negative");
        }
        if self.mfa.backup_codes_count < 1 {
            anyhow::bail!("mfa.backup_codes_count must be at least 1");
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
        if self.billing_reconciliation.scheduled_enabled
            && self.billing_reconciliation.scheduled_interval_secs < 60
        {
            anyhow::bail!(
                "billing_reconciliation.scheduled_interval_secs must be >= 60 when scheduled_enabled"
            );
        }
        if self.billing_reconciliation.alert_on_flagged
            && !self
                .billing_reconciliation
                .alert_webhook_url
                .trim()
                .is_empty()
            && url::Url::parse(self.billing_reconciliation.alert_webhook_url.trim()).is_err()
        {
            anyhow::bail!("billing_reconciliation.alert_webhook_url is not a valid URL");
        }
        self.redis.validate()?;
        if self.cert_service.url.is_empty() {
            anyhow::bail!(
                "cert_service.url is required and must not be empty\n\
                 cert_service.url 是必填项，不能为空"
            );
        }
        if let Err(e) = url::Url::parse(&self.cert_service.url) {
            anyhow::bail!(
                "cert_service.url is not a valid URL: {}\n\
                 cert_service.url 格式非法: {}",
                e,
                e
            );
        }
        if self.analytics.url.is_empty() {
            anyhow::bail!(
                "analytics.url is required and must not be empty\n\
                 analytics.url 是必填项，不能为空"
            );
        }
        if let Err(e) = url::Url::parse(&self.analytics.url) {
            anyhow::bail!(
                "analytics.url is not a valid URL: {}\n\
                 analytics.url 格式非法: {}",
                e,
                e
            );
        }
        self.config_watch.validate()?;
        Ok(())
    }
}
