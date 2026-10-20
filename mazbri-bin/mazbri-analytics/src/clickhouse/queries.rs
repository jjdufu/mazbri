use mazbri_core::num::SaturatingFrom;
use crate::greptimedb::{
    sanitize_like_value, validate_identifier_param, validate_like_param, validate_numeric_param,
    validate_uuid_param,
};
use crate::greptimedb::{
    AuditAnalyticsParams, AuditQueryParams, RequestLogAggregationParams, RequestLogQueryParams,
};
use crate::error::Result;

pub fn build_audit_log_conditions(params: &AuditQueryParams) -> Result<Vec<String>> {
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
    if let Some(admin_id) = params.admin_id {
        conditions.push(format!("admin_id = {admin_id}"));
    }
    if let Some(ref action) = params.action {
        let validated = validate_identifier_param(action)?;
        conditions.push(format!("action = '{validated}'"));
    }
    if let Some(ref resource_type) = params.resource_type {
        let validated = validate_identifier_param(resource_type)?;
        conditions.push(format!("resource_type = '{validated}'"));
    }
    Ok(conditions)
}

#[must_use]
pub fn build_where_clause(conditions: &[String]) -> String {
    if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    }
}

#[must_use]
pub fn build_audit_logs_count_sql(where_clause: &str) -> String {
    format!(
        "SELECT toUInt64(count()) AS total FROM audit_logs{where_clause}"
    )
}

pub fn build_audit_logs_data_sql(
    where_clause: &str,
    limit: usize,
    offset: usize,
) -> Result<String> {
    let limit = validate_numeric_param(i64::saturating_from(limit), 10000)?;
    let offset = validate_numeric_param(i64::saturating_from(offset), 1_000_000)?;
    Ok(format!(
        "SELECT timestamp, admin_id, admin_username, action, resource_type, \
         resource_id, description, ip_address, user_agent, request_id, \
         old_value, new_value \
         FROM audit_logs{where_clause} ORDER BY timestamp DESC LIMIT {limit} OFFSET {offset}"
    ))
}

#[must_use]
pub fn get_time_expression(group_by: &str) -> &'static str {
    match group_by {
        "hour" => "formatDateTime(timestamp, '%Y-%m-%d %H:00')",
        _ => "formatDateTime(timestamp, '%Y-%m-%d')",
    }
}

pub fn build_audit_analytics_conditions(params: &AuditAnalyticsParams) -> Vec<String> {
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
    if let Some(admin_id) = params.admin_id {
        conditions.push(format!("admin_id = {admin_id}"));
    }
    conditions
}

#[must_use]
pub fn build_audit_analytics_sql(time_expr: &str, where_clause: &str) -> String {
    format!(
        "SELECT \
            {time_expr} AS period, \
            action, \
            resource_type, \
            toUInt64(count()) AS count \
        FROM audit_logs{where_clause} \
        GROUP BY period, action, resource_type \
        ORDER BY period DESC, count DESC"
    )
}

/// Service predicates only — used as the inner scope for "latest N" universe queries.
pub fn build_request_log_service_conditions(params: &RequestLogQueryParams) -> Result<Vec<String>> {
    let mut service_preds: Vec<String> = Vec::new();
    if let Some(ref service) = params.service {
        let validated = validate_like_param(service)?;
        service_preds.push(format!("`service` = '{validated}'"));
    }
    if let Some(ref service_like) = params.service_like {
        let validated = validate_like_param(service_like)?;
        service_preds.push(format!("`service` LIKE '{validated}'"));
    }
    if let Some(ref service_in) = params.service_in {
        let mut exact: Vec<String> = Vec::new();
        for part in service_in.split(',') {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            let validated = validate_like_param(trimmed)?;
            exact.push(format!("'{validated}'"));
        }
        if !exact.is_empty() {
            service_preds.push(format!("`service` IN ({})", exact.join(", ")));
        }
    }
    if service_preds.len() <= 1 {
        Ok(service_preds)
    } else {
        Ok(vec![format!("({})", service_preds.join(" OR "))])
    }
}

/// Non-service predicates (time, q, filters, ids, …).
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
    if let Some(ref event_target) = params.event_target {
        let validated = validate_like_param(event_target)?;
        conditions.push(format!("event_target = '{validated}'"));
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
    conditions.extend(build_request_log_service_conditions(params)?);
    Ok(conditions)
}

const REQUEST_LOG_SELECT_COLS: &str = "timestamp, `service`, env, host, instance_id, event_target, message, request_id, trace_id, span_id, parent_span_id, \
         tenant_id, tenant_code, product_id, supplier_id, supplier_code, channel_id, channel_code, attempt_order, method, protocol, host_header, path, route_template, \
         status_code, duration_ms, upstream_host, upstream_status_code, upstream_service_time_ms, dependency_wait_ms, internal_ms, post_response_ms, request_bytes, response_bytes, client_ip, xff, user_agent, cache_hit, fallback_used, retry_count, error_code, error_stage, response_flags, log_file, raw";

#[must_use]
pub fn build_request_logs_count_sql(where_clause: &str) -> String {
    format!("SELECT toUInt64(count()) AS total FROM request_logs{where_clause}")
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
         FROM request_logs{where_clause} ORDER BY timestamp DESC LIMIT {limit} OFFSET {offset}"
    ))
}

