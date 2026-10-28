use super::super::AppState;
use crate::audit::{AuditAction, extract_client_ip};
use crate::middleware::auth::extract_user_id_response;
use crate::services::mfa::second_factor::{self, FactorError, FactorInput, FactorMethod};
use crate::services::mfa::step_up::{self, FreshPurpose, StepUpMethod, StepUpWindow};
use axum::{
    Extension, Json,
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MfaStepUpRequest {
    pub totp_code: Option<String>,
    pub backup_code: Option<String>,
    pub purpose: Option<String>,
}

fn error_response(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
    extra: &serde_json::Value,
) -> Response {
    let message = message.into();
    crate::audit::set_failure_reason(match code {
        "MFA_INVALID_CODE" => "mfa_invalid_code",
        "MFA_CODE_REUSED" => "mfa_code_reused",
        "MFA_LOCKED" => "mfa_locked",
        "MFA_NOT_ENABLED" => "mfa_not_enabled",
        "MFA_INVALID_REQUEST" => "mfa_invalid_request",
        _ => "mfa_check_unavailable",
    });
    if matches!(
        code,
        "MFA_LOCKED" | "MFA_CODE_REUSED" | "MFA_CHECK_UNAVAILABLE"
    ) {
        crate::audit::mark_security_event();
    }
    let mut body = serde_json::json!({
        "success": false,
        "code": code,
        "error": message,
    });
    if let (Some(obj), Some(extra_obj)) = (body.as_object_mut(), extra.as_object()) {
        for (key, value) in extra_obj {
            obj.insert(key.clone(), value.clone());
        }
    }
    let mut response = (status, Json(body)).into_response();
    crate::audit::AuditErrorInfo::attach(&mut response, code, &message);
    response
}

fn factor_error(error: &FactorError) -> Response {
    match error {
        FactorError::InvalidRequest => error_response(
            StatusCode::BAD_REQUEST,
            "MFA_INVALID_REQUEST",
            mazbri_i18n::mfa::invalid_request(),
            &serde_json::json!({}),
        ),
        FactorError::Invalid { attempts_remaining } => error_response(
            StatusCode::BAD_REQUEST,
            "MFA_INVALID_CODE",
            mazbri_i18n::mfa::invalid_code(),
            &serde_json::json!({ "attempts_remaining": attempts_remaining }),
        ),
        FactorError::Reused => error_response(
            StatusCode::BAD_REQUEST,
            "MFA_CODE_REUSED",
            mazbri_i18n::mfa::code_reused(),
            &serde_json::json!({}),
        ),
        FactorError::Locked { retry_after_secs } => error_response(
            StatusCode::FORBIDDEN,
            "MFA_LOCKED",
            "MFA 已锁定，请稍后再试",
            &serde_json::json!({ "retry_after_secs": retry_after_secs }),
        ),
        FactorError::NotEnabled => error_response(
            StatusCode::FORBIDDEN,
            "MFA_NOT_ENABLED",
            "此操作需要 MFA 二次验证，请先在「账号安全」页面绑定并启用 MFA",
            &serde_json::json!({ "setup_path": crate::handlers::mfa_guard::MFA_SETUP_PATH }),
        ),
        FactorError::Unavailable => error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "MFA_CHECK_UNAVAILABLE",
            mazbri_i18n::mfa::check_unavailable(),
            &serde_json::json!({}),
        ),
    }
}

