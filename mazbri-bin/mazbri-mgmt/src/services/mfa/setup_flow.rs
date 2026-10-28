use super::common::{
    MfaServiceErrorMode, get_mfa_service, hash_backup_codes, ok_json, prepare_setup_artifacts,
    store_setup_artifacts,
};
use crate::audit::{AuditAction, AuditService, user_audit};
use crate::mfa::MfaServiceCache;
use crate::repositories::mfa as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{error, info};
pub struct SetupParams<'a> {
    pub pool: &'a PgPool,
    pub password_policy_cache: &'a crate::services::PasswordPolicyCache,
    pub mfa_cache: &'a Arc<MfaServiceCache>,
    pub audit: &'a AuditService,
    pub user_id: uuid::Uuid,
    pub username: &'a str,
    pub current_password: &'a str,
    pub client_ip: Option<String>,
}

pub async fn setup(params: SetupParams<'_>) -> axum::response::Response {
    let SetupParams {
        pool,
        password_policy_cache,
        mfa_cache,
        audit,
        user_id,
        username,
        current_password,
        client_ip,
    } = params;
    if let Err(response) =
        verify_current_password(pool, password_policy_cache, user_id, current_password).await
    {
        return *response;
    }
    let state = match repo::get_mfa_setup_state(pool, user_id).await {
        Ok(state) => state,
        Err(error) => {
            error!("Database error during MFA setup: {}", error);
            return ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::database_error(),
            );
        }
    };
    let Some(state) = state else {
        return ApiResponse::err(StatusCode::NOT_FOUND, mazbri_i18n::mfa::user_not_found());
    };
    if state.mfa_enabled {
        return ApiResponse::err(
            StatusCode::BAD_REQUEST,
            mazbri_i18n::mfa::already_enabled_disable(),
        );
    }
    let mfa_service =
        match get_mfa_service(mfa_cache, MfaServiceErrorMode::ServerNotConfigured).await {
            Ok(service) => service,
            Err(response) => return *response,
        };
    let artifacts = match prepare_setup_artifacts(&mfa_service, username) {
        Ok(artifacts) => artifacts,
        Err(response) => return *response,
    };
    if let Err(response) = store_setup_artifacts(pool, user_id, &artifacts).await {
        return *response;
    }
    audit.log(user_audit(
        user_id,
        AuditAction::MfaSetup,
        Some(serde_json::json!(
            { "action" : "mfa_setup_initiated", "username" : username, }
        )),
        client_ip.clone(),
    ));
    info!("MFA setup initiated for user: {}", username);
    ok_json(serde_json::json!(
        { "secret" : artifacts.secret, "qr_code_url" : artifacts.qr_code_url,
        "backup_codes" : artifacts.backup_codes, "totp_digits" : artifacts
        .totp_digits }
    ))
}
async fn verify_current_password(
    pool: &PgPool,
    password_policy_cache: &crate::services::PasswordPolicyCache,
    user_id: uuid::Uuid,
    current_password: &str,
) -> Result<(), Box<axum::response::Response>> {
    let user = match crate::repositories::admin::get_admin_user_by_id(pool, user_id).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return Err(Box::new(ApiResponse::err(
                StatusCode::NOT_FOUND,
                mazbri_i18n::mfa::user_not_found(),
            )));
        }
        Err(error) => {
            error!("Database error during MFA setup password check: {error}");
            return Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::database_error(),
            )));
        }
    };
    let locked = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
        "SELECT locked_until FROM admin_users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten();
    if locked.is_some_and(|until| until > chrono::Utc::now()) {
        crate::audit::set_failure_reason("account_locked");
        return Err(Box::new(
            crate::error::ApiError::validation("账户已锁定，请稍后再试".to_string())
                .into_response(),
        ));
    }
    let Some(hash) = user.password_hash.as_deref() else {
        return Err(Box::new(ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::internal_error(),
        )));
    };
    if mazbri_auth::constant_time_argon2_verify(current_password.as_bytes(), hash).is_err() {
        let policy = password_policy_cache.get().await;
        if let Err(error) = crate::repositories::admin::record_login_failure(
            pool,
            user_id,
            policy.max_login_attempts,
            i64::from(policy.lockout_duration_minutes),
        )
        .await
        {
            error!("failed to record setup password failure: {error}");
            return Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::database_error(),
            )));
        }
        crate::audit::set_failure_reason("invalid_password");
        return Err(Box::new(
            crate::error::ApiError::validation("当前密码不正确".to_string()).into_response(),
        ));
    }
    Ok(())
}

pub async fn regenerate_backup_codes(
    pool: &PgPool,
    mfa_cache: &Arc<MfaServiceCache>,
    audit: &AuditService,
    user_id: uuid::Uuid,
    username: &str,
    client_ip: Option<String>,
) -> axum::response::Response {
    let mfa_enabled = match repo::is_mfa_enabled(pool, user_id).await {
        Ok(value) => value,
        Err(error) => {
            error!("Database error during backup codes regeneration: {}", error);
            return ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::database_error(),
            );
        }
    };
    let Some(true) = mfa_enabled else {
        return ApiResponse::err(StatusCode::NOT_FOUND, mazbri_i18n::mfa::user_not_found());
    };
    let mfa_service = match get_mfa_service(mfa_cache, MfaServiceErrorMode::InternalError).await {
        Ok(service) => service,
        Err(response) => return *response,
    };
    let backup_codes = mfa_service.generate_backup_codes();
    let hashed_codes = match hash_backup_codes(
        &mfa_service,
        &backup_codes,
        mazbri_i18n::mfa::failed_backup_codes(),
    ) {
        Ok(codes) => codes,
        Err(response) => return *response,
    };

    if let Err(error) = repo::update_backup_codes(pool, user_id, &hashed_codes).await {
        error!("Failed to update backup codes: {}", error);
        return ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::failed_update_backup(),
        );
    }
    audit.log(user_audit(
        user_id,
        AuditAction::MfaRegenerateCodes,
        Some(serde_json::json!(
            { "action" : "mfa_backup_codes_regenerated", "username" :
            username, }
        )),
        client_ip,
    ));
    info!("Backup codes regenerated for user: {}", username);
    ok_json(serde_json::json!({ "backup_codes" : backup_codes }))
}
