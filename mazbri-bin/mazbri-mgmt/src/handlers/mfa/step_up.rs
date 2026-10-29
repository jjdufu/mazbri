use super::super::AppState;
use crate::audit::{AuditAction, extract_client_ip};
use crate::middleware::auth::extract_user_id_response;
use crate::services::mfa::second_factor::{self, FactorError, FactorInput, FactorMethod};
use crate::services::mfa::step_up::{self, Category, CategoryStatus, StepUpMethod, StepUpWindow};
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Query, State},
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
    pub category: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct EndStepUpQuery {
    pub category: Option<String>,
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
        FactorError::Locked {
            retry_after_secs,
            triggered,
        } => {
            if *triggered {
                crate::audit::set_failure_reason("mfa_lock_triggered");
            } else {
                crate::audit::set_failure_reason("mfa_locked");
            }
            crate::audit::mark_security_event();
            let message = "MFA 已锁定，请稍后再试";
            let mut response = (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "code": "MFA_LOCKED",
                    "error": message,
                    "retry_after_secs": retry_after_secs,
                })),
            )
                .into_response();
            crate::audit::AuditErrorInfo::attach(&mut response, "MFA_LOCKED", message);
            response
        }
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

fn invalid_request() -> Response {
    error_response(
        StatusCode::BAD_REQUEST,
        "MFA_INVALID_REQUEST",
        mazbri_i18n::mfa::invalid_request(),
        &serde_json::json!({}),
    )
}

