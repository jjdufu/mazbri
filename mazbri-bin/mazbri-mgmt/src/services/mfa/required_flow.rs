use super::common::{
    MfaServiceErrorMode, authenticate_stage_token, consume_stage_token, get_mfa_service, ok_json,
    prepare_setup_artifacts, store_setup_artifacts, verify_and_enable_mfa,
};
use crate::audit::{AuditAction, AuditService, user_audit};
use crate::handlers::types::AdminUser;
use crate::mfa::MfaServiceCache;
use crate::middleware::admin_token::TokenKind;
use crate::repositories::mfa as repo;
use crate::response::ApiResponse;
use crate::services::admin::issue_stage_token;
use axum::http::StatusCode;
use mazbri_core::num::SaturatingFrom;
use mazbri_core::redis::RedisClient;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{error, info};

pub struct RequiredSetupParams<'a> {
    pub pool: &'a PgPool,
    pub redis: &'a RedisClient,
    pub mfa_cache: &'a Arc<MfaServiceCache>,
    pub audit: &'a AuditService,
    pub jwt_secret: &'a str,
    pub temp_token: &'a str,
    pub client_ip: Option<String>,
}

/// 强制 MFA 设置第一步：校验 `mfa_setup` 临时令牌，生成 TOTP 密钥；
/// 旧临时令牌被消费，响应中返回一个新的 `mfa_setup` 令牌供 `required-enable` 使用。
pub async fn required_setup(params: RequiredSetupParams<'_>) -> axum::response::Response {
    let pool = params.pool;
    let (claims, user_id) = match authenticate_stage_token(
        params.redis,
        params.jwt_secret,
        params.temp_token,
        TokenKind::MfaSetup,
    )
    .await
    {
        Ok(authenticated) => authenticated,
        Err(response) => return *response,
    };
    let state = match repo::get_mfa_required_setup_state(pool, user_id).await {
        Ok(state) => state,
        Err(error) => {
            error!("Database error during MFA required setup: {}", error);
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
        return ApiResponse::err(StatusCode::BAD_REQUEST, mazbri_i18n::mfa::already_enabled());
    }
    if !state.mfa_required {
        return ApiResponse::err(StatusCode::BAD_REQUEST, mazbri_i18n::mfa::not_required());
    }
    let mfa_service =
        match get_mfa_service(params.mfa_cache, MfaServiceErrorMode::InternalError).await {
            Ok(service) => service,
            Err(response) => return *response,
        };
    let artifacts = match prepare_setup_artifacts(&mfa_service, &claims.sub) {
        Ok(artifacts) => artifacts,
        Err(response) => return *response,
    };
    if let Err(response) = consume_stage_token(params.redis, &claims).await {
        return *response;
    }
    let stage_binding = crate::middleware::admin_token::SessionBinding::stage(claims.sid.clone());
    let Ok(next_token) = issue_stage_token(
        TokenKind::MfaSetup,
        &claims.sub,
        user_id,
        &claims.role,
        &stage_binding,
        params.jwt_secret,
    ) else {
        return ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::failed_generate_token(),
        );
    };
    if let Err(response) = store_setup_artifacts(pool, user_id, &artifacts).await {
        return *response;
    }
    params.audit.log(user_audit(
        user_id,
        AuditAction::MfaRequiredSetup,
        Some(serde_json::json!({
            "action": "mfa_required_setup_initiated",
            "username": claims.sub,
        })),
        params.client_ip,
    ));
    info!("MFA required setup initiated for user: {}", claims.sub);
    ok_json(serde_json::json!({
        "secret": artifacts.secret,
        "qr_code_url": artifacts.qr_code_url,
        "backup_codes": artifacts.backup_codes,
        "temp_token": next_token,
        "totp_digits": artifacts.totp_digits,
    }))
}

pub struct RequiredEnableParams<'a> {
    pub pool: &'a PgPool,
    pub redis: &'a RedisClient,
    pub mfa_cache: &'a Arc<MfaServiceCache>,
    pub audit: &'a AuditService,
    pub jwt_secret: &'a str,
    pub jwt_expiration_secs: u64,
    pub refresh_token_expiration_secs: u64,
    pub temp_token: &'a str,
    pub totp_code: &'a str,
    pub client_ip: Option<String>,
}

