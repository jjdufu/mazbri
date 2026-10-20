use super::common::{MfaServiceErrorMode, get_mfa_service, ok_json};
use crate::audit::{AuditAction, AuditService, user_audit};
use crate::handlers::types::AdminJwtClaims;
use crate::mfa::MfaServiceCache;
use crate::repositories::mfa as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use chrono::Utc;
use mazbri_core::num::SaturatingFrom;
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
    if let Some(encrypted_secret) = &state.mfa_secret {
        if let Ok(mfa_service) = mfa_cache.get().await {
            if let Ok(secret) = mfa_service.decrypt_secret(encrypted_secret) {
                if mfa_service.verify_totp(&secret, verification_code) {
                    verified = true;
                }
            }
        }
    }
    if !verified {
        if let Some(hash) = state.password_hash {
            if mazbri_auth::constant_time_argon2_verify(verification_code.as_bytes(), &hash).is_ok()
            {
                verified = true;
            }
        }
    }
    if !verified {
        warn!("Invalid verification code for MFA disable: {}", username);
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
        AuditAction::UserUpdate,
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
    pub mfa_cache: &'a Arc<MfaServiceCache>,
    pub audit: &'a AuditService,
    pub jwt_secret: &'a str,
    pub user_id: uuid::Uuid,
    pub username: &'a str,
    pub user_id_str: &'a str,
    pub role: &'a str,
    pub totp_code: Option<&'a str>,
    pub backup_code: Option<&'a str>,
    pub client_ip: Option<String>,
}

pub async fn verify(params: VerifyParams<'_>) -> axum::response::Response {
    let pool = params.pool;
    let mfa_cache = params.mfa_cache;
    let audit = params.audit;
    let jwt_secret = params.jwt_secret;
    let user_id = params.user_id;
    let username = params.username;
    let user_id_str = params.user_id_str;
    let role = params.role;
    let totp_code = params.totp_code;
    let backup_code = params.backup_code;
    let client_ip = params.client_ip;

    info!(
        "[mfa_verify] Processing: user_id={}, totp_code={:?}, backup_code={:?}",
        user_id, totp_code, backup_code
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
        return ApiResponse::err(StatusCode::FORBIDDEN, mazbri_i18n::mfa::locked());
    }
    let mut verified = false;
    let mut used_backup_code_idx: Option<usize> = None;
    if let Some(code) = totp_code {
        if let Some(encrypted_secret) = &state.mfa_secret {
            if let Ok(secret) = mfa_service.decrypt_secret(encrypted_secret) {
                if mfa_service.verify_totp(&secret, code) {
                    verified = true;
                }
            }
        }
    }
    if !verified {
        if let Some(code) = backup_code {
            if let Some(codes_value) = &state.mfa_backup_codes {
                if let Some(hashed_codes) = codes_value.as_array() {
                    let hashed_codes: Vec<String> = hashed_codes
                        .iter()
                        .filter_map(|value| value.as_str().map(std::string::ToString::to_string))
                        .collect();
                    if let Ok(Some(index)) = mfa_service.verify_backup_code(code, &hashed_codes) {
                        verified = true;
                        used_backup_code_idx = Some(index);
                    }
                }
            }
        }
    }
    if !verified {
        let new_attempts = state.mfa_attempts + 1;
        let lockout_until = if new_attempts >= mfa_service.config().max_attempts {
            Some(mfa_service.calculate_lockout_until())
        } else {
            None
        };
        if let Err(error) =
            repo::update_mfa_attempts(pool, user_id, new_attempts, lockout_until).await
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
        warn!(
            "MFA verification failed for user: {} (attempt {}/{})",
            username,
            new_attempts,
            mfa_service.config().max_attempts
        );
        return ApiResponse::err(StatusCode::BAD_REQUEST, mazbri_i18n::mfa::invalid_code());
    }
    if let Some(index) = used_backup_code_idx {
        if let Some(codes_value) = &state.mfa_backup_codes {
            if let Some(hashed_codes) = codes_value.as_array() {
                let mut new_codes: Vec<serde_json::Value> = hashed_codes.clone();
                new_codes.remove(index);
                if let Err(error) =
                    repo::remove_backup_code(pool, user_id, serde_json::json!(new_codes)).await
                {
                    error!(
                        "Failed to remove used backup code for user {}: {}",
                        username, error
                    );
                }
            }
        }
    }
    if let Err(error) = repo::reset_mfa_verify_state(pool, user_id).await {
        error!(
            "Failed to reset MFA attempts/update verified_at for user {}: {}",
            username, error
        );
    }
    let now = Utc::now().timestamp();
    let access_ttl =
        crate::services::admin::resolve_access_token_expiration_secs(pool, 86400).await;
    let expiration = now + i64::saturating_from(access_ttl);
    let full_claims = AdminJwtClaims {
        iss: "mazbri-mgmt-api".to_string(),
        sub: username.to_string(),
        user_id: user_id_str.to_string(),
        role: role.to_string(),
        iat: now,
        exp: expiration,
        jti: Some(uuid::Uuid::new_v4().to_string()),
        token_type: None,
    };
    let encoding_key = jsonwebtoken::EncodingKey::from_secret(jwt_secret.as_ref());
    let token = match jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &full_claims,
        &encoding_key,
    ) {
        Ok(token) => token,
        Err(error) => {
            error!("Failed to generate JWT: {}", error);
            return ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::failed_generate_token(),
            );
        }
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
    ok_json(serde_json::json!({ "token" : token, "expires_in" : 86400, "user" : user }))
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
