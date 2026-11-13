use crate::call_semantics::{SQL_COUNT_FAILURES, SQL_COUNT_SUCCESSES};
use crate::error::Result;
use crate::greptimedb::{RequestLogAggregationParams, RequestLogQueryParams};
use crate::greptimedb::{
    sanitize_like_value, validate_like_param, validate_numeric_param, validate_uuid_param,
};
use mazbri_core::num::SaturatingFrom;

/// Unix 秒 → `ClickHouse` 字面量，时区显式 UTC。
/// `request_logs.timestamp` 为 `DateTime64(9, 'UTC')`；裸 `toDateTime(unix)` 走服务器时区
///（本环境 `Asia/Shanghai`），与 UTC 列比较会偏 8 小时。
#[must_use]
pub fn ch_datetime_utc(unix_secs: i64) -> String {
    format!("toDateTime({unix_secs}, 'UTC')")
}

#[must_use]
pub fn build_where_clause(conditions: &[String]) -> String {
    if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    }
}

/// Event-target scope only — used as the inner scope for "latest N" universe queries.
#[must_use]
pub fn build_request_log_scope_conditions(params: &RequestLogQueryParams) -> Vec<String> {
    params
        .event_target
        .map(|target| format!("event_target = '{}'", target.as_str()))
        .into_iter()
        .collect()
}

/// Non-scope predicates (time, q, filters, ids, …).
pub fn build_request_log_filter_conditions(params: &RequestLogQueryParams) -> Result<Vec<String>> {
    let mut conditions: Vec<String> = Vec::new();
    if let Some(start_time) = &params.start_time {
        conditions.push(format!(
            "timestamp >= '{}'",
            start_time.format("%Y-%m-%d %H:%M:%S%.3f")
        ));
    }
    if let Some(end_time) = &params.end_time {
        conditions.push(format!(
            "timestamp <= '{}'",
            end_time.format("%Y-%m-%d %H:%M:%S%.3f")
        ));
    }
    if let Some(ref q) = params.q {
        let keyword = sanitize_like_value(q)?;
        let pattern = format!("%{keyword}%");
        conditions.push(format!(
            "(request_id LIKE '{pattern}' OR trace_id LIKE '{pattern}' OR tenant_id LIKE '{pattern}' OR tenant_code LIKE '{pattern}' OR product_id LIKE '{pattern}' OR supplier_code LIKE '{pattern}' OR channel_code LIKE '{pattern}' OR path LIKE '{pattern}' OR client_ip LIKE '{pattern}' OR upstream_host LIKE '{pattern}' OR error_code LIKE '{pattern}')"
        ));
    }
    for filter in &params.filters {
        let cond = crate::list_query::compile_filter_condition(filter).map_err(|e| {
            crate::greptimedb::SanitizationError(format!("{}: {}", e.code, e.message))
        })?;
        conditions.push(cond);
    }
    if let Some(ref request_id) = params.request_id {
        let validated = validate_like_param(request_id)?;
        conditions.push(format!("request_id = '{validated}'"));
    }
    if let Some(ref trace_id) = params.trace_id {
        let validated = validate_like_param(trace_id)?;
        conditions.push(format!("trace_id = '{validated}'"));
    }
    if let Some(ref path_filter) = params.path {
        let validated = validate_like_param(path_filter)?;
        conditions.push(format!("path LIKE '{validated}'"));
    }
    if let Some(ref tenant_id) = params.tenant_id {
        let validated = validate_uuid_param(tenant_id)?;
        conditions.push(format!("tenant_id = '{validated}'"));
    }
    if let Some(ref tenant_code) = params.tenant_code {
        let validated = validate_like_param(tenant_code)?;
        conditions.push(format!("tenant_code = '{validated}'"));
    }
    if let Some(ref product_id) = params.product_id {
        let validated = validate_like_param(product_id)?;
        conditions.push(format!("product_id = '{validated}'"));
    }
    if let Some(ref supplier_id) = params.supplier_id {
        let validated = validate_uuid_param(supplier_id)?;
        conditions.push(format!("supplier_id = '{validated}'"));
    }
    if let Some(ref channel_id) = params.channel_id {
        let validated = validate_uuid_param(channel_id)?;
        conditions.push(format!("channel_id = '{validated}'"));
    }
    if let Some(ref supplier_code) = params.supplier_code {
        let validated = validate_like_param(supplier_code)?;
        conditions.push(format!("supplier_code = '{validated}'"));
    }
    if let Some(ref channel_code) = params.channel_code {
        let validated = validate_like_param(channel_code)?;
        conditions.push(format!("channel_code = '{validated}'"));
    }
    if let Some(ref route_template) = params.route_template {
        let validated = validate_like_param(route_template)?;
        conditions.push(format!("route_template LIKE '{validated}'"));
    }
    if let Some(ref client_ip) = params.client_ip {
        let validated = validate_like_param(client_ip)?;
        conditions.push(format!("client_ip = '{validated}'"));
    }
    if let Some(ref upstream_host) = params.upstream_host {
        let validated = validate_like_param(upstream_host)?;
        conditions.push(format!("upstream_host = '{validated}'"));
    }
    if let Some(upstream_status_code) = params.upstream_status_code {
        conditions.push(format!("upstream_status_code = {upstream_status_code}"));
    }
    if let Some(ref error_code) = params.error_code {
        let validated = validate_like_param(error_code)?;
        conditions.push(format!("error_code = '{validated}'"));
    }
    if let Some(ref error_stage) = params.error_stage {
        let validated = validate_like_param(error_stage)?;
        conditions.push(format!("error_stage = '{validated}'"));
    }
    if let Some(ref response_flags) = params.response_flags {
        let validated = validate_like_param(response_flags)?;
        conditions.push(format!("response_flags = '{validated}'"));
    }
    if let Some(status_code) = params.status_code {
        conditions.push(format!("status_code = {status_code}"));
    }
    Ok(conditions)
}

