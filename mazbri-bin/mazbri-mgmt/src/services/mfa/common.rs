use crate::mfa::{MfaError, MfaService, MfaServiceCache};
use crate::repositories::mfa as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::error;
pub(super) enum MfaServiceErrorMode {
    ServerNotConfigured,
    InternalError,
}
pub(super) struct SetupArtifacts {
    pub secret: String,
    pub backup_codes: Vec<String>,
    pub encrypted_secret: String,
    pub hashed_codes: Vec<String>,
    pub qr_code_url: String,
    pub totp_digits: u32,
}
pub(super) fn is_required_by_global(
    mfa_enabled_globally: bool,
    required_roles: Option<&Vec<String>>,
    user_role: &str,
) -> bool {
    if mfa_enabled_globally {
        if let Some(roles) = required_roles {
            return roles.contains(&user_role.to_string());
        }
    }
    false
}
pub(super) async fn get_mfa_service(
    mfa_cache: &Arc<MfaServiceCache>,
    mode: MfaServiceErrorMode,
) -> Result<Arc<MfaService>, Box<axum::response::Response>> {
    match mfa_cache.get().await {
        Ok(service) => Ok(service),
        Err(MfaError::EncryptionKeyMissing)
            if matches!(mode, MfaServiceErrorMode::ServerNotConfigured) =>
        {
            error!("MFA encryption key not configured in database");
            Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::server_not_configured(),
            )))
        }
        Err(error) => {
            error!("Failed to create MFA service: {:?}", error);
            Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::internal_error(),
            )))
        }
    }
}
pub(super) fn hash_backup_codes(
    mfa_service: &MfaService,
    backup_codes: &[String],
    error_message: String,
) -> Result<Vec<String>, Box<axum::response::Response>> {
    let mut hashed_codes = Vec::with_capacity(backup_codes.len());
    for code in backup_codes {
        match mfa_service.hash_backup_code(code) {
            Ok(hashed) => hashed_codes.push(hashed),
            Err(error) => {
                error!("Failed to hash backup code: {}", error);
                return Err(Box::new(ApiResponse::err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error_message,
                )));
            }
        }
    }
    Ok(hashed_codes)
}
pub(super) fn prepare_setup_artifacts(
    mfa_service: &MfaService,
    username: &str,
) -> Result<SetupArtifacts, Box<axum::response::Response>> {
    let secret = mfa_service.generate_secret();
    let backup_codes = mfa_service.generate_backup_codes();
    let encrypted_secret = match mfa_service.encrypt_secret(&secret) {
        Ok(encrypted) => encrypted,
        Err(error) => {
            error!("Failed to encrypt MFA secret: {}", error);
            return Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::failed_setup(),
            )));
        }
    };
    let hashed_codes =
        hash_backup_codes(mfa_service, &backup_codes, mazbri_i18n::mfa::failed_setup())?;
    let qr_code_url = mfa_service.generate_qr_code_url(&secret, username);
    Ok(SetupArtifacts {
        secret,
        backup_codes,
        encrypted_secret,
        hashed_codes,
        qr_code_url,
        totp_digits: mfa_service.config().digits,
    })
}
pub(super) async fn store_setup_artifacts(
    pool: &PgPool,
    user_id: uuid::Uuid,
    artifacts: &SetupArtifacts,
) -> Result<(), Box<axum::response::Response>> {
    if let Err(error) = repo::store_mfa_secret(
        pool,
        user_id,
        &artifacts.encrypted_secret,
        &artifacts.hashed_codes,
    )
    .await
    {
        error!("Failed to store MFA secret: {}", error);
        return Err(Box::new(ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::failed_store(),
        )));
    }
    Ok(())
}
pub(super) async fn verify_and_enable_mfa(
    pool: &PgPool,
    user_id: uuid::Uuid,
    mfa_service: &MfaService,
    encrypted_secret: &str,
    totp_code: &str,
    username: &str,
) -> Result<(), Box<axum::response::Response>> {
    let secret = match mfa_service.decrypt_secret(encrypted_secret) {
        Ok(secret) => secret,
        Err(error) => {
            error!("Failed to decrypt MFA secret: {}", error);
            return Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::failed_verify(),
            )));
        }
    };
    if !mfa_service.verify_totp(&secret, totp_code) {
        tracing::warn!("Invalid TOTP code for user: {}", username);
        return Err(Box::new(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            mazbri_i18n::mfa::invalid_totp(),
        )));
    }
    if let Err(error) = repo::enable_mfa(pool, user_id).await {
        error!("Failed to enable MFA: {}", error);
        return Err(Box::new(ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::failed_enable(),
        )));
    }
    Ok(())
}

pub(super) fn ok_json(value: serde_json::Value) -> axum::response::Response {
    // Session tokens go into HttpOnly cookies when present; body keeps user/MFA fields only.
    crate::middleware::auth::json_with_session_cookies(value, None)
}
