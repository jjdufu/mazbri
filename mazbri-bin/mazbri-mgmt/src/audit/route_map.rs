//! 路由 → 审计动作映射。每条 mgmt API 路由都必须在此登记（由 `route_coverage` 测试保证）。

use super::types::{AuditAction as A, AuditResourceType as R};

/// API 前缀（路由模板在表中省略该前缀）。
pub const API_PREFIX: &str = "/api/v1/mgmt";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteKind {
    /// 登录 / MFA 等认证入口：成功与失败一律落库。
    Auth,
    /// 写操作：落库（关闭审计开关时仅跳过低风险成功记录）。
    Write,
    /// 敏感读取（完整证书、导出、审计日志、密钥列表等）：一律落库。
    SensitiveRead,
    /// 普通读取：只写结构化日志，401/403 才落库。
    Read,
}

#[derive(Debug, Clone, Copy)]
pub struct RouteAudit {
    pub method: &'static str,
    /// 相对 `/api/v1/mgmt` 的 axum 路由模板。
    pub path: &'static str,
    /// 普通读取没有专属动作，记录为 `api.read`。
    pub action: Option<A>,
    pub resource_type: R,
    /// 作为 `resource_id` 的路径参数名。
    pub resource_param: Option<&'static str>,
    pub kind: RouteKind,
    /// 安全事件：不受 `enable_audit_log` 开关影响。
    pub security: bool,
    /// 关键事件：同步落库。
    pub critical: bool,
}

const fn write(
    method: &'static str,
    path: &'static str,
    action: A,
    resource_type: R,
    resource_param: Option<&'static str>,
) -> RouteAudit {
    RouteAudit {
        method,
        path,
        action: Some(action),
        resource_type,
        resource_param,
        kind: RouteKind::Write,
        security: false,
        critical: false,
    }
}

/// 权限、账号、凭据、安全配置类写操作：安全事件 + 同步落库。
const fn secure(
    method: &'static str,
    path: &'static str,
    action: A,
    resource_type: R,
    resource_param: Option<&'static str>,
) -> RouteAudit {
    RouteAudit {
        security: true,
        critical: true,
        ..write(method, path, action, resource_type, resource_param)
    }
}

const fn auth(path: &'static str, action: A) -> RouteAudit {
    RouteAudit {
        kind: RouteKind::Auth,
        ..secure("POST", path, action, R::User, None)
    }
}

const fn sensitive(
    path: &'static str,
    action: A,
    resource_type: R,
    resource_param: Option<&'static str>,
    critical: bool,
) -> RouteAudit {
    RouteAudit {
        method: "GET",
        path,
        action: Some(action),
        resource_type,
        resource_param,
        kind: RouteKind::SensitiveRead,
        security: true,
        critical,
    }
}

const fn read(path: &'static str, resource_type: R) -> RouteAudit {
    RouteAudit {
        method: "GET",
        path,
        action: None,
        resource_type,
        resource_param: None,
        kind: RouteKind::Read,
        security: false,
        critical: false,
    }
}

const ID: Option<&str> = Some("id");
const CODE: Option<&str> = Some("code");
const PRODUCT: Option<&str> = Some("product_id");
const SUPPLIER: Option<&str> = Some("supplier_id");
const SERVICE: Option<&str> = Some("service_name");
const VERSION: Option<&str> = Some("version");

