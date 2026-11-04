use crate::analytics_client::AnalyticsHttpClient;
use crate::audit::AuditService;
use crate::config::MgmtApiConfig;
use crate::mfa::MfaServiceCache;
use crate::middleware::auth::AdminAuthState;
use crate::services::bypass_probe::BypassProbeClient;
use crate::services::connector_rate_limit::SlidingWindowLimiter;
use crate::services::{BillingReconciliationSettingsCache, MfaSettingsCache, PasswordPolicyCache};
use mazbri_product::ProductService;
use mazbri_upstream_runtime::RuntimeEngine;
use std::sync::Arc;

/// mgmt 访问 cert 所需的地址与凭据；token 非可选。
#[derive(Debug, Clone)]
pub struct CertServiceClientCfg {
    pub url: String,
    pub token: String,
}

#[derive(Clone)]
pub struct AppState {
    pub config: MgmtApiConfig,
    /// cert 地址（来自 `[cert_service].url`）与 token（来自环境变量）。
    pub cert_service: Arc<CertServiceClientCfg>,
    pub redis: Arc<mazbri_core::redis::RedisClient>,
    pub db: std::sync::Arc<sqlx::PgPool>,
    pub auth_db: std::sync::Arc<sqlx::PgPool>,
    pub ratelimit_db: std::sync::Arc<sqlx::PgPool>,
    pub cache_db: std::sync::Arc<sqlx::PgPool>,
    pub billing_db: std::sync::Arc<sqlx::PgPool>,
    pub admin_db: std::sync::Arc<sqlx::PgPool>,
    pub product_service: Arc<ProductService>,
    pub audit: AuditService,
    pub auth_state: Arc<AdminAuthState>,
    pub analytics_client: Arc<AnalyticsHttpClient>,
    pub cert_http_client: reqwest::Client,
    pub password_policy_cache: Arc<PasswordPolicyCache>,
    pub mfa_cache: Arc<MfaServiceCache>,
    pub mfa_settings_cache: Arc<MfaSettingsCache>,
    /// Hot-reloadable usage-reconciliation ops settings (admin DB).
    pub billing_reconciliation_settings: Arc<BillingReconciliationSettingsCache>,
    /// Global limiter for connector draft/live runs.
    pub connector_run_limiter: Arc<SlidingWindowLimiter>,
    /// 仅 SSRF 预检 / `auth_only` / `transform_preview` / microservice 本地探测。
    /// **不得**再作为 business/live 默认 Send，也 **不得**写入 `probe_path=bypass_mgmt_probe`。
    pub connector_runtime: Arc<RuntimeEngine>,
    /// business/live 经 bypass 隔离管理探针 HTTP（不经 gateway 计费）。
    pub bypass_probe: Arc<BypassProbeClient>,
}
