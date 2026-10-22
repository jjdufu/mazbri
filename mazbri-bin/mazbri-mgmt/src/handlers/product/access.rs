use crate::audit::extract_client_ip;
use crate::handlers::AppState;
use crate::handlers::types::{GrantAccessRequest, ListParams, RevokeAccessRequest};
use crate::middleware::auth::{AdminClaims, can_manage_tenants};
use crate::response::ApiResponse;
use crate::services::product as service;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
    http::StatusCode,
};
use mazbri_product::service::TenantProductConfig;
use serde::Deserialize;
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateTenantProductConfigRequest {
    pub config: TenantProductConfig,
    #[serde(default)]
    pub expected_updated_at: Option<chrono::DateTime<chrono::Utc>>,
}
pub async fn grant_tenant_access(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(product_id): Path<String>,
    Json(req): Json<GrantAccessRequest>,
) -> axum::response::Response {
    if !can_manage_tenants(&claims) {
        return ApiResponse::err(
            StatusCode::FORBIDDEN,
            "只有 admin 或 operator 可以授予访问权限",
        );
    }
    let tenant_id =
        match crate::services::tenant::resolve_tenant_id(&state.db, &req.tenant_code).await {
            Ok(uuid) => uuid,
            Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
        };
    let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) else {
        return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
    };
    let client_ip = extract_client_ip(&headers, Some(addr));
    let product_req = mazbri_product::service::GrantAccessRequest {
        tenant_id,
        config: req.config,
    };
    service::grant_tenant_access(service::GrantTenantAccessArgs {
        product_service: &state.product_service,
        product_id: &product_id,
        req: product_req,
        audit: &state.audit,
        operator_id,
        operator_name: &claims.sub,
        tenant_code: &req.tenant_code,
        client_ip,
    })
    .await
}
pub async fn revoke_tenant_access(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(product_id): Path<String>,
    Json(req): Json<RevokeAccessRequest>,
) -> axum::response::Response {
    if !can_manage_tenants(&claims) {
        return ApiResponse::err(
            StatusCode::FORBIDDEN,
            "只有 admin 或 operator 可以撤销访问权限",
        );
    }
    let tenant_id =
        match crate::services::tenant::resolve_tenant_id(&state.db, &req.tenant_code).await {
            Ok(uuid) => uuid,
            Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
        };
    let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) else {
        return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
    };
    let client_ip = extract_client_ip(&headers, Some(addr));
    service::revoke_tenant_access(service::RevokeTenantAccessArgs {
        product_service: &state.product_service,
        product_id: &product_id,
        tenant_id,
        audit: &state.audit,
        operator_id,
        operator_name: &claims.sub,
        tenant_code: &req.tenant_code,
        client_ip,
    })
    .await
}
pub async fn update_tenant_product_config(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path((code, product_id)): Path<(String, String)>,
    Json(req): Json<UpdateTenantProductConfigRequest>,
) -> axum::response::Response {
    if !can_manage_tenants(&claims) {
        return ApiResponse::err(
            StatusCode::FORBIDDEN,
            "只有 admin 或 operator 可以调整通道授权配置",
        );
    }
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) else {
        return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
    };
    let client_ip = extract_client_ip(&headers, Some(addr));
    service::update_tenant_product_config(service::UpdateTenantProductConfigArgs {
        product_service: &state.product_service,
        audit: &state.audit,
        tenant_id,
        product_id: &product_id,
        req: req.config,
        expected_updated_at: req.expected_updated_at,
        operator_id,
        operator_name: &claims.sub,
        client_ip,
    })
    .await
}

pub async fn get_tenant_product_config(
    State(state): State<AppState>,
    Path((code, product_id)): Path<(String, String)>,
) -> axum::response::Response {
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    service::get_tenant_product_config(&state.product_service, tenant_id, &product_id).await
}
pub async fn list_product_tenants(
    State(state): State<AppState>,
    Path(product_id): Path<String>,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    service::list_product_tenants(&state.db, &state.product_service, &product_id, &params).await
}
pub async fn list_authorizations(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    service::list_authorizations(&state.product_service, &params).await
}

#[cfg(test)]
mod deny_unknown_tests {
    use super::UpdateTenantProductConfigRequest;
    use serde_json::json;

    #[test]
    fn nested_config_request_parses() {
        let req: UpdateTenantProductConfigRequest = serde_json::from_value(json!({
            "config": { "unit_price": "0.002", "currency": "CNY", "cb_pro_fallback": true },
            "expected_updated_at": null
        }))
        .expect("nested request parses");
        assert!(req.config.cb_pro_fallback);
    }

    #[test]
    fn flat_config_request_is_rejected() {
        let err = serde_json::from_value::<UpdateTenantProductConfigRequest>(json!({
            "unit_price": "0.002", "currency": "CNY", "cb_pro_fallback": true
        }))
        .expect_err("legacy flat body must be rejected");
        assert!(err.to_string().contains("unknown field"));
    }
}
