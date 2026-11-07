//! Trace 检索、详情与服务依赖接口（数据在 `GreptimeDB` trace 库，`request_id` / 租户 / 产品先经 CH 解析）。
use super::request::{RequestLogQueryRequest, bad_request, build_query_params};
use super::{AppState, HandlerError};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use mazbri_analytics::greptimedb::traces::{
    DEFAULT_SEARCH_PAGE_SIZE, MAX_SEARCH_PAGE_SIZE, TraceSearchFilter, TraceSummary,
    validate_service_name, validate_trace_id,
};
use mazbri_analytics::greptimedb::{EventTarget, validate_like_param};
use serde::{Deserialize, Serialize};
use tracing::warn;

/// trace 保留期（秒），也是检索窗口最大跨度。
const TRACE_RETENTION_SECS: i64 = 7 * 86_400;
/// 默认检索窗口（秒）。
const DEFAULT_WINDOW_SECS: i64 = 3_600;
/// 依赖图最大窗口（秒）。
const MAX_DEPENDENCY_WINDOW_SECS: i64 = 86_400;
/// CH 解析 `trace_id` 时最多取的请求行数。
const MAX_RESOLVED_REQUESTS: u32 = 200;
/// 详情查询窗口超出 trace 开始时间的余量（秒）。
const DETAIL_SLACK_SECS: i64 = 60;

fn invalid(error: &str, message: impl Into<String>) -> HandlerError {
    HandlerError {
        error: error.to_string(),
        message: message.into(),
    }
}

fn failed(context: &str, e: &impl std::fmt::Display) -> Response {
    warn!("{context}: {e}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(HandlerError {
            error: "query_failed".to_string(),
            message: "查询失败".to_string(),
        }),
    )
        .into_response()
}

fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(HandlerError {
            error: "greptimedb_unavailable".to_string(),
            message: "GreptimeDB 未配置".to_string(),
        }),
    )
        .into_response()
}

/// 解析 Unix 秒窗口：缺省为 `[now - default_span, now]`，跨度不超过 `max_span`。
fn window(
    start: Option<i64>,
    end: Option<i64>,
    default_span: i64,
    max_span: i64,
) -> Result<(i64, i64), HandlerError> {
    let now = Utc::now().timestamp();
    let end = end.unwrap_or(now);
    let start = start.unwrap_or(end - default_span);
    if end <= start {
        return Err(invalid("invalid_range", "end 必须晚于 start"));
    }
    if end - start > max_span {
        return Err(invalid(
            "range_too_large",
            format!("时间跨度最长 {} 小时", max_span / 3_600),
        ));
    }
    Ok((start, end))
}

fn trimmed(v: Option<&String>) -> Option<&str> {
    v.map(|s| s.trim()).filter(|s| !s.is_empty())
}

/// `GET /api/v1/traces/search` 参数。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceSearchQuery {
    /// Unix 秒。
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub end: Option<i64>,
    #[serde(default)]
    pub trace_id: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub tenant_code: Option<String>,
    #[serde(default)]
    pub product_id: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub min_duration_ms: Option<u64>,
    #[serde(default)]
    pub max_duration_ms: Option<u64>,
    #[serde(default)]
    pub errors_only: bool,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub page_size: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct TraceSearchResponse {
    pub start: i64,
    pub end: i64,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
    /// 条件经 `request_logs` 解析出的候选 trace 数（未经 CH 解析时为 `None`）。
    pub resolved_from_logs: Option<usize>,
    pub traces: Vec<TraceSummary>,
}

fn validate_like(field: &str, v: &str) -> Result<String, HandlerError> {
    validate_like_param(v)
        .map(|_| v.to_string())
        .map_err(|e| invalid("invalid_param", format!("{field}: {e}")))
}

