use super::client::ClickHouseClient;
use super::queries::{
    build_aggregation_by_route_sql, build_aggregation_by_status_sql, build_aggregation_by_time_sql,
    build_aggregation_conditions, build_aggregation_summary_sql, build_audit_analytics_conditions,
    build_audit_analytics_sql, build_audit_log_conditions, build_audit_log_stats_sql,
    build_audit_logs_count_sql, build_audit_logs_data_sql,
    build_audit_stats_aggregation_conditions, build_request_log_conditions,
    build_request_logs_count_sql, build_request_logs_data_sql, build_where_clause,
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
}

fn classify_audit_stats_results(results: &[AuditAnalyticsResult]) -> StatsQueryDisposition {
    if results.is_empty() {
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
        // Always aggregate from the deduplicated detail table (request_logs FINAL). The former
        // per-minute stats MV counted every at-least-once redelivery and had no `service`
        // dimension (gateway + bypass rows of one request were both counted).
        self.aggregate_request_logs_from_raw(params).await
    }

    async fn aggregate_request_logs_from_raw(
        &self,
        params: &RequestLogAggregationParams,
    ) -> Result<RequestLogAggregation> {
        let conditions = build_aggregation_conditions(params)?;
        let where_clause = format!(" WHERE {}", conditions.join(" AND "));
        let raw_query_plan = RawRequestAggregationPlan::from_params(params);
        debug_assert!(raw_query_plan.query_budget() <= MAX_RAW_FALLBACK_QUERIES);
        tracing::info!(
            query_budget = raw_query_plan.query_budget(),
            include_path_breakdown = raw_query_plan.include_path_breakdown,
            include_status_breakdown = raw_query_plan.include_status_breakdown,
            include_time_breakdown = raw_query_plan.include_time_breakdown,
            "Executing request aggregation on request_logs FINAL"
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
