use crate::analytics_client::AnalyticsHttpClient;
use crate::handlers::types::AnalyticsParams;
use crate::repositories::analytics as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use mazbri_core::num::SaturatingFrom;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use tracing::info;
/// 浏览深度上限（与 analytics `MAX_TOTAL_RESULTS` 一致）。
const MAX_BROWSE_TOTAL: u64 = mazbri_core::limits::MAX_TOTAL_RESULTS;
const USAGE_ANALYTICS_SQL_HOUR: &str = "SELECT DATE_TRUNC('hour', recorded_at) AS period, COUNT(*) AS total_requests, \
     COUNT(DISTINCT api_endpoint) AS unique_endpoints, \
     SUM(billable_units) AS total_units, SUM(billable_amount) AS total_amount, \
     AVG(billing_service_duration_ms) AS avg_billing_service_duration_ms, MIN(billing_service_duration_ms) AS min_billing_service_duration_ms, \
     MAX(billing_service_duration_ms) AS max_billing_service_duration_ms, SUM(request_size + response_size) AS total_bytes \
     FROM usage_records WHERE tenant_id = ";
const USAGE_ANALYTICS_SQL_DAY: &str = "SELECT DATE_TRUNC('day', recorded_at) AS period, COUNT(*) AS total_requests, \
     COUNT(DISTINCT api_endpoint) AS unique_endpoints, \
     SUM(billable_units) AS total_units, SUM(billable_amount) AS total_amount, \
     AVG(billing_service_duration_ms) AS avg_billing_service_duration_ms, MIN(billing_service_duration_ms) AS min_billing_service_duration_ms, \
     MAX(billing_service_duration_ms) AS max_billing_service_duration_ms, SUM(request_size + response_size) AS total_bytes \
     FROM usage_records WHERE tenant_id = ";
const USAGE_ANALYTICS_SQL_WEEK: &str = "SELECT DATE_TRUNC('week', recorded_at) AS period, COUNT(*) AS total_requests, \
     COUNT(DISTINCT api_endpoint) AS unique_endpoints, \
     SUM(billable_units) AS total_units, SUM(billable_amount) AS total_amount, \
     AVG(billing_service_duration_ms) AS avg_billing_service_duration_ms, MIN(billing_service_duration_ms) AS min_billing_service_duration_ms, \
     MAX(billing_service_duration_ms) AS max_billing_service_duration_ms, SUM(request_size + response_size) AS total_bytes \
     FROM usage_records WHERE tenant_id = ";
const USAGE_ANALYTICS_SQL_MONTH: &str = "SELECT DATE_TRUNC('month', recorded_at) AS period, COUNT(*) AS total_requests, \
     COUNT(DISTINCT api_endpoint) AS unique_endpoints, \
     SUM(billable_units) AS total_units, SUM(billable_amount) AS total_amount, \
     AVG(billing_service_duration_ms) AS avg_billing_service_duration_ms, MIN(billing_service_duration_ms) AS min_billing_service_duration_ms, \
     MAX(billing_service_duration_ms) AS max_billing_service_duration_ms, SUM(request_size + response_size) AS total_bytes \
     FROM usage_records WHERE tenant_id = ";
const RATELIMIT_ANALYTICS_SQL_HOUR: &str = "SELECT DATE_TRUNC('hour', created_at) AS period, \
     COUNT(*) AS total_checks, \
     COUNT(*) FILTER (WHERE allowed = true) AS allowed_count, \
     COUNT(*) FILTER (WHERE allowed = false) AS denied_count, \
     COUNT(DISTINCT scope) AS unique_scopes, \
     COUNT(DISTINCT counter_type) AS unique_counter_types \
     FROM rate_limit_events WHERE tenant_id = ";
const RATELIMIT_ANALYTICS_SQL_DAY: &str = "SELECT DATE_TRUNC('day', created_at) AS period, \
     COUNT(*) AS total_checks, \
     COUNT(*) FILTER (WHERE allowed = true) AS allowed_count, \
     COUNT(*) FILTER (WHERE allowed = false) AS denied_count, \
     COUNT(DISTINCT scope) AS unique_scopes, \
     COUNT(DISTINCT counter_type) AS unique_counter_types \
     FROM rate_limit_events WHERE tenant_id = ";
const RATELIMIT_ANALYTICS_SQL_WEEK: &str = "SELECT DATE_TRUNC('week', created_at) AS period, \
     COUNT(*) AS total_checks, \
     COUNT(*) FILTER (WHERE allowed = true) AS allowed_count, \
     COUNT(*) FILTER (WHERE allowed = false) AS denied_count, \
     COUNT(DISTINCT scope) AS unique_scopes, \
     COUNT(DISTINCT counter_type) AS unique_counter_types \
     FROM rate_limit_events WHERE tenant_id = ";
const RATELIMIT_ANALYTICS_SQL_MONTH: &str = "SELECT DATE_TRUNC('month', created_at) AS period, \
     COUNT(*) AS total_checks, \
     COUNT(*) FILTER (WHERE allowed = true) AS allowed_count, \
     COUNT(*) FILTER (WHERE allowed = false) AS denied_count, \
     COUNT(DISTINCT scope) AS unique_scopes, \
     COUNT(DISTINCT counter_type) AS unique_counter_types \
     FROM rate_limit_events WHERE tenant_id = ";
