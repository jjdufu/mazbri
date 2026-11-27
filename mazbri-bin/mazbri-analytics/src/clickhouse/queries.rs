use crate::call_semantics::{SQL_COUNT_FAILURES, SQL_COUNT_SUCCESSES, SQL_IS_FAILURE};
use crate::error::Result;
use crate::greptimedb::SanitizationError;
use crate::greptimedb::{RequestLogAggregationParams, RequestLogQueryParams, validate_numeric_param, validate_uuid_param};
use crate::list_query::{BoundCond, BoundValue, escape_like};
use chrono::{DateTime, Utc};
use mazbri_core::num::SaturatingFrom;

const MAX_PARAM_BYTES: usize = 512;

/// Unix 秒 → `ClickHouse` 字面量，时区显式 UTC。
#[cfg(test)]
#[must_use]
pub fn ch_datetime_utc(unix_secs: i64) -> String {
    format!("toDateTime({unix_secs}, 'UTC')")
}

/// 解析时间字符串（支持整数/小数秒 Unix 时间戳、RFC3339 及无时区 UTC 字符串）。
#[must_use]
pub fn parse_utc_timestamp(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(ts) = s.parse::<i64>() {
        return DateTime::from_timestamp(ts, 0);
    }
    if let Some((secs_part, frac_part)) = s.split_once('.')
        && !secs_part.is_empty()
        && !frac_part.is_empty()
        && frac_part.len() <= 9
        && secs_part.bytes().all(|b| b.is_ascii_digit())
        && frac_part.bytes().all(|b| b.is_ascii_digit())
        && let Ok(secs) = secs_part.parse::<i64>()
    {
        let mut padded = String::with_capacity(9);
        padded.push_str(frac_part);
        while padded.len() < 9 {
            padded.push('0');
        }
        if let Ok(nanos) = padded.parse::<u32>() {
            return DateTime::from_timestamp(secs, nanos);
        }
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return Some(dt);
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(naive.and_utc());
        }
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        && let Some(naive) = date.and_hms_opt(0, 0, 0)
    {
        return Some(naive.and_utc());
    }
    None
}

fn format_utc_ns(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S%.9f").to_string()
}

fn validate_bound_scalar<'a>(field: &str, value: &'a str) -> Result<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SanitizationError(format!("{field} cannot be empty")).into());
    }
    if trimmed.len() > MAX_PARAM_BYTES {
        return Err(SanitizationError(format!("{field} exceeds max length")).into());
    }
    if trimmed.chars().any(char::is_control) {
        return Err(SanitizationError(format!("{field} contains control character")).into());
    }
    Ok(trimmed)
}

/// Escape `\` and `_` for explicit `LIKE` parameters (where caller may pass `%` wildcards),
/// while allowing `?`, `'`, `&`, `=` safely via `.bind(...)`.
fn escape_explicit_like(field: &str, value: &str) -> Result<String> {
    let s = validate_bound_scalar(field, value)?;
    Ok(s.replace('\\', "\\\\").replace('_', "\\_"))
}

#[must_use]
pub fn build_where_clause(conditions: &[BoundCond]) -> BoundCond {
    if conditions.is_empty() {
        return BoundCond::default();
    }
    let mut parts = Vec::with_capacity(conditions.len());
    let mut binds = Vec::new();
    for cond in conditions {
        parts.push(cond.sql.as_str());
        binds.extend(cond.binds.iter().cloned());
    }
    BoundCond {
        sql: format!(" WHERE {}", parts.join(" AND ")),
        binds,
    }
}

/// Event-target scope only — used as the inner scope for "latest N" universe queries.
#[must_use]
pub fn build_request_log_scope_conditions(params: &RequestLogQueryParams) -> Vec<BoundCond> {
    params
        .event_target
        .map(|target| {
            BoundCond::with_binds(
                "event_target = ?",
                vec![BoundValue::Str(target.as_str().to_string())],
            )
        })
        .into_iter()
        .collect()
}

