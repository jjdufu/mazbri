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
use mazbri_core::num::SaturatingFrom;
use mazbri_core::validation::limits;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
const MAX_PAGE_SIZE: u32 = limits::MAX_PAGE_SIZE;
const MAX_TOTAL_RESULTS: u64 = limits::MAX_TOTAL_RESULTS;
/// Latest-universe size for open list/search (no time range, non-selective).
const LATEST_UNIVERSE_SIZE: usize = limits::MAX_TOTAL_RESULTS_USIZE;
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
    /// JSON array of `ListFilter` objects.
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
    /// Comma-separated exact service names (OR with service / `service_like`).
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
        let page_size = u64::from(page_size.clamp(1, MAX_PAGE_SIZE));
        u32::saturating_from(MAX_TOTAL_RESULTS.div_ceil(page_size).max(1))
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
        let offset = u64::from(page.saturating_sub(1)) * u64::from(page_size);
        u32::saturating_from(offset.min(MAX_TOTAL_RESULTS.saturating_sub(1)))
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
    /// Exact `request_id` / `trace_id` — full history.
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
    /// Present when `query_mode` is `latest` (universe size used for the inner Top-N).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub universe_size: Option<u64>,
    /// True when total exceeds the offset browse depth (pager is capped).
    pub browse_capped: bool,
    pub data: Vec<RequestLogItem>,
}
fn bad_request(error: HandlerError) -> Response {
    (StatusCode::BAD_REQUEST, Json(error)).into_response()
}

fn list_query_error(e: mazbri_analytics::list_query::ListQueryError) -> HandlerError {
    HandlerError {
        error: e.code.to_string(),
        message: e.message,
    }
}

/// 解析 `filters` JSON 与扁平数值过滤参数，并校验。
fn parse_request_filters(
    req: &RequestLogQueryRequest,
) -> Result<Vec<mazbri_analytics::list_query::ListFilter>, HandlerError> {
    let mut filters = mazbri_analytics::list_query::parse_filters_json(req.filters.as_deref())
        .map_err(list_query_error)?;
    filters.extend(mazbri_analytics::list_query::collect_flat_numeric_filters(
        &mazbri_analytics::list_query::FlatNumericFilters {
            duration_ms_gt: req.duration_ms_gt,
            duration_ms_gte: req.duration_ms_gte,
            duration_ms_lt: req.duration_ms_lt,
            duration_ms_lte: req.duration_ms_lte,
            duration_ms_eq: req.duration_ms_eq,
            status_code_gt: req.status_code_gt,
            status_code_gte: req.status_code_gte,
            status_code_lt: req.status_code_lt,
            status_code_lte: req.status_code_lte,
            status_code_neq: req.status_code_neq,
            upstream_service_time_ms_gt: req.upstream_service_time_ms_gt,
            upstream_service_time_ms_gte: req.upstream_service_time_ms_gte,
        },
    ));
    mazbri_analytics::list_query::validate_filters(&filters).map_err(list_query_error)?;
    Ok(filters)
}

fn parse_optional_time(
    value: Option<&String>,
    field: &str,
) -> Result<Option<DateTime<Utc>>, HandlerError> {
    let Some(s) = value else {
        return Ok(None);
    };
    parse_time_string(s).map(Some).ok_or_else(|| HandlerError {
        error: format!("invalid_{field}"),
        message: format!("{field} 格式无效，请使用 Unix 时间戳或 ISO 8601 格式"),
    })
}

/// 查询时间窗：`(start, end, latest_universe_limit, universe_size)`。
type TimeWindow = (
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<usize>,
    Option<u64>,
);

