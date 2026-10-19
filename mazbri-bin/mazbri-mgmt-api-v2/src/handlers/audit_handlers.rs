use super::AppState;
use crate::audit::AuditLogQuery;
use crate::middleware::auth::AdminClaims;
use crate::response::ApiResponse;
use axum::{
    Extension,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header::CONTENT_DISPOSITION},
    response::{IntoResponse, Response},
};
use tracing::{debug, error};
fn can_access_audit_logs(claims: &AdminClaims) -> bool {
    claims.role == "R_ADMIN" || claims.role == "R_OPERATOR"
}
pub async fn list_audit_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Query(query): Query<AuditLogQuery>,
) -> axum::response::Response {
    if !can_access_audit_logs(&claims) {
        return ApiResponse::err(StatusCode::FORBIDDEN, "只有 admin 可以访问审计日志");
    }
    debug!(
        "Listing audit logs (by {}) with query: {:?}",
        claims.sub, query
    );
    match state.audit.list(query).await {
        Ok(response) => {
            ApiResponse::ok(serde_json::to_value(response).unwrap_or_default()).into_response()
        }
        Err(e) => {
            error!("Failed to list audit logs: {}", e);
            ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch audit logs: {}", e),
            )
        }
    }
}
pub async fn get_audit_log(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<uuid::Uuid>,
) -> axum::response::Response {
    if !can_access_audit_logs(&claims) {
        return ApiResponse::err(StatusCode::FORBIDDEN, "只有 admin 可以访问审计日志");
    }
    debug!("Getting audit log {} (by {})", id, claims.sub);
    match state.audit.get_by_id(id).await {
        Ok(Some(response)) => {
            ApiResponse::ok(serde_json::to_value(response).unwrap_or_default()).into_response()
        }
        Ok(None) => ApiResponse::err(StatusCode::NOT_FOUND, "审计日志未找到"),
        Err(e) => {
            error!("Failed to get audit log {}: {}", id, e);
            ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch audit log: {}", e),
            )
        }
    }
}
pub async fn export_audit_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Query(query): Query<AuditLogQuery>,
) -> Response {
    if !can_access_audit_logs(&claims) {
        return ApiResponse::err(StatusCode::FORBIDDEN, "只有 admin 可以访问审计日志");
    }
    debug!(
        "Exporting audit logs (by {}) with query: {:?}",
        claims.sub, query
    );
    match state.audit.export_csv(query).await {
        Ok(csv) => {
            let filename = format!(
                "audit_logs_{}.csv",
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            );
            let content_disposition = format!("attachment; filename=\"{}\"", filename);
            Response::builder()
                .status(StatusCode::OK)
                .header(
                    CONTENT_DISPOSITION,
                    HeaderValue::from_str(&content_disposition)
                        .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
                )
                .header("content-type", "text/csv; charset=utf-8")
                .body(axum::body::Body::from(csv))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(e) => {
            error!("Failed to export audit logs: {}", e);
            ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to export audit logs: {}", e),
            )
        }
    }
}