fn push_eq_cond(conditions: &mut Vec<BoundCond>, col: &str, opt: Option<&str>) -> Result<()> {
    if let Some(v) = opt.map(str::trim).filter(|s| !s.is_empty()) {
        let clean = validate_bound_scalar(col, v)?;
        conditions.push(BoundCond::with_binds(
            format!("{col} = ?"),
            vec![BoundValue::Str(clean.to_string())],
        ));
    }
    Ok(())
}

fn push_uuid_cond(conditions: &mut Vec<BoundCond>, col: &str, opt: Option<&str>) -> Result<()> {
    if let Some(v) = opt.map(str::trim).filter(|s| !s.is_empty()) {
        let validated = validate_uuid_param(v)?;
        conditions.push(BoundCond::with_binds(
            format!("{col} = ?"),
            vec![BoundValue::Str(validated.to_string())],
        ));
    }
    Ok(())
}

fn push_like_cond(conditions: &mut Vec<BoundCond>, col: &str, opt: Option<&str>) -> Result<()> {
    if let Some(v) = opt.filter(|s| !s.trim().is_empty()) {
        let pat = escape_explicit_like(col, v)?;
        conditions.push(BoundCond::with_binds(
            format!("{col} LIKE ?"),
            vec![BoundValue::Str(pat)],
        ));
    }
    Ok(())
}

/// Non-scope predicates (time `[start, end)`, `q`, structured filters, ids, …).
pub fn build_request_log_filter_conditions(
    params: &RequestLogQueryParams,
    include_time_bounds: bool,
) -> Result<Vec<BoundCond>> {
    let mut conditions: Vec<BoundCond> = Vec::new();
    if include_time_bounds {
        if let Some(start_time) = &params.start_time {
            conditions.push(BoundCond::with_binds(
                "timestamp >= toDateTime64(?, 9, 'UTC')",
                vec![BoundValue::Str(format_utc_ns(start_time))],
            ));
        }
        if let Some(end_time) = &params.end_time {
            conditions.push(BoundCond::with_binds(
                "timestamp < toDateTime64(?, 9, 'UTC')",
                vec![BoundValue::Str(format_utc_ns(end_time))],
            ));
        }
    }
    if params.errors_only {
        conditions.push(BoundCond::plain(SQL_IS_FAILURE));
    }
    if let Some(ref q) = params.q {
        let raw = validate_bound_scalar("q", q)?;
        let pattern = format!("%{}%", escape_like(raw));
        let cols = [
            "request_id",
            "trace_id",
            "tenant_id",
            "tenant_code",
            "product_id",
            "supplier_code",
            "channel_code",
            "path",
            "client_ip",
            "upstream_host",
            "error_code",
        ];
        let or_sql = cols
            .iter()
            .map(|c| format!("{c} LIKE ?"))
            .collect::<Vec<_>>()
            .join(" OR ");
        let binds = (0..cols.len())
            .map(|_| BoundValue::Str(pattern.clone()))
            .collect();
        conditions.push(BoundCond::with_binds(format!("({or_sql})"), binds));
    }
    for filter in &params.filters {
        let cond = crate::list_query::compile_filter_condition(filter)
            .map_err(|e| SanitizationError(format!("{}: {}", e.code, e.message)))?;
        conditions.push(cond);
    }
    push_eq_cond(&mut conditions, "request_id", params.request_id.as_deref())?;
    push_eq_cond(&mut conditions, "trace_id", params.trace_id.as_deref())?;
    push_like_cond(&mut conditions, "path", params.path.as_deref())?;
    push_uuid_cond(&mut conditions, "tenant_id", params.tenant_id.as_deref())?;
    push_eq_cond(&mut conditions, "tenant_code", params.tenant_code.as_deref())?;
    push_eq_cond(&mut conditions, "product_id", params.product_id.as_deref())?;
    push_uuid_cond(&mut conditions, "supplier_id", params.supplier_id.as_deref())?;
    push_uuid_cond(&mut conditions, "channel_id", params.channel_id.as_deref())?;
    push_eq_cond(&mut conditions, "supplier_code", params.supplier_code.as_deref())?;
    push_eq_cond(&mut conditions, "channel_code", params.channel_code.as_deref())?;
    push_like_cond(&mut conditions, "route_template", params.route_template.as_deref())?;
    push_eq_cond(&mut conditions, "client_ip", params.client_ip.as_deref())?;
    push_eq_cond(&mut conditions, "upstream_host", params.upstream_host.as_deref())?;
    if let Some(upstream_status_code) = params.upstream_status_code {
        conditions.push(BoundCond::with_binds(
            "upstream_status_code = ?",
            vec![BoundValue::U16(upstream_status_code)],
        ));
    }
    push_eq_cond(&mut conditions, "error_code", params.error_code.as_deref())?;
    push_eq_cond(&mut conditions, "error_stage", params.error_stage.as_deref())?;
    push_eq_cond(&mut conditions, "response_flags", params.response_flags.as_deref())?;
    if let Some(status_code) = params.status_code {
        conditions.push(BoundCond::with_binds(
            "status_code = ?",
            vec![BoundValue::U16(status_code)],
        ));
    }
    Ok(conditions)
}

