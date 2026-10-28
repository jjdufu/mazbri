use crate::audit::extract_client_ip;
use crate::handlers::AppState;
use crate::handlers::mfa_guard::require_step_up;
use crate::middleware::auth::{AdminClaims, is_business_admin};
use crate::response::ApiResponse;
use crate::services::product as service;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
};
fn can_manage_account_finance(claims: &AdminClaims) -> bool {
    is_business_admin(claims)
}
pub async fn get_tenant_account(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> axum::response::Response {
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    service::get_tenant_account(&state.db, &state.billing_db, tenant_id, &code).await
}
pub async fn recharge_tenant_account(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Json(req): Json<super::RechargeRequest>,
) -> axum::response::Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    if !can_manage_account_finance(&claims) {
        return ApiResponse::err(StatusCode::FORBIDDEN, "只有超级管理员或管理员可以充值账户");
    }
    if let Err(response) = require_step_up(&state, &claims).await {
        return *response;
    }
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    let operator_id = match uuid::Uuid::parse_str(&claims.user_id) {
        Ok(uid) if !uid.is_nil() => uid,
        _ => {
            return ApiResponse::err(StatusCode::UNAUTHORIZED, "令牌中的用户标识无效");
        }
    };
    service::recharge_tenant_account(service::RechargeTenantAccountArgs {
        db: &state.db,
        billing_db: &state.billing_db,
        audit: &state.audit,
        tenant_id,
        tenant_id_str: &code,
        req: &req,
        operator_id,
        operator_name: &claims.sub,
        client_ip,
    })
    .await
}
pub async fn refund_tenant_account(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Json(req): Json<super::RefundRequest>,
) -> axum::response::Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    if !can_manage_account_finance(&claims) {
        return ApiResponse::err(StatusCode::FORBIDDEN, "只有超级管理员或管理员可以退款账户");
    }
    if let Err(response) = require_step_up(&state, &claims).await {
        return *response;
    }
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    let operator_id = match uuid::Uuid::parse_str(&claims.user_id) {
        Ok(uid) if !uid.is_nil() => uid,
        _ => {
            return ApiResponse::err(StatusCode::UNAUTHORIZED, "令牌中的用户标识无效");
        }
    };
    service::refund_tenant_account(service::RefundTenantAccountArgs {
        db: &state.db,
        billing_db: &state.billing_db,
        audit: &state.audit,
        tenant_id,
        tenant_id_str: &code,
        req: &req,
        operator_id,
        operator_name: &claims.sub,
        client_ip,
    })
    .await
}
