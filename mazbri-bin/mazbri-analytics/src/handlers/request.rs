use super::{AppState, HandlerError, parse_time_string};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use mazbri_analytics::RequestLogEntry;
use mazbri_analytics::greptimedb::RequestLogQueryParams;
use mazbri_core::validation::limits;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
const MAX_PAGE_SIZE: u32 = limits::MAX_PAGE_SIZE;
const MAX_TOTAL_RESULTS: u64 = limits::MAX_TOTAL_RESULTS;
/// Latest-universe size for open list/search (no time range, non-selective).
const LATEST_UNIVERSE_SIZE: usize = limits::MAX_TOTAL_RESULTS as usize;
/// Safety lookback when only an upper time bound is provided.
const OPEN_END_LOOKBACK: Duration = Duration::days(7);
fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    20
}
#[derive(Debug, Clone, Deserialize)]
pub struct RequestLogQueryRequest {
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
    /// Free-text keyword (`q`).
    #[serde(default)]
    pub q: Option<String>,
    /// JSON array of ListFilter objects.
    #[serde(default)]
    pub filters: Option<String>,
    #[serde(default)]
    pub duration_ms_gt: Option<u64>,
    #[serde(default)]
    pub duration_ms_gte: Option<u64>,
    #[serde(default)]
    pub duration_ms_lt: Option<u64>,
    #[serde(default)]
    pub duration_ms_lte: Option<u64>,
    #[serde(default)]
    pub duration_ms_eq: Option<u64>,
    #[serde(default)]
    pub status_code_gt: Option<u16>,
    #[serde(default)]
    pub status_code_gte: Option<u16>,
    #[serde(default)]
    pub status_code_lt: Option<u16>,
    #[serde(default)]
    pub status_code_lte: Option<u16>,
    #[serde(default)]
    pub status_code_neq: Option<u16>,
    #[serde(default)]
    pub upstream_service_time_ms_gt: Option<u64>,
    #[serde(default)]
    pub upstream_service_time_ms_gte: Option<u64>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub trace_id: Option<String>,
    #[serde(default)]
    pub tenant_code: Option<String>,
    #[serde(default)]
    pub product_id: Option<String>,
    #[serde(default)]
    pub supplier_id: Option<String>,
    #[serde(default)]
    pub supplier_code: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub channel_code: Option<String>,
    #[serde(default, alias = "target")]
    pub event_target: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub route_template: Option<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub client_ip: Option<String>,
    #[serde(default)]
    pub upstream_host: Option<String>,
    #[serde(default)]
    pub upstream_status_code: Option<u16>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error_stage: Option<String>,
    #[serde(default)]
    pub response_flags: Option<String>,
    #[serde(default)]
    pub status_code: Option<u16>,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub service_like: Option<String>,
    /// Comma-separated exact service names (OR with service / service_like).
    #[serde(default)]
    pub service_in: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}
impl RequestLogQueryRequest {
    /// Maximum reachable page for the given page size under the browse-depth cap.
    pub fn max_page_for(page_size: u32) -> u32 {
        let page_size = page_size.clamp(1, MAX_PAGE_SIZE) as u64;
        MAX_TOTAL_RESULTS.div_ceil(page_size).max(1) as u32
    }

    pub fn validate(&self) -> Result<(), String> {
        let page_size = self.page_size.clamp(1, MAX_PAGE_SIZE);
        let max_page = Self::max_page_for(page_size);
        if self.page > max_page {
            return Err(format!(
                "页码超过最大深度限制 (page: {}, max: {})",
                self.page, max_page
            ));
        }
        Ok(())
    }
    pub fn capped_page_size(&self) -> u32 {
        self.page_size.clamp(1, MAX_PAGE_SIZE)
    }
    pub fn capped_offset(&self) -> u32 {
        let page = if self.page == 0 { 1 } else { self.page };
        let page_size = self.capped_page_size();
        let offset = (page.saturating_sub(1) as u64) * (page_size as u64);
        offset.min(MAX_TOTAL_RESULTS.saturating_sub(1)) as u32
    }