pub fn build_request_log_conditions(params: &RequestLogQueryParams) -> Result<Vec<BoundCond>> {
    let mut conditions = build_request_log_filter_conditions(params, true)?;
    conditions.extend(build_request_log_scope_conditions(params));
    Ok(conditions)
}

const REQUEST_LOG_SELECT_COLS: &str = "timestamp, `service`, host, instance_id, event_target, message, request_id, trace_id, span_id, parent_span_id, \
         tenant_id, tenant_code, product_id, supplier_id, supplier_code, channel_id, channel_code, attempt_order, method, protocol, host_header, path, route_template, \
         status_code, duration_ms, upstream_host, upstream_status_code, upstream_service_time_ms, dependency_wait_ms, internal_ms, post_response_ms, request_bytes, response_bytes, client_ip, xff, user_agent, cache_hit, fallback_used, retry_count, error_code, error_stage, response_flags, \
         error_source, retry_reasons, api_key_id, client_request_id, upstream_request_id, log_file, raw";

#[must_use]
pub fn build_request_logs_count_sql(where_clause: &BoundCond) -> BoundCond {
    BoundCond {
        sql: format!(
            "SELECT toUInt64(count()) AS total FROM request_logs FINAL{}",
            where_clause.sql
        ),
        binds: where_clause.binds.clone(),
    }
}

pub fn build_request_logs_data_sql(
    where_clause: &BoundCond,
    limit: usize,
    offset: usize,
) -> Result<BoundCond> {
    let limit = validate_numeric_param(i64::saturating_from(limit), 20_000)?;
    let offset = validate_numeric_param(i64::saturating_from(offset), 1_000_000)?;
    let mut binds = where_clause.binds.clone();
    binds.push(BoundValue::I64(limit));
    binds.push(BoundValue::I64(offset));
    Ok(BoundCond {
        sql: format!(
            "SELECT {REQUEST_LOG_SELECT_COLS} \
             FROM request_logs FINAL{} ORDER BY timestamp DESC LIMIT ? OFFSET ?",
            where_clause.sql
        ),
        binds,
    })
}

/// Latest-N universe: inner takes newest `universe_limit` rows (event-target scoped), outer applies filters.
pub fn build_request_logs_latest_universe_count_sql(
    universe_where: &BoundCond,
    outer_where: &BoundCond,
    universe_limit: usize,
) -> Result<BoundCond> {
    let universe_limit = validate_numeric_param(i64::saturating_from(universe_limit), 20_000)?;
    let mut binds = universe_where.binds.clone();
    binds.push(BoundValue::I64(universe_limit));
    binds.extend(outer_where.binds.iter().cloned());
    Ok(BoundCond {
        sql: format!(
            "SELECT toUInt64(count()) AS total FROM (\
                SELECT {REQUEST_LOG_SELECT_COLS} \
                FROM request_logs FINAL{} \
                ORDER BY timestamp DESC LIMIT ?\
             ) AS latest_universe{}",
            universe_where.sql, outer_where.sql
        ),
        binds,
    })
}

