//! 调用检索：时间分布直方图与单请求阶段耗时。
//!
//! 直方图与 `/api/v1/request/query` 共用请求参数与过滤条件，框选区间后明细总数与桶计数之和一致。
use super::request::{
    RequestLogItem, RequestLogQueryRequest, bad_request, build_query_params, parse_optional_time,
    parse_request_filters,
};
use super::{AppState, HandlerError};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use mazbri_analytics::clickhouse::types::StageLogEntry;
use mazbri_analytics::greptimedb::validate_like_param;
use mazbri_core::num::SaturatingFrom;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// 直方图最大跨度：明细表保留期。
const MAX_HISTOGRAM_SPAN: Duration = Duration::days(30);
/// 目标桶数上限。
const TARGET_BUCKETS: u64 = 120;
/// 可选桶宽（秒）。
const BUCKET_STEPS: [u32; 12] = [
    1, 5, 10, 30, 60, 300, 600, 1_800, 3_600, 10_800, 21_600, 86_400,
];
/// 按 `trace_id` 定位时最多关联的请求数。
const MAX_TRACE_REQUESTS: u32 = 50;

#[derive(Debug, Serialize)]
pub struct HistogramBucket {
    pub ts: i64,
    pub calls: u64,
    pub errors: u64,
}

#[derive(Debug, Serialize)]
pub struct HistogramResponse {
    /// Window bounds as Unix seconds, the same unit as `buckets[].ts` and the request.
    pub start: i64,
    pub end: i64,
    pub bucket_seconds: u32,
    pub total: u64,
    pub errors: u64,
    pub buckets: Vec<HistogramBucket>,
}

/// 选择不超过 [`TARGET_BUCKETS`] 个桶的最小桶宽。
fn pick_bucket_seconds(span: Duration) -> u32 {
    let span_secs = span.num_seconds().max(1).unsigned_abs();
    BUCKET_STEPS
        .iter()
        .copied()
        .find(|step| span_secs.div_ceil(u64::from(*step)) <= TARGET_BUCKETS)
        .unwrap_or(BUCKET_STEPS[BUCKET_STEPS.len() - 1])
}

fn invalid(error: &str, message: impl Into<String>) -> HandlerError {
    HandlerError {
        error: error.to_string(),
        message: message.into(),
    }
}