pub static ROUTES: &[RouteAudit] = &[
    // ---- auth ----
    auth("/auth/login", A::UserLogin),
    secure(
        "POST",
        "/auth/first-login-change-password",
        A::PasswordChangeFirstLogin,
        R::User,
        None,
    ),
    read("/auth/me", R::User),
    secure(
        "PUT",
        "/auth/change-password",
        A::PasswordChange,
        R::User,
        None,
    ),
    write("POST", "/auth/refresh", A::UserTokenRefresh, R::User, None),
    secure("POST", "/auth/logout", A::UserLogout, R::User, None),
    auth("/auth/mfa/required-setup", A::MfaRequiredSetup),
    auth("/auth/mfa/required-enable", A::MfaRequiredEnable),
    auth("/auth/mfa/setup", A::MfaSetup),
    secure("POST", "/auth/mfa/enable", A::MfaEnable, R::User, None),
    secure("POST", "/auth/mfa/disable", A::MfaDisable, R::User, None),
    auth("/auth/mfa/verify", A::UserLogin),
    read("/auth/mfa/status", R::User),
    read("/auth/mfa/step-up", R::User),
    secure("POST", "/auth/mfa/step-up", A::MfaStepUp, R::User, None),
    secure(
        "DELETE",
        "/auth/mfa/step-up",
        A::MfaStepUpEnd,
        R::User,
        None,
    ),
    secure(
        "POST",
        "/auth/mfa/regenerate-codes",
        A::MfaRegenerateCodes,
        R::User,
        None,
    ),
    // ---- products ----
    read("/products", R::Product),
    write("POST", "/products", A::ProductCreate, R::Product, None),
    write(
        "POST",
        "/products/reservations",
        A::ProductReservationCreate,
        R::Product,
        None,
    ),
    write(
        "DELETE",
        "/products/reservations/{reservation_id}",
        A::ProductReservationRelease,
        R::Product,
        Some("reservation_id"),
    ),
    read("/products/{product_id}", R::Product),
    write(
        "PUT",
        "/products/{product_id}",
        A::ProductUpdate,
        R::Product,
        PRODUCT,
    ),
    write(
        "DELETE",
        "/products/{product_id}",
        A::ProductDelete,
        R::Product,
        PRODUCT,
    ),
    secure(
        "POST",
        "/products/{product_id}/grant",
        A::ProductGrant,
        R::Product,
        PRODUCT,
    ),
    secure(
        "POST",
        "/products/{product_id}/revoke",
        A::ProductRevoke,
        R::Product,
        PRODUCT,
    ),
    read("/products/{product_id}/tenants", R::Product),
    read("/authorizations", R::Product),
    // ---- suppliers / channels / routes ----
    read("/suppliers", R::Supplier),
    write("POST", "/suppliers", A::SupplierCreate, R::Supplier, None),
    read("/suppliers/{id}", R::Supplier),
    write("PUT", "/suppliers/{id}", A::SupplierUpdate, R::Supplier, ID),
    write(
        "DELETE",
        "/suppliers/{id}",
        A::SupplierDelete,
        R::Supplier,
        ID,
    ),
    read("/channels", R::Channel),
    write("POST", "/channels", A::ChannelCreate, R::Channel, None),
    read("/channels/next-code", R::Channel),
    read("/channels/{id}", R::Channel),
    write("PUT", "/channels/{id}", A::ChannelUpdate, R::Channel, ID),
    write("DELETE", "/channels/{id}", A::ChannelDelete, R::Channel, ID),
    read("/route-rules", R::RouteRule),
    write(
        "POST",
        "/route-rules",
        A::RouteRuleCreate,
        R::RouteRule,
        None,
    ),
    read("/route-rules/{id}", R::RouteRule),
    write(
        "PUT",
        "/route-rules/{id}",
        A::RouteRuleUpdate,
        R::RouteRule,
        ID,
    ),
    write(
        "DELETE",
        "/route-rules/{id}",
        A::RouteRuleDelete,
        R::RouteRule,
        ID,
    ),
    write(
        "POST",
        "/route-rules/{id}/emergency-switch",
        A::RouteEmergencySwitch,
        R::RouteRule,
        ID,
    ),
    read(
        "/route-rules/{route_rule_id}/bindings",
        R::RouteChannelBinding,
    ),
    write(
        "POST",
        "/route-rules/{route_rule_id}/bindings",
        A::RouteChannelBindingCreate,
        R::RouteChannelBinding,
        Some("route_rule_id"),
    ),
    write(
        "PUT",
        "/route-channel-bindings/{id}",
        A::RouteChannelBindingUpdate,
        R::RouteChannelBinding,
        ID,
    ),
    write(
        "DELETE",
        "/route-channel-bindings/{id}",
        A::RouteChannelBindingDelete,
        R::RouteChannelBinding,
        ID,
    ),
    read("/gateway/route-version", R::RouteRule),
    write(
        "POST",
        "/upstreams/onboarding",
        A::UpstreamOnboarding,
        R::Supplier,
        None,
    ),
    write(
        "POST",
        "/upstreams/onboarding/dry-run",
        A::UpstreamOnboardingDryRun,
        R::Supplier,
        None,
    ),
    write(
        "POST",
        "/suppliers/{id}/validate-config",
        A::SupplierValidateConfig,
        R::Supplier,
        ID,
    ),
    // ---- upstream connectors ----
    read("/upstream-connectors/drafts", R::Connector),
    write(
        "POST",
        "/upstream-connectors/drafts",
        A::ConnectorDraftCreate,
        R::Connector,
        None,
    ),
    read("/upstream-connectors/drafts/{id}", R::Connector),
    write(
        "PATCH",
        "/upstream-connectors/drafts/{id}",
        A::ConnectorDraftUpdate,
        R::Connector,
        ID,
    ),
    write(
        "DELETE",
        "/upstream-connectors/drafts/{id}",
        A::ConnectorDraftDelete,
        R::Connector,
        ID,
    ),
    write(
        "POST",
        "/upstream-connectors/drafts/{id}/validate",
        A::ConnectorDraftValidate,
        R::Connector,
        ID,
    ),
    write(
        "POST",
        "/upstream-connectors/drafts/{id}/transform-preview",
        A::ConnectorTransformPreview,
        R::Connector,
        ID,
    ),
    read("/upstream-connectors/drafts/{id}/runs", R::Connector),
    write(
        "POST",
        "/upstream-connectors/drafts/{id}/runs",
        A::ConnectorRun,
        R::Connector,
        ID,
    ),
    read("/upstream-connectors/runs/{run_id}", R::Connector),
    write(
        "POST",
        "/upstream-connectors/drafts/{id}/publish",
        A::ConnectorPublish,
        R::Connector,
        ID,
    ),
    write(
        "POST",
        "/upstream-connectors/import/curl",
        A::ConnectorImport,
        R::Connector,
        None,
    ),
    write(
        "POST",
        "/upstream-connectors/import/postman",
        A::ConnectorImport,
        R::Connector,
        None,
    ),
    write(
        "POST",
        "/upstream-connectors/import/openapi",
        A::ConnectorImport,
        R::Connector,
        None,
    ),
    write(
        "POST",
        "/upstream-connectors/drafts/from-supplier/{supplier_id}",
        A::ConnectorDraftFromSupplier,
        R::Connector,
        SUPPLIER,
    ),
    sensitive(
        "/upstream-connectors/drafts/{id}/export/postman",
        A::ConnectorExport,
        R::Connector,
        ID,
        true,
    ),
    read(
        "/upstream-connectors/suppliers/{supplier_id}/versions",
        R::Connector,
    ),
    read(
        "/upstream-connectors/suppliers/{supplier_id}/versions/{vid}/diff",
        R::Connector,
    ),
    write(
        "POST",
        "/upstream-connectors/suppliers/{supplier_id}/rollback",
        A::ConnectorRollback,
        R::Supplier,
        SUPPLIER,
    ),
    write(
        "POST",
        "/upstream-connectors/suppliers/{supplier_id}/runs",
        A::ConnectorRun,
        R::Supplier,
        SUPPLIER,
    ),
    write(
        "POST",
        "/upstream-connectors/artifacts",
        A::ConnectorArtifactCreate,
        R::Connector,
        None,
    ),
    read("/upstream-connectors/artifacts/{id}", R::Connector),
    // ---- tenants ----
    read("/tenants", R::Tenant),
    write("POST", "/tenants", A::TenantCreate, R::Tenant, None),
    read("/tenants/{code}", R::Tenant),
    write("PUT", "/tenants/{code}", A::TenantUpdate, R::Tenant, CODE),
    secure(
        "DELETE",
        "/tenants/{code}",
        A::TenantDelete,
        R::Tenant,
        CODE,
    ),
    write(
        "PUT",
        "/tenants/{code}/enabled",
        A::TenantEnabledChange,
        R::Tenant,
        CODE,
    ),
    read("/tenants/{code}/crypto-profile", R::Tenant),
    secure(
        "PUT",
        "/tenants/{code}/crypto-profile",
        A::TenantCryptoProfileUpdate,
        R::Tenant,
        CODE,
    ),
    sensitive(
        "/tenants/{code}/crypto-keys",
        A::TenantCryptoKeyList,
        R::Tenant,
        CODE,
        false,
    ),
    secure(
        "POST",
        "/tenants/{code}/crypto-keys",
        A::TenantCryptoKeyCreate,
        R::Tenant,
        CODE,
    ),
    secure(
        "POST",
        "/tenants/{code}/crypto-keys/rotate",
        A::TenantCryptoKeyRotate,
        R::Tenant,
        CODE,
    ),
    secure(
        "POST",
        "/tenants/{code}/crypto-keys/{version}/activate",
        A::TenantCryptoKeyActivate,
        R::Tenant,
        VERSION,
    ),
    secure(
        "POST",
        "/tenants/{code}/crypto-keys/{version}/deprecate",
        A::TenantCryptoKeyDeprecate,
        R::Tenant,
        VERSION,
    ),
    secure(
        "POST",
        "/tenants/{code}/crypto-keys/{version}/disable",
        A::TenantCryptoKeyDisable,
        R::Tenant,
        VERSION,
    ),
    secure(
        "DELETE",
        "/tenants/{code}/crypto-keys/{version}",
        A::TenantCryptoKeyDelete,
        R::Tenant,
        VERSION,
    ),
    sensitive(
        "/tenant-crypto/export",
        A::TenantCryptoExport,
        R::Tenant,
        None,
        true,
    ),
    write(
        "POST",
        "/tenant-crypto/verify",
        A::TenantCryptoVerify,
        R::Tenant,
        None,
    ),
    sensitive(
        "/tenants/{code}/request-keys",
        A::RequestKeyList,
        R::Tenant,
        CODE,
        false,
    ),
    secure(
        "POST",
        "/tenants/{code}/request-keys",
        A::RequestKeyCreate,
        R::Tenant,
        CODE,
    ),
    secure(
        "POST",
        "/tenants/{code}/request-keys/{kid}/retire",
        A::RequestKeyRetire,
        R::Tenant,
        Some("kid"),
    ),
    read("/tenants/{code}/products", R::Product),
    read("/tenants/{code}/products/{product_id}/config", R::Product),
    write(
        "PUT",
        "/tenants/{code}/products/{product_id}/config",
        A::TenantProductConfigUpdate,
        R::Product,
        PRODUCT,
    ),
    read("/tenants/{code}/account", R::Account),
    secure(
        "POST",
        "/tenants/{code}/account/recharge",
        A::AccountRecharge,
        R::Account,
        CODE,
    ),
    secure(
        "POST",
        "/tenants/{code}/account/refund",
        A::AccountRefund,
        R::Account,
        CODE,
    ),
    read("/tenants/{code}/account/transactions", R::Account),
    secure(
        "POST",
        "/tenants/{code}/api-keys",
        A::ApiKeyCreate,
        R::ApiKey,
        CODE,
    ),
    sensitive(
        "/tenants/{code}/api-keys",
        A::ApiKeyList,
        R::ApiKey,
        CODE,
        false,
    ),
    secure(
        "PUT",
        "/tenants/{code}/api-keys/{key_prefix}",
        A::ApiKeyUpdate,
        R::ApiKey,
        Some("key_prefix"),
    ),
    secure(
        "DELETE",
        "/tenants/{code}/api-keys/{key_prefix}",
        A::ApiKeyRevoke,
        R::ApiKey,
        Some("key_prefix"),
    ),
    read("/tenants/{code}/whitelist", R::Tenant),
    secure(
        "POST",
        "/tenants/{code}/whitelist",
        A::WhitelistAdd,
        R::Tenant,
        CODE,
    ),
    secure(
        "DELETE",
        "/tenants/{code}/whitelist/{ip}",
        A::WhitelistRemove,
        R::Tenant,
        CODE,
    ),
    read("/tenants/{code}/config/ratelimit", R::Tenant),
    write(
        "PUT",
        "/tenants/{code}/config/ratelimit",
        A::TenantRateLimitUpdate,
        R::Tenant,
        CODE,
    ),
    write(
        "DELETE",
        "/tenants/{code}/config/ratelimit",
        A::TenantRateLimitDelete,
        R::Tenant,
        CODE,
    ),
    read("/tenants/{code}/ratelimit/commands", R::Tenant),
    write(
        "POST",
        "/tenants/{code}/ratelimit/commands",
        A::TenantRateLimitCommandCreate,
        R::Tenant,
        CODE,
    ),
    write(
        "POST",
        "/tenants/{code}/ratelimit/commands/{id}/cancel",
        A::TenantRateLimitCommandCancel,
        R::Tenant,
        ID,
    ),
    read("/tenants/{code}/config/cache", R::Tenant),
    write(
        "PUT",
        "/tenants/{code}/config/cache",
        A::TenantCacheUpdate,
        R::Tenant,
        CODE,
    ),
    read("/config/ratelimit", R::System),
    // ---- analytics ----
    read("/tenants/{code}/analytics", R::Tenant),
    read("/tenants/{code}/analytics/ratelimit", R::Tenant),
    read("/stats/platform", R::System),
    read("/stats/ratelimit", R::System),
    read("/stats/metrics/overview", R::System),
    read("/stats/metrics/timeseries", R::System),
    read("/stats/metrics/catalog", R::System),
    read("/stats/calls/customer", R::System),
    read("/stats/calls/upstream", R::System),
    // ---- settings / audit ----
    read("/settings/system", R::System),
    secure(
        "PUT",
        "/settings/system",
        A::SettingsUpdate,
        R::System,
        None,
    ),
    read("/settings/user", R::User),
    write(
        "PUT",
        "/settings/user",
        A::UserSettingsUpdate,
        R::User,
        None,
    ),
    read("/settings/appearance", R::System),
    write(
        "PUT",
        "/settings/appearance",
        A::AppearanceUpdate,
        R::System,
        None,
    ),
    write(
        "POST",
        "/settings/appearance/reset",
        A::AppearanceReset,
        R::System,
        None,
    ),
    read("/settings/password-policy", R::System),
    secure(
        "PUT",
        "/settings/password-policy",
        A::PasswordPolicyUpdate,
        R::System,
        None,
    ),
    read("/settings/mfa", R::System),
    secure(
        "PUT",
        "/settings/mfa",
        A::MfaSettingsUpdate,
        R::System,
        None,
    ),
    sensitive("/audit-logs", A::AuditLogList, R::AuditLog, None, false),
    sensitive("/audit-logs/{id}", A::AuditLogView, R::AuditLog, ID, false),
    sensitive(
        "/audit-logs/export",
        A::AuditLogExport,
        R::AuditLog,
        None,
        true,
    ),
    // ---- billing ----
    read("/billing/accounts", R::Billing),
    read("/billing/bills", R::Billing),
    write(
        "POST",
        "/billing/bills/generate",
        A::BillGenerate,
        R::Billing,
        None,
    ),
    write(
        "POST",
        "/billing/bills/generate-async",
        A::BillGenerateAsync,
        R::Billing,
        None,
    ),
    write(
        "POST",
        "/billing/reconciliation/audit",
        A::ReconciliationAudit,
        R::Billing,
        None,
    ),
    read("/billing/reconciliation/settings", R::Billing),
    write(
        "PUT",
        "/billing/reconciliation/settings",
        A::ReconciliationSettingsUpdate,
        R::Billing,
        None,
    ),
    read("/billing/reconciliation/discrepancies", R::Billing),
    write(
        "PATCH",
        "/billing/reconciliation/discrepancies/{id}",
        A::ReconciliationDiscrepancyUpdate,
        R::Billing,
        ID,
    ),
    write(
        "DELETE",
        "/billing/reconciliation/discrepancies/{id}",
        A::ReconciliationDiscrepancyDelete,
        R::Billing,
        ID,
    ),
    read("/billing/bills/{id}", R::Billing),
    write(
        "DELETE",
        "/billing/bills/{id}",
        A::BillDelete,
        R::Billing,
        ID,
    ),
    write(
        "POST",
        "/billing/bills/{id}/status",
        A::BillStatusUpdate,
        R::Billing,
        ID,
    ),
    read("/billing/bills/{id}/adjustments", R::Billing),
    write(
        "POST",
        "/billing/bills/{id}/adjustments",
        A::BillAdjustmentCreate,
        R::Billing,
        ID,
    ),
    read("/billing/adjustments", R::Billing),
    write(
        "POST",
        "/billing/adjustments/{id}/void",
        A::BillAdjustmentVoid,
        R::Billing,
        ID,
    ),
    write(
        "POST",
        "/billing/adjustment",
        A::AccountAdjustment,
        R::Account,
        None,
    ),
    // ---- admins ----
    read("/admins", R::Admin),
    secure("POST", "/admins", A::AdminCreate, R::Admin, None),
    read("/admins/{id}", R::Admin),
    secure("PUT", "/admins/{id}", A::AdminUpdate, R::Admin, ID),
    secure("DELETE", "/admins/{id}", A::AdminDelete, R::Admin, ID),
    secure(
        "PUT",
        "/admins/{id}/password",
        A::AdminPasswordReset,
        R::Admin,
        ID,
    ),
    secure(
        "PUT",
        "/admins/{id}/status",
        A::AdminStatusChange,
        R::Admin,
        ID,
    ),
    secure(
        "PUT",
        "/admins/{id}/force-password-reset",
        A::AdminForcePasswordReset,
        R::Admin,
        ID,
    ),
    secure(
        "POST",
        "/admins/{id}/reset-mfa",
        A::AdminMfaReset,
        R::Admin,
        ID,
    ),
    // ---- config center ----
    read("/config-center/configs", R::ConfigCenter),
    write(
        "POST",
        "/config-center/configs",
        A::ConfigCenterCreate,
        R::ConfigCenter,
        None,
    ),
    read("/config-center/configs/{service_name}", R::ConfigCenter),
    write(
        "PUT",
        "/config-center/configs/{service_name}",
        A::ConfigCenterUpdate,
        R::ConfigCenter,
        SERVICE,
    ),
    write(
        "DELETE",
        "/config-center/configs/{service_name}",
        A::ConfigCenterDelete,
        R::ConfigCenter,
        SERVICE,
    ),
    read(
        "/config-center/configs/{service_name}/history",
        R::ConfigCenter,
    ),
    write(
        "POST",
        "/config-center/configs/{service_name}/rollback/{version}",
        A::ConfigCenterRollback,
        R::ConfigCenter,
        SERVICE,
    ),
    read("/config-center/services", R::ConfigCenter),
    read("/config-center/service-names", R::ConfigCenter),
    sensitive(
        "/config-center/export",
        A::ConfigCenterExport,
        R::ConfigCenter,
        None,
        true,
    ),
    write(
        "POST",
        "/config-center/import",
        A::ConfigCenterImport,
        R::ConfigCenter,
        None,
    ),
    write(
        "POST",
        "/config-center/validate",
        A::ConfigCenterValidate,
        R::ConfigCenter,
        None,
    ),
    read("/config-center/nodes", R::ConfigCenter),
    // ---- callbacks ----
    read("/callbacks/subscriptions", R::Callback),
    write(
        "POST",
        "/callbacks/subscriptions",
        A::CallbackSubscriptionCreate,
        R::Callback,
        None,
    ),
    write(
        "DELETE",
        "/callbacks/subscriptions/{id}",
        A::CallbackSubscriptionDelete,
        R::Callback,
        ID,
    ),
    read("/callbacks/deliveries", R::Callback),
    write(
        "POST",
        "/callbacks/deliveries/{id}/retry",
        A::CallbackDeliveryRetry,
        R::Callback,
        ID,
    ),
    // ---- certificates (proxy to cert service) ----
    read("/certificates", R::Certificate),
    read("/certificates/{id}", R::Certificate),
    sensitive(
        "/certificates/{id}/full",
        A::CertificateViewFull,
        R::Certificate,
        ID,
        true,
    ),
    read("/certificates/{id}/auto-renewal", R::Certificate),
    write(
        "PUT",
        "/certificates/{id}/auto-renewal",
        A::CertificateAutoRenewalUpdate,
        R::Certificate,
        ID,
    ),
    secure(
        "POST",
        "/certificates/issue",
        A::CertificateIssue,
        R::Certificate,
        None,
    ),
    secure(
        "POST",
        "/certificates/renew/{id}",
        A::CertificateRenew,
        R::Certificate,
        ID,
    ),
    secure(
        "POST",
        "/certificates/revoke/{id}",
        A::CertificateRevoke,
        R::Certificate,
        ID,
    ),
    read("/certificates/ca", R::Certificate),
    secure(
        "POST",
        "/certificates/authorizations/license/request",
        A::AuthorizationLicenseRequest,
        R::Authorization,
        None,
    ),
    secure(
        "POST",
        "/certificates/authorizations/license/import",
        A::AuthorizationImport,
        R::Authorization,
        None,
    ),
    read(
        "/certificates/authorizations/license/status",
        R::Authorization,
    ),
];