    /// Exact id lookups may span full history; free-form list queries may not.
    fn is_selective_lookup(&self) -> bool {
        self.request_id
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty())
            || self
                .trace_id
                .as_deref()
                .is_some_and(|v| !v.trim().is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryMode {
    /// Exact request_id / trace_id — full history.
    Selective,
    /// Explicit time bounds — search within range (full volume in range).
    Ranged,
    /// No time bounds — latest N universe then filter.
    Latest,
}

impl QueryMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Selective => "selective",
            Self::Ranged => "ranged",
            Self::Latest => "latest",
        }
    }
}

/// Normalize ranged mode bounds: fill missing side; only-upper gets 7d lookback.
fn normalize_ranged_bounds(
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Utc::now();
    match (start_time, end_time) {
        (Some(s), Some(e)) => {
            if s <= e {
                (s, e)
            } else {
                (e, s)
            }
        }
        (Some(s), None) => (s, now),
        (None, Some(e)) => (e - OPEN_END_LOOKBACK, e),
        (None, None) => (now - OPEN_END_LOOKBACK, now),
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct RequestLogItem {
    pub timestamp: DateTime<Utc>,
    pub service: String,
    pub env: String,
    pub host: String,
    pub instance_id: String,
    #[serde(rename = "target")]
    pub event_target: String,
    pub message: String,
    pub request_id: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: String,
    pub tenant_id: String,
    pub tenant_code: String,
    pub product_id: String,
    pub supplier_id: String,
    pub supplier_code: String,
    pub channel_id: String,
    pub channel_code: String,
    pub attempt_order: u32,
    pub method: String,
    pub protocol: String,
    pub host_header: String,
    pub path: String,
    pub route_template: String,
    pub status_code: u16,
    pub duration_ms: u64,
    pub upstream_host: String,
    pub upstream_status_code: u16,
    pub upstream_service_time_ms: u64,
    pub dependency_wait_ms: u64,
    pub internal_ms: u64,
    pub post_response_ms: u64,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub client_ip: String,
    pub xff: String,
    pub user_agent: String,
    pub cache_hit: bool,
    pub fallback_used: bool,
    pub retry_count: u32,
    pub error_code: String,
    pub error_stage: String,
    pub response_flags: String,
    pub log_file: String,
    pub raw: String,
}
impl From<RequestLogEntry> for RequestLogItem {
    fn from(entry: RequestLogEntry) -> Self {
        Self {
            timestamp: entry.timestamp,
            service: entry.service,
            env: entry.env,
            host: entry.host,
            instance_id: entry.instance_id,
            event_target: entry.event_target,
            message: entry.message,
            request_id: entry.request_id,
            trace_id: entry.trace_id,
            span_id: entry.span_id,
            parent_span_id: entry.parent_span_id,
            tenant_id: entry.tenant_id,
            tenant_code: entry.tenant_code,
            product_id: entry.product_id,
            supplier_id: entry.supplier_id,
            supplier_code: entry.supplier_code,
            channel_id: entry.channel_id,
            channel_code: entry.channel_code,
            attempt_order: entry.attempt_order,
            method: entry.method,
            protocol: entry.protocol,
            host_header: entry.host_header,
            path: entry.path,
            route_template: entry.route_template,
            status_code: entry.status_code,
            duration_ms: entry.duration_ms,
            upstream_host: entry.upstream_host,
            upstream_status_code: entry.upstream_status_code,
            upstream_service_time_ms: entry.upstream_service_time_ms,
            dependency_wait_ms: entry.dependency_wait_ms,
            internal_ms: entry.internal_ms,
            post_response_ms: entry.post_response_ms,
            request_bytes: entry.request_bytes,
            response_bytes: entry.response_bytes,
            client_ip: entry.client_ip,
            xff: entry.xff,
            user_agent: entry.user_agent,
            cache_hit: entry.cache_hit,
            fallback_used: entry.fallback_used,
            retry_count: entry.retry_count,
            error_code: entry.error_code,
            error_stage: entry.error_stage,
            response_flags: entry.response_flags,
            log_file: entry.log_file,
            raw: entry.raw,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct RequestLogQueryResponse {
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
    /// `latest` | `ranged` | `selective`
    pub query_mode: String,
    /// Present when query_mode is `latest` (universe size used for the inner Top-N).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub universe_size: Option<u64>,
    /// True when total exceeds the offset browse depth (pager is capped).
    pub browse_capped: bool,
    pub data: Vec<RequestLogItem>,
}
pub async fn query_request_logs(
    State(state): State<AppState>,
    Query(req): Query<RequestLogQueryRequest>,
) -> Response {
    info!(
        page = req.page, page_size = req.page_size, request_id = ? req.request_id, q
        = ? req.q, event_target = ? req.event_target, path = ? req.path, tenant_id = ? req.tenant_id, product_id = ?
        req.product_id, supplier_id = ? req.supplier_id, channel_id = ? req.channel_id,
        "查询请求日志"
    );
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

    let mut filters = match mazbri_analytics::list_query::parse_filters_json(req.filters.as_deref())
    {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(HandlerError {
                    error: e.code.to_string(),
                    message: e.message,
                }),
            )
                .into_response();
        }
    };
    filters.extend(mazbri_analytics::list_query::collect_flat_numeric_filters(
        req.duration_ms_gt,
        req.duration_ms_gte,
        req.duration_ms_lt,
        req.duration_ms_lte,
        req.duration_ms_eq,
        req.status_code_gt,
        req.status_code_gte,
        req.status_code_lt,
        req.status_code_lte,
        req.status_code_neq,
        req.upstream_service_time_ms_gt,
        req.upstream_service_time_ms_gte,
    ));
    if let Err(e) = mazbri_analytics::list_query::validate_filters(&filters) {
        return (
            StatusCode::BAD_REQUEST,
            Json(HandlerError {
                error: e.code.to_string(),
                message: e.message,
            }),
        )
            .into_response();
    }

    let client = &state.clickhouse_client;
    let page_size = req.capped_page_size();
    let offset = req.capped_offset();
    let parsed_start = match req.start_time.as_ref() {
        Some(s) => match parse_time_string(s) {
            Some(t) => Some(t),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(HandlerError {
                        error: "invalid_start_time".to_string(),
                        message: "start_time 格式无效，请使用 Unix 时间戳或 ISO 8601 格式"
                            .to_string(),
                    }),
                )
                    .into_response();
            }
        },
        None => None,
    };
    let parsed_end = match req.end_time.as_ref() {
        Some(s) => match parse_time_string(s) {
            Some(t) => Some(t),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(HandlerError {
                        error: "invalid_end_time".to_string(),
                        message: "end_time 格式无效，请使用 Unix 时间戳或 ISO 8601 格式"
                            .to_string(),
                    }),
                )
                    .into_response();
            }
        },
        None => None,
    };

    let has_time_params = parsed_start.is_some() || parsed_end.is_some();
    let has_timestamp_filters = mazbri_analytics::list_query::filters_have_timestamp(&filters);
    let selective = req.is_selective_lookup();

    let query_mode = if selective {
        QueryMode::Selective
    } else if has_time_params || has_timestamp_filters {
        QueryMode::Ranged
    } else {
        QueryMode::Latest
    };

    let has_ts_lower = filters.iter().any(|f| {
        f.field == "timestamp"
            && matches!(
                f.op,
                mazbri_analytics::list_query::FilterOp::Gt
                    | mazbri_analytics::list_query::FilterOp::Gte
                    | mazbri_analytics::list_query::FilterOp::Between
                    | mazbri_analytics::list_query::FilterOp::Eq
            )
    });
    let has_ts_upper = filters.iter().any(|f| {
        f.field == "timestamp"
            && matches!(
                f.op,
                mazbri_analytics::list_query::FilterOp::Lt
                    | mazbri_analytics::list_query::FilterOp::Lte
                    | mazbri_analytics::list_query::FilterOp::Between
                    | mazbri_analytics::list_query::FilterOp::Eq
            )
    });

    let (start_time, end_time, latest_universe_limit, universe_size) = match query_mode {
        QueryMode::Selective => (None, None, None, None),
        QueryMode::Latest => (
            None,
            None,
            Some(LATEST_UNIVERSE_SIZE),
            Some(LATEST_UNIVERSE_SIZE as u64),
        ),
        QueryMode::Ranged => {
            // Explicit start/end params always normalize both sides.
            if has_time_params {
                let (s, e) = normalize_ranged_bounds(parsed_start, parsed_end);
                (Some(s), Some(e), None, None)
            } else {
                // Filter-only time range: inject safety floor when no lower bound exists.
                let now = Utc::now();
                let start = if has_ts_lower {
                    None
                } else {
                    Some(now - OPEN_END_LOOKBACK)
                };
                let end = if has_ts_upper { None } else { Some(now) };
                (start, end, None, None)
            }
        }
    };

    let q = req
        .q
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let params = RequestLogQueryParams {
        start_time,
        end_time,
        q,
        request_id: req.request_id.clone(),
        trace_id: req.trace_id.clone(),
        tenant_code: req.tenant_code.clone(),
        product_id: req.product_id.clone(),
        supplier_id: req.supplier_id.clone(),
        supplier_code: req.supplier_code.clone(),
        channel_id: req.channel_id.clone(),
        channel_code: req.channel_code.clone(),
        event_target: req.event_target.clone(),
        path: req.path.clone(),
        route_template: req.route_template.clone(),
        tenant_id: req.tenant_id.clone(),
        client_ip: req.client_ip.clone(),
        upstream_host: req.upstream_host.clone(),
        upstream_status_code: req.upstream_status_code,
        error_code: req.error_code.clone(),
        error_stage: req.error_stage.clone(),
        response_flags: req.response_flags.clone(),
        status_code: req.status_code,
        service: req.service.clone(),
        service_like: req.service_like.clone(),
        service_in: req.service_in.clone(),
        filters,
        latest_universe_limit,
        limit: page_size as usize,
        offset: offset as usize,
    };
    match client.query_request_logs(&params).await {
        Ok(result) => {
            let max_page = RequestLogQueryRequest::max_page_for(page_size);
            // Latest mode total is already ≤ universe; ranged/selective may exceed browse depth.
            let browse_capped = match query_mode {
                QueryMode::Latest => false,
                QueryMode::Ranged | QueryMode::Selective => result.total > MAX_TOTAL_RESULTS,
            };
            let display_total = match query_mode {
                QueryMode::Latest => result.total.min(MAX_TOTAL_RESULTS),
                QueryMode::Ranged | QueryMode::Selective => result.total,
            };
            let total_pages = if page_size > 0 {
                let pages_from_total = display_total.div_ceil(page_size as u64) as u32;
                if matches!(query_mode, QueryMode::Latest) {
                    pages_from_total
                } else {
                    pages_from_total.min(max_page)
                }
            } else {
                0
            };
            let response = RequestLogQueryResponse {
                total: display_total,
                page: req.page,
                page_size,
                total_pages,
                query_mode: query_mode.as_str().to_string(),
                universe_size,
                browse_capped,
                data: result.logs.into_iter().map(RequestLogItem::from).collect(),
            };
            info!(
                total = response.total,
                page = response.page,
                total_pages = response.total_pages,
                query_mode = %response.query_mode,
                browse_capped,
                returned = response.data.len(),
                "查询完成"
            );
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            warn!("查询请求日志失败: {}", e);
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