pub fn build_request_logs_latest_universe_data_sql(
    universe_where: &BoundCond,
    outer_where: &BoundCond,
    universe_limit: usize,
    limit: usize,
    offset: usize,
) -> Result<BoundCond> {
    let universe_limit = validate_numeric_param(i64::saturating_from(universe_limit), 20_000)?;
    let limit = validate_numeric_param(i64::saturating_from(limit), 20_000)?;
    let offset = validate_numeric_param(i64::saturating_from(offset), 1_000_000)?;
    let mut binds = universe_where.binds.clone();
    binds.push(BoundValue::I64(universe_limit));
    binds.extend(outer_where.binds.iter().cloned());
    binds.push(BoundValue::I64(limit));
    binds.push(BoundValue::I64(offset));
    Ok(BoundCond {
        sql: format!(
            "SELECT {REQUEST_LOG_SELECT_COLS} FROM (\
                SELECT {REQUEST_LOG_SELECT_COLS} \
                FROM request_logs FINAL{} \
                ORDER BY timestamp DESC LIMIT ?\
             ) AS latest_universe{} \
             ORDER BY timestamp DESC LIMIT ? OFFSET ?",
            universe_where.sql, outer_where.sql
        ),
        binds,
    })
}

/// 调用量与错误随时间分布：与明细列表共用同一组过滤条件，保证框选区间计数一致。
#[must_use]
pub fn build_request_logs_histogram_sql(where_clause: &BoundCond, bucket_seconds: u32) -> BoundCond {
    let ts_bucket = if bucket_seconds >= 86_400 {
        format!("toStartOfInterval(timestamp, INTERVAL {bucket_seconds} SECOND, 'Asia/Shanghai')")
    } else {
        format!("toStartOfInterval(timestamp, INTERVAL {bucket_seconds} SECOND)")
    };
    BoundCond {
        sql: format!(
            "SELECT toUnixTimestamp({ts_bucket}) AS ts, \
             toUInt64(count()) AS calls, \
             toUInt64({SQL_COUNT_FAILURES}) AS errors \
             FROM request_logs FINAL{} GROUP BY ts ORDER BY ts",
            where_clause.sql
        ),
        binds: where_clause.binds.clone(),
    }
}

/// 指定请求的阶段耗时行（`request_stage_logs` 为 `ReplacingMergeTree`，读加 `FINAL` 并参数绑定）。
pub fn build_request_stages_sql(request_ids: &[String]) -> Result<BoundCond> {
    if request_ids.is_empty() {
        return Err(SanitizationError("request_ids cannot be empty".to_string()).into());
    }
    let mut binds = Vec::with_capacity(request_ids.len());
    for id in request_ids {
        let clean = validate_bound_scalar("request_id", id)?;
        binds.push(BoundValue::Str(clean.to_string()));
    }
    let placeholders = vec!["?"; binds.len()].join(", ");
    Ok(BoundCond {
        sql: format!(
            "SELECT timestamp, `service`, event_target, request_id, attempt_order, stage_name, stage_ms, \
             status_code, error_code, error_stage, toString(upstream_mode) AS upstream_mode, toString(result) AS result \
             FROM request_stage_logs FINAL WHERE request_id IN ({placeholders}) \
             ORDER BY request_id, timestamp, attempt_order LIMIT {MAX_STAGE_ROWS}"
        ),
        binds,
    })
}

/// 单次阶段查询返回的最大行数。
pub const MAX_STAGE_ROWS: usize = 2_000;