fn select_usage_sql_template(group_by: Option<&str>) -> &'static str {
    match group_by {
        Some("hour") => USAGE_ANALYTICS_SQL_HOUR,
        Some("week") => USAGE_ANALYTICS_SQL_WEEK,
        Some("month") => USAGE_ANALYTICS_SQL_MONTH,
        _ => USAGE_ANALYTICS_SQL_DAY,
    }
}
fn select_ratelimit_sql_template(group_by: Option<&str>) -> &'static str {
    match group_by {
        Some("hour") => RATELIMIT_ANALYTICS_SQL_HOUR,
        Some("week") => RATELIMIT_ANALYTICS_SQL_WEEK,
        Some("month") => RATELIMIT_ANALYTICS_SQL_MONTH,
        _ => RATELIMIT_ANALYTICS_SQL_DAY,
    }
}
/// `GET /stats/ops/inventory`：平台规模（不随时间窗变化）。
pub async fn get_platform_inventory(db: &PgPool) -> axum::response::Response {
    match repo::platform_inventory(db).await {
        Ok((total, enabled, products)) => ApiResponse::ok(serde_json::json!({
            "tenants": { "total": total, "enabled": enabled },
            "products": { "enabled": products },
        }))
        .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "platform inventory failed");
            ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "平台规模查询失败")
        }
    }
}
pub async fn get_usage_analytics(
    db: &PgPool,
    billing_db: &PgPool,
    id_or_code: &str,
    params: &AnalyticsParams,
) -> axum::response::Response {
    info!("Fetching usage analytics for tenant: {}", id_or_code);
    let tenant_id = match crate::services::tenant::resolve_tenant_id(db, id_or_code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    match repo::tenant_exists(db, tenant_id).await {
        Ok(0) => {
            return ApiResponse::err(
                StatusCode::NOT_FOUND,
                format!("Tenant not found: {id_or_code}"),
            );
        }
        Err(e) => {
            tracing::error!(
                tenant_id = % id_or_code, error = % e, "Tenant existence check failed"
            );
            return ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "无法验证租户信息");
        }
        _ => {}
    }
    let sql_template = select_usage_sql_template(params.group_by.as_deref());
    let mut analytics_builder = repo::build_usage_analytics_query(
        sql_template,
        tenant_id,
        params.start_time,
        params.end_time,
    );
    let analytics = match analytics_builder.build().fetch_all(billing_db).await {
        Ok(rows) => rows
            .into_iter()
            .map(|row: sqlx::postgres::PgRow| {
                serde_json::json!(
                    { "period" : row.get::< sqlx::types::chrono::DateTime <
                    sqlx::types::chrono::Utc >, _ > ("period"), "total_requests" :
                    row.get::< i64, _ > ("total_requests"), "unique_endpoints" : row
                    .get::< i64, _ > ("unique_endpoints"), "total_units" : row.get::<
                    i64, _ > ("total_units"), "total_amount" : row.get::<
                    rust_decimal::Decimal, _ > ("total_amount"), "avg_billing_service_duration_ms" :
                    row.get::< Option < f64 >, _ > ("avg_billing_service_duration_ms"),
                    "min_billing_service_duration_ms" : row.get::< Option < i32 >, _ >
                    ("min_billing_service_duration_ms"), "max_billing_service_duration_ms" : row.get::< Option < i32
                    >, _ > ("max_billing_service_duration_ms"), "total_bytes" : row.get::< i64, _ >
                    ("total_bytes"), }
                )
            })
            .collect::<Vec<_>>(),
        Err(e) => {
            tracing::error!(tenant_id = %tenant_id, error = %e, "租户用量统计查询失败");
            return ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "租户用量统计查询失败");
        }
    };
    let mut top_endpoints_builder =
        repo::build_top_endpoints_query(tenant_id, params.start_time, params.end_time);
    let top_endpoints = match top_endpoints_builder.build().fetch_all(billing_db).await {
        Ok(rows) => rows
            .into_iter()
            .map(|row: sqlx::postgres::PgRow| {
                serde_json::json!(
                    { "endpoint" : row.get::< String, _ > ("api_endpoint"), "method"
                    : row.get::< String, _ > ("http_method"), "request_count" : row
                    .get::< i64, _ > ("request_count"), "avg_billing_service_duration_ms" : row
                    .get::< Option < f64 >, _ > ("avg_billing_service_duration_ms"), "total_cost" :
                    row.get::< rust_decimal::Decimal, _ > ("total_cost"), }
                )
            })
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    let mut status_builder =
        repo::build_status_distribution_query(tenant_id, params.start_time, params.end_time);
    let status_distribution = match status_builder.build().fetch_all(billing_db).await {
        Ok(rows) => rows
            .into_iter()
            .map(|row: sqlx::postgres::PgRow| {
                serde_json::json!(
                    { "status" : row.get::< i32, _ > ("response_status"), "count" :
                    row.get::< i64, _ > ("count"), }
                )
            })
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    ApiResponse::ok(serde_json::json!(
        { "time_range" : { "start" : params.start_time, "end" : params.end_time
        }, "group_by" : params.group_by.clone().unwrap_or_else(|| "day"
        .to_string()), "analytics" : analytics, "top_endpoints" : top_endpoints,
        "status_distribution" : status_distribution, }
    ))
    .into_response()
}
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitAnalyticsParams {
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub group_by: Option<String>,
}
/// 拒绝原因 / 范围分布 / 拒绝最多的 API（查询失败时为空列表）。
async fn fetch_ratelimit_breakdowns(
    ratelimit_db: &PgPool,
    tenant_id: uuid::Uuid,
    params: &RateLimitAnalyticsParams,
) -> (
    Vec<serde_json::Value>,
    Vec<serde_json::Value>,
    Vec<serde_json::Value>,
) {
    let mut denial_builder =
        repo::build_denial_reasons_query(tenant_id, params.start_time, params.end_time);
    let denial_reasons = match denial_builder.build().fetch_all(ratelimit_db).await {
        Ok(rows) => rows
            .into_iter()
            .map(|row: sqlx::postgres::PgRow| {
                serde_json::json!(
                    { "reason" : row.get::< Option < String >, _ > ("reason"),
                    "count" : row.get::< i64, _ > ("count"), "percentage" : row
                    .get::< Option < f64 >, _ > ("percentage"), }
                )
            })
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    let mut scope_builder =
        repo::build_scope_breakdown_query(tenant_id, params.start_time, params.end_time);
    let scope_breakdown = match scope_builder.build().fetch_all(ratelimit_db).await {
        Ok(rows) => rows
            .into_iter()
            .map(|row: sqlx::postgres::PgRow| {
                serde_json::json!(
                    { "scope" : row.get::< String, _ > ("scope"), "counter_type" :
                    row.get::< String, _ > ("counter_type"), "total_checks" : row
                    .get::< i64, _ > ("total_checks"), "allowed_count" : row.get::<
                    i64, _ > ("allowed_count"), "denied_count" : row.get::< i64, _ >
                    ("denied_count"), }
                )
            })
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    let mut top_denied_builder =
        repo::build_top_denied_apis_query(tenant_id, params.start_time, params.end_time);
    let top_denied_apis = match top_denied_builder.build().fetch_all(ratelimit_db).await {
        Ok(rows) => rows
            .into_iter()
            .map(|row: sqlx::postgres::PgRow| {
                serde_json::json!(
                    { "api_endpoint" : row.get::< String, _ > ("api_endpoint"),
                    "total_checks" : row.get::< i64, _ > ("total_checks"),
                    "denied_count" : row.get::< i64, _ > ("denied_count"),
                    "denial_rate" : row.get::< f64, _ > ("denial_rate"), }
                )
            })
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    (denial_reasons, scope_breakdown, top_denied_apis)
}

pub async fn get_ratelimit_analytics(
    db: &PgPool,
    ratelimit_db: &PgPool,
    id_or_code: &str,
    params: &RateLimitAnalyticsParams,
) -> axum::response::Response {
    info!("Fetching rate limit analytics for tenant: {}", id_or_code);
    let tenant_id = match crate::services::tenant::resolve_tenant_id(db, id_or_code).await {
        Ok(uuid) => uuid,
        Err(e) => return ApiResponse::err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    match repo::tenant_exists(db, tenant_id).await {
        Ok(0) => {
            return ApiResponse::err(
                StatusCode::NOT_FOUND,
                format!("Tenant not found: {id_or_code}"),
            );
        }
        Err(e) => {
            tracing::error!(
                tenant_id = % id_or_code, error = % e, "Tenant existence check failed"
            );
            return ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "无法验证租户信息");
        }
        _ => {}
    }
    let sql_template = select_ratelimit_sql_template(params.group_by.as_deref());
    let mut analytics_builder = repo::build_ratelimit_analytics_query(
        sql_template,
        tenant_id,
        params.start_time,
        params.end_time,
    );
    let analytics = match analytics_builder.build().fetch_all(ratelimit_db).await {
        Ok(rows) => rows
            .into_iter()
            .map(|row: sqlx::postgres::PgRow| {
                serde_json::json!(
                    { "period" : row.get::< sqlx::types::chrono::DateTime <
                    sqlx::types::chrono::Utc >, _ > ("period"), "total_checks" : row
                    .get::< i64, _ > ("total_checks"), "allowed_count" : row.get::<
                    i64, _ > ("allowed_count"), "denied_count" : row.get::< i64, _ >
                    ("denied_count"), "unique_scopes" : row.get::< i64, _ >
                    ("unique_scopes"), "unique_counter_types" : row.get::< i64, _ >
                    ("unique_counter_types"), }
                )
            })
            .collect::<Vec<_>>(),
        Err(e) => {
            tracing::error!(tenant_id = %tenant_id, error = %e, "租户限流统计查询失败");
            return ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "租户限流统计查询失败");
        }
    };
    let (denial_reasons, scope_breakdown, top_denied_apis) =
        fetch_ratelimit_breakdowns(ratelimit_db, tenant_id, params).await;
    ApiResponse::ok(serde_json::json!(
        { "time_range" : { "start" : params.start_time, "end" : params.end_time
        }, "group_by" : params.group_by.clone().unwrap_or_else(|| "day"
        .to_string()), "analytics" : analytics, "denial_reasons" :
        denial_reasons, "scope_breakdown" : scope_breakdown, "top_denied_apis" :
        top_denied_apis, }
    ))
    .into_response()
}
pub async fn get_ratelimit_platform_stats(
    ratelimit_db: &PgPool,
    params: &RateLimitAnalyticsParams,
) -> axum::response::Response {
    info!("Fetching platform-wide rate limit statistics");
    let mut overall_builder =
        repo::build_ratelimit_platform_overall_query(params.start_time, params.end_time);
    let overall_stats = match overall_builder.build().fetch_one(ratelimit_db).await {
        Ok(row) => {
            let total_checks = row.get::<i64, _>("total_checks");
            let total_denied = row.get::<i64, _>("total_denied");
            serde_json::json!(
                { "total_checks" : total_checks, "total_allowed" : row.get::< i64, _ >
                ("total_allowed"), "total_denied" : total_denied, "active_tenants" : row
                .get::< i64, _ > ("active_tenants"), "overall_deny_rate" : if
                total_checks > 0 { (mazbri_core::num::i64_to_f64(total_denied) / mazbri_core::num::i64_to_f64(total_checks)) * 100.0 }
                else { 0.0 }, }
            )
        }
        Err(_) => serde_json::json!(null),
    };
    let mut top_tenants_builder =
        repo::build_ratelimit_platform_top_tenants_query(params.start_time, params.end_time);
    let top_tenants = match top_tenants_builder.build().fetch_all(ratelimit_db).await {
        Ok(rows) => rows
            .into_iter()
            .map(|row: sqlx::postgres::PgRow| {
                serde_json::json!(
                    { "tenant_id" : row.get::< uuid::Uuid, _ > ("tenant_id"),
                    "tenant_name" : row.get::< Option < String >, _ >
                    ("tenant_name"), "total_checks" : row.get::< i64, _ >
                    ("total_checks"), "denied_count" : row.get::< i64, _ >
                    ("denied_count"), }
                )
            })
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    let mut scope_dist_builder =
        repo::build_ratelimit_platform_scope_dist_query(params.start_time, params.end_time);
    let scope_distribution = match scope_dist_builder.build().fetch_all(ratelimit_db).await {
        Ok(rows) => rows
            .into_iter()
            .map(|row: sqlx::postgres::PgRow| {
                serde_json::json!(
                    { "scope" : row.get::< String, _ > ("scope"), "counter_type" :
                    row.get::< String, _ > ("counter_type"), "total_checks" : row
                    .get::< i64, _ > ("total_checks"), "denied_count" : row.get::<
                    i64, _ > ("denied_count"), }
                )
            })
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    let active_configs = repo::get_active_rate_limit_configs(ratelimit_db).await;
    ApiResponse::ok(serde_json::json!(
        { "time_range" : { "start" : params.start_time, "end" : params.end_time
        }, "overall_stats" : overall_stats, "top_tenants_by_denials" :
        top_tenants, "scope_distribution" : scope_distribution, "active_configs"
        : active_configs, }
    ))
    .into_response()
}
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsTimeseriesParams {
    pub metric: String,
    /// 起始时间（Unix 秒）
    pub start: i64,
    /// 结束时间（Unix 秒）
    pub end: i64,
    pub step: Option<u64>,
    pub service: Option<String>,
}
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallMonitorParams {
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    /// Free-text keyword. Canonical query name is `q`.
    #[serde(default)]
    pub q: Option<String>,
    /// JSON array of `ListFilter` objects (passed through to analytics).
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
    pub serial_no: Option<String>,
    /// Exact trace id (gateway and bypass rows of one call share it).
    #[serde(default)]
    pub trace_id: Option<String>,
    /// Exact error code.
    #[serde(default)]
    pub error_code: Option<String>,
    pub tenant_name: Option<String>,
    pub supplier_code: Option<String>,
    pub channel_code: Option<String>,
    pub client_ip: Option<String>,
    pub status_code: Option<u16>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}
#[derive(Debug, serde::Serialize)]
pub struct CallMonitorItem {
    pub timestamp: serde_json::Value,
    pub serial_no: String,
    pub trace_id: String,
    pub tenant_id: String,
    pub tenant_name: String,
    pub supplier: String,
    pub supplier_name: String,
    pub channel_id: String,
    pub channel_code: String,
    pub channel_name: String,
    pub response_time: serde_json::Value,
    pub client_ip: String,
    pub request_ip: String,
    pub duration_ms: u64,
    pub upstream_service_time_ms: u64,
    pub dependency_wait_ms: u64,
    pub internal_ms: u64,
    pub post_response_ms: u64,
    pub status_code: u16,
    pub response_result: String,
    /// 业务成功（与 analytics `is_success_row` 对齐）。
    pub success: bool,
    /// 失败时的规范化错误码；成功为 `null`（禁止空串 / `(empty)`）。
    pub error_code: Option<String>,
    pub error_stage: String,
    pub response_flags: String,
    pub detail: serde_json::Value,
}
#[derive(Debug, serde::Serialize)]
pub struct CallMonitorResponse {
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
    /// `latest` | `ranged` | `selective` (from analytics).
    #[serde(default)]
    pub query_mode: String,
    /// Latest-universe size when `query_mode` is `latest`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub universe_size: Option<u64>,
    /// total exceeds offset browse depth; pager is capped.
    #[serde(default)]
    pub browse_capped: bool,
    pub data: Vec<CallMonitorItem>,
}
/// 数值型扁平过滤参数（原样透传给 analytics）。
fn push_numeric_filters(
    request_params: &mut Vec<(&'static str, String)>,
    params: &CallMonitorParams,
) {
    let numeric = [
        (
            "duration_ms_gt",
            params.duration_ms_gt.map(|v| v.to_string()),
        ),
        (
            "duration_ms_gte",
            params.duration_ms_gte.map(|v| v.to_string()),
        ),
        (
            "duration_ms_lt",
            params.duration_ms_lt.map(|v| v.to_string()),
        ),
        (
            "duration_ms_lte",
            params.duration_ms_lte.map(|v| v.to_string()),
        ),
        (
            "duration_ms_eq",
            params.duration_ms_eq.map(|v| v.to_string()),
        ),
        (
            "status_code_gt",
            params.status_code_gt.map(|v| v.to_string()),
        ),
        (
            "status_code_gte",
            params.status_code_gte.map(|v| v.to_string()),
        ),
        (
            "status_code_lt",
            params.status_code_lt.map(|v| v.to_string()),
        ),
        (
            "status_code_lte",
            params.status_code_lte.map(|v| v.to_string()),
        ),
        (
            "status_code_neq",
            params.status_code_neq.map(|v| v.to_string()),
        ),
        (
            "upstream_service_time_ms_gt",
            params.upstream_service_time_ms_gt.map(|v| v.to_string()),
        ),
        (
            "upstream_service_time_ms_gte",
            params.upstream_service_time_ms_gte.map(|v| v.to_string()),
        ),
    ];
    for (key, value) in numeric {
        push_opt_param(request_params, key, value);
    }
}

/// 调用监控分页元数据（以 analytics 返回为准，缺省时本地推导）。
struct CallMonitorPaging {
    query_mode: String,
    universe_size: Option<u64>,
    total_pages: u32,
    browse_capped: bool,
}

fn call_monitor_paging(
    body: &serde_json::Value,
    selective: bool,
    page_size: u32,
    total: u64,
    max_page: u32,
) -> CallMonitorPaging {
    let query_mode = body
        .get("query_mode")
        .and_then(|v| v.as_str())
        .unwrap_or(if selective { "selective" } else { "latest" })
        .to_string();
    let latest = query_mode == "latest";
    let universe_size = body
        .get("universe_size")
        .and_then(serde_json::Value::as_u64)
        .or(latest.then_some(MAX_BROWSE_TOTAL));
    let total_pages = body
        .get("total_pages")
        .and_then(serde_json::Value::as_u64)
        .map_or_else(
            || u32::saturating_from(total.div_ceil(u64::from(page_size))),
            u32::saturating_from,
        );
    let total_pages = if latest {
        total_pages
    } else {
        total_pages.min(max_page)
    };
    let browse_capped = body
        .get("browse_capped")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
        || (!latest && total > MAX_BROWSE_TOTAL);
    CallMonitorPaging {
        query_mode,
        universe_size,
        total_pages,
        browse_capped,
    }
}

/// 列表与直方图共用的 analytics 查询参数（同一组过滤条件，框选区间时两者计数一致）。
struct CallMonitorQuery {
    params: Vec<(&'static str, String)>,
    selective: bool,
}

/// 解析租户/渠道名称并组装透传给 analytics 的过滤参数。
///
/// `keep_time_on_selective` 为 false 时，精确 id 查询忽略时间范围（列表全历史定位）。
async fn build_call_monitor_query(
    db: &PgPool,
    kind: &str,
    params: &CallMonitorParams,
    keep_time_on_selective: bool,
) -> Result<CallMonitorQuery, String> {
    // Exact 流水号 / request_id lookup: prefer serial_no, else promote UUID-shaped `q`.
    let serial_no =
        resolve_call_monitor_serial_no(params.serial_no.as_deref(), params.q.as_deref());
    let trace_id = params
        .trace_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    let selective = serial_no.is_some() || trace_id.is_some();
    // Pass time bounds through as-is. Analytics dual-channel:
    // selective → full history; ranged (start/end or timestamp filters) → in-range;
    // else → latest 20k universe. Do not inject a default 24h window.
    let drop_time = selective && !keep_time_on_selective;
    let start_time = if drop_time { None } else { params.start_time };
    let end_time = if drop_time { None } else { params.end_time };
    let tenant_id = resolve_tenant_filter(db, params.tenant_name.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let product_id = resolve_product_filter(db, params.channel_code.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let mut request_params = call_monitor_scope_filters(kind);
    push_opt_param(&mut request_params, "request_id", serial_no);
    push_opt_param(&mut request_params, "trace_id", trace_id);
    // When an exact id is set, skip free-text `q` (same id would only AND a redundant LIKE).
    if !selective {
        push_opt_param(&mut request_params, "q", params.q.clone());
    }
    push_opt_param(&mut request_params, "filters", params.filters.clone());
    push_numeric_filters(&mut request_params, params);
    push_opt_param(&mut request_params, "tenant_id", tenant_id);
    push_opt_param(&mut request_params, "product_id", product_id);
    push_opt_param(&mut request_params, "client_ip", params.client_ip.clone());
    push_opt_param(&mut request_params, "error_code", params.error_code.clone());
    push_opt_param(
        &mut request_params,
        "supplier_code",
        params.supplier_code.clone(),
    );
    push_opt_param(
        &mut request_params,
        "status_code",
        params.status_code.map(|v| v.to_string()),
    );
    push_opt_param(
        &mut request_params,
        "start_time",
        start_time.map(|v| v.to_string()),
    );
    push_opt_param(
        &mut request_params,
        "end_time",
        end_time.map(|v| v.to_string()),
    );
    Ok(CallMonitorQuery {
        params: request_params,
        selective,
    })
}

pub async fn get_call_monitor_logs(
    db: &PgPool,
    analytics_client: &AnalyticsHttpClient,
    kind: &str,
    params: &CallMonitorParams,
) -> axum::response::Response {
    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 1000);
    let CallMonitorQuery {
        params: mut request_params,
        selective,
    } = match build_call_monitor_query(db, kind, params, false).await {
        Ok(query) => query,
        Err(message) => return ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, message),
    };
    request_params.push(("page", page.to_string()));
    request_params.push(("page_size", page_size.to_string()));
    let body = match analytics_client.query_request_logs(&request_params).await {
        Ok(body) => body,
        // Do not mask failures as an empty list with total=0 — that blanks the
        // table and drops pagination (user cannot navigate back).
        Err(e) => return e.into_response("查询调用日志"),
    };
    let data = body
        .get("data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let total = body
        .get("total")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let max_page = u32::saturating_from(MAX_BROWSE_TOTAL.div_ceil(u64::from(page_size)).max(1));
    let CallMonitorPaging {
        query_mode,
        universe_size,
        total_pages,
        browse_capped,
    } = call_monitor_paging(&body, selective, page_size, total, max_page);
    let (tenant_names, channel_names) =
        tokio::join!(load_tenant_names(db, &data), load_channel_names(db, &data));
    let tenant_names = tenant_names.unwrap_or_default();
    let channel_names = channel_names.unwrap_or_default();
    let items = data
        .into_iter()
        .map(|row| map_call_monitor_item(row, &tenant_names, &channel_names, kind))
        .collect::<Vec<_>>();
    ApiResponse::ok(CallMonitorResponse {
        total,
        page,
        page_size,
        total_pages,
        query_mode,
        universe_size,
        browse_capped,
        data: items,
    })
    .into_response()
}
/// 调用直方图：与列表同一组过滤条件；必须给出 `start_time` 与 `end_time`（框选区间）。
pub async fn get_call_histogram(
    db: &PgPool,
    analytics_client: &AnalyticsHttpClient,
    kind: &str,
    params: &CallMonitorParams,
) -> axum::response::Response {
    if params.start_time.is_none() || params.end_time.is_none() {
        return ApiResponse::err(StatusCode::BAD_REQUEST, "直方图需要 start_time 与 end_time");
    }
    let query = match build_call_monitor_query(db, kind, params, true).await {
        Ok(query) => query,
        Err(message) => return ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, message),
    };
    match analytics_client.request_histogram(&query.params).await {
        Ok(body) => ApiResponse::ok(body).into_response(),
        Err(e) => e.into_response("查询调用直方图"),
    }
}

/// 阶段 Tab 查询参数：`request_id` 与 `trace_id` 二选一。
#[derive(Debug, serde::Deserialize)]
pub struct CallStageParams {
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub trace_id: Option<String>,
}

/// 单请求阶段耗时与关联 access 行（gateway + bypass）。
pub async fn get_call_stages(
    analytics_client: &AnalyticsHttpClient,
    params: &CallStageParams,
) -> axum::response::Response {
    let mut request_params = Vec::new();
    push_opt_param(
        &mut request_params,
        "request_id",
        params.request_id.as_deref().map(normalize_serial_no),
    );
    push_opt_param(
        &mut request_params,
        "trace_id",
        params
            .trace_id
            .as_deref()
            .map(str::trim)
            .map(str::to_string),
    );
    if request_params.len() != 1 {
        return ApiResponse::err(StatusCode::BAD_REQUEST, "request_id 与 trace_id 需二选一");
    }
    match analytics_client.request_stages(&request_params).await {
        Ok(body) => ApiResponse::ok(body).into_response(),
        Err(e) => e.into_response("查询阶段耗时"),
    }
}

/// 客户调用：网关写入的 access 行；上游调用：bypass 出站写入的 access 行。
fn call_monitor_scope_filters(kind: &str) -> Vec<(&'static str, String)> {
    match kind {
        "upstream" => vec![("event_target", "bypass".to_string())],
        _ => vec![("event_target", "gateway".to_string())],
    }
}

/// Prefer explicit `serial_no`; otherwise if `q` is a full UUID (hyphenated or 32-hex), treat as exact id.
fn resolve_call_monitor_serial_no(serial_no: Option<&str>, q: Option<&str>) -> Option<String> {
    if let Some(raw) = serial_no.map(str::trim).filter(|s| !s.is_empty()) {
        return Some(normalize_serial_no(raw));
    }
    let q = q.map(str::trim).filter(|s| !s.is_empty())?;
    // Only promote when the entire keyword is UUID-shaped (not partial text search).
    match uuid::Uuid::parse_str(q) {
        Ok(id) => Some(id.to_string()),
        Err(_) => None,
    }
}
fn push_opt_param(
    params: &mut Vec<(&'static str, String)>,
    key: &'static str,
    value: Option<String>,
) {
    if let Some(value) = value
        && !value.trim().is_empty()
    {
        params.push((key, value));
    }
}
async fn resolve_tenant_filter(
    db: &PgPool,
    tenant_name: Option<&str>,
) -> Result<Option<String>, sqlx::Error> {
    let Some(value) = tenant_name.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let id = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM tenants WHERE name = $1 OR tenant_id = $1 OR tenant_code = $1 LIMIT 1",
    )
    .bind(value)
    .fetch_optional(db)
    .await?;
    Ok(id.map(|v| v.to_string()))
}
async fn resolve_product_filter(
    db: &PgPool,
    channel_code: Option<&str>,
) -> Result<Option<String>, sqlx::Error> {
    let Some(value) = channel_code.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let product_id = sqlx::query_scalar::<_, String>(
        "SELECT product_id FROM products WHERE product_id = $1 OR name = $1 LIMIT 1",
    )
    .bind(value)
    .fetch_optional(db)
    .await?;
    Ok(product_id)
}
async fn load_tenant_names(
    db: &PgPool,
    rows: &[serde_json::Value],
) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
    let ids = rows
        .iter()
        .filter_map(|row| row.get("tenant_id").and_then(|v| v.as_str()))
        .filter_map(|id| uuid::Uuid::parse_str(id).ok())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let rows = sqlx::query("SELECT id, name FROM tenants WHERE id = ANY($1)")
        .bind(&ids)
        .fetch_all(db)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let id: uuid::Uuid = row.get("id");
            let name: String = row.get("name");
            (id.to_string(), name)
        })
        .collect())
}
#[derive(Debug, Clone, Default)]
struct ChannelDisplayNames {
    supplier_name: String,
    channel_name: String,
}
async fn load_channel_names(
    db: &PgPool,
    rows: &[serde_json::Value],
) -> Result<HashMap<String, ChannelDisplayNames>, sqlx::Error> {
    let channel_ids = rows
        .iter()
        .filter_map(|row| row.get("channel_id").and_then(|value| value.as_str()))
        .filter_map(|value| uuid::Uuid::parse_str(value).ok())
        .collect::<Vec<_>>();
    let channel_codes = rows
        .iter()
        .filter_map(|row| row.get("channel_code").and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();
    let mut result: HashMap<String, ChannelDisplayNames> = HashMap::new();
    if !channel_ids.is_empty() {
        let rows = sqlx::query(
            "SELECT c.id::text AS channel_id, c.channel_code, c.name AS channel_name, s.name AS supplier_name \
             FROM channels c JOIN suppliers s ON s.id = c.supplier_id WHERE c.id = ANY($1)",
        )
        .bind(&channel_ids)
        .fetch_all(db)
        .await?;
        for row in rows {
            let channel_id: String = row.get("channel_id");
            let channel_code: String = row.get("channel_code");
            let channel_name: String = row.get("channel_name");
            let supplier_name: String = row.get("supplier_name");
            let meta = ChannelDisplayNames {
                supplier_name,
                channel_name,
            };
            result.insert(channel_id, meta.clone());
            if !channel_code.trim().is_empty() {
                result.entry(channel_code).or_insert(meta);
            }
        }
    }
    if !channel_codes.is_empty() {
        let rows = sqlx::query(
            "SELECT c.channel_code, c.name AS channel_name, s.name AS supplier_name \
             FROM channels c JOIN suppliers s ON s.id = c.supplier_id WHERE c.channel_code = ANY($1)",
        )
        .bind(&channel_codes)
        .fetch_all(db)
        .await?;
        for row in rows {
            let channel_code: String = row.get("channel_code");
            let channel_name: String = row.get("channel_name");
            let supplier_name: String = row.get("supplier_name");
            result.entry(channel_code).or_insert(ChannelDisplayNames {
                supplier_name,
                channel_name,
            });
        }
    }
    Ok(result)
}
fn map_call_monitor_item(
    row: serde_json::Value,
    tenant_names: &std::collections::HashMap<String, String>,
    channel_names: &HashMap<String, ChannelDisplayNames>,
    _kind: &str,
) -> CallMonitorItem {
    let tenant_id = string_field(&row, "tenant_id");
    let channel_id = string_field(&row, "channel_id");
    let channel_code = string_field(&row, "channel_code");
    let channel_meta = channel_names
        .get(&channel_id)
        .or_else(|| channel_names.get(&channel_code));
    let status_code = u16::saturating_from(u64_field(&row, "status_code"));
    let duration_ms = u64_field(&row, "duration_ms");
    let upstream_service_time_ms = u64_field(&row, "upstream_service_time_ms");
    let dependency_wait_ms = u64_field(&row, "dependency_wait_ms");
    let internal_ms = u64_field(&row, "internal_ms");
    let post_response_ms = u64_field(&row, "post_response_ms");
    let cache_hit = bool_field(&row, "cache_hit");
    let fallback_used = bool_field(&row, "fallback_used");
    let base_channel_name = first_non_empty(&[
        channel_meta
            .as_ref()
            .map(|meta| meta.channel_name.clone())
            .unwrap_or_default(),
        string_field(&row, "channel_name"),
        string_field(&row, "channel_code"),
    ]);
    // Response-cache hit or CBPro fallback: prefix matched route channel for ops visibility.
    let channel_name = with_cb_channel_prefix(base_channel_name, cache_hit, fallback_used);
    let raw_error_code = string_field(&row, "error_code");
    let raw_error_class = string_field(&row, "error_class");
    let (success, error_code) = crate::services::call_semantics::normalize_call_error_code(
        status_code,
        &raw_error_code,
        &raw_error_class,
    );
    CallMonitorItem {
        timestamp: row
            .get("timestamp")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
        // Display-normalize historical simple (32-hex) ids to hyphenated UUID.
        serial_no: normalize_serial_no(&string_field(&row, "request_id")),
        trace_id: string_field(&row, "trace_id"),
        tenant_name: tenant_names.get(&tenant_id).cloned().unwrap_or_default(),
        tenant_id,
        supplier: first_non_empty(&[
            channel_meta
                .as_ref()
                .map(|meta| meta.supplier_name.clone())
                .unwrap_or_default(),
            string_field(&row, "supplier_name"),
            string_field(&row, "supplier_code"),
            string_field(&row, "supplier_id"),
        ]),
        supplier_name: channel_meta
            .as_ref()
            .map(|meta| meta.supplier_name.clone())
            .unwrap_or_default(),
        channel_id,
        channel_code,
        channel_name,
        response_time: row
            .get("timestamp")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
        client_ip: string_field(&row, "client_ip"),
        request_ip: string_field(&row, "client_ip"),
        duration_ms,
        upstream_service_time_ms,
        dependency_wait_ms,
        internal_ms,
        post_response_ms,
        status_code,
        // Access logs store Envoy-style flags as `response_flags` (ok → "-", UF/UT/…).
        // Historical/schema name `response_result` is rarely populated; prefer flags.
        response_result: first_non_empty(&[
            string_field(&row, "response_result"),
            string_field(&row, "response_flags"),
        ]),
        success,
        error_code,
        error_stage: string_field(&row, "error_stage"),
        response_flags: string_field(&row, "response_flags"),
        detail: row,
    }
}
fn string_field(row: &serde_json::Value, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}
fn u64_field(row: &serde_json::Value, key: &str) -> u64 {
    row.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0)
}
fn bool_field(row: &serde_json::Value, key: &str) -> bool {
    row.get(key).and_then(serde_json::Value::as_bool).unwrap_or(false)
}
fn normalize_serial_no(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match uuid::Uuid::parse_str(trimmed) {
        Ok(id) => id.to_string(),
        Err(_) => trimmed.to_string(),
    }
}
/// Prefix channel display for response-cache / CBPro-served calls: `CB:{matched channel}`.
fn with_cb_channel_prefix(base_name: String, cache_hit: bool, fallback_used: bool) -> String {
    if !cache_hit && !fallback_used {
        return base_name;
    }
    let trimmed = base_name.trim();
    if trimmed.is_empty() {
        return "CB".to_string();
    }
    if trimmed.starts_with("CB:") || trimmed.starts_with("CB/") || trimmed == "CB" {
        return trimmed.to_string();
    }
    format!("CB:{trimmed}")
}

fn first_non_empty(values: &[String]) -> String {
    values
        .iter()
        .find(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}
/// Overview：`traffic` 跟 `[start, end]`（60s–31d，不截断），`realtime` / `components` 为当前时刻。
/// 响应经 [`crate::services::analytics_window::apply_overview_field_gate`] 按角色裁剪。
pub async fn get_metrics_overview(
    analytics_client: &AnalyticsHttpClient,
    role: &str,
    params: &MetricsOverviewParams,
) -> axum::response::Response {
    use crate::services::analytics_window::{
        OverviewFieldGate, apply_overview_field_gate, resolve_timeseries_window,
    };

    let window = match resolve_timeseries_window(params.start, params.end) {
        Ok(window) => window,
        Err(message) => return ApiResponse::err(StatusCode::BAD_REQUEST, message),
    };
    let q = [
        ("start", window.start_unix.to_string()),
        ("end", window.end_unix.to_string()),
    ];
    let mut body = match analytics_client.metrics_overview(&q).await {
        Ok(body) => body,
        Err(e) => return e.into_response("指标总览"),
    };
    apply_overview_field_gate(&mut body, OverviewFieldGate::for_role(role));
    ApiResponse::ok(body).into_response()
}

/// `GET /stats/metrics/overview` 参数（Unix 秒，必填）。
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsOverviewParams {
    pub start: i64,
    pub end: i64,
}
pub async fn get_metrics_catalog(
    analytics_client: &AnalyticsHttpClient,
) -> axum::response::Response {
    match analytics_client.metrics_catalog().await {
        Ok(body) => ApiResponse::ok(body).into_response(),
        Err(e) => e.into_response("指标目录"),
    }
}
pub async fn get_services_health(
    analytics_client: &AnalyticsHttpClient,
) -> axum::response::Response {
    match analytics_client.metrics_services_health().await {
        Ok(body) => ApiResponse::ok(body).into_response(),
        Err(e) => e.into_response("服务健康"),
    }
}

/// 金额类 OLAP 模板（成本）：与 analytics `OlapQuery::is_money` 口径一致。
fn olap_is_money(body: &serde_json::Value) -> bool {
    let template = body.get("template").and_then(serde_json::Value::as_str);
    let metric = body
        .get("params")
        .and_then(|p| p.get("metric"))
        .and_then(serde_json::Value::as_str);
    matches!(
        (template, metric),
        (Some("cost_trend"), _) | (Some("top_by_dim"), Some("cost"))
    )
}

pub async fn get_metrics_topology(
    analytics_client: &AnalyticsHttpClient,
    start: Option<i64>,
    end: Option<i64>,
) -> axum::response::Response {
    match analytics_client.metrics_topology(start, end).await {
        Ok(body) => ApiResponse::ok(body).into_response(),
        Err(e) => e.into_response("服务拓扑"),
    }
}

pub async fn olap_query(
    analytics_client: &AnalyticsHttpClient,
    claims: &crate::middleware::auth::AdminClaims,
    body: &serde_json::Value,
) -> axum::response::Response {
    if olap_is_money(body) && !crate::middleware::route_access::BIZ.contains(&claims.role.as_str())
    {
        return ApiResponse::err(
            StatusCode::FORBIDDEN,
            "金额类模板仅 R_ADMIN、R_FINANCE 可查询",
        );
    }
    match analytics_client.olap_query(body).await {
        Ok(data) => ApiResponse::ok(data).into_response(),
        Err(e) => e.into_response("OLAP 查询"),
    }
}

pub async fn get_metric_timeseries(
    analytics_client: &AnalyticsHttpClient,
    params: &MetricsTimeseriesParams,
) -> axum::response::Response {
    use crate::services::analytics_window::resolve_timeseries_window;

    let window = match resolve_timeseries_window(params.start, params.end) {
        Ok(window) => window,
        Err(message) => return ApiResponse::err(StatusCode::BAD_REQUEST, message),
    };
    match analytics_client
        .metric_timeseries(
            &params.metric,
            window.start_unix,
            window.end_unix,
            params.step,
            params.service.as_deref(),
        )
        .await
    {
        Ok(body) => ApiResponse::ok(body).into_response(),
        Err(e) => e.into_response("指标时序"),
    }
}
#[cfg(test)]
mod olap_tests {
    use super::olap_is_money;

    #[test]
    fn money_templates_are_detected() {
        assert!(olap_is_money(
            &serde_json::json!({"template": "cost_trend", "params": {}})
        ));
        assert!(olap_is_money(
            &serde_json::json!({"template": "top_by_dim", "params": {"dim": "tenant_code", "metric": "cost"}})
        ));
        assert!(!olap_is_money(
            &serde_json::json!({"template": "top_by_dim", "params": {"dim": "tenant_code", "metric": "p95"}})
        ));
    }
}