/// 校验检索参数，返回 `GreptimeDB` 过滤条件与（若需）CH 解析请求。
fn parse_search(
    q: &TraceSearchQuery,
) -> Result<(TraceSearchFilter, Option<RequestLogQueryRequest>), HandlerError> {
    let (start, end) = window(q.start, q.end, DEFAULT_WINDOW_SECS, TRACE_RETENTION_SECS)?;
    if let (Some(min), Some(max)) = (q.min_duration_ms, q.max_duration_ms)
        && min > max
    {
        return Err(invalid(
            "invalid_duration",
            "min_duration_ms 不能大于 max_duration_ms",
        ));
    }
    let trace_ids = trimmed(q.trace_id.as_ref())
        .map(|id| validate_trace_id(id).map_err(|e| invalid("invalid_trace_id", e)))
        .transpose()?
        .map(|id| vec![id]);
    let service = trimmed(q.service.as_ref())
        .map(|s| validate_service_name(s).map_err(|e| invalid("invalid_service", e)))
        .transpose()?;
    let request_id = trimmed(q.request_id.as_ref())
        .map(|v| validate_like("request_id", v))
        .transpose()?;
    let tenant_code = trimmed(q.tenant_code.as_ref())
        .map(|v| validate_like("tenant_code", v))
        .transpose()?;
    let product_id = trimmed(q.product_id.as_ref())
        .map(|v| validate_like("product_id", v))
        .transpose()?;
    let lookup =
        (request_id.is_some() || tenant_code.is_some() || product_id.is_some()).then(|| {
            RequestLogQueryRequest {
                request_id,
                tenant_code,
                product_id,
                event_target: Some(EventTarget::Gateway),
                start_time: DateTime::<Utc>::from_timestamp(start, 0).map(|t| t.to_rfc3339()),
                end_time: DateTime::<Utc>::from_timestamp(end, 0).map(|t| t.to_rfc3339()),
                page: 1,
                page_size: MAX_RESOLVED_REQUESTS,
                ..RequestLogQueryRequest::default()
            }
        });
    let filter = TraceSearchFilter {
        start,
        end,
        trace_ids,
        service,
        min_duration_ms: q.min_duration_ms,
        max_duration_ms: q.max_duration_ms,
        errors_only: q.errors_only,
        page: q.page.unwrap_or(1).max(1),
        page_size: q
            .page_size
            .unwrap_or(DEFAULT_SEARCH_PAGE_SIZE)
            .clamp(1, MAX_SEARCH_PAGE_SIZE),
    };
    Ok((filter, lookup))
}

/// `GET /api/v1/traces/search`
pub async fn search_traces(
    State(state): State<AppState>,
    Query(q): Query<TraceSearchQuery>,
) -> Response {
    let Some(client) = state.greptimedb_client.as_ref() else {
        return unavailable();
    };
    let (mut filter, lookup) = match parse_search(&q) {
        Ok(parsed) => parsed,
        Err(e) => return bad_request(e),
    };
    let mut resolved_from_logs = None;
    if let Some(req) = lookup {
        let start = DateTime::<Utc>::from_timestamp(filter.start, 0);
        let end = DateTime::<Utc>::from_timestamp(filter.end, 0);
        let params = build_query_params(&req, Vec::new(), (start, end, None, None));
        let logs = match state.clickhouse_client.query_request_logs(&params).await {
            Ok(result) => result.logs,
            Err(e) => return failed("按 request_logs 解析 trace_id 失败", &e),
        };
        let mut ids: Vec<String> = logs
            .into_iter()
            .filter_map(|l| validate_trace_id(&l.trace_id).ok())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if let Some(explicit) = &filter.trace_ids {
            ids.retain(|id| explicit.contains(id));
        }
        resolved_from_logs = Some(ids.len());
        filter.trace_ids = Some(ids);
    }
    match client.search_traces(&filter).await {
        Ok(page) => Json(TraceSearchResponse {
            start: filter.start,
            end: filter.end,
            total: page.total,
            page: page.page,
            page_size: page.page_size,
            resolved_from_logs,
            traces: page.traces,
        })
        .into_response(),
        Err(e) => failed("trace 检索失败", &e),
    }
}

/// `GET /api/v1/traces/{trace_id}` 参数：可选的 trace 开始时间提示（Unix 秒）。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceDetailQuery {
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub end: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TraceNotFound {
    pub error: &'static str,
    pub message: &'static str,
    pub trace_id: String,
}

