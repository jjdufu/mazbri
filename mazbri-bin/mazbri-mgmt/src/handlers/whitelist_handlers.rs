use super::AppState;
use super::types::AddToWhitelistRequest;
use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::middleware::auth::AdminClaims;
use crate::response::ApiResponse;
use crate::services::whitelist as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, State},
    http::HeaderMap,
    http::StatusCode,
    response::IntoResponse,
};
use tracing::info;
pub async fn get_whitelist(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> axum::response::Response {
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    match crate::repositories::whitelist::fetch_entries(&state.db, tenant_id).await {
        Ok(entries) => ApiResponse::ok(serde_json::json!({ "entries" : entries })).into_response(),
        Err(e) => ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get whitelist: {e}"),
        ),
    }
}
pub async fn add_to_whitelist(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
    Json(payload): Json<AddToWhitelistRequest>,
) -> axum::response::Response {
    info!("Adding IP whitelist for tenant: {}", code);
    let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) else {
            return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
        };
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    if !crate::repositories::whitelist::tenant_exists(&state.db, tenant_id).await {
        return ApiResponse::err(StatusCode::NOT_FOUND, format!("Tenant not found: {code}"));
    }
    let result = svc::add_entries(
        &state.db,
        tenant_id,
        payload.ip_addresses.as_slice(),
        &payload.description,
    )
    .await;
    let client_ip = extract_client_ip(&headers, Some(addr));
    state.audit.log(create_audit_log(
        operator_id,
        AuditAction::WhitelistAdd,
        AuditResourceType::Tenant,
        tenant_id.to_string(),
        Some(serde_json::json!(
            { "tenant_code" : code, "ip_addresses" : payload.ip_addresses.as_slice(),
            "description" : &payload.description, "total_added" : result.total_added,
            "total_errors" : result.total_errors }
        )),
        client_ip,
    ));
    ApiResponse::ok(serde_json::json!(
        { "added" : result.added, "errors" : result.errors, "total_added" :
        result.total_added, "total_errors" : result.total_errors, }
    ))
    .into_response()
}
pub async fn remove_from_whitelist(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path((code, ip)): Path<(String, String)>,
) -> axum::response::Response {
    info!("Removing IP {} from whitelist for tenant: {}", ip, code);
    let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) else {
            return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
        };
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    match svc::remove_entry(&state.db, tenant_id, &ip).await {
        Ok(ip_str) => {
            let client_ip = extract_client_ip(&headers, Some(addr));
            state.audit.log(create_audit_log(
                operator_id,
                AuditAction::WhitelistRemove,
                AuditResourceType::Tenant,
                tenant_id.to_string(),
                Some(serde_json::json!(
                    { "tenant_code" : code, "ip_address" : ip_str }
                )),
                client_ip,
            ));
            ApiResponse::ok(serde_json::json!(
                { "message" : "IP removed from whitelist", "tenant_code" : code,
                "ip_address" : ip_str, }
            ))
            .into_response()
        }
        Err(svc::RemoveError::NotFound(msg)) => ApiResponse::err(StatusCode::NOT_FOUND, msg),
        Err(svc::RemoveError::InvalidFormat(msg)) => ApiResponse::err(StatusCode::BAD_REQUEST, msg),
        Err(svc::RemoveError::Database(msg)) => {
            ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    }
}
