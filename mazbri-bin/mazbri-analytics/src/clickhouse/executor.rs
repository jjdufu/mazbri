use super::client::ClickHouseClient;
use super::queries::{
    build_aggregation_by_route_sql, build_aggregation_by_status_sql, build_aggregation_by_time_sql,
    build_aggregation_conditions, build_aggregation_summary_sql, build_audit_analytics_conditions,
    build_audit_analytics_sql, build_audit_log_conditions, build_audit_log_stats_sql,
    build_audit_logs_count_sql, build_audit_logs_data_sql,
    build_audit_stats_aggregation_conditions, build_request_log_conditions,
    build_request_log_route_stats_sql, build_request_log_stats_by_status_sql,
    build_request_log_stats_summary_sql, build_request_logs_count_sql, build_request_logs_data_sql,
    build_stats_aggregation_conditions, build_where_clause, get_stats_time_bucket_expression,
    get_stats_time_expression, get_time_bucket_expression, get_time_expression,
};
use super::types::{
    AuditAnalyticsRow, AuditLogRow, GroupedCountRow, RequestLogRow, SummaryRow, TimeBucketRow,
};
use crate::error::Result;
use crate::greptimedb::{
    AuditAnalyticsParams, AuditAnalyticsResult, AuditLogEntry, AuditQueryParams, AuditQueryResult,
    RequestLogAggregation, RequestLogAggregationParams, RequestLogEntry, RequestLogQueryParams,
    RequestLogQueryResult,
};
use std::collections::HashMap;

