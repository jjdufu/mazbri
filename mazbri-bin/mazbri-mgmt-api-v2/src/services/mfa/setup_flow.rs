use super::common::{
    MfaServiceErrorMode, get_mfa_service, hash_backup_codes, ok_json, prepare_setup_artifacts,
    store_setup_artifacts,
};
use crate::audit::{AuditAction, AuditService, user_audit};
use crate::mfa::MfaServiceCache;
use crate::repositories::mfa as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{error, info};
pub async fn setup(
    pool: &PgPool,
    mfa_cache: &Arc<MfaServiceCache>,
    audit: &AuditService,
    user_id: uuid::Uuid,
    username: &str,
    client_ip: Option<String>,
) -> axum::response::Response {
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
        AuditAction::UserUpdate,
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
        AuditAction::UserUpdate,
        Some(serde_json::json!(
            { "action" : "mfa_backup_codes_regenerated", "username" :
            username, }
        )),
        client_ip,
    ));
    info!("Backup codes regenerated for user: {}", username);
    ok_json(serde_json::json!({ "backup_codes" : backup_codes }))
}