fn query_failed(context: &str, e: &impl std::fmt::Display) -> Response {
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

/// 校验并归一化直方图时间窗：必须给出起止，且跨度不超过 30 天。
fn histogram_window(
    req: &RequestLogQueryRequest,
) -> Result<(DateTime<Utc>, DateTime<Utc>), HandlerError> {
    let start = parse_optional_time(req.start_time.as_ref(), "start_time")?
        .ok_or_else(|| invalid("missing_start_time", "直方图需要 start_time"))?;
    let end = parse_optional_time(req.end_time.as_ref(), "end_time")?
        .ok_or_else(|| invalid("missing_end_time", "直方图需要 end_time"))?;
    if end <= start {
        return Err(invalid("invalid_range", "end_time 必须晚于 start_time"));
    }
    if end - start > MAX_HISTOGRAM_SPAN {
        return Err(invalid("range_too_large", "直方图跨度最长 30 天"));
    }
    Ok((start, end))
}

/// `GET /api/v1/request/histogram`：调用量与错误随时间分布。
pub async fn request_histogram(
    State(state): State<AppState>,
    Query(req): Query<RequestLogQueryRequest>,
) -> Response {
    let (start, end) = match histogram_window(&req) {
        Ok(window) => window,
        Err(error) => return bad_request(error),
    };
    let filters = match parse_request_filters(&req) {
        Ok(filters) => filters,
        Err(error) => return bad_request(error),
    };
    let bucket_seconds = pick_bucket_seconds(end - start);
    let params = build_query_params(&req, filters, (Some(start), Some(end), None, None));
    match state
        .clickhouse_client
        .histogram_request_logs(&params, bucket_seconds)
        .await
    {
        Ok(rows) => {
            let total = rows.iter().map(|r| r.calls).sum();
            let errors = rows.iter().map(|r| r.errors).sum();
            let buckets = rows
                .into_iter()
                .map(|r| HistogramBucket {
                    ts: i64::from(r.ts),
                    calls: r.calls,
                    errors: r.errors,
                })
                .collect();
            Json(HistogramResponse {
                start: start.timestamp(),
                end: end.timestamp(),
                bucket_seconds,
                total,
                errors,
                buckets,
            })
            .into_response()
        }
        Err(e) => query_failed("查询调用直方图失败", &e),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageLookup {
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub trace_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StageLookupResponse {
    /// 命中的 access 行（gateway 与 bypass 共享 `trace_id`）。
    pub calls: Vec<RequestLogItem>,
    pub stages: Vec<StageLogEntry>,
}

/// 仅接受 `request_id` 或 `trace_id` 之一，取值只允许安全字符。
fn stage_lookup_key(
    lookup: &StageLookup,
) -> Result<(Option<String>, Option<String>), HandlerError> {
    let clean = |v: &Option<String>| {
        v.as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| {
                validate_like_param(v)
                    .map_err(|e| invalid("invalid_id", e.to_string()))
                    .map(|_| v.to_string())
            })
            .transpose()
    };
    let request_id = clean(&lookup.request_id)?;
    let trace_id = clean(&lookup.trace_id)?;
    match (&request_id, &trace_id) {
        (None, None) => Err(invalid("missing_id", "需要 request_id 或 trace_id")),
        (Some(_), Some(_)) => Err(invalid("ambiguous_id", "request_id 与 trace_id 只能二选一")),
        _ => Ok((request_id, trace_id)),
    }
}

fn lookup_request(request_id: Option<String>, trace_id: Option<String>) -> RequestLogQueryRequest {
    RequestLogQueryRequest {
        request_id,
        trace_id,
        page: 1,
        page_size: MAX_TRACE_REQUESTS,
        ..RequestLogQueryRequest::default()
    }
}

/// `GET /api/v1/request/stages`：按 `request_id` 或 `trace_id` 查 access 行与阶段耗时。
pub async fn request_stages(
    State(state): State<AppState>,
    Query(lookup): Query<StageLookup>,
) -> Response {
    let (request_id, trace_id) = match stage_lookup_key(&lookup) {
        Ok(key) => key,
        Err(error) => return bad_request(error),
    };
    let req = lookup_request(request_id, trace_id);
    let params = build_query_params(&req, Vec::new(), (None, None, None, None));
    let calls = match state.clickhouse_client.query_request_logs(&params).await {
        Ok(result) => result.logs,
        Err(e) => return query_failed("查询调用记录失败", &e),
    };
    let mut request_ids: Vec<String> = calls.iter().map(|c| c.request_id.clone()).collect();
    request_ids.sort_unstable();
    request_ids.dedup();
    request_ids.truncate(usize::saturating_from(MAX_TRACE_REQUESTS));
    match state.clickhouse_client.request_stages(&request_ids).await {
        Ok(stages) => Json(StageLookupResponse {
            calls: calls.into_iter().map(RequestLogItem::from).collect(),
            stages,
        })
        .into_response(),
        Err(e) => query_failed("查询阶段耗时失败", &e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_width_keeps_bucket_count_bounded() {
        assert_eq!(pick_bucket_seconds(Duration::minutes(5)), 5);
        assert_eq!(pick_bucket_seconds(Duration::hours(1)), 30);
        assert_eq!(pick_bucket_seconds(Duration::hours(24)), 1_800);
        assert_eq!(pick_bucket_seconds(Duration::days(30)), 21_600);
    }

    #[test]
    fn stage_lookup_requires_exactly_one_id() {
        let both = StageLookup {
            request_id: Some("a".into()),
            trace_id: Some("b".into()),
        };
        assert!(stage_lookup_key(&both).is_err());
        let none = StageLookup {
            request_id: Some("  ".into()),
            trace_id: None,
        };
        assert!(stage_lookup_key(&none).is_err());
        let bad = StageLookup {
            request_id: None,
            trace_id: Some("x' OR 1=1".into()),
        };
        assert!(stage_lookup_key(&bad).is_err());
        let ok = StageLookup {
            request_id: None,
            trace_id: Some("f248c8be2060480ca3b2bd64a7232b19".into()),
        };
        assert!(matches!(stage_lookup_key(&ok), Ok((None, Some(_)))));
    }

    #[test]
    fn histogram_bounds_serialize_as_unix_seconds() {
        let body = serde_json::to_value(HistogramResponse {
            start: 1_793_503_500,
            end: 1_793_505_000,
            bucket_seconds: 60,
            total: 0,
            errors: 0,
            buckets: Vec::new(),
        })
        .expect("serialize");
        assert_eq!(body["start"], 1_793_503_500);
        assert_eq!(body["end"], 1_793_505_000);
    }
}
