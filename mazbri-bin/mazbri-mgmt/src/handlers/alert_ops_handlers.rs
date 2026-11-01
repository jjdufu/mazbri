//! 告警完善：静默 / 抑制 / 值班 / 渠道 HTTP 处理器（写操作需平台类 MFA step-up）。
use super::AppState;
use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::middleware::auth::{AdminClaims, extract_user_id};
use crate::response::ApiResponse;
use crate::services::alert_ops::{
    self as svc, AlertOpsError, ChannelInput, InhibitInput, OncallInput, SilenceInput,
};
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;
use uuid::Uuid;

impl IntoResponse for AlertOpsError {
    fn into_response(self) -> Response {
        match self {
            Self::Validation(message) => ApiResponse::err(StatusCode::BAD_REQUEST, message),
            Self::NotFound(message) => ApiResponse::err(StatusCode::NOT_FOUND, message),
            Self::Conflict(message) => ApiResponse::err(StatusCode::CONFLICT, message),
            Self::Db(error) => {
                tracing::error!(%error, "告警完善数据库操作失败");
                ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "数据库操作失败")
            }
        }
    }
}

struct WriteContext {
    operator_id: Uuid,
    created_by: String,
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
        created_by: claims.sub.clone(),
        client_ip: extract_client_ip(headers, Some(addr)),
    })
}

fn audit(state: &AppState, ctx: &WriteContext, action: AuditAction, id: Uuid, details: serde_json::Value) {
    state.audit.log(create_audit_log(
        ctx.operator_id,
        action,
        AuditResourceType::Alert,
        id.to_string(),
        Some(details),
        ctx.client_ip.clone(),
    ));
}

// ---- silences ----

