use super::common::{
    MfaServiceErrorMode, consume_stage_token, get_mfa_service, ok_json, resign_bound_session,
    revoke_old_refresh,
};
use crate::audit::{AuditAction, AuditService, user_audit};
use crate::mfa::MfaServiceCache;
use crate::middleware::auth::AdminClaims;
use crate::repositories::mfa as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use mazbri_core::num::SaturatingFrom;
use mazbri_core::redis::RedisClient;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{error, info};
/// # Errors
///
/// 未启用、策略禁止或数据库失败时返回响应。
pub async fn ensure_mfa_can_be_disabled(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<(), Box<axum::response::Response>> {
    let state = repo::get_mfa_disable_state(pool, user_id)
        .await
        .map_err(|error| {
            error!("Database error during MFA disable: {error}");
            Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::database_error(),
            ))
        })?
        .ok_or_else(|| {
            Box::new(ApiResponse::err(
                StatusCode::NOT_FOUND,
                mazbri_i18n::mfa::user_not_found(),
            ))
        })?;
    if !state.mfa_enabled {
        return Err(Box::new(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            mazbri_i18n::mfa::not_enabled(),
        )));
    }
    if state.mfa_required {
        crate::audit::set_failure_reason("mfa_required_by_policy");
        return Err(Box::new(ApiResponse::err(
            StatusCode::FORBIDDEN,
            mazbri_i18n::mfa::required_cannot_disable(),
        )));
    }
    Ok(())
}

pub struct DisableParams<'a> {
    pub pool: &'a PgPool,
    pub audit: &'a AuditService,
    pub user_id: uuid::Uuid,
    pub claims: &'a crate::middleware::auth::AdminClaims,
    pub old_refresh: &'a crate::middleware::auth::AdminClaims,
    pub sxp: i64,
    pub jwt_secret: &'a str,
    pub redis: &'a mazbri_core::redis::RedisClient,
    pub access_ttl_fallback: u64,
    pub client_ip: Option<String>,
}

pub async fn disable(params: DisableParams<'_>) -> axum::response::Response {
    let DisableParams {
        pool,
        audit,
        user_id,
        claims,
        old_refresh,
        sxp,
        jwt_secret,
        redis,
        access_ttl_fallback,
        client_ip,
    } = params;
    let username = claims.sub.as_str();
    if let Err(response) = ensure_mfa_can_be_disabled(pool, user_id).await {
        return *response;
    }
    let issued_at = chrono::Utc::now().timestamp();
    if sxp <= issued_at {
        return crate::error::ApiError::Unauthorized.into_response();
    }
    let changed_at =
        chrono::DateTime::from_timestamp(issued_at, 0).unwrap_or_else(chrono::Utc::now);
    if repo::disable_mfa(pool, user_id, changed_at).await.is_err() {
        return ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::failed_disable(),
        );
    }
    let resigned = match resign_bound_session(
        pool,
        claims,
        jwt_secret,
        access_ttl_fallback,
        sxp,
        issued_at,
    )
    .await
    {
        Ok(resigned) => resigned,
        Err(response) => return *response,
    };
    revoke_old_refresh(redis, old_refresh, issued_at).await;
    crate::services::mfa::step_up::close_session(redis, &claims.sid).await;
    audit.log(user_audit(
        user_id,
        AuditAction::MfaDisable,
        Some(serde_json::json!(
            { "action" : "mfa_disabled", "username" : username, }
        )),
        client_ip,
    ));
    info!("MFA disabled for user: {}", username);
    crate::middleware::auth::json_with_session_cookies(
        serde_json::json!({
            "message": mazbri_i18n::mfa::disabled_success(),
            "messageKey": "api.mfa.disabled_success",
            "token": resigned.access_token,
            "refresh_token": resigned.refresh_token,
            "expires_in": resigned.expires_in,
        }),
        Some(resigned.refresh_max_age),
    )
}
pub struct VerifyParams<'a> {
    pub pool: &'a PgPool,
    pub redis: &'a RedisClient,
    pub mfa_cache: &'a Arc<MfaServiceCache>,
    pub audit: &'a AuditService,
    pub jwt_secret: &'a str,
    pub refresh_token_expiration_secs: u64,
    /// 已按 `mfa_pending` 类型校验过的临时令牌声明。
    pub temp_claims: &'a AdminClaims,
    pub user_id: uuid::Uuid,
    pub totp_code: Option<&'a str>,
    pub backup_code: Option<&'a str>,
    pub client_ip: Option<String>,
}

