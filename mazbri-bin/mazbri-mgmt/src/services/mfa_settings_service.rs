use crate::repositories::mfa_settings as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sqlx::PgPool;
use tracing::{error, info};
#[derive(Debug, Clone, Default)]
pub struct UpdateMfaSettingsInput {
    pub totp_issuer: Option<String>,
    pub totp_digits: Option<i32>,
    pub totp_period: Option<i32>,
    pub backup_codes_count: Option<i32>,
}
#[must_use]
pub fn validate_update_fields(input: &UpdateMfaSettingsInput) -> Option<axum::response::Response> {
    if let Some(issuer) = &input.totp_issuer
        && issuer.trim().is_empty()
    {
        return Some(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            "发行者名称不能为空",
        ));
    }
    if let Some(digits) = input.totp_digits
        && digits != 6
    {
        return Some(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            "TOTP 位数固定为行业默认 6 位",
        ));
    }
    if let Some(period) = input.totp_period
        && period != 30
    {
        return Some(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            "TOTP 周期固定为行业默认 30 秒",
        ));
    }
    if let Some(count) = input.backup_codes_count
        && count != 10
    {
        return Some(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            "备用码数量固定为行业默认 10 个",
        ));
    }
    None
}
fn row_to_json(row: &repo::MfaSettingsRow) -> serde_json::Value {
    serde_json::json!(
        { "totp_issuer" : row.totp_issuer, "totp_digits" : row.totp_digits, "totp_period" :
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
    match repo::update_mfa_settings(
        db,
        repo::UpdateMfaSettingsParams {
            totp_issuer: input.totp_issuer.as_ref().map(|s| s.trim().to_string()),
            totp_digits: Some(6),
            totp_period: Some(30),
            backup_codes_count: Some(10),
            updated_by,
        },
    )
    .await
    {
        Ok(row) => {
            info!("MFA settings updated by admin {}", admin_username);
            ApiResponse::ok(serde_json::json!({ "data" : row_to_json(& row) })).into_response()
        }
        Err(e) => {
            error!("Failed to update MFA settings: {}", e);
            ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "更新 MFA 设置失败")
        }
    }
}
