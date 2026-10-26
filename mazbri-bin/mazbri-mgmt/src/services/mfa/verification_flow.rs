use super::common::{MfaServiceErrorMode, consume_stage_token, get_mfa_service, ok_json};
use crate::audit::{AuditAction, AuditService, user_audit};
use crate::mfa::{MfaService, MfaServiceCache};
use crate::middleware::auth::AdminClaims;
use crate::repositories::mfa as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use mazbri_core::num::SaturatingFrom;
use mazbri_core::redis::RedisClient;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{error, info, warn};
pub async fn disable(
    pool: &PgPool,
    mfa_cache: &Arc<MfaServiceCache>,
    audit: &AuditService,
    user_id: uuid::Uuid,
    username: &str,
    verification_code: &str,
    client_ip: Option<String>,
) -> axum::response::Response {
    let state = match repo::get_mfa_disable_state(pool, user_id).await {
        Ok(state) => state,
        Err(error) => {
            error!("Database error during MFA disable: {}", error);
            return ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::database_error(),
            );
        }
    };
    let Some(state) = state else {
        return ApiResponse::err(StatusCode::NOT_FOUND, mazbri_i18n::mfa::user_not_found());
    };
    if !state.mfa_enabled {
        return ApiResponse::err(StatusCode::BAD_REQUEST, mazbri_i18n::mfa::not_enabled());
    }
    if state.mfa_required {
        return ApiResponse::err(
            StatusCode::FORBIDDEN,
            mazbri_i18n::mfa::required_cannot_disable(),
        );
    }
    let mut verified = false;
    if let Some(encrypted_secret) = &state.mfa_secret
        && let Ok(mfa_service) = mfa_cache.get().await
        && let Ok(secret) = mfa_service.decrypt_secret(encrypted_secret)
        && mfa_service.verify_totp(&secret, verification_code)
    {
        verified = true;
    }
    if !verified
        && let Some(hash) = state.password_hash
        && mazbri_auth::constant_time_argon2_verify(verification_code.as_bytes(), &hash).is_ok()
    {
        verified = true;
    }
    if !verified {
        warn!("Invalid verification code for MFA disable: {}", username);
        crate::audit::set_failure_reason("mfa_invalid_code");
        return ApiResponse::err(
            StatusCode::BAD_REQUEST,
            mazbri_i18n::mfa::invalid_totp_or_password(),
        );
    }
    if let Err(error) = repo::disable_mfa(pool, user_id).await {
        error!("Failed to disable MFA: {}", error);
        return ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::failed_disable(),
        );
    }
    audit.log(user_audit(
        user_id,
        AuditAction::MfaDisable,
        Some(serde_json::json!(
            { "action" : "mfa_disabled", "username" : username, }
        )),
        client_ip,
    ));
    info!("MFA disabled for user: {}", username);
    ok_json(serde_json::json!(
        { "message" : mazbri_i18n::mfa::disabled_success(), "messageKey" :
        "api.mfa.disabled_success" }
    ))
}
pub struct VerifyParams<'a> {
    pub pool: &'a PgPool,
    pub redis: &'a RedisClient,
    pub mfa_cache: &'a Arc<MfaServiceCache>,
    pub audit: &'a AuditService,
    pub jwt_secret: &'a str,
    /// 已按 `mfa_pending` 类型校验过的临时令牌声明。
    pub temp_claims: &'a AdminClaims,
    pub user_id: uuid::Uuid,
    pub totp_code: Option<&'a str>,
    pub backup_code: Option<&'a str>,
    pub client_ip: Option<String>,
}

/// 哪一种验证码匹配成功。
enum MfaCodeMatch {
    Totp,
    /// 命中的备用码下标
    Backup(usize),
}