fn rfc3339(unix: i64) -> String {
    DateTime::<Utc>::from_timestamp(unix, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

fn parse_category(value: &str) -> Result<Category, Box<Response>> {
    Category::parse(value).ok_or_else(|| Box::new(invalid_request()))
}

fn step_up_body(
    window: &StepUpWindow,
    category: Category,
    ttl: u64,
    configured_ttl: u64,
    digits: u32,
) -> serde_json::Value {
    serde_json::json!({
        "active": true,
        "category": category.as_str(),
        "opened_at": rfc3339(window.opened_at),
        "expires_at": rfc3339(window.until),
        "remaining_secs": ttl,
        "ttl_secs": configured_ttl,
        "server_time": Utc::now().to_rfc3339(),
        "method": window.method.as_str(),
        "totp_digits": digits,
    })
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

pub(crate) async fn rollback_step_up_grants(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    restore: Option<&second_factor::BackupRestore>,
) {
    if let Some(restore) = restore
        && !second_factor::restore_backup_code(pool, user_id, restore).await
    {
        crate::audit::set_failure_reason("mfa_backup_restore_failed");
        crate::audit::mark_security_event();
    }
}

struct StepUpGate {
    user_id: uuid::Uuid,
    category: Category,
    sxp: i64,
    now: i64,
    checked: crate::handlers::mfa_guard::CheckedSubject,
}

async fn gate_step_up(
    state: &AppState,
    claims: &crate::middleware::auth::AdminClaims,
    req: &MfaStepUpRequest,
) -> Result<StepUpGate, Box<Response>> {
    let user_id =
        extract_user_id_response(claims).map_err(|error| Box::new(error.into_response()))?;
    let category = parse_category(&req.category)?;
    if let Err(error) =
        second_factor::exclusive_codes(req.totp_code.as_deref(), req.backup_code.as_deref())
    {
        return Err(Box::new(factor_error(&error)));
    }
    let now = Utc::now().timestamp();
    let Some(sxp) = claims.sxp else {
        return Err(Box::new(
            crate::error::ApiError::Unauthorized.into_response(),
        ));
    };
    if uuid::Uuid::parse_str(&claims.sid).is_err() || sxp <= now {
        return Err(Box::new(
            crate::error::ApiError::Unauthorized.into_response(),
        ));
    }
    let checked = crate::handlers::mfa_guard::load_checked_subject(
        &state.admin_db,
        &state.password_policy_cache,
        claims,
    )
    .await
    .map_err(|error| Box::new(error.into_response()))?;
    Ok(StepUpGate {
        user_id,
        category,
        sxp,
        now,
        checked,
    })
}

pub async fn mfa_step_up(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
    headers: axum::http::HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(req): Json<MfaStepUpRequest>,
) -> Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let gate = match gate_step_up(&state, &claims, &req).await {
        Ok(gate) => gate,
        Err(response) => return *response,
    };
    let StepUpGate {
        user_id,
        category,
        sxp,
        now,
        checked,
    } = gate;
    let (mfa_service, verified) = match load_verified_factor(&state, checked.user_id, &req).await {
        Ok(loaded) => loaded,
        Err(response) => return *response,
    };
    let ttl = step_up::effective_ttl(state.config.mfa.step_up_ttl_secs, sxp, now);
    if verified.auth_epoch != checked.epoch || ttl.is_none() {
        rollback_step_up_grants(
            &state.admin_db,
            checked.user_id,
            verified.backup_restore.as_ref(),
        )
        .await;
        return crate::error::ApiError::Unauthorized.into_response();
    }
    let Some(ttl) = ttl else {
        return crate::error::ApiError::Unauthorized.into_response();
    };
    let method = match verified.method {
        FactorMethod::Totp => StepUpMethod::Totp,
        FactorMethod::BackupCode => StepUpMethod::BackupCode,
    };
    let window = StepUpWindow {
        uid: checked.user_id.to_string(),
        epoch: checked.epoch,
        opened_at: now,
        until: now.saturating_add(i64::try_from(ttl).unwrap_or(i64::MAX)),
        method,
    };
    if let Err(error) =
        step_up::open_category_window(&state.redis, &claims.sid, category, &window, ttl).await
    {
        tracing::error!("failed to open MFA step-up window: {error}");
        rollback_step_up_grants(
            &state.admin_db,
            checked.user_id,
            verified.backup_restore.as_ref(),
        )
        .await;
        return factor_error(&FactorError::Unavailable);
    }
    let data = step_up_body(
        &window,
        category,
        ttl,
        state.config.mfa.step_up_ttl_secs,
        mfa_service.config().digits,
    );
    state.audit.log(crate::audit::user_audit(
        user_id,
        AuditAction::MfaStepUp,
        Some(serde_json::json!({
            "method": method.as_str(),
            "category": category.as_str(),
            "ttl_secs": ttl,
            "expires_at": rfc3339(window.until),
            "sid": claims.sid,
        })),
        client_ip,
    ));
    crate::response::ApiResponse::ok(data).into_response()
}

fn status_response(
    mfa_enabled: bool,
    mfa_required: bool,
    digits: u32,
    ttl_secs: u64,
    categories: &[CategoryStatus],
) -> Response {
    let categories = categories
        .iter()
        .map(|status| {
            let expires_at = status.expires_at.map(rfc3339);
            step_up::category_status_body(status, expires_at.as_deref())
        })
        .collect::<Vec<_>>();
    crate::response::ApiResponse::ok(serde_json::json!({
        "mfa_enabled": mfa_enabled,
        "mfa_required": mfa_required,
        "totp_digits": digits,
        "ttl_secs": ttl_secs,
        "server_time": Utc::now().to_rfc3339(),
        "categories": categories,
    }))
    .into_response()
}

async fn configured_digits(state: &AppState) -> u32 {
    state
        .mfa_cache
        .get()
        .await
        .ok()
        .map_or(state.config.mfa.digits, |service| service.config().digits)
}

async fn load_live_categories(
    state: &AppState,
    sid: &str,
    user_id: uuid::Uuid,
    epoch: i64,
) -> Result<Vec<CategoryStatus>, Box<Response>> {
    let now = Utc::now().timestamp();
    let uid = user_id.to_string();
    let mut items = Vec::with_capacity(Category::ALL.len());
    for category in Category::ALL {
        let window = step_up::load_category_window(&state.redis, sid, category)
            .await
            .map_err(|error| {
                tracing::error!("failed to read step-up window: {error}");
                Box::new(factor_error(&FactorError::Unavailable))
            })?;
        let status = step_up::assess_category(category, window.as_ref(), &uid, epoch, now);
        if status.reason == Some("invalidated")
            && let Some(window) = window.as_ref()
            && let Err(error) = step_up::delete_category_if_match(
                &state.redis,
                sid,
                category,
                &window.uid,
                window.epoch,
            )
            .await
        {
            tracing::warn!("failed to drop invalidated step-up window: {error}");
        }
        items.push(status);
    }
    Ok(items)
}

pub async fn mfa_step_up_status(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
) -> Response {
    let user_id = match extract_user_id_response(&claims) {
        Ok(uid) => uid,
        Err(error) => return error.into_response(),
    };
    let digits = configured_digits(&state).await;
    let ttl_secs = state.config.mfa.step_up_ttl_secs;
    let row = match crate::repositories::mfa::get_mfa_status(&state.admin_db, user_id).await {
        Ok(Some(row)) => row,
        Ok(None) => {
            return status_response(
                false,
                false,
                digits,
                ttl_secs,
                &step_up::inactive_categories("invalidated"),
            );
        }
        Err(error) => {
            tracing::error!("failed to read MFA status: {error}");
            return factor_error(&FactorError::Unavailable);
        }
    };
    let checked = match crate::handlers::mfa_guard::load_checked_subject(
        &state.admin_db,
        &state.password_policy_cache,
        &claims,
    )
    .await
    {
        Ok(checked) => checked,
        Err(crate::handlers::mfa_guard::SubjectLoadError::Unavailable) => {
            return factor_error(&FactorError::Unavailable);
        }
        Err(_) => {
            return status_response(
                row.mfa_enabled,
                row.mfa_required,
                digits,
                ttl_secs,
                &step_up::inactive_categories("invalidated"),
            );
        }
    };
    if !row.mfa_enabled {
        return status_response(
            false,
            row.mfa_required,
            digits,
            ttl_secs,
            &step_up::inactive_categories("none"),
        );
    }
    match load_live_categories(&state, &claims.sid, checked.user_id, checked.epoch).await {
        Ok(categories) => status_response(true, row.mfa_required, digits, ttl_secs, &categories),
        Err(response) => *response,
    }
}

pub async fn mfa_step_up_end(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
    Query(query): Query<EndStepUpQuery>,
    headers: axum::http::HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
) -> Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let user_id = match extract_user_id_response(&claims) {
        Ok(uid) => uid,
        Err(error) => return error.into_response(),
    };
    if uuid::Uuid::parse_str(&claims.sid).is_err() {
        return crate::error::ApiError::Unauthorized.into_response();
    }
    let category = match query.category.as_deref() {
        None => None,
        Some(value) => match parse_category(value) {
            Ok(category) => Some(category),
            Err(response) => return *response,
        },
    };
    let body = if let Some(category) = category {
        if let Err(error) = step_up::close_category(&state.redis, &claims.sid, category).await {
            tracing::error!("failed to close MFA category window: {error}");
            return factor_error(&FactorError::Unavailable);
        }
        state.audit.log(crate::audit::user_audit(
            user_id,
            AuditAction::MfaStepUpEnd,
            Some(serde_json::json!({
                "reason": "manual",
                "sid": claims.sid,
                "category": category.as_str(),
            })),
            client_ip,
        ));
        serde_json::json!({ "scope": "category", "category": category.as_str() })
    } else {
        if let Err(error) = step_up::close_all_categories(&state.redis, &claims.sid).await {
            tracing::error!("failed to close MFA step-up session: {error}");
            return factor_error(&FactorError::Unavailable);
        }
        state.audit.log(crate::audit::user_audit(
            user_id,
            AuditAction::MfaStepUpEnd,
            Some(serde_json::json!({
                "reason": "manual",
                "sid": claims.sid,
                "scope": "all",
            })),
            client_ip,
        ));
        serde_json::json!({ "scope": "all" })
    };
    crate::response::ApiResponse::ok(body).into_response()
}
