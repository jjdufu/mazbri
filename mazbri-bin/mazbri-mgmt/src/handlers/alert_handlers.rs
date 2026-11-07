//! 告警中心 HTTP 处理器：规则 CRUD（平台配置类 step-up）、事件列表、概览统计。
use super::AppState;
use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::middleware::auth::{AdminClaims, extract_user_id, is_admin};
use crate::response::ApiResponse;
use crate::services::alerts::{
    self as svc, AlertError, AlertRuleInput, EnabledInput, EventListQuery,
};
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;
use uuid::Uuid;

impl IntoResponse for AlertError {
    fn into_response(self) -> Response {
        match self {
            Self::Validation(message) => ApiResponse::err(StatusCode::BAD_REQUEST, message),
            Self::NotFound(message) => ApiResponse::err(StatusCode::NOT_FOUND, message),
            Self::Conflict(message) => ApiResponse::err(StatusCode::CONFLICT, message),
            Self::Db(error) => {
                tracing::error!(%error, "告警中心数据库操作失败");
                ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "数据库操作失败")
            }
        }
    }
}

struct WriteContext {
    operator_id: Uuid,
    client_ip: Option<String>,
}

fn write_context(
    claims: &AdminClaims,
    headers: &HeaderMap,
    addr: SocketAddr,
) -> Result<WriteContext, Box<Response>> {
    let operator_id = extract_user_id(claims).map_err(|_| {
        Box::new(ApiResponse::err(
            StatusCode::UNAUTHORIZED,
            "令牌中的用户标识无效",
        ))
    })?;
    Ok(WriteContext {
        operator_id,
        client_ip: extract_client_ip(headers, Some(addr)),
    })
}

fn audit(
    state: &AppState,
    ctx: &WriteContext,
    action: AuditAction,
    id: Uuid,
    details: serde_json::Value,
) {
    state.audit.log(create_audit_log(
        ctx.operator_id,
        action,
        AuditResourceType::Alert,
        id.to_string(),
        Some(details),
        ctx.client_ip.clone(),
    ));
}

/// 结构校验 + webhook SSRF 预检。
async fn checked_input(
    state: &AppState,
    input: AlertRuleInput,
) -> Result<AlertRuleInput, Box<Response>> {
    let input = svc::validate_rule(input).map_err(|e| Box::new(e.into_response()))?;
    for url in &input.notify.webhooks {
        if let Err(message) =
            crate::services::ssrf::validate_public_http_url(state.connector_runtime.ssrf(), url)
                .await
        {
            return Err(Box::new(ApiResponse::err(
                StatusCode::BAD_REQUEST,
                format!("webhook 地址不可用: {message}"),
            )));
        }
    }
    Ok(input)
}

fn rule_details(rule: &svc::AlertRule) -> serde_json::Value {
    serde_json::to_value(rule).unwrap_or_else(|_| serde_json::json!({}))
}

pub async fn list_alert_rules(State(state): State<AppState>) -> Response {
    match svc::list_rules(&state.admin_db).await {
        Ok(rules) => ApiResponse::ok(rules).into_response(),
        Err(error) => error.into_response(),
    }
}

pub async fn create_alert_rule(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<AlertRuleInput>,
) -> Response {
    let input = match checked_input(&state, payload).await {
        Ok(input) => input,
        Err(response) => return *response,
    };
    if let Err(response) = crate::handlers::mfa_guard::require_category(
        &state,
        &claims,
        crate::services::mfa::Category::Platform,
    )
    .await
    {
        return *response;
    }
    let ctx = match write_context(&claims, &headers, addr) {
        Ok(ctx) => ctx,
        Err(response) => return *response,
    };
    match svc::create_rule(&state.admin_db, &input).await {
        Ok(rule) => {
            audit(
                &state,
                &ctx,
                AuditAction::AlertRuleCreate,
                rule.id,
                rule_details(&rule),
            );
            crate::response::created(rule)
        }
        Err(error) => error.into_response(),
    }
}

pub async fn update_alert_rule(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(payload): Json<AlertRuleInput>,
) -> Response {
    let input = match checked_input(&state, payload).await {
        Ok(input) => input,
        Err(response) => return *response,
    };
    let before = match svc::get_rule(&state.admin_db, id).await {
        Ok(rule) => rule,
        Err(error) => return error.into_response(),
    };
    if let Err(response) = crate::handlers::mfa_guard::require_category(
        &state,
        &claims,
        crate::services::mfa::Category::Platform,
    )
    .await
    {
        return *response;
    }
    let ctx = match write_context(&claims, &headers, addr) {
        Ok(ctx) => ctx,
        Err(response) => return *response,
    };
    match svc::update_rule(&state.admin_db, id, &input).await {
        Ok(rule) => {
            audit(
                &state,
                &ctx,
                AuditAction::AlertRuleUpdate,
                id,
                serde_json::json!({ "before": rule_details(&before), "after": rule_details(&rule) }),
            );
            ApiResponse::ok(rule).into_response()
        }
        Err(error) => error.into_response(),
    }
}

pub async fn set_alert_rule_enabled(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(payload): Json<EnabledInput>,
) -> Response {
    if let Err(error) = svc::get_rule(&state.admin_db, id).await {
        return error.into_response();
    }
    if let Err(response) = crate::handlers::mfa_guard::require_category(
        &state,
        &claims,
        crate::services::mfa::Category::Platform,
    )
    .await
    {
        return *response;
    }
    let ctx = match write_context(&claims, &headers, addr) {
        Ok(ctx) => ctx,
        Err(response) => return *response,
    };
    match svc::set_rule_enabled(&state.admin_db, id, payload.enabled).await {
        Ok(rule) => {
            audit(
                &state,
                &ctx,
                AuditAction::AlertRuleUpdate,
                id,
                serde_json::json!({ "name": rule.name, "enabled": rule.enabled }),
            );
            ApiResponse::ok(rule).into_response()
        }
        Err(error) => error.into_response(),
    }
}

pub async fn delete_alert_rule(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    if let Err(error) = svc::get_rule(&state.admin_db, id).await {
        return error.into_response();
    }
    if let Err(response) = crate::handlers::mfa_guard::require_category(
        &state,
        &claims,
        crate::services::mfa::Category::Platform,
    )
    .await
    {
        return *response;
    }
    let ctx = match write_context(&claims, &headers, addr) {
        Ok(ctx) => ctx,
        Err(response) => return *response,
    };
    match svc::delete_rule(&state.admin_db, id).await {
        Ok(rule) => {
            audit(
                &state,
                &ctx,
                AuditAction::AlertRuleDelete,
                id,
                rule_details(&rule),
            );
            ApiResponse::ok(serde_json::json!({ "id": id })).into_response()
        }
        Err(error) => error.into_response(),
    }
}

pub async fn list_alert_events(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Query(query): Query<EventListQuery>,
) -> Response {
    if query.include_seed && !is_admin(&claims) {
        return ApiResponse::err(
            StatusCode::FORBIDDEN,
            "仅超级管理员可查看种子告警",
        )
        .into_response();
    }
    match svc::list_events(&state.admin_db, &query).await {
        Ok(page) => ApiResponse::ok(page).into_response(),
        Err(error) => error.into_response(),
    }
}

pub async fn alert_summary(State(state): State<AppState>) -> Response {
    match svc::summary(&state.admin_db).await {
        Ok(summary) => ApiResponse::ok(summary).into_response(),
        Err(error) => error.into_response(),
    }
}