/// 先校验 TOTP，再校验备用码；都不匹配返回 `None`。
fn match_mfa_code(
    mfa_service: &MfaService,
    state: &repo::MfaVerifyState,
    totp_code: Option<&str>,
    backup_code: Option<&str>,
) -> Option<MfaCodeMatch> {
    if let (Some(code), Some(encrypted_secret)) = (totp_code, &state.mfa_secret)
        && let Ok(secret) = mfa_service.decrypt_secret(encrypted_secret)
        && mfa_service.verify_totp(&secret, code)
    {
        return Some(MfaCodeMatch::Totp);
    }
    let code = backup_code?;
    let hashed_codes: Vec<String> = state
        .mfa_backup_codes
        .as_ref()?
        .as_array()?
        .iter()
        .filter_map(|value| value.as_str().map(std::string::ToString::to_string))
        .collect();
    match mfa_service.verify_backup_code(code, &hashed_codes) {
        Ok(Some(index)) => Some(MfaCodeMatch::Backup(index)),
        _ => None,
    }
}

/// 记录一次失败尝试（必要时锁定）并返回错误响应。
async fn record_failed_attempt(
    pool: &PgPool,
    mfa_service: &MfaService,
    user_id: uuid::Uuid,
    username: &str,
    attempts: i32,
) -> axum::response::Response {
    let new_attempts = attempts + 1;
    let lockout_until = if new_attempts >= mfa_service.config().max_attempts {
        Some(mfa_service.calculate_lockout_until())
    } else {
        None
    };
    if let Err(error) = repo::update_mfa_attempts(pool, user_id, new_attempts, lockout_until).await
    {
        error!(
            "Failed to update MFA attempt count for user {}: {}. Lockout mechanism may be bypassed!",
            username, error
        );
        return ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::database_error(),
        );
    }
    if lockout_until.is_some() {
        crate::audit::set_failure_reason("mfa_lock_triggered");
        crate::audit::mark_security_event();
    } else {
        crate::audit::set_failure_reason("mfa_invalid_code");
    }
    warn!(
        "MFA verification failed for user: {} (attempt {}/{})",
        username,
        new_attempts,
        mfa_service.config().max_attempts
    );
    ApiResponse::err(StatusCode::BAD_REQUEST, mazbri_i18n::mfa::invalid_code())
}

/// 删除已使用的备用码（失败只记日志）。
async fn consume_backup_code(
    pool: &PgPool,
    user_id: uuid::Uuid,
    username: &str,
    state: &repo::MfaVerifyState,
    index: usize,
) {
    let Some(hashed_codes) = state
        .mfa_backup_codes
        .as_ref()
        .and_then(serde_json::Value::as_array)
    else {
        return;
    };
    let mut new_codes: Vec<serde_json::Value> = hashed_codes.clone();
    new_codes.remove(index);
    if let Err(error) = repo::remove_backup_code(pool, user_id, serde_json::json!(new_codes)).await
    {
        error!(
            "Failed to remove used backup code for user {}: {}",
            username, error
        );
    }
}

/// MFA 校验通过后签发正式访问令牌，返回令牌与有效期（秒）。
async fn issue_verified_token(
    pool: &PgPool,
    jwt_secret: &str,
    username: &str,
    user_id_str: &str,
    role: &str,
) -> Result<(String, u64), Box<axum::response::Response>> {
    let access_ttl =
        crate::services::admin::resolve_access_token_expiration_secs(pool, 86400).await;
    crate::services::admin::generate_admin_jwt(
        username,
        user_id_str,
        role,
        jwt_secret,
        i64::saturating_from(access_ttl),
    )
    .map(|token| (token, access_ttl))
    .map_err(|error| {
        error!("Failed to generate JWT: {}", error);
        Box::new(ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::failed_generate_token(),
        ))
    })
}