pub fn build_request_log_conditions(params: &RequestLogQueryParams) -> Result<Vec<String>> {
    let mut conditions = build_request_log_filter_conditions(params)?;
    conditions.extend(build_request_log_scope_conditions(params));
    Ok(conditions)
}

const REQUEST_LOG_SELECT_COLS: &str = "timestamp, `service`, host, instance_id, event_target, message, request_id, trace_id, span_id, parent_span_id, \
         tenant_id, tenant_code, product_id, supplier_id, supplier_code, channel_id, channel_code, attempt_order, method, protocol, host_header, path, route_template, \
         status_code, duration_ms, upstream_host, upstream_status_code, upstream_service_time_ms, dependency_wait_ms, internal_ms, post_response_ms, request_bytes, response_bytes, client_ip, xff, user_agent, cache_hit, fallback_used, retry_count, error_code, error_stage, response_flags, log_file, raw";

#[must_use]
pub fn build_request_logs_count_sql(where_clause: &str) -> String {
    format!("SELECT toUInt64(count()) AS total FROM request_logs FINAL{where_clause}")
}

pub fn build_request_logs_data_sql(
    where_clause: &str,
    limit: usize,
    offset: usize,
) -> Result<String> {
    let limit = validate_numeric_param(i64::saturating_from(limit), 20_000)?;
    let offset = validate_numeric_param(i64::saturating_from(offset), 1_000_000)?;
    Ok(format!(
        "SELECT {REQUEST_LOG_SELECT_COLS} \
         FROM request_logs FINAL{where_clause} ORDER BY timestamp DESC LIMIT {limit} OFFSET {offset}"
    ))
}

/// Latest-N universe: inner takes newest `universe_limit` rows (event-target scoped), outer applies filters.
pub fn build_request_logs_latest_universe_count_sql(
    universe_where: &str,
    outer_where: &str,
    universe_limit: usize,
) -> Result<String> {
    let universe_limit = validate_numeric_param(i64::saturating_from(universe_limit), 20_000)?;
    Ok(format!(
        "SELECT toUInt64(count()) AS total FROM (\
            SELECT {REQUEST_LOG_SELECT_COLS} \
            FROM request_logs FINAL{universe_where} \
            ORDER BY timestamp DESC LIMIT {universe_limit}\
         ) AS latest_universe{outer_where}"
    ))
}

pub fn build_request_logs_latest_universe_data_sql(
    universe_where: &str,
    outer_where: &str,
    universe_limit: usize,
    limit: usize,
    offset: usize,
) -> Result<String> {
    let universe_limit = validate_numeric_param(i64::saturating_from(universe_limit), 20_000)?;
    let limit = validate_numeric_param(i64::saturating_from(limit), 20_000)?;
    let offset = validate_numeric_param(i64::saturating_from(offset), 1_000_000)?;
    Ok(format!(
        "SELECT {REQUEST_LOG_SELECT_COLS} FROM (\
            SELECT {REQUEST_LOG_SELECT_COLS} \
            FROM request_logs FINAL{universe_where} \
            ORDER BY timestamp DESC LIMIT {universe_limit}\
         ) AS latest_universe{outer_where} \
         ORDER BY timestamp DESC LIMIT {limit} OFFSET {offset}"
    ))
}