async fn load_required_enable_state(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<(repo::MfaRequiredEnableState, String), Box<axum::response::Response>> {
    let state = match repo::get_mfa_required_enable_state(pool, user_id).await {
        Ok(state) => state,
        Err(error) => {
            error!("Database error during MFA required enable: {}", error);
            return Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::database_error(),
            )));
        }
    };
    let Some(mut state) = state else {
        return Err(Box::new(ApiResponse::err(
            StatusCode::NOT_FOUND,
            mazbri_i18n::mfa::user_not_found(),
        )));
    };
    if state.mfa_enabled {
        return Err(Box::new(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            mazbri_i18n::mfa::already_enabled_simple(),
        )));
    }
    if !state.mfa_required {
        return Err(Box::new(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            mazbri_i18n::mfa::not_required(),
        )));
    }
    let Some(encrypted_secret) = state.mfa_secret.take() else {
        return Err(Box::new(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            mazbri_i18n::mfa::required_setup_needed(),
        )));
    };
    Ok((state, encrypted_secret))
}

fn issue_bound_session(
    claims: &crate::middleware::auth::AdminClaims,
    role: &str,
    jwt_secret: &str,
    access_ttl: u64,
    refresh_ttl: u64,
    issued_at: i64,
) -> Result<
    (
        crate::middleware::admin_token::IssuedToken,
        crate::middleware::admin_token::IssuedToken,
    ),
    Box<axum::response::Response>,
> {
    let binding = crate::middleware::admin_token::SessionBinding {
        sid: claims.sid.clone(),
        sxp: Some(issued_at.saturating_add(i64::saturating_from(refresh_ttl).max(1))),
    };
    let token = crate::services::admin::generate_admin_jwt(
        &claims.sub,
        &claims.user_id,
        role,
        &binding,
        jwt_secret,
        i64::saturating_from(access_ttl),
        issued_at,
    )
    .map_err(|error| {
        error!("Failed to generate JWT token: {}", error);
        Box::new(ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::failed_generate_token(),
        ))
    })?;
    let refresh_token = crate::services::admin::issue_refresh_token(
        &claims.sub,
        &claims.user_id,
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
    Ok((token, refresh_token))
}

pub async fn required_enable(params: RequiredEnableParams<'_>) -> axum::response::Response {
    let pool = params.pool;
    let (claims, user_id) = match authenticate_stage_token(
        params.redis,
        params.jwt_secret,
        params.temp_token,
        TokenKind::MfaSetup,
    )
    .await
    {
        Ok(authenticated) => authenticated,
        Err(response) => return *response,
    };
    let (state, encrypted_secret) = match load_required_enable_state(pool, user_id).await {
        Ok(loaded) => loaded,
        Err(response) => return *response,
    };
    let mfa_service =
        match get_mfa_service(params.mfa_cache, MfaServiceErrorMode::InternalError).await {
            Ok(service) => service,
            Err(response) => return *response,
        };
    let issued_at = chrono::Utc::now().timestamp();
    let changed_at =
        chrono::DateTime::from_timestamp(issued_at, 0).unwrap_or_else(chrono::Utc::now);
    let epoch = match verify_and_enable_mfa(
        pool,
        user_id,
        &mfa_service,
        &encrypted_secret,
        params.totp_code,
        &claims.sub,
        changed_at,
    )
    .await
    {
        Ok(epoch) => epoch,
        Err(response) => return *response,
    };
    let refresh_ttl = i64::try_from(params.refresh_token_expiration_secs)
        .unwrap_or(i64::MAX)
        .max(1);
    let sxp = issued_at.saturating_add(refresh_ttl);
    if let Err(error) = crate::services::mfa::step_up::open_totp_window(
        params.redis,
        &claims.sid,
        &user_id.to_string(),
        epoch,
        issued_at,
        mfa_service.config().step_up_ttl_secs,
        sxp,
    )
    .await
    {
        tracing::error!("failed to open step-up window after required enable: {error}");
    }
    if let Err(response) = consume_stage_token(params.redis, &claims).await {
        return *response;
    }
    let access_ttl = crate::services::admin::resolve_access_token_expiration_secs(
        pool,
        params.jwt_expiration_secs,
    )
    .await;
    let (token, refresh_token) = match issue_bound_session(
        &claims,
        &state.role,
        params.jwt_secret,
        access_ttl,
        params.refresh_token_expiration_secs,
        issued_at,
    ) {
        Ok(issued) => issued,
        Err(response) => return *response,
    };
    params.audit.log(user_audit(
        user_id,
        AuditAction::MfaRequiredEnable,
        Some(serde_json::json!({
            "action": "mfa_required_enabled",
            "username": claims.sub,
        })),
        params.client_ip.clone(),
    ));
    info!("MFA required enabled for user: {}", claims.sub);
    let user = AdminUser {
        id: claims.user_id.clone(),
        username: claims.sub.clone(),
        role: state.role,
        created_at: state.created_at.to_rfc3339(),
    };
    ok_json(serde_json::json!({
        "message": mazbri_i18n::mfa::enabled_success(),
        "messageKey": "api.mfa.enabled_success",
        "token": token.token,
        "refresh_token": refresh_token.token,
        "expires_in": token.exp.saturating_sub(token.iat).max(1),
        "user": user,
    }))
}
