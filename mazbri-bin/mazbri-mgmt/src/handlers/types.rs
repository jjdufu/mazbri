use mazbri_core::validation::BoundedVec;
use mazbri_core::validation::limits;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub const MAX_PAGE_SIZE: u32 = limits::MAX_PAGE_SIZE;
#[derive(Debug, Deserialize, Clone, TS)]
#[ts(export_to = "test_common.ts")]
#[serde(deny_unknown_fields)]
pub struct ListParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    pub status: Option<String>,
    /// Free-text keyword. Canonical query name is `q`.
    #[serde(default)]
    pub q: Option<String>,
    /// JSON array of `{field,op,value}` list filters (List Query contract).
    #[serde(default)]
    pub filters: Option<String>,
}
impl ListParams {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.page_size > MAX_PAGE_SIZE {
            Err(format!(
                "page_size 超过最大限制: {} > {}",
                self.page_size, MAX_PAGE_SIZE
            ))
        } else {
            Ok(())
        }
    }
    #[must_use]
    pub fn capped_page_size(&self) -> u32 {
        self.page_size.min(MAX_PAGE_SIZE)
    }
    #[must_use]
    pub fn offset(&self) -> u32 {
        let page = if self.page == 0 { 1 } else { self.page };
        let page_size = self.capped_page_size();
        (page - 1) * page_size
    }
}
fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    20
}
#[derive(Debug, Deserialize, TS)]
#[ts(export_to = "tenant.ts")]
#[serde(deny_unknown_fields)]
pub struct CreateTenantRequest {
    pub tenant_code: String,
    pub tenant_id: String,
    pub name: String,
    pub contact_person: Option<String>,
    pub contact_phone: Option<String>,
    pub contact_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_expiration_secs: Option<i32>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetTenantStatusRequest {
    pub status: String,
    pub reason: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateApiKeyRequest {
    pub scopes: BoundedVec<String, { limits::MAX_API_KEY_SCOPES }>,
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub expires_days: Option<i64>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export_to = "auth.ts")]
#[serde(deny_unknown_fields)]
pub struct UpdateApiKeyRequest {
    #[serde(default = "default_renewal_days")]
    pub expires_days: Option<i64>,
}
impl UpdateApiKeyRequest {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), String> {
        if let Some(days) = self.expires_days {
            if !(1..=3650).contains(&days) {
                return Err("续期天数必须在 1 到 3650 天之间".to_string());
            }
        }
        Ok(())
    }
}
/// 续期请求未给出 `expires_days` 时的默认值；显式 `null` 表示永不过期。
const DEFAULT_RENEWAL_DAYS: Option<i64> = Some(365);
fn default_renewal_days() -> Option<i64> {
    DEFAULT_RENEWAL_DAYS
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddToWhitelistRequest {
    pub ip_addresses: BoundedVec<String, { limits::MAX_IP_ADDRESSES }>,
    pub description: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeAccessRequest {
    pub tenant_code: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantAccessRequest {
    pub tenant_code: String,
    #[serde(default)]
    pub config: Option<mazbri_product::service::TenantProductConfig>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsParams {
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub group_by: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformStatsParams {
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLoginRequest {
    pub username: String,
    pub password: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirstLoginChangePasswordRequest {
    pub username: String,
    pub current_password: String,
    pub new_password: String,
    pub new_username: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
    pub confirm_password: String,
}
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export_to = "auth.ts")]
pub struct AdminUser {
    pub id: String,
    pub username: String,
    pub role: String,
    pub created_at: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminJwtClaims {
    pub iss: String,
    pub sub: String,
    pub user_id: String,
    pub role: String,
    pub iat: i64,
    pub exp: i64,
    /// Token id for logout / denylist (H-11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemSettingsUpdate {
    pub site_name: Option<String>,
    pub site_url: Option<String>,
    /// 租户调用的 API 网关根地址（向导 curl / 联调示例，非管理后台 URL）
    pub tenant_gateway_url: Option<String>,
    pub enable_audit_log: Option<bool>,
    pub session_timeout: Option<i64>,
    pub max_upload_size: Option<i64>,
}
#[derive(Debug, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct AdminLogoutRequest {
    /// Optional refresh token to revoke alongside the access token.
    pub refresh_token: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserSettingsUpdate {
    pub language: Option<String>,
    pub email: Option<String>,
}
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct AdminListParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    pub role: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
    /// JSON array of `{field,op,value}` list filters.
    #[serde(default)]
    pub filters: Option<String>,
}
impl AdminListParams {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.page_size > MAX_PAGE_SIZE {
            Err(format!(
                "page_size 超过最大限制: {} > {}",
                self.page_size, MAX_PAGE_SIZE
            ))
        } else {
            Ok(())
        }
    }
    #[must_use]
    pub fn capped_page_size(&self) -> u32 {
        self.page_size.min(MAX_PAGE_SIZE)
    }
    #[must_use]
    pub fn offset(&self) -> u32 {
        let page = if self.page == 0 { 1 } else { self.page };
        let page_size = self.capped_page_size();
        (page - 1) * page_size
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAdminRequest {
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub password: String,
    pub role: String,
    #[serde(default)]
    pub mfa_required: bool,
    #[serde(default = "default_force_password_reset")]
    pub force_password_reset: bool,
}
fn default_force_password_reset() -> bool {
    true
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateAdminRequest {
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub role: Option<String>,
    pub mfa_required: Option<bool>,
    pub force_password_reset: Option<bool>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetPasswordRequest {
    pub new_password: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForcePasswordResetRequest {
    #[serde(alias = "force_password_reset")]
    pub force_reset: bool,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetAdminStatusRequest {
    pub is_active: bool,
}
#[derive(Debug, Serialize)]
pub struct MfaSetupResponse {
    pub secret: String,
    pub qr_code_url: String,
    pub backup_codes: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MfaEnableRequest {
    pub totp_code: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MfaDisableRequest {
    pub verification_code: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MfaVerifyRequest {
    pub temp_token: String,
    pub totp_code: Option<String>,
    pub backup_code: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct MfaStatusResponse {
    pub enabled: bool,
    pub verified_at: Option<String>,
    pub backup_codes_remaining: usize,
}
#[derive(Debug, Serialize, TS)]
#[ts(export_to = "auth.ts")]
pub struct LoginResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<AdminUser>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub mfa_required: bool,
    #[serde(default)]
    pub mfa_setup_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_token: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct RegenerateBackupCodesResponse {
    pub backup_codes: Vec<String>,
}