pub fn build_aggregation_conditions(params: &RequestLogAggregationParams) -> Result<Vec<BoundCond>> {
    let mut conditions: Vec<BoundCond> = vec![
        BoundCond::with_binds(
            "timestamp >= toDateTime64(?, 9, 'UTC')",
            vec![BoundValue::Str(format_utc_ns(&params.start_time))],
        ),
        BoundCond::with_binds(
            "timestamp < toDateTime64(?, 9, 'UTC')",
            vec![BoundValue::Str(format_utc_ns(&params.end_time))],
        ),
    ];
    if let Some(ref tenant_id) = params.tenant_id
        && !tenant_id.trim().is_empty()
    {
        let validated = validate_uuid_param(tenant_id.trim())?;
        conditions.push(BoundCond::with_binds(
            "tenant_id = ?",
            vec![BoundValue::Str(validated.to_string())],
        ));
    }
    if let Some(target) = params.event_target {
        conditions.push(BoundCond::with_binds(
            "event_target = ?",
            vec![BoundValue::Str(target.as_str().to_string())],
        ));
    }
    Ok(conditions)
}

#[must_use]
pub fn build_aggregation_summary_sql(where_clause: &BoundCond) -> BoundCond {
    BoundCond {
        sql: format!(
            "SELECT toUInt64(count()) AS total, \
            ifNotFinite(AVG(duration_ms), 0) AS avg_duration_ms, \
            ifNotFinite(AVG(upstream_service_time_ms), 0) AS avg_upstream_service_time_ms, \
            ifNotFinite(AVG(dependency_wait_ms), 0) AS avg_dependency_wait_ms, \
            ifNotFinite(AVG(internal_ms), 0) AS avg_internal_ms, \
            ifNotFinite(AVG(post_response_ms), 0) AS avg_post_response_ms \
            FROM request_logs FINAL{}",
            where_clause.sql
        ),
        binds: where_clause.binds.clone(),
    }
}

#[must_use]
pub fn build_aggregation_by_route_sql(where_clause: &BoundCond) -> BoundCond {
    BoundCond {
        sql: format!(
            "SELECT route_template AS key, toUInt64(count()) AS cnt \
            FROM request_logs FINAL{} \
            GROUP BY route_template ORDER BY cnt DESC LIMIT 50",
            where_clause.sql
        ),
        binds: where_clause.binds.clone(),
    }
}

#[must_use]
pub fn build_aggregation_by_status_sql(where_clause: &BoundCond) -> BoundCond {
    BoundCond {
        sql: format!(
            "SELECT toString(status_code) AS key, toUInt64(count()) AS cnt \
            FROM request_logs FINAL{} \
            GROUP BY status_code",
            where_clause.sql
        ),
        binds: where_clause.binds.clone(),
    }
}

#[must_use]
pub fn get_time_bucket_expression(group_by: &str) -> String {
    match group_by {
        "hour" => "toStartOfHour(timestamp, 'UTC')".to_string(),
        _ => "toStartOfDay(timestamp, 'UTC')".to_string(),
    }
}

#[must_use]
pub fn build_aggregation_by_time_sql(time_expr: &str, where_clause: &BoundCond) -> BoundCond {
    BoundCond {
        sql: format!(
            "SELECT toDateTime({time_expr}, 'UTC') AS time, toUInt64(count()) AS count \
            FROM request_logs FINAL{} \
            GROUP BY time ORDER BY time",
            where_clause.sql
        ),
        binds: where_clause.binds.clone(),
    }
}

