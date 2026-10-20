use crate::handlers::AppState;
use crate::handlers::types::ListParams;
use crate::response::ApiResponse;
use crate::services::product as service;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
};
pub async fn list_tenant_products(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    service::list_tenant_products(&state.product_service, tenant_id, &params).await
}
pub async fn list_tenant_transactions(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Query(query): Query<super::TransactionListQuery>,
) -> axum::response::Response {
    let tenant_id = match crate::services::tenant::resolve_tenant_id(&state.db, &code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    service::list_tenant_transactions(&state.db, &state.billing_db, tenant_id, &code, &query).await
}
