use crate::audit::extract_client_ip;
use crate::handlers::AppState;
use crate::handlers::types::ListParams;
use crate::middleware::auth::{AdminClaims, can_manage_tenants};
use crate::response::ApiResponse;
use crate::services::product as service;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
    http::StatusCode,
};
use mazbri_product::model::{CreateProductRequest, UpdateProductRequest};
pub async fn list_products(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    service::list_products(&state.product_service, &params).await
}
pub async fn create_product(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(req): Json<CreateProductRequest>,
) -> axum::response::Response {
    if !can_manage_tenants(&claims) {
        return ApiResponse::err(StatusCode::FORBIDDEN, "只有 admin 或 operator 可以创建产品");
    }
    let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) else {
            return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
        };
    let client_ip = extract_client_ip(&headers, Some(addr));
    service::create_product(
        &state.product_service,
        req,
        &claims.sub,
        &state.audit,
        operator_id,
        client_ip,
    )
    .await
}
pub async fn create_product_reservation(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !can_manage_tenants(&claims) {
        return ApiResponse::err(
            StatusCode::FORBIDDEN,
            "只有 admin 或 operator 可以预留产品编码",
        );
    }
    let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) else {
            return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
        };
    let client_ip = extract_client_ip(&headers, Some(addr));
    service::create_product_reservation(
        &state.product_service,
        &claims.sub,
        &state.audit,
        operator_id,
        client_ip,
    )
    .await
}
pub async fn release_product_reservation(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(reservation_id): Path<uuid::Uuid>,
) -> axum::response::Response {
    if !can_manage_tenants(&claims) {
        return ApiResponse::err(
            StatusCode::FORBIDDEN,
            "只有 admin 或 operator 可以释放产品编码",
        );
    }
    let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) else {
            return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
        };
    let client_ip = extract_client_ip(&headers, Some(addr));
    service::release_product_reservation(
        &state.product_service,
        reservation_id,
        &claims.sub,
        &state.audit,
        operator_id,
        client_ip,
    )
    .await
}
pub async fn get_product(
    State(state): State<AppState>,
    Path(product_id): Path<String>,
) -> axum::response::Response {
    service::get_product(&state.product_service, &product_id).await
}
pub async fn update_product(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(product_id): Path<String>,
    Json(req): Json<UpdateProductRequest>,
) -> axum::response::Response {
    if !can_manage_tenants(&claims) {
        return ApiResponse::err(StatusCode::FORBIDDEN, "只有 admin 或 operator 可以更新产品");
    }
    let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) else {
            return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
        };
    let client_ip = extract_client_ip(&headers, Some(addr));
    service::update_product(
        &state.product_service,
        &product_id,
        req,
        &claims.sub,
        &state.audit,
        operator_id,
        client_ip,
    )
    .await
}
pub async fn delete_product(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(product_id): Path<String>,
) -> axum::response::Response {
    if !can_manage_tenants(&claims) {
        return ApiResponse::err(StatusCode::FORBIDDEN, "只有 admin 或 operator 可以删除产品");
    }
    let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) else {
            return ApiResponse::err(StatusCode::BAD_REQUEST, "Invalid operator ID format");
        };
    let client_ip = extract_client_ip(&headers, Some(addr));
    service::delete_product(
        &state.product_service,
        &product_id,
        &claims.sub,
        &state.audit,
        operator_id,
        client_ip,
    )
    .await
}