pub async fn list_silences(State(state): State<AppState>) -> Response {
    match svc::list_silences(&state.admin_db).await {
        Ok(rows) => ApiResponse::ok(rows).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn create_silence(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(input): Json<SilenceInput>,
) -> Response {
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
        Ok(c) => c,
        Err(r) => return *r,
    };
    match svc::create_silence(&state.admin_db, input, &ctx.created_by).await {
        Ok(row) => {
            audit(
                &state,
                &ctx,
                AuditAction::AlertSilenceCreate,
                row.id,
                serde_json::to_value(&row).unwrap_or_else(|_| serde_json::json!({})),
            );
            ApiResponse::ok(row).into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn delete_silence(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<Uuid>,
) -> Response {
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
        Ok(c) => c,
        Err(r) => return *r,
    };
    match svc::delete_silence(&state.admin_db, id).await {
        Ok(()) => {
            audit(&state, &ctx, AuditAction::AlertSilenceDelete, id, serde_json::json!({}));
            ApiResponse::ok(serde_json::json!({"id": id})).into_response()
        }
        Err(e) => e.into_response(),
    }
}

// ---- inhibit ----

pub async fn list_inhibits(State(state): State<AppState>) -> Response {
    match svc::list_inhibits(&state.admin_db).await {
        Ok(rows) => ApiResponse::ok(rows).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn create_inhibit(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(input): Json<InhibitInput>,
) -> Response {
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
        Ok(c) => c,
        Err(r) => return *r,
    };
    match svc::create_inhibit(&state.admin_db, input).await {
        Ok(row) => {
            audit(
                &state,
                &ctx,
                AuditAction::AlertInhibitCreate,
                row.id,
                serde_json::to_value(&row).unwrap_or_else(|_| serde_json::json!({})),
            );
            ApiResponse::ok(row).into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn update_inhibit(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<Uuid>,
    Json(input): Json<InhibitInput>,
) -> Response {
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
        Ok(c) => c,
        Err(r) => return *r,
    };
    match svc::update_inhibit(&state.admin_db, id, input).await {
        Ok(row) => {
            audit(
                &state,
                &ctx,
                AuditAction::AlertInhibitUpdate,
                row.id,
                serde_json::to_value(&row).unwrap_or_else(|_| serde_json::json!({})),
            );
            ApiResponse::ok(row).into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn delete_inhibit(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<Uuid>,
) -> Response {
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
        Ok(c) => c,
        Err(r) => return *r,
    };
    match svc::delete_inhibit(&state.admin_db, id).await {
        Ok(()) => {
            audit(&state, &ctx, AuditAction::AlertInhibitDelete, id, serde_json::json!({}));
            ApiResponse::ok(serde_json::json!({"id": id})).into_response()
        }
        Err(e) => e.into_response(),
    }
}

// ---- oncall ----

pub async fn list_oncalls(State(state): State<AppState>) -> Response {
    match svc::list_oncalls(&state.admin_db).await {
        Ok(rows) => ApiResponse::ok(rows).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn create_oncall(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(input): Json<OncallInput>,
) -> Response {
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
        Ok(c) => c,
        Err(r) => return *r,
    };
    match svc::create_oncall(&state.admin_db, input).await {
        Ok(row) => {
            audit(
                &state,
                &ctx,
                AuditAction::OncallCreate,
                row.id,
                serde_json::to_value(&row).unwrap_or_else(|_| serde_json::json!({})),
            );
            ApiResponse::ok(row).into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn update_oncall(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<Uuid>,
    Json(input): Json<OncallInput>,
) -> Response {
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
        Ok(c) => c,
        Err(r) => return *r,
    };
    match svc::update_oncall(&state.admin_db, id, input).await {
        Ok(row) => {
            audit(
                &state,
                &ctx,
                AuditAction::OncallUpdate,
                row.id,
                serde_json::to_value(&row).unwrap_or_else(|_| serde_json::json!({})),
            );
            ApiResponse::ok(row).into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn delete_oncall(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<Uuid>,
) -> Response {
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
        Ok(c) => c,
        Err(r) => return *r,
    };
    match svc::delete_oncall(&state.admin_db, id).await {
        Ok(()) => {
            audit(&state, &ctx, AuditAction::OncallDelete, id, serde_json::json!({}));
            ApiResponse::ok(serde_json::json!({"id": id})).into_response()
        }
        Err(e) => e.into_response(),
    }
}

// ---- channels ----

pub async fn list_channels(State(state): State<AppState>) -> Response {
    match svc::list_channels(&state.admin_db).await {
        Ok(rows) => ApiResponse::ok(rows).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn create_channel(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(input): Json<ChannelInput>,
) -> Response {
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
        Ok(c) => c,
        Err(r) => return *r,
    };
    // webhook URL SSRF 预检（公开 config.url）
    if input.channel_type == "webhook"
        && let Some(url) = input.config.get("url").and_then(|v| v.as_str())
        && let Err(message) =
            crate::services::ssrf::validate_public_http_url(state.connector_runtime.ssrf(), url).await
    {
        return ApiResponse::err(StatusCode::BAD_REQUEST, format!("webhook 地址不可用: {message}"))
            .into_response();
    }
    match svc::create_channel(&state.admin_db, input).await {
        Ok(row) => {
            audit(
                &state,
                &ctx,
                AuditAction::NotifyChannelCreate,
                row.id,
                serde_json::json!({"id": row.id, "name": row.name, "type": row.channel_type}),
            );
            ApiResponse::ok(row).into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn update_channel(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<Uuid>,
    Json(input): Json<ChannelInput>,
) -> Response {
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
        Ok(c) => c,
        Err(r) => return *r,
    };
    if input.channel_type == "webhook"
        && let Some(url) = input.config.get("url").and_then(|v| v.as_str())
        && let Err(message) =
            crate::services::ssrf::validate_public_http_url(state.connector_runtime.ssrf(), url).await
    {
        return ApiResponse::err(StatusCode::BAD_REQUEST, format!("webhook 地址不可用: {message}"))
            .into_response();
    }
    match svc::update_channel(&state.admin_db, id, input).await {
        Ok(row) => {
            audit(
                &state,
                &ctx,
                AuditAction::NotifyChannelUpdate,
                row.id,
                serde_json::json!({"id": row.id, "name": row.name, "type": row.channel_type}),
            );
            ApiResponse::ok(row).into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn delete_channel(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<Uuid>,
) -> Response {
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
        Ok(c) => c,
        Err(r) => return *r,
    };
    match svc::delete_channel(&state.admin_db, id).await {
        Ok(()) => {
            audit(&state, &ctx, AuditAction::NotifyChannelDelete, id, serde_json::json!({}));
            ApiResponse::ok(serde_json::json!({"id": id})).into_response()
        }
        Err(e) => e.into_response(),
    }
}
