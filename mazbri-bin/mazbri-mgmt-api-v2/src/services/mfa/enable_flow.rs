use super::common::{MfaServiceErrorMode, get_mfa_service, ok_json, verify_and_enable_mfa};
use crate::audit::{AuditAction, AuditService, user_audit};
use crate::mfa::MfaServiceCache;
use crate::repositories::mfa as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::info;
pub async fn enable(
    pool: &PgPool,
    mfa_cache: &Arc<MfaServiceCache>,
    audit: &AuditService,
    user_id: uuid::Uuid,
    username: &str,
    totp_code: &str,
    client_ip: Option<String>,
) -> axum::response::Response {
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
    if let Err(response) = verify_and_enable_mfa(
        pool,
        user_id,
        &mfa_service,
        &encrypted_secret,
        totp_code,
        username,
    )
    .await
    {
        return *response;
    }

    audit.log(user_audit(
        user_id,
        AuditAction::UserUpdate,
        Some(serde_json::json!(
            { "action" : "mfa_enabled", "username" : username, }
        )),
        client_ip,
    ));
    info!("MFA enabled for user: {}", username);
    ok_json(serde_json::json!(
        { "message" : mazbri_i18n::mfa::enabled_success(), "messageKey" :
        "api.mfa.enabled_success" }
    ))
}
