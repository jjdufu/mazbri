use crate::repositories::mfa_settings as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sqlx::PgPool;
use tracing::{error, info};
#[derive(Debug, Clone, Default)]
pub struct UpdateMfaSettingsInput {
    pub mfa_enabled_globally: Option<bool>,
    pub mfa_required_for_all: Option<bool>,
    pub mfa_required_for_roles: Option<Vec<String>>,
    pub totp_issuer: Option<String>,
    pub totp_digits: Option<i32>,
    pub totp_period: Option<i32>,
    pub backup_codes_count: Option<i32>,
}
#[must_use]
pub fn validate_update_fields(input: &UpdateMfaSettingsInput) -> Option<axum::response::Response> {
    if let Some(digits) = input.totp_digits
        && digits != 6
        && digits != 8
    {
        return Some(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            "TOTP 位数必须是 6 或 8",
        ));
    }
    if let Some(period) = input.totp_period
        && !(15..=60).contains(&period)
    {
        return Some(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            "TOTP 周期必须在 15-60 秒之间",
        ));
    }
    if let Some(count) = input.backup_codes_count
        && !(5..=20).contains(&count)
    {
        return Some(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            "备用码数量必须在 5-20 之间",
        ));
    }
    if let Some(roles) = input.mfa_required_for_roles.as_ref() {
        for role in roles {
            if !crate::middleware::route_access::ALL_ROLES.contains(&role.as_str()) {
                return Some(ApiResponse::err(
                    StatusCode::BAD_REQUEST,
                    "mfa_required_for_roles 中包含无效角色",
                ));
            }
        }
    }
    None
}
fn row_to_json(row: &repo::MfaSettingsRow) -> serde_json::Value {
    serde_json::json!(
        { "mfa_enabled_globally" : row.mfa_enabled_globally, "mfa_required_for_all" : row
        .mfa_required_for_all, "mfa_required_for_roles" : row.mfa_required_for_roles,
        "totp_issuer" : row.totp_issuer, "totp_digits" : row.totp_digits, "totp_period" :
        row.totp_period, "backup_codes_count" : row.backup_codes_count, "updated_at" :
        row.updated_at, }
    )
}
pub async fn get_settings(db: &PgPool) -> axum::response::Response {
    match repo::get_mfa_settings(db).await {
        Ok(row) => {
            ApiResponse::ok(serde_json::json!({ "data" : row_to_json(& row) })).into_response()
        }
        Err(e) => {
            error!("Failed to get MFA settings: {}", e);
            ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "获取 MFA 设置失败")
        }
    }
}
pub async fn update_settings(
    db: &PgPool,
    admin_username: &str,
    updated_by: &str,
    input: &UpdateMfaSettingsInput,
) -> axum::response::Response {
    if let Some(err_resp) = validate_update_fields(input) {
        return err_resp;
    }
    let clearing_all_mfa = input.mfa_enabled_globally == Some(false);
    match repo::update_mfa_settings(
        db,
        repo::UpdateMfaSettingsParams {
            mfa_enabled_globally: input.mfa_enabled_globally,
            mfa_required_for_all: input.mfa_required_for_all,
            mfa_required_for_roles: input.mfa_required_for_roles.clone(),
            totp_issuer: input.totp_issuer.clone(),
            totp_digits: input.totp_digits,
            totp_period: input.totp_period,
            backup_codes_count: input.backup_codes_count,
            updated_by,
        },
    )
    .await
    {
        Ok(row) => {
            if clearing_all_mfa {
                match repo::clear_all_user_mfa(db).await {
                    Ok(count) => {
                        info!(
                            "Cleared MFA bindings for {} users (MFA globally disabled)",
                            count
                        );
                    }
                    Err(e) => error!("Failed to clear user MFA bindings: {}", e),
                }
            }
            info!("MFA settings updated by admin {}", admin_username);
            ApiResponse::ok(serde_json::json!({ "data" : row_to_json(& row) })).into_response()
        }
        Err(e) => {
            error!("Failed to update MFA settings: {}", e);
            ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "更新 MFA 设置失败")
        }
    }
}