/// MFA 校验通过后签发访问令牌与刷新令牌。
async fn issue_verified_token(
    pool: &PgPool,
    jwt_secret: &str,
    username: &str,
    user_id_str: &str,
    role: &str,
    sid: &str,
    refresh_token_expiration_secs: u64,
) -> Result<(String, String, u64, u64), Box<axum::response::Response>> {
    let issued_at = chrono::Utc::now().timestamp();
    let binding = crate::middleware::admin_token::SessionBinding {
        sid: sid.to_string(),
        sxp: Some(
            issued_at.saturating_add(i64::saturating_from(refresh_token_expiration_secs).max(1)),
        ),
    };
    let access_ttl =
        crate::services::admin::resolve_access_token_expiration_secs(pool, 86400).await;
    let token = crate::services::admin::generate_admin_jwt(
        username,
        user_id_str,
        role,
        &binding,
        jwt_secret,
        i64::saturating_from(access_ttl),
        issued_at,
    )
    .map_err(|error| {
        error!("Failed to generate JWT: {}", error);
        Box::new(ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::failed_generate_token(),
        ))
    })?;
    let refresh_token = crate::services::admin::issue_refresh_token(
        username,
        user_id_str,
        role,
        &binding,
        jwt_secret,
        issued_at,
    )
    .map_err(|error| {
        error!("Failed to generate refresh token: {}", error);
        Box::new(ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::failed_generate_token(),
        ))
    })?;
    let expires_in =
        u64::try_from(token.exp.saturating_sub(token.iat).max(1)).unwrap_or(access_ttl);
    let refresh_max_age = u64::try_from(
        binding
            .sxp
            .unwrap_or(token.exp)
            .saturating_sub(issued_at)
            .max(1),
    )
    .unwrap_or(refresh_token_expiration_secs);
    Ok((
        token.token,
        refresh_token.token,
        expires_in,
        refresh_max_age,
    ))
}

fn factor_login_error(
    error: &crate::services::mfa::second_factor::FactorError,
) -> axum::response::Response {
    use crate::services::mfa::second_factor::FactorError;
    match error {
        FactorError::InvalidRequest => {
            crate::audit::set_failure_reason("mfa_invalid_request");
            crate::response::coded_err(
                StatusCode::BAD_REQUEST,
                "MFA_INVALID_REQUEST",
                mazbri_i18n::mfa::invalid_request(),
                &serde_json::json!({}),
            )
        }
        FactorError::Invalid { attempts_remaining } => {
            crate::audit::set_failure_reason("mfa_invalid_code");
            crate::response::coded_err(
                StatusCode::BAD_REQUEST,
                "MFA_INVALID_CODE",
                mazbri_i18n::mfa::invalid_code(),
                &serde_json::json!({ "attempts_remaining": attempts_remaining }),
            )
        }
        FactorError::Reused => {
            crate::audit::set_failure_reason("mfa_code_reused");
            crate::response::coded_err(
                StatusCode::BAD_REQUEST,
                "MFA_CODE_REUSED",
                mazbri_i18n::mfa::code_reused(),
                &serde_json::json!({}),
            )
        }
        FactorError::Locked {
            retry_after_secs,
            triggered,
        } => {
            if *triggered {
                crate::audit::set_failure_reason("mfa_lock_triggered");
            } else {
                crate::audit::set_failure_reason("mfa_locked");
            }
            crate::audit::mark_security_event();
            crate::response::coded_err(
                StatusCode::FORBIDDEN,
                "MFA_LOCKED",
                mazbri_i18n::mfa::locked(),
                &serde_json::json!({ "retry_after_secs": retry_after_secs }),
            )
        }
        FactorError::NotEnabled => {
            ApiResponse::err(StatusCode::BAD_REQUEST, mazbri_i18n::mfa::not_enabled())
        }
        FactorError::Unavailable => {
            crate::audit::set_failure_reason("mfa_check_unavailable");
            crate::response::coded_err(
                StatusCode::SERVICE_UNAVAILABLE,
                "MFA_CHECK_UNAVAILABLE",
                mazbri_i18n::mfa::check_unavailable(),
                &serde_json::json!({}),
            )
        }
    }
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
    if let Err(error) = crate::services::mfa::second_factor::exclusive_codes(totp_code, backup_code)
    {
        return factor_login_error(&error);
    }
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

    let verified = match crate::services::mfa::second_factor::verify_second_factor(
        crate::services::mfa::second_factor::FactorInput {
            pool,
            service: &mfa_service,
            user_id,
            totp_code,
            backup_code,
        },
    )
    .await
    {
        Ok(verified) => verified,
        Err(error) => return factor_login_error(&error),
    };
    if let Err(response) = consume_stage_token(params.redis, params.temp_claims).await {
        if let Some(restore) = &verified.backup_restore
            && !crate::services::mfa::second_factor::restore_backup_code(pool, user_id, restore)
                .await
        {
            crate::audit::set_failure_reason("mfa_backup_restore_failed");
            crate::audit::mark_security_event();
        }
        return *response;
    }
    let used_backup_code = matches!(
        verified.method,
        crate::services::mfa::second_factor::FactorMethod::BackupCode
    );
    finish_verified_login(FinishVerifiedLoginParams {
        pool,
        audit,
        jwt_secret,
        refresh_token_expiration_secs: params.refresh_token_expiration_secs,
        user_id,
        username,
        user_id_str,
        role,
        sid: params.temp_claims.sid.as_str(),
        created_at: state.created_at,
        used_backup_code,
        client_ip,
    })
    .await
}