/// 调用量与错误随时间分布：与明细列表共用同一组过滤条件，保证框选区间计数一致。
/// 错误口径：[`crate::call_semantics::SQL_IS_FAILURE`]（与成功率 SSOT 对账）。
#[must_use]
pub fn build_request_logs_histogram_sql(where_clause: &str, bucket_seconds: u32) -> String {
    format!(
        "SELECT toUnixTimestamp(toStartOfInterval(timestamp, INTERVAL {bucket_seconds} SECOND)) AS ts, \
         toUInt64(count()) AS calls, \
         toUInt64({SQL_COUNT_FAILURES}) AS errors \
         FROM request_logs FINAL{where_clause} GROUP BY ts ORDER BY ts"
    )
}

/// 指定请求的阶段耗时行（`request_stage_logs` 只有 `request_id`，无 `trace_id`）。
pub fn build_request_stages_sql(request_ids: &[String]) -> Result<String> {
    let ids = request_ids
        .iter()
        .map(|id| validate_like_param(id).map(|v| format!("'{v}'")))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .join(", ");
    Ok(format!(
        "SELECT timestamp, `service`, event_target, request_id, attempt_order, stage_name, stage_ms, \
         status_code, error_code, error_stage, toString(upstream_mode) AS upstream_mode, toString(result) AS result \
         FROM request_stage_logs WHERE request_id IN ({ids}) \
         ORDER BY request_id, timestamp, attempt_order LIMIT {MAX_STAGE_ROWS}"
    ))
}

/// 单次阶段查询返回的最大行数。
pub const MAX_STAGE_ROWS: usize = 2_000;

pub fn build_aggregation_conditions(params: &RequestLogAggregationParams) -> Result<Vec<String>> {
    let mut conditions: Vec<String> = Vec::new();
    conditions.push(format!(
        "timestamp >= '{}'",
        params.start_time.format("%Y-%m-%d %H:%M:%S%.3f")
    ));
    conditions.push(format!(
        "timestamp <= '{}'",
        params.end_time.format("%Y-%m-%d %H:%M:%S%.3f")
    ));
    if let Some(ref tenant_id) = params.tenant_id {
        let validated = validate_uuid_param(tenant_id)?;
        conditions.push(format!("tenant_id = '{validated}'"));
    }
    if let Some(target) = params.event_target {
        conditions.push(format!("event_target = '{}'", target.as_str()));
    }
    Ok(conditions)
}

#[must_use]
pub fn build_aggregation_summary_sql(where_clause: &str) -> String {
    format!(
        "SELECT toUInt64(count()) AS total, \
        ifNotFinite(AVG(duration_ms), 0) AS avg_duration_ms, \
        ifNotFinite(AVG(upstream_service_time_ms), 0) AS avg_upstream_service_time_ms, \
        ifNotFinite(AVG(dependency_wait_ms), 0) AS avg_dependency_wait_ms, \
        ifNotFinite(AVG(internal_ms), 0) AS avg_internal_ms, \
        ifNotFinite(AVG(post_response_ms), 0) AS avg_post_response_ms \
        FROM request_logs FINAL{where_clause}"
    )
}

#[must_use]
pub fn build_aggregation_by_route_sql(where_clause: &str) -> String {
    format!(
        "SELECT route_template AS key, toUInt64(count()) AS cnt \
        FROM request_logs FINAL{where_clause} \
        GROUP BY route_template ORDER BY cnt DESC LIMIT 50"
    )
}

#[must_use]
pub fn build_aggregation_by_status_sql(where_clause: &str) -> String {
    format!(
        "SELECT toString(status_code) AS key, toUInt64(count()) AS cnt \
        FROM request_logs FINAL{where_clause} \
        GROUP BY status_code"
    )
}

#[must_use]
pub fn get_time_bucket_expression(group_by: &str) -> String {
    // 日/小时桶显式 UTC，与 DateTime64(..., 'UTC') 列及收入等趋势图口径一致。
    match group_by {
        "hour" => "toStartOfHour(timestamp, 'UTC')".to_string(),
        _ => "toStartOfDay(timestamp, 'UTC')".to_string(),
    }
}

