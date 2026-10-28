use super::common::{
    MfaServiceErrorMode, advance_enable_step, authenticate_stage_token, commit_required_enable,
    consume_stage_token, get_mfa_service, ok_json, prepare_setup_artifacts, resign_bound_session,
    store_setup_artifacts, unavailable_with_session, verify_totp_for_enable,
};
use crate::audit::{AuditAction, AuditService, user_audit};
use crate::handlers::types::AdminUser;
use crate::mfa::MfaServiceCache;
use crate::middleware::admin_token::TokenKind;
use crate::repositories::mfa as repo;
use crate::response::ApiResponse;
use crate::services::admin::issue_stage_token;
use axum::http::StatusCode;
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
    let step_at = match verify_totp_for_enable(
        &mfa_service,
        &encrypted_secret,
        params.totp_code,
        &claims.sub,
    ) {
        Ok(step_at) => step_at,
        Err(response) => return *response,
    };
    if let Err(response) = advance_enable_step(pool, user_id, step_at).await {
        return *response;
    }
    if let Err(response) = consume_stage_token(params.redis, &claims).await {
        return *response;
    }
    let issued_at = chrono::Utc::now().timestamp();
    let changed_at =
        chrono::DateTime::from_timestamp(issued_at, 0).unwrap_or_else(chrono::Utc::now);
    let epoch = match commit_required_enable(pool, user_id, changed_at).await {
        Ok(epoch) => epoch,
        Err(response) => return *response,
    };
    finish_required_enable(FinishRequiredEnable {
        params: &params,
        pool,
        claims: &claims,
        user_id,
        role: state.role,
        created_at: state.created_at,
        epoch,
        issued_at,
        step_up_ttl: mfa_service.config().step_up_ttl_secs,
    })
    .await
}

struct FinishRequiredEnable<'a> {
    params: &'a RequiredEnableParams<'a>,
    pool: &'a PgPool,
    claims: &'a crate::middleware::auth::AdminClaims,
    user_id: uuid::Uuid,
    role: String,
    created_at: chrono::DateTime<chrono::Utc>,
    epoch: i64,
    issued_at: i64,
    step_up_ttl: u64,
}

async fn finish_required_enable(args: FinishRequiredEnable<'_>) -> axum::response::Response {
    let refresh_ttl = i64::try_from(args.params.refresh_token_expiration_secs)
        .unwrap_or(i64::MAX)
        .max(1);
    let sxp = args.issued_at.saturating_add(refresh_ttl);
    let session_claims = crate::middleware::auth::AdminClaims {
        role: args.role.clone(),
        ..args.claims.clone()
    };
    let resigned = match resign_bound_session(
        args.pool,
        &session_claims,
        args.params.jwt_secret,
        args.params.jwt_expiration_secs,
        sxp,
        args.issued_at,
    )
    .await
    {
        Ok(resigned) => resigned,
        Err(response) => return *response,
    };
    let window_opened = crate::services::mfa::step_up::open_totp_window(
        args.params.redis,
        &args.claims.sid,
        &args.user_id.to_string(),
        args.epoch,
        args.issued_at,
        args.step_up_ttl,
        sxp,
    )
    .await
    .is_ok();
    args.params.audit.log(user_audit(
        args.user_id,
        AuditAction::MfaRequiredEnable,
        Some(serde_json::json!({
            "action": "mfa_required_enabled",
            "username": args.claims.sub,
        })),
        args.params.client_ip.clone(),
    ));
    info!("MFA required enabled for user: {}", args.claims.sub);
    if !window_opened {
        tracing::error!("failed to open step-up window after required enable");
        return unavailable_with_session(
            &resigned.access_token,
            &resigned.refresh_token,
            u64::try_from(resigned.expires_in).unwrap_or(1),
            resigned.refresh_max_age,
        );
    }
    let user = AdminUser {
        id: args.claims.user_id.clone(),
        username: args.claims.sub.clone(),
        role: args.role,
        created_at: args.created_at.to_rfc3339(),
    };
    crate::middleware::auth::json_with_session_cookies(
        serde_json::json!({
            "message": mazbri_i18n::mfa::enabled_success(),
            "messageKey": "api.mfa.enabled_success",
            "token": resigned.access_token,
            "refresh_token": resigned.refresh_token,
            "expires_in": resigned.expires_in,
            "user": user,
        }),
        Some(resigned.refresh_max_age),
    )
}