fn rfc3339(unix: i64) -> String {
    DateTime::<Utc>::from_timestamp(unix, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

fn parse_purpose(value: Option<&str>) -> Result<Option<FreshPurpose>, Box<Response>> {
    match value {
        None => Ok(None),
        Some(value) => FreshPurpose::parse(value).map(Some).ok_or_else(|| {
            Box::new(error_response(
                StatusCode::BAD_REQUEST,
                "MFA_INVALID_REQUEST",
                mazbri_i18n::mfa::invalid_request(),
                &serde_json::json!({}),
            ))
        }),
    }
}

fn step_up_body(
    window: &StepUpWindow,
    ttl: u64,
    configured_ttl: u64,
    digits: u32,
    fresh: Option<(FreshPurpose, u64)>,
) -> serde_json::Value {
    let mut data = serde_json::json!({
        "active": true,
        "opened_at": rfc3339(window.opened_at),
        "expires_at": rfc3339(window.until),
        "remaining_secs": ttl,
        "ttl_secs": configured_ttl,
        "server_time": Utc::now().to_rfc3339(),
        "method": window.method.as_str(),
        "totp_digits": digits,
    });
    if let Some((purpose, fresh_ttl)) = fresh
        && let Some(obj) = data.as_object_mut()
    {
        obj.insert(
            "fresh".to_string(),
            serde_json::json!({
                "purpose": purpose.as_str(),
                "expires_in_secs": fresh_ttl,
            }),
        );
    }
    data
}

async fn load_verified_factor(
    state: &AppState,
    user_id: uuid::Uuid,
    req: &MfaStepUpRequest,
) -> Result<
    (
        std::sync::Arc<crate::mfa::MfaService>,
        second_factor::FactorSuccess,
    ),
    Box<Response>,
> {
    let mfa_service = state.mfa_cache.get().await.map_err(|error| {
        tracing::error!("MFA service unavailable during step-up: {error}");
        Box::new(factor_error(&FactorError::Unavailable))
    })?;
    let verified = second_factor::verify_second_factor(FactorInput {
        pool: &state.admin_db,
        service: &mfa_service,
        user_id,
        totp_code: req.totp_code.as_deref(),
        backup_code: req.backup_code.as_deref(),
    })
    .await
    .map_err(|error| Box::new(factor_error(&error)))?;
    Ok((mfa_service, verified))
}

pub async fn mfa_step_up(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
    headers: axum::http::HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(req): Json<MfaStepUpRequest>,
) -> Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let user_id = match extract_user_id_response(&claims) {
        Ok(uid) => uid,
        Err(error) => return error.into_response(),
    };
    let purpose = match parse_purpose(req.purpose.as_deref()) {
        Ok(purpose) => purpose,
        Err(response) => return *response,
    };
    let Some(sxp) = claims.sxp else {
        return crate::error::ApiError::Unauthorized.into_response();
    };
    let (mfa_service, verified) = match load_verified_factor(&state, user_id, &req).await {
        Ok(loaded) => loaded,
        Err(response) => return *response,
    };
    let now = Utc::now().timestamp();
    let Some(ttl) = step_up::effective_ttl(state.config.mfa.step_up_ttl_secs, sxp, now) else {
        if let Some(restore) = &verified.backup_restore
            && !second_factor::restore_backup_code(&state.admin_db, user_id, restore).await
        {
            crate::audit::set_failure_reason("mfa_backup_restore_failed");
            crate::audit::mark_security_event();
        }
        return crate::error::ApiError::Unauthorized.into_response();
    };
    let method = match verified.method {
        FactorMethod::Totp => StepUpMethod::Totp,
        FactorMethod::BackupCode => StepUpMethod::BackupCode,
    };
    let window = StepUpWindow {
        uid: user_id.to_string(),
        epoch: verified.auth_epoch,
        opened_at: now,
        until: now.saturating_add(i64::try_from(ttl).unwrap_or(i64::MAX)),
        method,
    };
    if let Err(error) = step_up::open_window(&state.redis, &claims.sid, &window, ttl).await {
        tracing::error!("failed to open MFA step-up window: {error}");
        if let Some(restore) = &verified.backup_restore
            && !second_factor::restore_backup_code(&state.admin_db, user_id, restore).await
        {
            crate::audit::set_failure_reason("mfa_backup_restore_failed");
            crate::audit::mark_security_event();
        }
        return factor_error(&FactorError::Unavailable);
    }
    let fresh = if let Some(purpose) = purpose {
        let Some(fresh_ttl) = step_up::fresh_ttl(sxp, now) else {
            return factor_error(&FactorError::Unavailable);
        };
        if let Err(error) = step_up::grant_fresh(
            &state.redis,
            &claims.sid,
            &user_id.to_string(),
            verified.auth_epoch,
            purpose,
            fresh_ttl,
            now,
        )
        .await
        {
            tracing::error!("failed to grant fresh MFA authorization: {error}");
            return factor_error(&FactorError::Unavailable);
        }
        Some((purpose, fresh_ttl))
    } else {
        None
    };
    let data = step_up_body(
        &window,
        ttl,
        state.config.mfa.step_up_ttl_secs,
        mfa_service.config().digits,
        fresh,
    );
    state.audit.log(crate::audit::user_audit(
        user_id,
        AuditAction::MfaStepUp,
        Some(serde_json::json!({
            "method": method.as_str(),
            "purpose": purpose.map(FreshPurpose::as_str),
            "ttl_secs": ttl,
            "expires_at": rfc3339(window.until),
            "sid": claims.sid,
        })),
        client_ip,
    ));
    crate::response::ApiResponse::ok(data).into_response()
}

pub async fn mfa_step_up_status(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
) -> Response {
    let user_id = match extract_user_id_response(&claims) {
        Ok(uid) => uid,
        Err(error) => return error.into_response(),
    };
    let row = match crate::repositories::mfa::get_mfa_status(&state.admin_db, user_id).await {
        Ok(Some(row)) => row,
        Ok(None) => {
            return crate::response::ApiResponse::ok(serde_json::json!({
                "mfa_enabled": false,
                "mfa_required": false,
                "active": false,
                "reason": "none",
            }))
            .into_response();
        }
        Err(error) => {
            tracing::error!("failed to read MFA status: {error}");
            return factor_error(&FactorError::Unavailable);
        }
    };
    let digits = state
        .mfa_cache
        .get()
        .await
        .ok()
        .map(|service| service.config().digits);
    let mut data = serde_json::json!({
        "mfa_enabled": row.mfa_enabled,
        "mfa_required": row.mfa_required,
        "totp_digits": digits.unwrap_or(state.config.mfa.digits),
        "active": false,
        "ttl_secs": state.config.mfa.step_up_ttl_secs,
        "server_time": Utc::now().to_rfc3339(),
    });
    if !row.mfa_enabled {
        if let Some(obj) = data.as_object_mut() {
            obj.insert("reason".to_string(), serde_json::json!("none"));
        }
        return crate::response::ApiResponse::ok(data).into_response();
    }
    let subject =
        match crate::repositories::admin::get_step_up_subject(&state.admin_db, user_id).await {
            Ok(Some(subject)) => subject,
            Ok(None) => return crate::error::ApiError::Unauthorized.into_response(),
            Err(error) => {
                tracing::error!("failed to read step-up subject: {error}");
                return factor_error(&FactorError::Unavailable);
            }
        };
    let window = match step_up::load_window(&state.redis, &claims.sid).await {
        Ok(window) => window,
        Err(error) => {
            tracing::error!("failed to read step-up window: {error}");
            return factor_error(&FactorError::Unavailable);
        }
    };
    let now = Utc::now().timestamp();
    let (active, reason) = match window.as_ref() {
        None => (false, Some("none")),
        Some(window) => {
            match step_up::classify_window(window, &user_id.to_string(), subject.auth_epoch, now) {
                Ok(()) => (true, None),
                Err(step_up::WindowMiss::Expired) => (false, Some("expired")),
                Err(step_up::WindowMiss::Invalidated) => {
                    let _ = step_up::delete_if_match(
                        &state.redis,
                        &format!("admin:mfa_step_up:{}", claims.sid),
                        &window.uid,
                        window.epoch,
                        "",
                    )
                    .await;
                    (false, Some("invalidated"))
                }
            }
        }
    };
    if let Some(obj) = data.as_object_mut() {
        obj.insert("active".to_string(), serde_json::json!(active));
        if let Some(window) = window.filter(|_| active) {
            obj.insert(
                "expires_at".to_string(),
                serde_json::json!(rfc3339(window.until)),
            );
            obj.insert(
                "remaining_secs".to_string(),
                serde_json::json!(step_up::remaining_secs(window.until, now)),
            );
        }
        if let Some(reason) = reason {
            obj.insert("reason".to_string(), serde_json::json!(reason));
        }
    }
    crate::response::ApiResponse::ok(data).into_response()
}

pub async fn mfa_step_up_end(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
    headers: axum::http::HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
) -> Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let user_id = match extract_user_id_response(&claims) {
        Ok(uid) => uid,
        Err(error) => return error.into_response(),
    };
    step_up::close_session(&state.redis, &claims.sid).await;
    state.audit.log(crate::audit::user_audit(
        user_id,
        AuditAction::MfaStepUpEnd,
        Some(serde_json::json!({ "reason": "manual", "sid": claims.sid })),
        client_ip,
    ));
    crate::response::ApiResponse::ok(serde_json::json!({ "active": false })).into_response()
}
