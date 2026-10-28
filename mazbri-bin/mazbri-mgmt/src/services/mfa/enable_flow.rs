use super::common::{MfaServiceErrorMode, get_mfa_service, verify_and_enable_mfa};
use crate::audit::{AuditAction, AuditService, user_audit};
use crate::mfa::MfaServiceCache;
use crate::repositories::mfa as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use mazbri_core::num::SaturatingFrom;
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
    let mfa_service = match get_mfa_service(mfa_cache, MfaServiceErrorMode::InternalError).await {
        Ok(service) => service,
        Err(response) => return *response,
    };
    let Some(sxp) = claims.sxp else {
        return crate::error::ApiError::Unauthorized.into_response();
    };
    let issued_at = chrono::Utc::now().timestamp();
    if sxp <= issued_at {
        return crate::error::ApiError::Unauthorized.into_response();
    }
    let changed_at =
        chrono::DateTime::from_timestamp(issued_at, 0).unwrap_or_else(chrono::Utc::now);
    let epoch = match verify_and_enable_mfa(
        pool,
        user_id,
        &mfa_service,
        &encrypted_secret,
        totp_code,
        username,
        changed_at,
    )
    .await
    {
        Ok(epoch) => epoch,
        Err(response) => return *response,
    };
    finish_enabled_session(FinishEnable {
        pool,
        redis,
        audit,
        claims,
        jwt_secret,
        access_ttl_fallback,
        refresh_token,
        client_ip,
        user_id,
        username,
        issued_at,
        sxp,
        epoch,
        digits: mfa_service.config().digits,
        step_up_ttl: mfa_service.config().step_up_ttl_secs,
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
    refresh_token: Option<&'a str>,
    client_ip: Option<String>,
    user_id: uuid::Uuid,
    username: &'a str,
    issued_at: i64,
    sxp: i64,
    epoch: i64,
    digits: u32,
    step_up_ttl: u64,
}

async fn finish_enabled_session(params: FinishEnable<'_>) -> axum::response::Response {
    let binding = crate::middleware::admin_token::SessionBinding {
        sid: params.claims.sid.clone(),
        sxp: Some(params.sxp),
    };
    let access_ttl = crate::services::admin::resolve_access_token_expiration_secs(
        params.pool,
        params.access_ttl_fallback,
    )
    .await;
    let token = match crate::services::admin::generate_admin_jwt(
        params.username,
        &params.claims.user_id,
        &params.claims.role,
        &binding,
        params.jwt_secret,
        i64::saturating_from(access_ttl),
        params.issued_at,
    ) {
        Ok(token) => token,
        Err(error) => {
            tracing::error!("failed to resign access token after MFA enable: {error}");
            return ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::failed_generate_token(),
            );
        }
    };
    let refresh = match crate::services::admin::issue_refresh_token(
        params.username,
        &params.claims.user_id,
        &params.claims.role,
        &binding,
        params.jwt_secret,
        params.issued_at,
    ) {
        Ok(token) => token,
        Err(error) => {
            tracing::error!("failed to resign refresh token after MFA enable: {error}");
            return ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::failed_generate_token(),
            );
        }
    };
    if let Err(error) = crate::services::mfa::step_up::open_totp_window(
        params.redis,
        &params.claims.sid,
        &params.user_id.to_string(),
        params.epoch,
        params.issued_at,
        params.step_up_ttl,
        params.sxp,
    )
    .await
    {
        tracing::error!("failed to open step-up window after enable: {error}");
    }
    if let Some(old) = params.refresh_token
        && let Ok(old_claims) =
            crate::middleware::admin_token::decode_refresh(old, params.jwt_secret)
        && let Some(jti) = old_claims.jti
    {
        let ttl = (old_claims.exp - params.issued_at).max(1);
        if let Err(error) = crate::middleware::auth::revoke_jti(params.redis, &jti, ttl).await {
            tracing::warn!("failed to revoke previous refresh token after enable: {error}");
        }
    }
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
    let max_age = u64::try_from(params.sxp.saturating_sub(params.issued_at).max(1)).unwrap_or(1);
    crate::middleware::auth::json_with_session_cookies(
        serde_json::json!({
            "message": mazbri_i18n::mfa::enabled_success(),
            "messageKey": "api.mfa.enabled_success",
            "token": token.token,
            "refresh_token": refresh.token,
            "expires_in": token.exp.saturating_sub(token.iat).max(1),
            "remaining_secs": crate::services::mfa::step_up::effective_ttl(
                params.step_up_ttl,
                params.sxp,
                params.issued_at,
            ),
            "totp_digits": params.digits,
        }),
        Some(max_age),
    )
}