pub async fn verify(params: VerifyParams<'_>) -> axum::response::Response {
    let pool = params.pool;
    let mfa_cache = params.mfa_cache;
    let audit = params.audit;
    let jwt_secret = params.jwt_secret;
    let user_id = params.user_id;
    let username = params.temp_claims.sub.as_str();
    let user_id_str = params.temp_claims.user_id.as_str();
    let role = params.temp_claims.role.as_str();
    let totp_code = params.totp_code;
    let backup_code = params.backup_code;
    let client_ip = params.client_ip;

    info!(
        "[mfa_verify] Processing: user_id={}, has_totp_code={}, has_backup_code={}",
        user_id,
        totp_code.is_some(),
        backup_code.is_some()
    );
    let state = match repo::get_mfa_verify_state(pool, user_id).await {
        Ok(state) => state,
        Err(error) => {
            error!("Database error during MFA verify: {}", error);
            return ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::database_error(),
            );
        }
    };
    let Some(state) = state else {
        return ApiResponse::err(StatusCode::NOT_FOUND, mazbri_i18n::mfa::user_not_found());
    };
    if !state.mfa_enabled {
        return ApiResponse::err(StatusCode::BAD_REQUEST, mazbri_i18n::mfa::not_enabled());
    }
    if state.mfa_secret.is_none() {
        return ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::not_setup(),
        );
    }

    let mfa_service = match get_mfa_service(mfa_cache, MfaServiceErrorMode::InternalError).await {
        Ok(service) => service,
        Err(response) => return *response,
    };

    if mfa_service.is_locked(state.mfa_attempts, state.mfa_locked_until) {
        crate::audit::set_failure_reason("mfa_locked");
        return ApiResponse::err(StatusCode::FORBIDDEN, mazbri_i18n::mfa::locked());
    }
    let Some(matched) = match_mfa_code(&mfa_service, &state, totp_code, backup_code) else {
        return record_failed_attempt(pool, &mfa_service, user_id, username, state.mfa_attempts)
            .await;
    };
    // 一次性：验证码通过后立即原子消费临时令牌，重放/并发请求在此被拒绝。
    if let Err(response) = consume_stage_token(params.redis, params.temp_claims).await {
        return *response;
    }
    let used_backup_code_idx = match matched {
        MfaCodeMatch::Totp => None,
        MfaCodeMatch::Backup(index) => {
            consume_backup_code(pool, user_id, username, &state, index).await;
            Some(index)
        }
    };
    if let Err(error) = repo::reset_mfa_verify_state(pool, user_id).await {
        error!(
            "Failed to reset MFA attempts/update verified_at for user {}: {}",
            username, error
        );
    }
    let (token, access_ttl) =
        match issue_verified_token(pool, jwt_secret, username, user_id_str, role).await {
            Ok(issued) => issued,
            Err(response) => return *response,
        };
    if let Err(error) = audit
        .log_critical(user_audit(
            user_id,
            AuditAction::UserLogin,
            Some(serde_json::json!(
                { "username" : username, "role" : role, "mfa_verified" : true,
                "used_backup_code" : used_backup_code_idx.is_some(), }
            )),
            client_ip,
        ))
        .await
    {
        error!(
            "Failed to write critical audit log for MFA verification: {}",
            error
        );
    }
    info!("MFA verification successful for user: {}", username);
    let user = serde_json::json!(
        { "id" : user_id_str, "username" : username, "role" : role, "created_at" : state
        .created_at.to_rfc3339() }
    );
    ok_json(serde_json::json!({ "token" : token, "expires_in" : access_ttl, "user" : user }))
}
pub async fn status(pool: &PgPool, user_id: uuid::Uuid) -> axum::response::Response {
    let row = match repo::get_mfa_status(pool, user_id).await {
        Ok(row) => row,
        Err(error) => {
            error!("Database error during MFA status: {}", error);
            return ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::database_error(),
            );
        }
    };
    let Some(row) = row else {
        return ApiResponse::err(StatusCode::NOT_FOUND, mazbri_i18n::mfa::user_not_found());
    };
    let backup_codes_remaining = row
        .mfa_backup_codes
        .and_then(|value| value.as_array().map(std::vec::Vec::len))
        .unwrap_or(0);
    ok_json(serde_json::json!(
        { "enabled" : row.mfa_enabled, "verified_at" : row.mfa_verified_at.map(| time
        | time.to_rfc3339()), "backup_codes_remaining" : backup_codes_remaining }
    ))
}
