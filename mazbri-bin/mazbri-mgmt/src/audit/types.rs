use serde::{Deserialize, Serialize};
/// 定义审计动作枚举及其稳定字符串（单一来源，避免枚举与映射漂移）。
macro_rules! audit_actions {
    ($($variant:ident => $name:literal,)+) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        #[serde(rename_all = "snake_case")]
        pub enum AuditAction {
            $($variant,)+
        }
        impl AuditAction {
            #[must_use]
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(AuditAction::$variant => $name,)+
                }
            }
        }
    };
}

audit_actions! {
    TenantCreate => "tenant.create",
    TenantUpdate => "tenant.update",
    TenantDelete => "tenant.delete",
    TenantSuspend => "tenant.suspend",
    TenantActivate => "tenant.activate",
    ApiKeyCreate => "apikey.create",
    ApiKeyRevoke => "apikey.revoke",
    ApiKeyUpdate => "apikey.update",
    UserLogin => "user.login",
    UserLogout => "user.logout",
    UserUpdate => "user.update",
    QuotaUpdate => "quota.update",
    PlanChange => "plan.change",
    ProductGrant => "product.grant",
    ProductRevoke => "product.revoke",
    ProductCreate => "product.create",
    ProductUpdate => "product.update",
    ProductDelete => "product.delete",
    ProductReservationCreate => "product.reservation_create",
    ProductReservationRelease => "product.reservation_release",
    AccountRecharge => "account.recharge",
    AccountRefund => "account.refund",
    BillGenerate => "bill.generate",
    BillDelete => "bill.delete",
    BillStatusUpdate => "bill.status_update",
    AccountAdjustment => "account.adjustment",
    ReconciliationAudit => "billing.reconciliation_audit",
    ReconciliationDiscrepancyUpdate => "billing.reconciliation_discrepancy_update",
    ReconciliationDiscrepancyDelete => "billing.reconciliation_discrepancy_delete",
    BillAdjustmentCreate => "billing.bill_adjustment_create",
    BillAdjustmentVoid => "billing.bill_adjustment_void",
    ReconciliationSettingsUpdate => "billing.reconciliation_settings_update",
    AdminCreate => "admin.create",
    AdminUpdate => "admin.update",
    AdminDelete => "admin.delete",
    AdminPasswordReset => "admin.password_reset",
    AdminStatusChange => "admin.status_change",
    CertificateIssue => "certificate.issue",
    CertificateRenew => "certificate.renew",
    CertificateRevoke => "certificate.revoke",
    SettingsUpdate => "settings.update",
    PasswordPolicyUpdate => "password_policy.update",
    ConfigCenterCreate => "config_center.create",
    ConfigCenterUpdate => "config_center.update",
    ConfigCenterDelete => "config_center.delete",
    ConfigCenterRollback => "config_center.rollback",
    ConfigCenterExport => "config_center.export",
    ConfigCenterImport => "config_center.import",
    AppearanceUpdate => "appearance.update",
    AppearanceReset => "appearance.reset",
    MfaSettingsUpdate => "mfa_settings.update",
    WhitelistAdd => "tenant.whitelist_add",
    WhitelistRemove => "tenant.whitelist_remove",
    TenantCryptoProfileUpdate => "tenant.crypto_profile_update",
    TenantCryptoKeyCreate => "tenant.crypto_key_create",
    TenantCryptoKeyRotate => "tenant.crypto_key_rotate",
    TenantCryptoKeyActivate => "tenant.crypto_key_activate",
    TenantCryptoKeyDeprecate => "tenant.crypto_key_deprecate",
    TenantCryptoKeyDisable => "tenant.crypto_key_disable",
    TenantCryptoKeyDelete => "tenant.crypto_key_delete",
    TenantCryptoExport => "tenant.crypto_export",
    TenantCryptoVerify => "tenant.crypto_verify",
    PasswordChangeFirstLogin => "password.first_login_change",
    PasswordChange => "password.change",
    UserSettingsUpdate => "user.settings_update",
    TenantRateLimitUpdate => "tenant.ratelimit_update",
    TenantRateLimitDelete => "tenant.ratelimit_delete",
    TenantRateLimitCommandCreate => "tenant.ratelimit_command_create",
    TenantRateLimitCommandCancel => "tenant.ratelimit_command_cancel",
    TenantCacheUpdate => "tenant.cache_update",
    CallbackDeliveryRetry => "callback.delivery_retry",
    SupplierCreate => "supplier.create",
    SupplierUpdate => "supplier.update",
    SupplierDelete => "supplier.delete",
    UpstreamOnboarding => "upstream.onboarding",
    SupplierValidateConfig => "supplier.validate_config",
    ChannelCreate => "channel.create",
    ChannelUpdate => "channel.update",
    ChannelDelete => "channel.delete",
    RouteRuleCreate => "route_rule.create",
    RouteRuleUpdate => "route_rule.update",
    RouteRuleDelete => "route_rule.delete",
    RouteEmergencySwitch => "route.emergency_switch",
    RouteChannelBindingCreate => "route_channel_binding.create",
    RouteChannelBindingUpdate => "route_channel_binding.update",
    RouteChannelBindingDelete => "route_channel_binding.delete",
    CallbackSubscriptionCreate => "callback.subscription_create",
    CallbackSubscriptionUpdate => "callback.subscription_update",
    CallbackSubscriptionDelete => "callback.subscription_delete",
    AuthorizationImport => "authorization.import",
    AuthorizationExport => "authorization.export",
    AuthorizationRenew => "authorization.renew",
    AuthorizationActivate => "authorization.activate",
    AuthorizationRevoke => "authorization.revoke",
    AuthorizationRotate => "authorization.rotate",
    AuthorizationDisable => "authorization.disable",
    ConnectorRun => "connector.run",
    ConnectorPublish => "connector.publish",
    ConnectorForcePublish => "connector.force_publish",
    ConnectorRollback => "connector.rollback",
    ConnectorImport => "connector.import",
}
impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ActorType {
    #[default]
    Admin,
    System,
}
impl ActorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorType::Admin => "admin",
            ActorType::System => "system",
        }
    }
}
impl std::fmt::Display for ActorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AuditStatus {
    #[default]
    Success,
    Failure,
}
impl AuditStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditStatus::Success => "success",
            AuditStatus::Failure => "failure",
        }
    }
}
impl std::fmt::Display for AuditStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuditResourceType {
    Tenant,
    ApiKey,
    User,
    Quota,
    Plan,
    Product,
    Account,
    System,
    Admin,
    Certificate,
    ConfigCenter,
    Supplier,
    Channel,
    RouteRule,
    RouteChannelBinding,
    Callback,
}
impl AuditResourceType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditResourceType::Tenant => "tenant",
            AuditResourceType::ApiKey => "apikey",
            AuditResourceType::User => "user",
            AuditResourceType::Quota => "quota",
            AuditResourceType::Plan => "plan",
            AuditResourceType::Product => "product",
            AuditResourceType::Account => "account",
            AuditResourceType::System => "system",
            AuditResourceType::Admin => "admin",
            AuditResourceType::Certificate => "certificate",
            AuditResourceType::ConfigCenter => "config_center",
            AuditResourceType::Supplier => "supplier",
            AuditResourceType::Channel => "channel",
            AuditResourceType::RouteRule => "route_rule",
            AuditResourceType::RouteChannelBinding => "route_channel_binding",
            AuditResourceType::Callback => "callback",
        }
    }
}
impl std::fmt::Display for AuditResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct AuditLogQuery {
    pub tenant_id: Option<uuid::Uuid>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub actor_id: Option<uuid::Uuid>,
    /// Legacy free-text keyword (broad ILIKE). Prefer `q` + structured `filters`.
    pub actor_name: Option<String>,
    /// Free-text keyword (same broad ILIKE as historical `actor_name`).
    #[serde(default)]
    pub q: Option<String>,
    /// JSON array of `{field,op,value}` list filters.
    #[serde(default)]
    pub filters: Option<String>,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub status: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}
