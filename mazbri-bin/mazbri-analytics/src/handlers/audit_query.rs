use super::{
    AppState, AuditLogItem, AuditQueryRequest, AuditQueryResponse, HandlerError, parse_time_string,
};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_analytics::greptimedb::AuditQueryParams;
use mazbri_core::num::SaturatingFrom;
use mazbri_core::validation::limits;
use tracing::{error, info};
const MAX_PAGE_SIZE: u32 = limits::MAX_AUDIT_PAGE_SIZE;
const MAX_TOTAL_RESULTS: u64 = limits::MAX_TOTAL_RESULTS;
impl AuditQueryRequest {
    pub fn max_page_for(page_size: u32) -> u32 {
        let page_size = u64::from(page_size.clamp(1, MAX_PAGE_SIZE));
        u32::saturating_from(MAX_TOTAL_RESULTS.div_ceil(page_size).max(1))
    }

    pub fn validate(&self) -> Result<(), String> {
        let max_page = Self::max_page_for(self.page_size);
        if self.page > max_page {
            return Err(format!(
                "页码超过最大深度限制 (page: {}, max: {})",
                self.page, max_page
            ));
        }
        Ok(())
    }
    pub fn capped_offset(&self) -> u32 {
        let offset = u64::from(self.page.saturating_sub(1)) * u64::from(self.capped_page_size());
        u32::saturating_from(offset.min(MAX_TOTAL_RESULTS.saturating_sub(1)))
    }
    pub fn capped_page_size(&self) -> u32 {
        self.page_size.clamp(1, MAX_PAGE_SIZE)
    }
}
pub async fn query_audit_logs(
    State(state): State<AppState>,
    Query(req): Query<AuditQueryRequest>,
) -> Response {
    info!(
        page = req.page, page_size = req.page_size, admin_id = ? req.admin_id, action = ?
        req.action, "查询审计日志"
    );
    if let Some(admin_id) = req.admin_id
        && admin_id <= 0
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(HandlerError {
                error: "invalid_admin_id".to_string(),
                message: "admin_id must be a positive integer".to_string(),
            }),
        )
            .into_response();
    }
    if let Err(e) = req.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(HandlerError {
                error: "invalid_pagination".to_string(),
                message: e,
            }),
        )
            .into_response();
    }
    let client = &state.clickhouse_client;
    let page_size = req.capped_page_size();
    let offset = req.capped_offset();
    let start_time = req.start_time.as_ref().and_then(|s| parse_time_string(s));
    let end_time = req.end_time.as_ref().and_then(|s| parse_time_string(s));
    let params = AuditQueryParams {
        start_time,
        end_time,
        admin_id: req.admin_id,
        action: req.action.clone(),
        resource_type: req.resource_type.clone(),
        limit: page_size as usize,
        offset: offset as usize,
    };
    match client.query_audit_logs(&params).await {
        Ok(result) => {
            let max_page = AuditQueryRequest::max_page_for(page_size);
            let total_pages = if page_size > 0 {
                u32::saturating_from(result.total.div_ceil(u64::from(page_size))).min(max_page)
            } else {
                0
            };
            let response = AuditQueryResponse {
                total: result.total,
                page: req.page,
                page_size,
                total_pages,
                data: result.logs.into_iter().map(AuditLogItem::from).collect(),
            };
            info!(
                total = response.total,
                page = response.page,
                returned = response.data.len(),
                "查询完成"
            );
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("查询审计日志失败: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(HandlerError {
                    error: "query_failed".to_string(),
                    message: "查询失败".to_string(),
                }),
            )
                .into_response()
        }
    }
}
