use super::common::{
    MfaServiceErrorMode, commit_voluntary_enable, get_mfa_service, guard_session_rotation,
    resign_bound_session, revoke_old_refresh, verify_totp_for_enable,
};
use crate::audit::{AuditAction, AuditService, user_audit};
use crate::mfa::MfaServiceCache;
use crate::repositories::mfa as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::info;
pub struct EnableParams<'a> {
    pub pool: &'a PgPool,
    pub redis: &'a mazbri_core::redis::RedisClient,
    pub mfa_cache: &'a Arc<MfaServiceCache>,
    pub audit: &'a AuditService,
    pub claims: &'a crate::middleware::auth::AdminClaims,
    pub jwt_secret: &'a str,
    pub access_ttl_fallback: u64,
    pub totp_code: &'a str,
    pub refresh_token: Option<&'a str>,
    pub client_ip: Option<String>,
}

pub async fn enable(params: EnableParams<'_>) -> axum::response::Response {
    let EnableParams {
        pool,
        redis,
        mfa_cache,
        audit,
        claims,
        jwt_secret,
        access_ttl_fallback,
        totp_code,
        refresh_token,
        client_ip,
    } = params;
    let Ok(user_id) = uuid::Uuid::parse_str(&claims.user_id) else {
        return crate::error::ApiError::Unauthorized.into_response();
    };
    let username = claims.sub.as_str();
    let state = match repo::get_mfa_setup_state(pool, user_id).await {
        Ok(state) => state,
        Err(error) => {
            tracing::error!("Database error during MFA enable: {}", error);
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
            mazbri_i18n::mfa::already_enabled_simple(),
        );
    }
    let Some(encrypted_secret) = state.mfa_secret else {
        return ApiResponse::err(StatusCode::BAD_REQUEST, mazbri_i18n::mfa::not_setup());
    };
    let issued_at = chrono::Utc::now().timestamp();
    let rotation = match guard_session_rotation(claims, refresh_token, jwt_secret, issued_at) {
        Ok(rotation) => rotation,
        Err(response) => return *response,
    };
    let mfa_service = match get_mfa_service(mfa_cache, MfaServiceErrorMode::InternalError).await {
        Ok(service) => service,
        Err(response) => return *response,
    };
    let step_at = match verify_totp_for_enable(&mfa_service, &encrypted_secret, totp_code, username)
    {
        Ok(step_at) => step_at,
        Err(response) => return *response,
    };
    let changed_at =
        chrono::DateTime::from_timestamp(issued_at, 0).unwrap_or_else(chrono::Utc::now);
    if let Err(response) = commit_voluntary_enable(pool, user_id, step_at, changed_at).await {
        return *response;
    }
    finish_enabled_session(FinishEnable {
        pool,
        redis,
        audit,
        claims,
        jwt_secret,
        access_ttl_fallback,
        old_refresh: &rotation.old_refresh,
        client_ip,
        user_id,
        username,
        issued_at,
        sxp: rotation.sxp,
        digits: mfa_service.config().digits,
    })
    .await
}

struct FinishEnable<'a> {
    pool: &'a PgPool,
    redis: &'a mazbri_core::redis::RedisClient,
    audit: &'a AuditService,
    claims: &'a crate::middleware::auth::AdminClaims,
    jwt_secret: &'a str,
    access_ttl_fallback: u64,
    old_refresh: &'a crate::middleware::auth::AdminClaims,
    client_ip: Option<String>,
    user_id: uuid::Uuid,
    username: &'a str,
    issued_at: i64,
    sxp: i64,
    digits: u32,
}

async fn finish_enabled_session(params: FinishEnable<'_>) -> axum::response::Response {
    let resigned = match resign_bound_session(
        params.pool,
        params.claims,
        params.jwt_secret,
        params.access_ttl_fallback,
        params.sxp,
        params.issued_at,
    )
    .await
    {
        Ok(resigned) => resigned,
        Err(response) => return *response,
    };
    revoke_old_refresh(params.redis, params.old_refresh, params.issued_at).await;
    params.audit.log(user_audit(
        params.user_id,
        AuditAction::MfaEnable,
        Some(serde_json::json!({
            "action": "mfa_enabled",
            "username": params.username,
            "sid": params.claims.sid,
        })),
        params.client_ip,
    ));
    info!("MFA enabled for user: {}", params.username);
    crate::middleware::auth::json_with_session_cookies(
        serde_json::json!({
            "message": mazbri_i18n::mfa::enabled_success(),
            "messageKey": "api.mfa.enabled_success",
            "token": resigned.access_token,
            "refresh_token": resigned.refresh_token,
            "expires_in": resigned.expires_in,
            "totp_digits": params.digits,
        }),
        Some(resigned.refresh_max_age),
    )
}
