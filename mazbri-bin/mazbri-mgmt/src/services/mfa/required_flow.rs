use super::common::{
    MfaServiceErrorMode, get_mfa_service, is_required_by_global, ok_json, prepare_setup_artifacts,
    store_setup_artifacts, verify_and_enable_mfa,
};
use crate::audit::{AuditAction, AuditService, user_audit};
use crate::handlers::types::{AdminJwtClaims, AdminUser};
use crate::mfa::MfaServiceCache;
use crate::repositories::mfa as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use mazbri_core::num::SaturatingFrom;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{error, info, warn};
fn decode_temp_token(
    jwt_secret: &str,
    temp_token: &str,
) -> Result<AdminJwtClaims, Box<axum::response::Response>> {
    let decoding_key = jsonwebtoken::DecodingKey::from_secret(jwt_secret.as_ref());
    let validation = jsonwebtoken::Validation::default();
    match jsonwebtoken::decode::<AdminJwtClaims>(temp_token, &decoding_key, &validation) {
        Ok(data) => Ok(data.claims),
        Err(error) => {
            warn!("Invalid temp token for MFA required flow: {}", error);
            Err(Box::new(ApiResponse::err(
                StatusCode::UNAUTHORIZED,
                mazbri_i18n::mfa::invalid_temp_token(),
            )))
        }
    }
}
fn parse_claim_user_id(
    claims: &AdminJwtClaims,
) -> Result<uuid::Uuid, Box<axum::response::Response>> {
    match uuid::Uuid::parse_str(&claims.user_id) {
        Ok(user_id) => Ok(user_id),
        Err(error) => {
            warn!("Invalid user_id in temp token claims: {}", error);
            Err(Box::new(ApiResponse::err(
                StatusCode::UNAUTHORIZED,
                mazbri_i18n::mfa::invalid_temp_token(),
            )))
        }
    }
}
pub async fn required_setup(
    pool: &PgPool,
    mfa_cache: &Arc<MfaServiceCache>,
    audit: &AuditService,
    jwt_secret: &str,
    temp_token: &str,
    client_ip: Option<String>,
) -> axum::response::Response {
    let claims = match decode_temp_token(jwt_secret, temp_token) {
        Ok(claims) => claims,
        Err(response) => return *response,
    };
    let user_id = match parse_claim_user_id(&claims) {
        Ok(user_id) => user_id,
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
    let mfa_required_by_global = is_required_by_global(
        state.mfa_enabled_globally,
        state.mfa_required_for_roles.as_ref(),
        &state.role,
    );
    if !state.mfa_required && !mfa_required_by_global {
        return ApiResponse::err(StatusCode::BAD_REQUEST, mazbri_i18n::mfa::not_required());
    }
    let mfa_service = match get_mfa_service(mfa_cache, MfaServiceErrorMode::InternalError).await {
        Ok(service) => service,
        Err(response) => return *response,
    };
    let artifacts = match prepare_setup_artifacts(&mfa_service, &claims.sub) {
        Ok(artifacts) => artifacts,
        Err(response) => return *response,
    };
    if let Err(response) = store_setup_artifacts(pool, user_id, &artifacts).await {
        return *response;
    }
    audit.log(user_audit(
        user_id,
        AuditAction::UserUpdate,
        Some(serde_json::json!(
            { "action" : "mfa_required_setup_initiated", "username" : claims
            .sub, }
        )),
        client_ip,
    ));
    info!("MFA required setup initiated for user: {}", claims.sub);
    ok_json(serde_json::json!(
        { "secret" : artifacts.secret, "qr_code_url" : artifacts.qr_code_url,
        "backup_codes" : artifacts.backup_codes, "temp_token" : temp_token,
        "totp_digits" : artifacts.totp_digits }
    ))
}

pub struct RequiredEnableParams<'a> {
    pub pool: &'a PgPool,
    pub mfa_cache: &'a Arc<MfaServiceCache>,
    pub audit: &'a AuditService,
    pub jwt_secret: &'a str,
    pub jwt_expiration_secs: u64,
    pub temp_token: &'a str,
    pub totp_code: &'a str,
    pub client_ip: Option<String>,
}

/// 读取并校验「强制启用 MFA」所需的用户状态；返回状态与加密的 TOTP 密钥。
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
    let mfa_required_by_global = is_required_by_global(
        state.mfa_enabled_globally,
        state.mfa_required_for_roles.as_ref(),
        &state.role,
    );
    if !state.mfa_required && !mfa_required_by_global {
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
    let mfa_cache = params.mfa_cache;
    let audit = params.audit;
    let jwt_secret = params.jwt_secret;
    let jwt_expiration_secs = params.jwt_expiration_secs;
    let temp_token = params.temp_token;
    let totp_code = params.totp_code;
    let client_ip = params.client_ip;

    let claims = match decode_temp_token(jwt_secret, temp_token) {
        Ok(claims) => claims,
        Err(response) => return *response,
    };
    let user_id = match parse_claim_user_id(&claims) {
        Ok(user_id) => user_id,
        Err(response) => return *response,
    };
    let (state, encrypted_secret) = match load_required_enable_state(pool, user_id).await {
        Ok(loaded) => loaded,
        Err(response) => return *response,
    };
    let mfa_service = match get_mfa_service(mfa_cache, MfaServiceErrorMode::InternalError).await {
        Ok(service) => service,
        Err(response) => return *response,
    };
    if let Err(response) = verify_and_enable_mfa(
        pool,
        user_id,
        &mfa_service,
        &encrypted_secret,
        totp_code,
        &claims.sub,
    )
    .await
    {
        return *response;
    }
    let token = match crate::services::admin::generate_admin_jwt(
        &claims.sub,
        &claims.user_id,
        &state.role,
        jwt_secret,
        i64::saturating_from(jwt_expiration_secs),
    ) {
        Ok(token) => token,
        Err(error) => {
            error!("Failed to generate JWT token: {}", error);
            return ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::failed_generate_token(),
            );
        }
    };
    audit.log(user_audit(
        user_id,
        AuditAction::UserUpdate,
        Some(serde_json::json!(
            { "action" : "mfa_required_enabled", "username" : claims.sub, }
        )),
        client_ip,
    ));
    info!("MFA required enabled for user: {}", claims.sub);
    let user = AdminUser {
        id: claims.user_id.clone(),
        username: claims.sub.clone(),
        role: state.role,
        created_at: state.created_at.to_rfc3339(),
    };
    ok_json(serde_json::json!(
        { "message" : mazbri_i18n::mfa::enabled_success(), "messageKey" :
        "api.mfa.enabled_success", "token" : token, "expires_in" : 86400, "user" :
        user }
    ))
}
