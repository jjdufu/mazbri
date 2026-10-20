use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    TenantCreate,
    TenantUpdate,
    TenantDelete,
    TenantSuspend,
    TenantActivate,
    ApiKeyCreate,
    ApiKeyRevoke,
    ApiKeyUpdate,
    UserLogin,
    UserLogout,
    UserUpdate,
    QuotaUpdate,
    PlanChange,
    ProductGrant,
    ProductRevoke,
    ProductCreate,
    ProductUpdate,
    ProductDelete,
    ProductReservationCreate,
    ProductReservationRelease,
    AccountRecharge,
    AccountRefund,
    BillGenerate,
    BillDelete,
    BillStatusUpdate,
    AccountAdjustment,
    ReconciliationAudit,
    ReconciliationDiscrepancyUpdate,
    ReconciliationDiscrepancyDelete,
    BillAdjustmentCreate,
    BillAdjustmentVoid,
    ReconciliationSettingsUpdate,
    AdminCreate,
    AdminUpdate,
    AdminDelete,
    AdminPasswordReset,
    AdminStatusChange,
    CertificateIssue,
    CertificateRenew,
    CertificateRevoke,
    SettingsUpdate,
    PasswordPolicyUpdate,
    ConfigCenterCreate,
    ConfigCenterUpdate,
    ConfigCenterDelete,
    ConfigCenterRollback,
    ConfigCenterExport,
    ConfigCenterImport,
    AppearanceUpdate,
    AppearanceReset,
    MfaSettingsUpdate,
    WhitelistAdd,
    WhitelistRemove,
    TenantCryptoProfileUpdate,
    TenantCryptoKeyCreate,
    TenantCryptoKeyRotate,
    TenantCryptoKeyActivate,
    TenantCryptoKeyDeprecate,
    TenantCryptoKeyDisable,
    TenantCryptoKeyDelete,
    TenantCryptoExport,
    TenantCryptoVerify,
    PasswordChangeFirstLogin,
    PasswordChange,
    UserSettingsUpdate,
    TenantRateLimitUpdate,
    TenantRateLimitDelete,
    TenantRateLimitCommandCreate,
    TenantRateLimitCommandCancel,
    TenantCacheUpdate,
    CallbackDeliveryRetry,
    SupplierCreate,
    SupplierUpdate,
    SupplierDelete,
    UpstreamOnboarding,
    SupplierValidateConfig,
    ChannelCreate,
    ChannelUpdate,
    ChannelDelete,
    RouteRuleCreate,
    RouteRuleUpdate,
    RouteRuleDelete,
    RouteEmergencySwitch,
    RouteChannelBindingCreate,
    RouteChannelBindingUpdate,
    RouteChannelBindingDelete,
    CallbackSubscriptionCreate,
    CallbackSubscriptionUpdate,
    CallbackSubscriptionDelete,
    AuthorizationImport,
    AuthorizationExport,
    AuthorizationRenew,
    AuthorizationActivate,
    AuthorizationRevoke,
    AuthorizationRotate,
    AuthorizationDisable,
    ConnectorRun,
    ConnectorPublish,
    ConnectorForcePublish,
    ConnectorRollback,
    ConnectorImport,
}
impl AuditAction {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditAction::TenantCreate => "tenant.create",
            AuditAction::TenantUpdate => "tenant.update",
            AuditAction::TenantDelete => "tenant.delete",
            AuditAction::TenantSuspend => "tenant.suspend",
            AuditAction::TenantActivate => "tenant.activate",
            AuditAction::ApiKeyCreate => "apikey.create",
            AuditAction::ApiKeyRevoke => "apikey.revoke",
            AuditAction::ApiKeyUpdate => "apikey.update",
            AuditAction::UserLogin => "user.login",
            AuditAction::UserLogout => "user.logout",
            AuditAction::UserUpdate => "user.update",
            AuditAction::QuotaUpdate => "quota.update",
            AuditAction::PlanChange => "plan.change",
            AuditAction::ProductGrant => "product.grant",
            AuditAction::ProductRevoke => "product.revoke",
            AuditAction::ProductCreate => "product.create",
            AuditAction::ProductUpdate => "product.update",
            AuditAction::ProductDelete => "product.delete",
            AuditAction::ProductReservationCreate => "product.reservation_create",
            AuditAction::ProductReservationRelease => "product.reservation_release",
            AuditAction::AccountRecharge => "account.recharge",
            AuditAction::AccountRefund => "account.refund",
            AuditAction::BillGenerate => "bill.generate",
            AuditAction::BillDelete => "bill.delete",
            AuditAction::BillStatusUpdate => "bill.status_update",
            AuditAction::AccountAdjustment => "account.adjustment",
            AuditAction::ReconciliationAudit => "billing.reconciliation_audit",
            AuditAction::ReconciliationDiscrepancyUpdate => {
                "billing.reconciliation_discrepancy_update"
            }
            AuditAction::ReconciliationDiscrepancyDelete => {
                "billing.reconciliation_discrepancy_delete"
            }
            AuditAction::BillAdjustmentCreate => "billing.bill_adjustment_create",
            AuditAction::BillAdjustmentVoid => "billing.bill_adjustment_void",
            AuditAction::ReconciliationSettingsUpdate => "billing.reconciliation_settings_update",
            AuditAction::AdminCreate => "admin.create",
            AuditAction::AdminUpdate => "admin.update",
            AuditAction::AdminDelete => "admin.delete",
            AuditAction::AdminPasswordReset => "admin.password_reset",
            AuditAction::AdminStatusChange => "admin.status_change",
            AuditAction::CertificateIssue => "certificate.issue",
            AuditAction::CertificateRenew => "certificate.renew",
            AuditAction::CertificateRevoke => "certificate.revoke",
            AuditAction::SettingsUpdate => "settings.update",
            AuditAction::PasswordPolicyUpdate => "password_policy.update",
            AuditAction::ConfigCenterCreate => "config_center.create",
            AuditAction::ConfigCenterUpdate => "config_center.update",
            AuditAction::ConfigCenterDelete => "config_center.delete",
            AuditAction::ConfigCenterRollback => "config_center.rollback",
            AuditAction::ConfigCenterExport => "config_center.export",
            AuditAction::ConfigCenterImport => "config_center.import",
            AuditAction::AppearanceUpdate => "appearance.update",
            AuditAction::AppearanceReset => "appearance.reset",
            AuditAction::MfaSettingsUpdate => "mfa_settings.update",
            AuditAction::WhitelistAdd => "tenant.whitelist_add",
            AuditAction::WhitelistRemove => "tenant.whitelist_remove",
            AuditAction::TenantCryptoProfileUpdate => "tenant.crypto_profile_update",
            AuditAction::TenantCryptoKeyCreate => "tenant.crypto_key_create",
            AuditAction::TenantCryptoKeyRotate => "tenant.crypto_key_rotate",
            AuditAction::TenantCryptoKeyActivate => "tenant.crypto_key_activate",
            AuditAction::TenantCryptoKeyDeprecate => "tenant.crypto_key_deprecate",
            AuditAction::TenantCryptoKeyDisable => "tenant.crypto_key_disable",
            AuditAction::TenantCryptoKeyDelete => "tenant.crypto_key_delete",
            AuditAction::TenantCryptoExport => "tenant.crypto_export",
            AuditAction::TenantCryptoVerify => "tenant.crypto_verify",
            AuditAction::PasswordChangeFirstLogin => "password.first_login_change",
            AuditAction::PasswordChange => "password.change",
            AuditAction::UserSettingsUpdate => "user.settings_update",
            AuditAction::TenantRateLimitUpdate => "tenant.ratelimit_update",
            AuditAction::TenantRateLimitDelete => "tenant.ratelimit_delete",
            AuditAction::TenantRateLimitCommandCreate => "tenant.ratelimit_command_create",
            AuditAction::TenantRateLimitCommandCancel => "tenant.ratelimit_command_cancel",
            AuditAction::TenantCacheUpdate => "tenant.cache_update",
            AuditAction::CallbackDeliveryRetry => "callback.delivery_retry",
            AuditAction::SupplierCreate => "supplier.create",
            AuditAction::SupplierUpdate => "supplier.update",
            AuditAction::SupplierDelete => "supplier.delete",
            AuditAction::UpstreamOnboarding => "upstream.onboarding",
            AuditAction::SupplierValidateConfig => "supplier.validate_config",
            AuditAction::ChannelCreate => "channel.create",
            AuditAction::ChannelUpdate => "channel.update",
            AuditAction::ChannelDelete => "channel.delete",
            AuditAction::RouteRuleCreate => "route_rule.create",
            AuditAction::RouteRuleUpdate => "route_rule.update",
            AuditAction::RouteRuleDelete => "route_rule.delete",
            AuditAction::RouteEmergencySwitch => "route.emergency_switch",
            AuditAction::RouteChannelBindingCreate => "route_channel_binding.create",
            AuditAction::RouteChannelBindingUpdate => "route_channel_binding.update",
            AuditAction::RouteChannelBindingDelete => "route_channel_binding.delete",
            AuditAction::CallbackSubscriptionCreate => "callback.subscription_create",
            AuditAction::CallbackSubscriptionUpdate => "callback.subscription_update",
            AuditAction::CallbackSubscriptionDelete => "callback.subscription_delete",
            AuditAction::AuthorizationImport => "authorization.import",
            AuditAction::AuthorizationExport => "authorization.export",
            AuditAction::AuthorizationRenew => "authorization.renew",
            AuditAction::AuthorizationActivate => "authorization.activate",
            AuditAction::AuthorizationRevoke => "authorization.revoke",
            AuditAction::AuthorizationRotate => "authorization.rotate",
            AuditAction::AuthorizationDisable => "authorization.disable",
            AuditAction::ConnectorRun => "connector.run",
            AuditAction::ConnectorPublish => "connector.publish",
            AuditAction::ConnectorForcePublish => "connector.force_publish",
            AuditAction::ConnectorRollback => "connector.rollback",
            AuditAction::ConnectorImport => "connector.import",
        }
    }
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