const MAX_RAW_FALLBACK_QUERIES: usize = 4;
const MAX_DEGRADED_RAW_FALLBACK_QUERIES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatsQueryDisposition {
    UseStats,
    EmptyResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawRequestAggregationPlan {
    include_path_breakdown: bool,
    include_status_breakdown: bool,
    include_time_breakdown: bool,
}

impl RawRequestAggregationPlan {
    fn from_params(params: &RequestLogAggregationParams) -> Self {
        Self {
            include_path_breakdown: params.group_by_route,
            include_status_breakdown: true,
            include_time_breakdown: params.group_by_time.is_some(),
        }
    }

    fn query_budget(self) -> usize {
        1 + usize::from(self.include_path_breakdown)
            + usize::from(self.include_status_breakdown)
            + usize::from(self.include_time_breakdown)
    }

    fn degrade_for_request_path(self) -> Self {
        if self.query_budget() <= MAX_DEGRADED_RAW_FALLBACK_QUERIES {
            return self;
        }
        Self {
            include_path_breakdown: false,
            include_status_breakdown: true,
            include_time_breakdown: false,
        }
    }
}

fn classify_audit_stats_results(results: &[AuditAnalyticsResult]) -> StatsQueryDisposition {
    if results.is_empty() {
        StatsQueryDisposition::EmptyResult
    } else {
        StatsQueryDisposition::UseStats
    }
}

fn classify_request_stats_aggregation(agg: &RequestLogAggregation) -> StatsQueryDisposition {
    if agg.total_requests == 0
        && agg.by_route.is_empty()
        && agg.by_status.is_empty()
        && agg.by_time.is_empty()
    {
        StatsQueryDisposition::EmptyResult
    } else {
        StatsQueryDisposition::UseStats
    }
}

impl ClickHouseClient {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn query_audit_logs(&self, params: &AuditQueryParams) -> Result<AuditQueryResult> {
        self.with_query_timeout("audit_logs", self.query_audit_logs_inner(params))
            .await
    }

    async fn query_audit_logs_inner(&self, params: &AuditQueryParams) -> Result<AuditQueryResult> {
        let conditions = build_audit_log_conditions(params)?;
        let where_clause = build_where_clause(&conditions);
        let count_sql = build_audit_logs_count_sql(&where_clause);
        let total = self.execute_count_query_inner(&count_sql).await?;
        let data_sql = build_audit_logs_data_sql(&where_clause, params.limit, params.offset)?;
        let rows = self.execute_query::<AuditLogRow>(&data_sql).await?;
        let logs: Vec<AuditLogEntry> = rows.into_iter().map(Into::into).collect();
        tracing::debug!("Queried {} audit logs (total: {})", logs.len(), total);
        Ok(AuditQueryResult { logs, total })
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn query_audit_analytics(
        &self,
        params: &AuditAnalyticsParams,
    ) -> Result<Vec<AuditAnalyticsResult>> {
        self.with_query_timeout("audit_analytics", self.query_audit_analytics_inner(params))
            .await
    }

    async fn query_audit_analytics_inner(
        &self,
        params: &AuditAnalyticsParams,
    ) -> Result<Vec<AuditAnalyticsResult>> {
        if should_use_raw_audit_analytics(params) {
            tracing::debug!("admin_id filter present, using raw audit log query");
            return self.query_audit_analytics_from_raw(params).await;
        }
        match self.query_audit_analytics_from_stats(params).await {
            Ok(results) => match classify_audit_stats_results(&results) {
                StatsQueryDisposition::UseStats => {
                    tracing::debug!("Using stats tables for audit analytics");
                    Ok(results)
                }
                StatsQueryDisposition::EmptyResult => {
                    tracing::info!("Stats tables returned an empty audit result set");
                    Ok(results)
                }
            },
            Err(error) => {
                tracing::warn!(
                    error = % error,
                    "Stats audit analytics query failed, falling back to raw query"
                );
                self.query_audit_analytics_from_raw(params).await
            }
        }
    }

    async fn query_audit_analytics_from_raw(
        &self,
        params: &AuditAnalyticsParams,
    ) -> Result<Vec<AuditAnalyticsResult>> {
        let time_expr = get_time_expression(&params.group_by);
        let conditions = build_audit_analytics_conditions(params);
        let where_clause = build_where_clause(&conditions);
        let sql = build_audit_analytics_sql(time_expr, &where_clause);
        let rows = self.execute_query::<AuditAnalyticsRow>(&sql).await?;
        let results = rows
            .into_iter()
            .map(|r| AuditAnalyticsResult {
                period: r.period,
                action: r.action,
                resource_type: r.resource_type,
                count: r.count,
            })
            .collect::<Vec<_>>();
        tracing::debug!("Analytics query returned {} results", results.len());
        Ok(results)
    }

    async fn query_audit_analytics_from_stats(
        &self,
        params: &AuditAnalyticsParams,
    ) -> Result<Vec<AuditAnalyticsResult>> {
        let time_expr = get_stats_time_expression(&params.group_by);
        let conditions = build_audit_stats_aggregation_conditions(params);
        let where_clause = build_where_clause(&conditions);
        let sql = build_audit_log_stats_sql(time_expr, &where_clause);
        let rows = self.execute_query::<AuditAnalyticsRow>(&sql).await?;
        let results = rows
            .into_iter()
            .map(|r| AuditAnalyticsResult {
                period: r.period,
                action: r.action,
                resource_type: r.resource_type,
                count: r.count,
            })
            .collect::<Vec<_>>();
        tracing::debug!("Stats audit analytics returned {} results", results.len());
        Ok(results)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn query_request_logs(
        &self,
        params: &RequestLogQueryParams,
    ) -> Result<RequestLogQueryResult> {
        self.with_query_timeout("request_logs", self.query_request_logs_inner(params))
            .await
    }

    async fn query_request_logs_inner(
        &self,
        params: &RequestLogQueryParams,
    ) -> Result<RequestLogQueryResult> {
        let (count_sql, data_sql) = if let Some(universe_limit) = params.latest_universe_limit {
            // Latest-N: service scope → top N by time → then apply search/filters.
            let service_conds =
                crate::clickhouse::queries::build_request_log_service_conditions(params)?;
            let mut filter_conds =
                crate::clickhouse::queries::build_request_log_filter_conditions(params)?;
            // Time bounds must not appear inside latest-universe mode.
            filter_conds
                .retain(|c| !c.starts_with("timestamp >= ") && !c.starts_with("timestamp <= "));
            let universe_where = build_where_clause(&service_conds);
            let outer_where = build_where_clause(&filter_conds);
            (
                crate::clickhouse::queries::build_request_logs_latest_universe_count_sql(
                    &universe_where,
                    &outer_where,
                    universe_limit,
                )?,
                crate::clickhouse::queries::build_request_logs_latest_universe_data_sql(
                    &universe_where,
                    &outer_where,
                    universe_limit,
                    params.limit,
                    params.offset,
                )?,
            )
        } else {
            let conditions = build_request_log_conditions(params)?;
            let where_clause = build_where_clause(&conditions);
            (
                build_request_logs_count_sql(&where_clause),
                build_request_logs_data_sql(&where_clause, params.limit, params.offset)?,
            )
        };
        let total = self.execute_count_query_inner(&count_sql).await?;
        let rows = self.execute_query::<RequestLogRow>(&data_sql).await?;
        let logs: Vec<RequestLogEntry> = rows.into_iter().map(Into::into).collect();
        tracing::debug!("Queried {} request logs (total: {})", logs.len(), total);
        Ok(RequestLogQueryResult { logs, total })
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn aggregate_request_logs(
        &self,
        params: &RequestLogAggregationParams,
    ) -> Result<RequestLogAggregation> {
        self.with_query_timeout(
            "request_log_aggregation",
            self.aggregate_request_logs_inner(params),
        )
        .await
    }

    async fn aggregate_request_logs_inner(
        &self,
        params: &RequestLogAggregationParams,
    ) -> Result<RequestLogAggregation> {
        // Stats MV has no `service` dimension and would double-count gateway+bypass.
        if params.tenant_id.is_some() || params.service.is_some() {
            tracing::debug!(
                tenant = ?params.tenant_id,
                service = ?params.service,
                "tenant/service filter present, using raw request_logs aggregation"
            );
            return self.aggregate_request_logs_from_raw(params, false).await;
        }
        match self.aggregate_request_logs_from_stats(params).await {
            Ok(agg) => match classify_request_stats_aggregation(&agg) {
                StatsQueryDisposition::UseStats => {
                    tracing::debug!("Using stats tables for request log aggregation");
                    Ok(agg)
                }
                StatsQueryDisposition::EmptyResult => {
                    tracing::info!("Stats tables returned an empty request aggregation result");
                    Ok(agg)
                }
            },
            Err(error) => {
                tracing::warn!(
                    error = % error,
                    "Stats request aggregation query failed, falling back to raw query"
                );
                self.aggregate_request_logs_from_raw(params, true).await
            }
        }
    }

    async fn aggregate_request_logs_from_raw(
        &self,
        params: &RequestLogAggregationParams,
        degrade_for_request_path: bool,
    ) -> Result<RequestLogAggregation> {
        let conditions = build_aggregation_conditions(params)?;
        let where_clause = format!(" WHERE {}", conditions.join(" AND "));
        let raw_query_plan = if degrade_for_request_path {
            RawRequestAggregationPlan::from_params(params).degrade_for_request_path()
        } else {
            RawRequestAggregationPlan::from_params(params)
        };
        debug_assert!(raw_query_plan.query_budget() <= MAX_RAW_FALLBACK_QUERIES);
        tracing::info!(
            query_budget = raw_query_plan.query_budget(),
            include_path_breakdown = raw_query_plan.include_path_breakdown,
            include_status_breakdown = raw_query_plan.include_status_breakdown,
            include_time_breakdown = raw_query_plan.include_time_breakdown,
            degraded_for_request_path = degrade_for_request_path,
            "Executing raw request aggregation fallback within query budget"
        );
        let summary_sql = build_aggregation_summary_sql(&where_clause);
        let summary_results = self.execute_query::<SummaryRow>(&summary_sql).await?;
        let summary = summary_results.into_iter().next().unwrap_or_default();

        if summary.total == 0 {
            tracing::info!(
                "Raw request aggregation summary returned zero rows; skipping breakdown queries"
            );
            return Ok(assemble_aggregation(
                params,
                &SummaryRow::default(),
                (HashMap::new(), HashMap::new(), Vec::new()),
            ));
        }

        let by_route = if raw_query_plan.include_path_breakdown {
            let route_sql = build_aggregation_by_route_sql(&where_clause);
            self.query_grouped_counts(&route_sql).await?
        } else {
            HashMap::new()
        };

        let by_status_raw: HashMap<String, u64> = if raw_query_plan.include_status_breakdown {
            let status_sql = build_aggregation_by_status_sql(&where_clause);
            self.query_grouped_counts(&status_sql).await?
        } else {
            HashMap::new()
        };

        let by_status = parse_status_counts(by_status_raw, "");

        let by_time = if raw_query_plan.include_time_breakdown {
            let group_by = params
                .group_by_time
                .as_deref()
                .expect("time breakdown requires group_by_time");
            let time_expr = get_time_bucket_expression(group_by);
            let time_sql = build_aggregation_by_time_sql(&time_expr, &where_clause);
            let time_rows = self.execute_query::<TimeBucketRow>(&time_sql).await?;
            time_rows
                .into_iter()
                .map(TimeBucketRow::into_time_bucket)
                .collect()
        } else {
            Vec::new()
        };

        tracing::debug!(
            "Aggregated request logs: total={}, avg_duration={:.2}ms, avg_upstream_service_time={:.2}ms, avg_dependency_wait={:.2}ms, routes={}, statuses={}, time_buckets={}",
            summary.total,
            summary.avg_duration_ms,
            summary.avg_upstream_service_time_ms,
            summary.avg_dependency_wait_ms,
            by_route.len(),
            by_status.len(),
            by_time.len()
        );

        Ok(assemble_aggregation(
            params,
            &summary,
            (by_route, by_status, by_time),
        ))
    }

    async fn aggregate_request_logs_from_stats(
        &self,
        params: &RequestLogAggregationParams,
    ) -> Result<RequestLogAggregation> {
        let conditions = build_stats_aggregation_conditions(params);
        let where_clause = build_where_clause(&conditions);
        let summary_sql = build_request_log_stats_summary_sql(&where_clause);
        let summary_results = self.execute_query::<SummaryRow>(&summary_sql).await?;
        let summary = summary_results.into_iter().next().unwrap_or_default();

        if summary.total == 0 {
            return Ok(assemble_aggregation(
                params,
                &SummaryRow::default(),
                (HashMap::new(), HashMap::new(), Vec::new()),
            ));
        }

        let by_route = if params.group_by_route {
            let route_sql = build_request_log_route_stats_sql(&where_clause);
            self.query_grouped_counts(&route_sql).await?
        } else {
            HashMap::new()
        };

        let status_sql = build_request_log_stats_by_status_sql(&where_clause);
        let by_status_raw: HashMap<String, u64> = self.query_grouped_counts(&status_sql).await?;
        let by_status = parse_status_counts(by_status_raw, " from stats");

        let by_time = if let Some(ref group_by) = params.group_by_time {
            let time_expr = get_stats_time_bucket_expression(group_by);
            let time_sql = format!(
                "SELECT toDateTime({time_expr}) AS time, toUInt64(ifNull(SUM(total_count), 0)) AS count \
                FROM request_log_stats{where_clause} \
                GROUP BY time ORDER BY time"
            );
            let time_rows = self.execute_query::<TimeBucketRow>(&time_sql).await?;
            time_rows
                .into_iter()
                .map(TimeBucketRow::into_time_bucket)
                .collect()
        } else {
            Vec::new()
        };

        tracing::debug!(
            "Aggregated from stats: total={}, avg_duration={:.2}ms, avg_upstream_service_time={:.2}ms, avg_dependency_wait={:.2}ms",
            summary.total,
            summary.avg_duration_ms,
            summary.avg_upstream_service_time_ms,
            summary.avg_dependency_wait_ms
        );

        Ok(assemble_aggregation(
            params,
            &summary,
            (by_route, by_status, by_time),
        ))
    }

    async fn query_grouped_counts(&self, sql: &str) -> Result<HashMap<String, u64>> {
        let results = self.execute_query::<GroupedCountRow>(sql).await?;
        let mut result = HashMap::new();
        for row in results {
            result.insert(row.key, row.cnt);
        }
        Ok(result)
    }
}

fn should_use_raw_audit_analytics(params: &AuditAnalyticsParams) -> bool {
    params.admin_id.is_some()
}

fn assemble_aggregation(
    params: &RequestLogAggregationParams,
    summary: &SummaryRow,
    (by_route, by_status, by_time): (
        HashMap<String, u64>,
        HashMap<u16, u64>,
        Vec<crate::greptimedb::TimeBucket>,
    ),
) -> RequestLogAggregation {
    RequestLogAggregation {
        period_start: params.start_time,
        period_end: params.end_time,
        total_requests: summary.total,
        avg_duration_ms: summary.avg_duration_ms,
        avg_upstream_service_time_ms: summary.avg_upstream_service_time_ms,
        avg_dependency_wait_ms: summary.avg_dependency_wait_ms,
        avg_internal_ms: summary.avg_internal_ms,
        avg_post_response_ms: summary.avg_post_response_ms,
        by_route,
        by_status,
        by_time,
    }
}

/// 解析状态码分组；非法键最多告警 2 条后汇总。
fn parse_status_counts(raw: HashMap<String, u64>, source: &str) -> HashMap<u16, u64> {
    let mut by_status = HashMap::new();
    let mut invalid_count = 0;
    for (k, v) in raw {
        if let Ok(code) = k.parse::<u16>() {
            by_status.insert(code, v);
        } else {
            invalid_count += 1;
            if invalid_count <= 2 {
                tracing::warn!(key = % k, "Failed to parse status code as u16{source}, skipping");
            }
        }
    }
    if invalid_count > 2 {
        tracing::warn!(
            "... and {} more invalid status codes{source}",
            invalid_count - 2
        );
    }
    by_status
}