const fn default_page() -> u32 {
    1
}
const fn default_limit() -> u32 {
    20
}
impl AuditLogQuery {
    #[must_use]
    pub fn offset(&self) -> i64 {
        i64::from(self.page.saturating_sub(1)) * i64::from(self.limit.min(MAX_PAGE_SIZE))
    }
    #[must_use]
    pub fn effective_limit(&self) -> i64 {
        i64::from(self.limit.min(MAX_PAGE_SIZE))
    }
}
const MAX_PAGE_SIZE: u32 = 100;
#[derive(Debug, Clone, Serialize)]
pub struct AuditLogResponse {
    pub id: uuid::Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub actor_id: Option<uuid::Uuid>,
    pub actor_type: String,
    pub actor_name: Option<String>,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub details: serde_json::Value,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct AuditLogListResponse {
    pub logs: Vec<AuditLogResponse>,
    pub total: i64,
    pub page: u32,
    pub limit: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct AuditLogDetailResponse {
    pub id: uuid::Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub actor_id: Option<uuid::Uuid>,
    pub actor_type: String,
    pub actor_name: Option<String>,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub details: serde_json::Value,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub changes: Option<AuditLogChanges>,
}
#[derive(Debug, Clone, Serialize)]
pub struct AuditLogChanges {
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
}
#[derive(Debug, Clone)]
pub struct AuditLogRequest {
    pub user_id: uuid::Uuid,
    pub actor_type: ActorType,
    pub action: AuditAction,
    pub resource_type: AuditResourceType,
    pub resource_id: Option<String>,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub status: AuditStatus,
    pub error_message: Option<String>,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
}
impl Default for AuditLogRequest {
    fn default() -> Self {
        Self {
            user_id: uuid::Uuid::nil(),
            actor_type: ActorType::Admin,
            action: AuditAction::UserUpdate,
            resource_type: AuditResourceType::System,
            resource_id: None,
            details: None,
            ip_address: None,
            user_agent: None,
            status: AuditStatus::Success,
            error_message: None,
            old_value: None,
            new_value: None,
        }
    }
}
