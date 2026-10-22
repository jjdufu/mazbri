use super::AppState;
use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::middleware::auth::{AdminClaims, can_manage_tenants, is_admin};
use crate::response::ApiResponse;
use crate::services::ratelimit_commands as svc;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::{Extension, Json};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListCommandsQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}

fn map_error(err: svc::ControlCommandError) -> axum::response::Response {
    match err {
        svc::ControlCommandError::Validation(e) => ApiResponse::err(StatusCode::BAD_REQUEST, e),
        svc::ControlCommandError::NotFound(e) => ApiResponse::err(StatusCode::NOT_FOUND, e),
        svc::ControlCommandError::Internal(e) => {
            ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, e)
        }
    }
}

pub async fn create_control_command(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
    Json(payload): Json<svc::CreateControlCommandRequest>,
) -> axum::response::Response {
    // Temporary controls are higher privilege than routine tenant config edits.
    if !is_admin(&claims) {
        return ApiResponse::err(
            StatusCode::FORBIDDEN,
            "Only administrators can issue rate-limit control commands",
        );
    }
    let Ok(operator_id) = Uuid::parse_str(&claims.user_id) else {
        return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
    };
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    match svc::create_control_command(
        &state.db,
        &state.ratelimit_db,
        tenant_id,
        &claims.user_id,
        &payload,
    )
    .await
    {
        Ok(data) => {
            let client_ip = extract_client_ip(&headers, Some(addr));
            state.audit.log(create_audit_log(
                operator_id,
                AuditAction::TenantRateLimitCommandCreate,
                AuditResourceType::Tenant,
                tenant_id.to_string(),
                Some(serde_json::json!({
                    "tenant_code": code,
                    "tenant_id": tenant_id,
                    "command_type": payload.command_type,
                    "command_id": data.id,
                    "deny_for_secs": payload.deny_for_secs,
                    "deny_until_unix": payload.deny_until_unix,
                    "quota_multiplier": payload.quota_multiplier,
                    "expires_in_secs": payload.expires_in_secs,
                    "reason": payload.reason,
                })),
                client_ip,
            ));
            ApiResponse::ok(data).into_response()
        }
        Err(e) => map_error(e),
    }
}

pub async fn list_control_commands(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(code): Path<String>,
    Query(query): Query<ListCommandsQuery>,
) -> axum::response::Response {
    if !can_manage_tenants(&claims) {
        return ApiResponse::err(StatusCode::FORBIDDEN, "Insufficient permissions");
    }
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    match svc::list_control_commands(&state.db, &state.ratelimit_db, tenant_id, query.limit).await {
        Ok(data) => ApiResponse::ok(serde_json::json!({
            "commands": data,
            "total": data.len(),
        }))
        .into_response(),
        Err(e) => map_error(e),
    }
}

pub async fn cancel_control_command(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path((code, command_id)): Path<(String, String)>,
) -> axum::response::Response {
    if !is_admin(&claims) {
        return ApiResponse::err(
            StatusCode::FORBIDDEN,
            "Only administrators can cancel rate-limit control commands",
        );
    }
    let Ok(operator_id) = Uuid::parse_str(&claims.user_id) else {
        return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
    };
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    let Ok(command_uuid) = Uuid::parse_str(&command_id) else {
        return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid command id format");
    };
    match svc::cancel_control_command(&state.db, &state.ratelimit_db, tenant_id, command_uuid).await
    {
        Ok(data) => {
            let client_ip = extract_client_ip(&headers, Some(addr));
            state.audit.log(create_audit_log(
                operator_id,
                AuditAction::TenantRateLimitCommandCancel,
                AuditResourceType::Tenant,
                tenant_id.to_string(),
                Some(serde_json::json!({
                    "tenant_code": code,
                    "tenant_id": tenant_id,
                    "command_id": command_id,
                    "command_type": data.command_type,
                })),
                client_ip,
            ));
            ApiResponse::ok(data).into_response()
        }
        Err(e) => map_error(e),
    }
}