fn resolve_time_window(
    query_mode: QueryMode,
    parsed_start: Option<DateTime<Utc>>,
    parsed_end: Option<DateTime<Utc>>,
    filters: &[mazbri_analytics::list_query::ListFilter],
) -> TimeWindow {
    use mazbri_analytics::list_query::FilterOp;
    match query_mode {
        QueryMode::Selective => (None, None, None, None),
        QueryMode::Latest => (
            None,
            None,
            Some(LATEST_UNIVERSE_SIZE),
            Some(LATEST_UNIVERSE_SIZE as u64),
        ),
        // Explicit start/end params always normalize both sides.
        QueryMode::Ranged if parsed_start.is_some() || parsed_end.is_some() => {
            let (s, e) = normalize_ranged_bounds(parsed_start, parsed_end);
            (Some(s), Some(e), None, None)
        }
        QueryMode::Ranged => {
            let has_ts = |ops: &[FilterOp]| {
                filters
                    .iter()
                    .any(|f| f.field == "timestamp" && ops.contains(&f.op))
            };
            let has_ts_lower =
                has_ts(&[FilterOp::Gt, FilterOp::Gte, FilterOp::Between, FilterOp::Eq]);
            let has_ts_upper =
                has_ts(&[FilterOp::Lt, FilterOp::Lte, FilterOp::Between, FilterOp::Eq]);
            // Filter-only time range: inject safety floor when no lower bound exists.
            let now = Utc::now();
            let start = (!has_ts_lower).then(|| now - OPEN_END_LOOKBACK);
            let end = (!has_ts_upper).then_some(now);
            (start, end, None, None)
        }
    }
}

fn build_query_params(
    req: &RequestLogQueryRequest,
    filters: Vec<mazbri_analytics::list_query::ListFilter>,
    window: TimeWindow,
) -> RequestLogQueryParams {
    let (start_time, end_time, latest_universe_limit, _) = window;
    let q = req
        .q
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    RequestLogQueryParams {
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
        limit: req.capped_page_size() as usize,
        offset: req.capped_offset() as usize,
    }
}

fn build_query_response(
    req: &RequestLogQueryRequest,
    query_mode: QueryMode,
    universe_size: Option<u64>,
    result: mazbri_analytics::greptimedb::RequestLogQueryResult,
) -> RequestLogQueryResponse {
    let page_size = req.capped_page_size();
    let max_page = RequestLogQueryRequest::max_page_for(page_size);
    let latest = matches!(query_mode, QueryMode::Latest);
    // Latest mode total is already ≤ universe; ranged/selective may exceed browse depth.
    let browse_capped = !latest && result.total > MAX_TOTAL_RESULTS;
    let display_total = if latest {
        result.total.min(MAX_TOTAL_RESULTS)
    } else {
        result.total
    };
    let total_pages = if page_size > 0 {
        let pages_from_total = u32::saturating_from(display_total.div_ceil(u64::from(page_size)));
        if latest {
            pages_from_total
        } else {
            pages_from_total.min(max_page)
        }
    } else {
        0
    };
    RequestLogQueryResponse {
        total: display_total,
        page: req.page,
        page_size,
        total_pages,
        query_mode: query_mode.as_str().to_string(),
        universe_size,
        browse_capped,
        data: result.logs.into_iter().map(RequestLogItem::from).collect(),
    }
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
        return bad_request(HandlerError {
            error: "invalid_pagination".to_string(),
            message: e,
        });
    }
    let filters = match parse_request_filters(&req) {
        Ok(filters) => filters,
        Err(error) => return bad_request(error),
    };
    let (parsed_start, parsed_end) = match (
        parse_optional_time(req.start_time.as_ref(), "start_time"),
        parse_optional_time(req.end_time.as_ref(), "end_time"),
    ) {
        (Ok(start), Ok(end)) => (start, end),
        (Err(error), _) | (_, Err(error)) => return bad_request(error),
    };
    let has_time_params = parsed_start.is_some() || parsed_end.is_some();
    let has_timestamp_filters = mazbri_analytics::list_query::filters_have_timestamp(&filters);
    let query_mode = if req.is_selective_lookup() {
        QueryMode::Selective
    } else if has_time_params || has_timestamp_filters {
        QueryMode::Ranged
    } else {
        QueryMode::Latest
    };
    let window = resolve_time_window(query_mode, parsed_start, parsed_end, &filters);
    let universe_size = window.3;
    let params = build_query_params(&req, filters, window);
    match state.clickhouse_client.query_request_logs(&params).await {
        Ok(result) => {
            let response = build_query_response(&req, query_mode, universe_size, result);
            info!(
                total = response.total,
                page = response.page,
                total_pages = response.total_pages,
                query_mode = %response.query_mode,
                browse_capped = response.browse_capped,
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