#[must_use]
pub fn build_aggregation_by_time_sql(time_expr: &str, where_clause: &str) -> String {
    format!(
        "SELECT toDateTime({time_expr}, 'UTC') AS time, toUInt64(count()) AS count \
        FROM request_logs FINAL{where_clause} \
        GROUP BY time ORDER BY time"
    )
}

/// Upstream (supplier-side) usage summary, computed on read from the deduplicated access log.
///
/// Only `event_target = 'bypass'` rows are counted: every customer -> upstream hop is logged exactly once
/// on bypass, while the gateway row of the same request would double count it. `FINAL` collapses
/// at-least-once redeliveries (`request_logs` is a `ReplacingMergeTree`), so supplier bills and the
/// reconciliation reference count are exact. `extra_conditions` are joined with `AND` after the event-target filter.
#[must_use]
pub fn build_upstream_usage_summary_sql(extra_conditions: &[String]) -> String {
    let mut conditions = vec!["event_target = 'bypass'".to_string()];
    conditions.extend(extra_conditions.iter().cloned());
    let where_clause = conditions.join(" AND ");
    format!(
        "SELECT \
            supplier_id, supplier_code, channel_id, channel_code, \
            toUInt64(count()) AS total_calls, \
            toUInt64({SQL_COUNT_SUCCESSES}) AS success_calls, \
            toUInt64({SQL_COUNT_FAILURES}) AS failed_calls, \
            toUInt64(sum(request_bytes + response_bytes)) AS total_payload_bytes, \
            toUInt64(sum(duration_ms)) AS total_latency_ms \
         FROM request_logs FINAL \
         WHERE {where_clause} \
         GROUP BY supplier_id, supplier_code, channel_id, channel_code"
    )
}

/// 窗口 `[start, end)`（Unix 秒，UTC）内全量有调用去重 `product_id` 集合（无 `TopN` `LIMIT` 截断）。
#[must_use]
pub const fn build_active_products_sql() -> &'static str {
    "SELECT DISTINCT toString(product_id) AS product_id \
     FROM request_logs \
     WHERE event_target = 'gateway' AND product_id != '' \
       AND timestamp >= toDateTime(toInt64(?), 'UTC') \
       AND timestamp < toDateTime(toInt64(?), 'UTC')"
}

/// 按整点小时（UTC）分桶的上游用量聚合 SQL，时间条件由调用方按左闭右开 `[from, to)` 传入。
#[must_use]
pub fn build_upstream_hourly_usage_sql(extra_conditions: &[String]) -> String {
    let mut conditions = vec!["event_target = 'bypass'".to_string()];
    conditions.extend(extra_conditions.iter().cloned());
    let where_clause = conditions.join(" AND ");
    format!(
        "SELECT \
            toUnixTimestamp(toStartOfHour(timestamp, 'UTC')) AS window_start_unix, \
            supplier_id, supplier_code, channel_id, channel_code, \
            toUInt64(count()) AS total_calls, \
            toUInt64({SQL_COUNT_SUCCESSES}) AS success_calls, \
            toUInt64({SQL_COUNT_FAILURES}) AS failed_calls, \
            toUInt64(sum(request_bytes + response_bytes)) AS total_payload_bytes, \
            toUInt64(sum(duration_ms)) AS total_latency_ms \
         FROM request_logs FINAL \
         WHERE {where_clause} \
         GROUP BY window_start_unix, supplier_id, supplier_code, channel_id, channel_code \
         ORDER BY window_start_unix ASC"
    )
}

#[cfg(test)]
mod upstream_usage_sql_tests {
    use super::{
        build_active_products_sql, build_upstream_hourly_usage_sql,
        build_upstream_usage_summary_sql,
    };