/// Upstream (supplier-side) usage summary (`request_logs FINAL`, `[start, end)` UTC, `.bind(...)`).
pub fn build_upstream_usage_summary_bound(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    supplier_code: Option<&str>,
    channel_code: Option<&str>,
    supplier_id: Option<&str>,
    channel_id: Option<&str>,
) -> Result<BoundCond> {
    let mut conds = vec![
        "event_target = 'bypass'",
        "timestamp >= toDateTime64(?, 9, 'UTC')",
        "timestamp < toDateTime64(?, 9, 'UTC')",
    ];
    let mut binds = vec![
        BoundValue::Str(format_utc_ns(&start)),
        BoundValue::Str(format_utc_ns(&end)),
    ];
    let mut push_opt = |col: &'static str, raw: Option<&str>| -> Result<()> {
        if let Some(v) = raw.map(str::trim).filter(|s| !s.is_empty()) {
            let clean = validate_bound_scalar(col, v)?;
            conds.push(match col {
                "supplier_code" => "supplier_code = ?",
                "channel_code" => "channel_code = ?",
                "supplier_id" => "supplier_id = ?",
                "channel_id" => "channel_id = ?",
                _ => unreachable!(),
            });
            binds.push(BoundValue::Str(clean.to_string()));
        }
        Ok(())
    };
    push_opt("supplier_code", supplier_code)?;
    push_opt("channel_code", channel_code)?;
    push_opt("supplier_id", supplier_id)?;
    push_opt("channel_id", channel_id)?;
    let where_clause = conds.join(" AND ");
    Ok(BoundCond {
        sql: format!(
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
        ),
        binds,
    })
}

/// 窗口 `[start, end)`（Unix 秒，UTC）内全量有调用去重 `product_id` 集合（`FINAL` 去重，无 `LIMIT` 截断）。
#[must_use]
pub const fn build_active_products_sql() -> &'static str {
    "SELECT DISTINCT toString(product_id) AS product_id \
     FROM request_logs FINAL \
     WHERE event_target = 'gateway' AND product_id != '' \
       AND timestamp >= toDateTime(?, 'UTC') \
       AND timestamp < toDateTime(?, 'UTC')"
}

/// 按整点小时（UTC）分桶的上游用量聚合 SQL（左闭右开 `[start_unix, end_unix)` + 原生 `.bind(...)`）。
pub fn build_upstream_hourly_usage_bound(
    start_unix: i64,
    end_unix: i64,
    supplier_id: Option<&str>,
    channel_id: Option<&str>,
) -> Result<BoundCond> {
    let mut conds = vec![
        "event_target = 'bypass'",
        "timestamp >= toDateTime(?, 'UTC')",
        "timestamp < toDateTime(?, 'UTC')",
    ];
    let mut binds = vec![BoundValue::I64(start_unix), BoundValue::I64(end_unix)];
    if let Some(si) = supplier_id.map(str::trim).filter(|s| !s.is_empty()) {
        let validated = validate_uuid_param(si)?;
        conds.push("supplier_id = ?");
        binds.push(BoundValue::Str(validated.to_string()));
    }
    if let Some(ci) = channel_id.map(str::trim).filter(|s| !s.is_empty()) {
        let validated = validate_uuid_param(ci)?;
        conds.push("channel_id = ?");
        binds.push(BoundValue::Str(validated.to_string()));
    }
    let where_clause = conds.join(" AND ");
    Ok(BoundCond {
        sql: format!(
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
        ),
        binds,
    })
}

#[cfg(test)]
mod upstream_usage_sql_tests {
    use super::{
        BoundValue, build_active_products_sql, build_upstream_hourly_usage_bound,
        build_upstream_usage_summary_bound,
    };
    use chrono::DateTime;