/// 去掉 API 前缀与结尾斜杠后的路由模板。
#[must_use]
pub fn relative_template(matched_path: &str) -> &str {
    let relative = matched_path
        .strip_prefix(API_PREFIX)
        .unwrap_or(matched_path);
    let trimmed = relative.trim_end_matches('/');
    if trimmed.is_empty() { "/" } else { trimmed }
}

/// 按方法 + 路由模板（含或不含 API 前缀）查找映射。
#[must_use]
pub fn lookup(method: &str, matched_path: &str) -> Option<&'static RouteAudit> {
    let template = relative_template(matched_path);
    ROUTES
        .iter()
        .find(|route| route.method.eq_ignore_ascii_case(method) && route.path == template)
}

/// 未登记路由的兜底动作名：`api.<method>`。
#[must_use]
pub fn fallback_action(method: &str) -> String {
    format!("api.{}", method.to_ascii_lowercase())
}

/// 未登记路由的兜底资源类型：路径首段。
#[must_use]
pub fn fallback_resource_type(matched_path: &str) -> String {
    relative_template(matched_path)
        .trim_start_matches('/')
        .split('/')
        .next()
        .filter(|segment| !segment.is_empty() && !segment.starts_with('{'))
        .unwrap_or("api")
        .replace('-', "_")
}