    fn squash(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn reads_deduplicated_bypass_rows_only() {
        let sql = squash(&build_upstream_usage_summary_sql(&[]));
        assert!(sql.contains("FROM request_logs FINAL WHERE event_target = 'bypass' GROUP BY"));
        assert!(!sql.contains("upstream_hourly_usage"));
        assert!(!sql.contains("mazbri-gateway"));
    }

    #[test]
    fn aggregates_calls_bytes_and_latency_per_supplier_channel() {
        let sql = squash(&build_upstream_usage_summary_sql(&[]));
        assert!(sql.contains("toUInt64(count()) AS total_calls"));
        assert!(sql.contains("AS success_calls"));
        assert!(sql.contains("AS failed_calls"));
        assert!(sql.contains("countIf"), "须用 call_semantics countIf: {sql}");
        assert!(sql.contains("toUInt64(sum(request_bytes + response_bytes)) AS total_payload_bytes"));
        assert!(sql.contains("toUInt64(sum(duration_ms)) AS total_latency_ms"));
        assert!(sql.contains("GROUP BY supplier_id, supplier_code, channel_id, channel_code"));
        assert!(!sql.contains("(empty)"));
    }

    #[test]
    fn appends_time_and_scope_filters_after_event_target() {
        let conds = vec![
            "timestamp >= parseDateTimeBestEffortOrNull(?)".to_string(),
            "timestamp <= parseDateTimeBestEffortOrNull(?)".to_string(),
            "supplier_id = 's1'".to_string(),
        ];
        let sql = squash(&build_upstream_usage_summary_sql(&conds));
        assert!(sql.contains(
            "WHERE event_target = 'bypass' AND timestamp >= parseDateTimeBestEffortOrNull(?) \
             AND timestamp <= parseDateTimeBestEffortOrNull(?) AND supplier_id = 's1' GROUP BY"
        ));
    }

    #[test]
    fn hourly_usage_groups_by_utc_hour_and_deduplicates_bypass() {
        let conds = vec![
            "timestamp >= toDateTime(toInt64(?), 'UTC')".to_string(),
            "timestamp < toDateTime(toInt64(?), 'UTC')".to_string(),
        ];
        let sql = squash(&build_upstream_hourly_usage_sql(&conds));
        assert!(sql.contains(
            "toUnixTimestamp(toStartOfHour(timestamp, 'UTC')) AS window_start_unix"
        ));
        assert!(sql.contains(
            "FROM request_logs FINAL WHERE event_target = 'bypass' \
             AND timestamp >= toDateTime(toInt64(?), 'UTC') \
             AND timestamp < toDateTime(toInt64(?), 'UTC') \
             GROUP BY window_start_unix, supplier_id, supplier_code, channel_id, channel_code"
        ));
    }

    #[test]
    fn active_products_sql_has_no_topn_limit_and_uses_half_open_window() {
        let sql = squash(build_active_products_sql());
        assert!(sql.contains("SELECT DISTINCT toString(product_id) AS product_id FROM request_logs"));
        assert!(sql.contains("event_target = 'gateway'"));
        assert!(sql.contains("timestamp >= toDateTime(toInt64(?), 'UTC')"));
        assert!(sql.contains("timestamp < toDateTime(toInt64(?), 'UTC')"));
        assert!(!sql.to_uppercase().contains("LIMIT"));
    }
}

#[cfg(test)]
mod call_search_sql_tests {
    use super::{MAX_STAGE_ROWS, build_request_logs_histogram_sql, build_request_stages_sql};

    #[test]
    fn histogram_counts_calls_and_errors_per_bucket() {
        let sql = build_request_logs_histogram_sql(" WHERE event_target = 'gateway'", 60);
        assert!(sql.contains("INTERVAL 60 SECOND"));
        assert!(sql.contains("countIf(NOT ("), "直方图错误须对齐 SQL_IS_FAILURE: {sql}");
        assert!(!sql.contains("(empty)"));
        assert!(sql.contains("FROM request_logs FINAL WHERE event_target = 'gateway' GROUP BY ts"));
    }

    #[test]
    fn stages_sql_lists_ids_and_rejects_quotes() {
        let ids = vec!["a-1".to_string(), "b-2".to_string()];
        let sql = build_request_stages_sql(&ids).unwrap_or_default();
        assert!(sql.contains("request_id IN ('a-1', 'b-2')"));
        assert!(sql.contains(&format!("LIMIT {MAX_STAGE_ROWS}")));
        assert!(build_request_stages_sql(&["x' OR 1=1".to_string()]).is_err());
    }
}


#[cfg(test)]
mod time_bucket_tests {
    use super::{ch_datetime_utc, get_time_bucket_expression};

    #[test]
    fn time_bucket_expression_is_explicit_utc() {
        assert_eq!(get_time_bucket_expression("hour"), "toStartOfHour(timestamp, 'UTC')");
        assert_eq!(get_time_bucket_expression("day"), "toStartOfDay(timestamp, 'UTC')");
        assert_eq!(ch_datetime_utc(1_700_000_000), "toDateTime(1700000000, 'UTC')");
    }
}