    fn squash(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn reads_deduplicated_bypass_rows_and_binds_backslash_safely() {
        let start = DateTime::from_timestamp(1_700_000_000, 0).expect("start");
        let end = DateTime::from_timestamp(1_700_003_600, 0).expect("end");
        let bound = build_upstream_usage_summary_bound(
            start,
            end,
            Some("test\\"),
            Some("ch?x=1"),
            Some("s1"),
            None,
        )
        .expect("bound");
        let sql = squash(&bound.sql);
        assert!(sql.contains("FROM request_logs FINAL WHERE event_target = 'bypass'"));
        assert!(sql.contains("timestamp >= toDateTime64(?, 9, 'UTC')"));
        assert!(sql.contains("timestamp < toDateTime64(?, 9, 'UTC')"));
        assert!(sql.contains("supplier_code = ? AND channel_code = ? AND supplier_id = ?"));
        assert!(!sql.contains("test\\"));
        assert_eq!(bound.binds.len(), 5);
        assert_eq!(bound.binds[2], BoundValue::Str("test\\".to_string()));
        assert_eq!(bound.binds[3], BoundValue::Str("ch?x=1".to_string()));
    }

    #[test]
    fn hourly_usage_groups_by_utc_hour_and_deduplicates_bypass() {
        let bound = build_upstream_hourly_usage_bound(
            1_700_000_000,
            1_700_003_600,
            Some("00000000-0000-0000-0000-000000000001"),
            None,
        )
        .expect("bound");
        let sql = squash(&bound.sql);
        assert!(sql.contains(
            "toUnixTimestamp(toStartOfHour(timestamp, 'UTC')) AS window_start_unix"
        ));
        assert!(sql.contains(
            "FROM request_logs FINAL WHERE event_target = 'bypass' \
             AND timestamp >= toDateTime(?, 'UTC') \
             AND timestamp < toDateTime(?, 'UTC') \
             AND supplier_id = ? \
             GROUP BY window_start_unix, supplier_id, supplier_code, channel_id, channel_code"
        ));
        assert_eq!(bound.binds.len(), 3);
    }

    #[test]
    fn active_products_sql_has_final_no_topn_limit_and_half_open_window() {
        let sql = squash(build_active_products_sql());
        assert!(sql.contains("SELECT DISTINCT toString(product_id) AS product_id FROM request_logs FINAL"));
        assert!(sql.contains("event_target = 'gateway'"));
        assert!(sql.contains("timestamp >= toDateTime(?, 'UTC')"));
        assert!(sql.contains("timestamp < toDateTime(?, 'UTC')"));
        assert!(!sql.to_uppercase().contains("LIMIT"));
    }
}

#[cfg(test)]
mod call_search_sql_tests {
    use super::{
        BoundCond, BoundValue, MAX_STAGE_ROWS, build_request_logs_histogram_sql,
        build_request_stages_sql,
    };

    #[test]
    fn histogram_counts_calls_and_errors_per_bucket() {
        let where_clause = BoundCond::with_binds(
            " WHERE event_target = ?",
            vec![BoundValue::Str("gateway".to_string())],
        );
        let bound = build_request_logs_histogram_sql(&where_clause, 60);
        assert!(bound.sql.contains("INTERVAL 60 SECOND"));
        assert!(
            bound.sql.contains("countIf(NOT ("),
            "直方图错误须对齐 SQL_IS_FAILURE: {}",
            bound.sql
        );
        assert!(!bound.sql.contains("(empty)"));
        assert!(
            bound
                .sql
                .contains("FROM request_logs FINAL WHERE event_target = ? GROUP BY ts")
        );
    }

    #[test]
    fn stages_sql_binds_ids_and_reads_final() {
        let ids = vec!["a-1".to_string(), "b-2?x='1'\\".to_string()];
        let bound = build_request_stages_sql(&ids).expect("stages");
        assert!(bound.sql.contains("FROM request_stage_logs FINAL WHERE request_id IN (?, ?)"));
        assert!(bound.sql.contains(&format!("LIMIT {MAX_STAGE_ROWS}")));
        assert_eq!(
            bound.binds,
            vec![
                BoundValue::Str("a-1".to_string()),
                BoundValue::Str("b-2?x='1'\\".to_string()),
            ]
        );
        assert!(build_request_stages_sql(&[]).is_err());
        assert!(build_request_stages_sql(&["bad\0id".to_string()]).is_err());
    }
}

#[cfg(test)]
mod time_bucket_tests {
    use super::{ch_datetime_utc, get_time_bucket_expression};

    #[test]
    fn time_bucket_expression_is_explicit_utc() {
        assert_eq!(
            get_time_bucket_expression("hour"),
            "toStartOfHour(timestamp, 'UTC')"
        );
        assert_eq!(
            get_time_bucket_expression("day"),
            "toStartOfDay(timestamp, 'UTC')"
        );
        assert_eq!(
            ch_datetime_utc(1_700_000_000),
            "toDateTime(1700000000, 'UTC')"
        );
    }
}