#[cfg(test)]
pub(crate) mod route_sources {
    //! 测试辅助：直接解析路由源码，保证各类“每条路由都必须登记”的检查
    //! 与真实路由表同源（新增路由忘记登记会让相关测试失败）。
    use std::collections::BTreeSet;

    const ROUTE_SOURCES: &[(&str, &str)] = &[
        ("", include_str!("../handlers/routes/admins.rs")),
        ("", include_str!("../handlers/routes/analytics.rs")),
        ("", include_str!("../handlers/routes/auth.rs")),
        ("", include_str!("../handlers/routes/billing.rs")),
        ("", include_str!("../handlers/routes/callbacks.rs")),
        ("", include_str!("../handlers/routes/config_center.rs")),
        ("", include_str!("../handlers/routes/products.rs")),
        ("", include_str!("../handlers/routes/settings.rs")),
        ("", include_str!("../handlers/routes/suppliers_routes.rs")),
        ("", include_str!("../handlers/routes/tenants.rs")),
        ("", include_str!("../handlers/routes/upstream.rs")),
        ("/certificates", include_str!("../proxy/cert/routes.rs")),
    ];

    const METHODS: &[&str] = &["get", "post", "put", "delete", "patch"];

    /// 从 `.route("path", get(..).post(..))` 源码中提取 (METHOD, path)。
    fn extract_routes(prefix: &str, source: &str) -> Vec<(String, String)> {
        let mut found = Vec::new();
        let mut rest = source;
        while let Some(start) = rest.find(".route(") {
            let body_start = start + ".route(".len();
            let mut depth = 1usize;
            let mut end = body_start;
            for (offset, ch) in rest[body_start..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = body_start + offset;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let body = &rest[body_start..end];
            let mut quoted = body.splitn(3, '"');
            quoted.next();
            let path = quoted.next().unwrap_or_default();
            let handlers = quoted.next().unwrap_or_default();
            let full = format!("{prefix}{path}");
            let full = full.trim_end_matches('/');
            let full = if full.is_empty() { "/" } else { full };
            for method in METHODS {
                let needle = format!("{method}(");
                let mut search = handlers;
                while let Some(pos) = search.find(&needle) {
                    let preceded_by_ident = search[..pos]
                        .chars()
                        .next_back()
                        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
                    if !preceded_by_ident {
                        found.push((method.to_ascii_uppercase(), full.to_string()));
                    }
                    search = &search[pos + needle.len()..];
                }
            }
            rest = &rest[end..];
        }
        found
    }

    /// 从路由源码中解析出的全部 (METHOD, 相对路径)，供路由映射类覆盖测试共用。
    pub(crate) fn source_routes() -> BTreeSet<(String, String)> {
        ROUTE_SOURCES
            .iter()
            .flat_map(|(prefix, source)| extract_routes(prefix, source))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::route_sources::source_routes;
    use super::{ROUTES, RouteKind, fallback_action, fallback_resource_type, lookup};
    use std::collections::BTreeSet;

    #[test]
    fn route_coverage_every_route_is_mapped() {
        let routes = source_routes();
        assert!(routes.len() > 150, "解析到的路由过少: {}", routes.len());
        let missing: Vec<_> = routes
            .iter()
            .filter(|(method, path)| lookup(method, path).is_none())
            .collect();
        assert!(missing.is_empty(), "以下路由缺少审计映射: {missing:?}");
    }

    #[test]
    fn route_map_has_no_stale_or_duplicate_entries() {
        let routes = source_routes();
        let mut seen = BTreeSet::new();
        for route in ROUTES {
            let key = (route.method.to_string(), route.path.to_string());
            assert!(seen.insert(key.clone()), "重复映射: {key:?}");
            assert!(routes.contains(&key), "映射指向不存在的路由: {key:?}");
        }
    }

    #[test]
    fn route_map_invariants() {
        for route in ROUTES {
            match route.kind {
                RouteKind::Read => {
                    assert_eq!(route.method, "GET", "{}", route.path);
                    assert!(route.action.is_none(), "{}", route.path);
                }
                RouteKind::SensitiveRead => {
                    assert_eq!(route.method, "GET", "{}", route.path);
                    assert!(route.security && route.action.is_some(), "{}", route.path);
                }
                RouteKind::Write | RouteKind::Auth => {
                    assert_ne!(route.method, "GET", "{}", route.path);
                    assert!(route.action.is_some(), "{}", route.path);
                }
            }
            if let Some(param) = route.resource_param {
                assert!(
                    route.path.contains(&format!("{{{param}}}")),
                    "{} 不含参数 {param}",
                    route.path
                );
            }
        }
    }

    #[test]
    fn required_sensitive_reads_are_classified() {
        for path in [
            "/certificates/{id}/full",
            "/audit-logs",
            "/audit-logs/{id}",
            "/audit-logs/export",
            "/tenants/{code}/api-keys",
            "/tenants/{code}/crypto-keys",
            "/tenants/{code}/request-keys",
            "/config-center/export",
            "/tenant-crypto/export",
            "/upstream-connectors/drafts/{id}/export/postman",
        ] {
            let route = lookup("GET", path).unwrap_or_else(|| panic!("未映射: {path}"));
            assert_eq!(route.kind, RouteKind::SensitiveRead, "{path}");
        }
    }

    #[test]
    fn lookup_accepts_full_prefix_and_trailing_slash() {
        assert!(lookup("GET", "/api/v1/mgmt/certificates/").is_some());
        assert!(lookup("post", "/api/v1/mgmt/auth/login").is_some());
        assert!(lookup("GET", "/api/v1/mgmt/unknown").is_none());
        assert_eq!(fallback_action("PATCH"), "api.patch");
        assert_eq!(
            fallback_resource_type("/api/v1/mgmt/new-thing/{id}"),
            "new_thing"
        );
    }
}