/// `GET /api/v1/traces/{trace_id}`
pub async fn get_trace(
    State(state): State<AppState>,
    Path(trace_id): Path<String>,
    Query(q): Query<TraceDetailQuery>,
) -> Response {
    let Some(client) = state.greptimedb_client.as_ref() else {
        return unavailable();
    };
    let trace_id = match validate_trace_id(&trace_id) {
        Ok(id) => id,
        Err(e) => return bad_request(invalid("invalid_trace_id", e)),
    };
    let now = Utc::now().timestamp();
    let (start, end) = match window(
        q.start.map(|s| s - DETAIL_SLACK_SECS),
        Some(
            q.end
                .map_or(now + DETAIL_SLACK_SECS, |e| e + DETAIL_SLACK_SECS),
        ),
        TRACE_RETENTION_SECS,
        TRACE_RETENTION_SECS + 2 * DETAIL_SLACK_SECS,
    ) {
        Ok(w) => w,
        Err(e) => return bad_request(e),
    };
    match client.trace_detail(&trace_id, start, end).await {
        Ok(Some(detail)) => Json(detail).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(TraceNotFound {
                error: "trace_not_found",
                message: "trace 不存在或已超过保留期",
                trace_id,
            }),
        )
            .into_response(),
        Err(e) => failed("trace 详情查询失败", &e),
    }
}

/// `GET /api/v1/traces/dependencies` 参数（Unix 秒，默认最近 1 小时，最长 24 小时）。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyQuery {
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub end: Option<i64>,
}


/// `GET /api/v1/traces/services`
pub async fn list_trace_services(
    State(state): State<AppState>,
    Query(q): Query<DependencyQuery>,
) -> Response {
    let Some(client) = state.greptimedb_client.as_ref() else {
        return unavailable();
    };
    let (start, end) = match window(
        q.start,
        q.end,
        DEFAULT_WINDOW_SECS,
        MAX_DEPENDENCY_WINDOW_SECS,
    ) {
        Ok(w) => w,
        Err(e) => return bad_request(e),
    };
    match client.list_trace_services(start, end).await {
        Ok(services) => Json(services).into_response(),
        Err(e) => failed("trace 服务列表查询失败", &e),
    }
}

/// `GET /api/v1/traces/dependencies`
pub async fn trace_dependencies(
    State(state): State<AppState>,
    Query(q): Query<DependencyQuery>,
) -> Response {
    let Some(client) = state.greptimedb_client.as_ref() else {
        return unavailable();
    };
    let (start, end) = match window(
        q.start,
        q.end,
        DEFAULT_WINDOW_SECS,
        MAX_DEPENDENCY_WINDOW_SECS,
    ) {
        Ok(w) => w,
        Err(e) => return bad_request(e),
    };
    match client.trace_dependencies(start, end).await {
        Ok(graph) => Json(graph).into_response(),
        Err(e) => failed("服务依赖查询失败", &e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_defaults_to_last_hour() {
        let (f, lookup) = parse_search(&TraceSearchQuery::default()).expect("default ok");
        assert_eq!(f.end - f.start, DEFAULT_WINDOW_SECS);
        assert_eq!(f.page, 1);
        assert_eq!(f.page_size, DEFAULT_SEARCH_PAGE_SIZE);
        assert!(lookup.is_none());
    }

    #[test]
    fn request_id_goes_through_logs() {
        let (f, lookup) = parse_search(&TraceSearchQuery {
            request_id: Some("23222057-38d7-48ed-be6c-22e7eefa9add".into()),
            ..TraceSearchQuery::default()
        })
        .expect("ok");
        assert!(f.trace_ids.is_none());
        let req = lookup.expect("needs CH lookup");
        assert_eq!(req.event_target, Some(EventTarget::Gateway));
        assert!(req.start_time.is_some());
    }

    #[test]
    fn search_rejects_bad_input() {
        let bad = |q: TraceSearchQuery| parse_search(&q).is_err();
        assert!(bad(TraceSearchQuery {
            trace_id: Some("zz".into()),
            ..TraceSearchQuery::default()
        }));
        assert!(bad(TraceSearchQuery {
            service: Some("x' or 1=1".into()),
            ..TraceSearchQuery::default()
        }));
        assert!(bad(TraceSearchQuery {
            start: Some(0),
            end: Some(TRACE_RETENTION_SECS + 1),
            ..TraceSearchQuery::default()
        }));
        assert!(bad(TraceSearchQuery {
            min_duration_ms: Some(10),
            max_duration_ms: Some(5),
            ..TraceSearchQuery::default()
        }));
    }
}
