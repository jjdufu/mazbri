//! Trace 页面：analytics trace 接口转发，以及按 `request_id` 关联计费流水（`usage_outbox` → `usage_records`）。
use crate::analytics_client::AnalyticsHttpClient;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// 单次计费流水查询最多关联的请求数。
const MAX_BILLING_REQUESTS: usize = 50;

/// `GET /stats/traces/search` 参数（原样转发给 analytics）。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceSearchParams {
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
    pub errors_only: Option<bool>,
    #[serde(default)]
    pub limit: Option<u32>,
}

/// 详情 / 依赖图的时间窗参数（Unix 秒）。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceWindowParams {
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub end: Option<i64>,
}

fn push<T: ToString>(out: &mut Vec<(&'static str, String)>, key: &'static str, v: Option<T>) {
    if let Some(v) = v {
        let s = v.to_string();
        let s = s.trim();
        if !s.is_empty() {
            out.push((key, s.to_string()));
        }
    }
}

fn search_params(p: &TraceSearchParams) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    push(&mut out, "start", p.start);
    push(&mut out, "end", p.end);
    push(&mut out, "trace_id", p.trace_id.as_deref());
    push(&mut out, "request_id", p.request_id.as_deref());
    push(&mut out, "tenant_code", p.tenant_code.as_deref());
    push(&mut out, "product_id", p.product_id.as_deref());
    push(&mut out, "service", p.service.as_deref());
    push(&mut out, "min_duration_ms", p.min_duration_ms);
    push(&mut out, "max_duration_ms", p.max_duration_ms);
    push(&mut out, "errors_only", p.errors_only.filter(|v| *v));
    push(&mut out, "limit", p.limit);
    out
}

fn window_params(p: &TraceWindowParams) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    push(&mut out, "start", p.start);
    push(&mut out, "end", p.end);
    out
}

/// 透传 analytics 结果：成功包成 `ApiResponse`，失败走 [`crate::analytics_client::AnalyticsError::into_response`]。
async fn forward(
    client: &AnalyticsHttpClient,
    path: &str,
    params: &[(&str, String)],
    what: &str,
) -> axum::response::Response {
    match client.traces(path, params).await {
        Ok(data) => ApiResponse::ok(data).into_response(),
        Err(e) => e.into_response(what),
    }
}

pub async fn search(
    client: &AnalyticsHttpClient,
    params: &TraceSearchParams,
) -> axum::response::Response {
    forward(client, "/search", &search_params(params), "trace 检索").await
}

pub async fn detail(
    client: &AnalyticsHttpClient,
    trace_id: &str,
    params: &TraceWindowParams,
) -> axum::response::Response {
    let id = trace_id.trim();
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return ApiResponse::err(StatusCode::BAD_REQUEST, "trace_id 格式无效");
    }
    forward(
        client,
        &format!("/{id}"),
        &window_params(params),
        "trace 详情查询",
    )
    .await
}

pub async fn dependencies(
    client: &AnalyticsHttpClient,
    params: &TraceWindowParams,
) -> axum::response::Response {
    forward(
        client,
        "/dependencies",
        &window_params(params),
        "服务依赖查询",
    )
    .await
}

/// `GET /stats/traces/billing` 参数：逗号分隔的 `request_id`。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceBillingParams {
    pub request_ids: String,
}

/// 一条请求的计费流水（outbox 事件与落账记录）。
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TraceBillingRow {
    pub request_id: Uuid,
    pub event_id: Uuid,
    pub outbox_status: String,
    pub retry_count: i32,
    pub error_message: Option<String>,
    pub trace_parent: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub processed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub usage_record_id: Option<i64>,
    pub product_id: Option<String>,
    pub billable_units: Option<i64>,
    pub billable_amount: Option<String>,
    pub charge_status: Option<String>,
    pub charged_at: Option<chrono::DateTime<chrono::Utc>>,
    pub recorded_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn parse_request_ids(raw: &str) -> Result<Vec<Uuid>, String> {
    let ids = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| Uuid::parse_str(s).map_err(|_| format!("request_id 格式无效: {s}")))
        .collect::<Result<Vec<_>, _>>()?;
    if ids.is_empty() {
        return Err("需要 request_ids".to_string());
    }
    if ids.len() > MAX_BILLING_REQUESTS {
        return Err(format!("request_ids 最多 {MAX_BILLING_REQUESTS} 个"));
    }
    Ok(ids)
}

pub async fn billing(pool: &PgPool, params: &TraceBillingParams) -> axum::response::Response {
    let ids = match parse_request_ids(&params.request_ids) {
        Ok(ids) => ids,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e),
    };
    let rows = sqlx::query_as::<_, TraceBillingRow>(
        "SELECT o.request_id, o.event_id, o.status::text AS outbox_status, o.retry_count, \
                o.error_message, o.trace_parent, o.created_at, o.processed_at, \
                r.id AS usage_record_id, COALESCE(r.product_id, o.product_id) AS product_id, \
                r.billable_units, r.billable_amount::text AS billable_amount, \
                r.charge_status, r.charged_at, r.recorded_at \
         FROM usage_outbox o LEFT JOIN usage_records r ON r.event_id = o.event_id \
         WHERE o.request_id = ANY($1) ORDER BY o.created_at",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await;
    match rows {
        Ok(rows) => ApiResponse::ok(rows).into_response(),
        Err(e) => {
            tracing::warn!("查询 trace 计费流水失败: {e}");
            ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "查询计费流水失败")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_params_skip_empty_and_false() {
        let p = TraceSearchParams {
            request_id: Some("  ".into()),
            service: Some("mazbri-gateway".into()),
            errors_only: Some(false),
            min_duration_ms: Some(1500),
            ..TraceSearchParams::default()
        };
        assert_eq!(
            search_params(&p),
            vec![
                ("service", "mazbri-gateway".to_string()),
                ("min_duration_ms", "1500".to_string())
            ]
        );
    }

    #[test]
    fn request_ids_are_validated() {
        assert!(parse_request_ids("").is_err());
        assert!(parse_request_ids("x").is_err());
        let ok = parse_request_ids("23222057-38d7-48ed-be6c-22e7eefa9add, ").expect("valid");
        assert_eq!(ok.len(), 1);
        let many = vec!["23222057-38d7-48ed-be6c-22e7eefa9add"; MAX_BILLING_REQUESTS + 1].join(",");
        assert!(parse_request_ids(&many).is_err());
    }
}