/// Latest-N universe: inner takes newest `universe_limit` rows (service-scoped), outer applies filters.
pub fn build_request_logs_latest_universe_count_sql(
    universe_where: &str,
    outer_where: &str,
    universe_limit: usize,
) -> Result<String> {
    let universe_limit = validate_numeric_param(i64::saturating_from(universe_limit), 20_000)?;
    Ok(format!(
        "SELECT toUInt64(count()) AS total FROM (\
            SELECT {REQUEST_LOG_SELECT_COLS} \
            FROM request_logs{universe_where} \
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
            FROM request_logs{universe_where} \
            ORDER BY timestamp DESC LIMIT {universe_limit}\
         ) AS latest_universe{outer_where} \
         ORDER BY timestamp DESC LIMIT {limit} OFFSET {offset}"
    ))
}

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
    if let Some(ref service) = params.service {
        let validated = validate_like_param(service)?;
        conditions.push(format!("`service` = '{validated}'"));
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
        FROM request_logs{where_clause}"
    )
}

#[must_use]
pub fn build_aggregation_by_route_sql(where_clause: &str) -> String {
    format!(
        "SELECT route_template AS key, toUInt64(count()) AS cnt \
        FROM request_logs{where_clause} \
        GROUP BY route_template ORDER BY cnt DESC LIMIT 50"
    )
}

#[must_use]
pub fn build_aggregation_by_status_sql(where_clause: &str) -> String {
    format!(
        "SELECT toString(status_code) AS key, toUInt64(count()) AS cnt \
        FROM request_logs{where_clause} \
        GROUP BY status_code"
    )
}

#[must_use]
pub fn get_time_bucket_expression(group_by: &str) -> String {
    match group_by {
        "hour" => "toStartOfHour(timestamp)".to_string(),
        _ => "toStartOfDay(timestamp)".to_string(),
    }
}

/// Time bucket expression for `request_log_stats` / `audit_log_stats`
/// (column is `time_window`, not `timestamp`).
#[must_use]
pub fn get_stats_time_bucket_expression(group_by: &str) -> String {
    match group_by {
        "hour" => "toStartOfHour(time_window)".to_string(),
        _ => "toStartOfDay(time_window)".to_string(),
    }
}

#[must_use]
pub fn build_aggregation_by_time_sql(time_expr: &str, where_clause: &str) -> String {
    format!(
        "SELECT toDateTime({time_expr}) AS time, toUInt64(count()) AS count \
        FROM request_logs{where_clause} \
        GROUP BY time ORDER BY time"
    )
}

pub fn build_stats_aggregation_conditions(params: &RequestLogAggregationParams) -> Vec<String> {
    let mut conditions: Vec<String> = Vec::new();
    conditions.push(format!(
        "time_window >= '{}'",
        params.start_time.format("%Y-%m-%d %H:%M:%S")
    ));
    conditions.push(format!(
        "time_window <= '{}'",
        params.end_time.format("%Y-%m-%d %H:%M:%S")
    ));
    conditions
}

#[must_use]
pub fn build_request_log_stats_summary_sql(where_clause: &str) -> String {
    format!(
        "SELECT \
            toUInt64(ifNull(SUM(total_count), 0)) AS total, \
            ifNull(SUM(sum_duration_ms) / NULLIF(SUM(total_count), 0), 0) AS avg_duration_ms, \
            ifNull(SUM(sum_upstream_service_time_ms) / NULLIF(SUM(total_count), 0), 0) AS avg_upstream_service_time_ms, \
            ifNull(SUM(sum_dependency_wait_ms) / NULLIF(SUM(total_count), 0), 0) AS avg_dependency_wait_ms, \
            ifNull(SUM(sum_internal_ms) / NULLIF(SUM(total_count), 0), 0) AS avg_internal_ms, \
            ifNull(SUM(sum_post_response_ms) / NULLIF(SUM(total_count), 0), 0) AS avg_post_response_ms \
         FROM request_log_stats{where_clause}"
    )
}

#[must_use]
pub fn build_request_log_stats_by_status_sql(where_clause: &str) -> String {
    format!(
        "SELECT toString(status_code) AS key, toUInt64(ifNull(SUM(total_count), 0)) AS cnt \
        FROM request_log_stats{where_clause} \
        GROUP BY status_code"
    )
}

#[must_use]
pub fn build_request_log_route_stats_sql(where_clause: &str) -> String {
    format!(
        "SELECT route_template AS key, toUInt64(ifNull(SUM(total_count), 0)) AS cnt \
        FROM request_log_stats{where_clause} \
        GROUP BY route_template ORDER BY cnt DESC LIMIT 50"
    )
}

pub fn build_audit_stats_aggregation_conditions(params: &AuditAnalyticsParams) -> Vec<String> {
    let mut conditions: Vec<String> = Vec::new();
    if let Some(start_time) = &params.start_time {
        conditions.push(format!(
            "time_window >= '{}'",
            start_time.format("%Y-%m-%d %H:%M:%S")
        ));
    }
    if let Some(end_time) = &params.end_time {
        conditions.push(format!(
            "time_window <= '{}'",
            end_time.format("%Y-%m-%d %H:%M:%S")
        ));
    }
    if let Some(admin_id) = params.admin_id {
        conditions.push(format!("admin_id = {admin_id}"));
    }
    conditions
}

#[must_use]
pub fn build_audit_log_stats_sql(time_expr: &str, where_clause: &str) -> String {
    format!(
        "SELECT \
            {time_expr} AS period, \
            action, \
            resource_type, \
            toUInt64(ifNull(SUM(total_count), 0)) AS count \
        FROM audit_log_stats{where_clause} \
        GROUP BY period, action, resource_type \
        ORDER BY period DESC, count DESC"
    )
}

/// Period expression for audit stats analytics — must return String to match
/// `RowBinary` field `period: String` (same shape as raw audit analytics).
#[must_use]
pub fn get_stats_time_expression(group_by: &str) -> &'static str {
    match group_by {
        "hour" => "formatDateTime(time_window, '%Y-%m-%d %H:00')",
        _ => "formatDateTime(time_window, '%Y-%m-%d')",
    }
}