struct FinishVerifiedLoginParams<'a> {
    pool: &'a PgPool,
    audit: &'a AuditService,
    jwt_secret: &'a str,
    refresh_token_expiration_secs: u64,
    user_id: uuid::Uuid,
    username: &'a str,
    user_id_str: &'a str,
    role: &'a str,
    sid: &'a str,
    created_at: chrono::DateTime<chrono::Utc>,
    used_backup_code: bool,
    client_ip: Option<String>,
}

async fn finish_verified_login(params: FinishVerifiedLoginParams<'_>) -> axum::response::Response {
    let (token, refresh_token, access_ttl, refresh_max_age) = match issue_verified_token(
        params.pool,
        params.jwt_secret,
        params.username,
        params.user_id_str,
        params.role,
        params.sid,
        params.refresh_token_expiration_secs,
    )
    .await
    {
        Ok(issued) => issued,
        Err(response) => return *response,
    };
    if let Err(error) = params
        .audit
        .log_critical(user_audit(
            params.user_id,
            AuditAction::UserLogin,
            Some(serde_json::json!({
                "username": params.username,
                "role": params.role,
                "mfa_verified": true,
                "used_backup_code": params.used_backup_code,
            })),
            params.client_ip,
        ))
        .await
    {
        error!(
            "Failed to write critical audit log for MFA verification: {}",
            error
        );
    }
    info!("MFA verification successful for user: {}", params.username);
    let user = serde_json::json!({
        "id": params.user_id_str,
        "username": params.username,
        "role": params.role,
        "created_at": params.created_at.to_rfc3339(),
    });
    crate::middleware::auth::json_with_session_cookies(
        serde_json::json!({
            "token": token,
            "refresh_token": refresh_token,
            "expires_in": access_ttl,
            "user": user,
        }),
        Some(refresh_max_age),
    )
}
pub async fn status(
    pool: &PgPool,
    mfa_cache: &MfaServiceCache,
    fallback_digits: u32,
    user_id: uuid::Uuid,
) -> axum::response::Response {
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
    let totp_digits = mfa_cache
        .get()
        .await
        .ok()
        .map_or(fallback_digits, |service| service.config().digits);
    ok_json(serde_json::json!({
        "enabled": row.mfa_enabled,
        "changed_at": row.mfa_changed_at.map(|time| time.to_rfc3339()),
        "mfa_required": row.mfa_required,
        "backup_codes_remaining": backup_codes_remaining,
        "totp_digits": totp_digits,
    }))
}
