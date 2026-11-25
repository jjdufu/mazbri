use super::types::TotalRow;
use crate::config::ClickHouseConfig;
use crate::error::{AnalyticsError, Result};
use crate::list_query::{BoundCond, BoundValue};
use chrono::{DateTime, Utc};
use clickhouse::Client;
use std::time::Duration;

#[derive(Clone)]
pub struct ClickHouseClient {
    pub(super) inner: Client,
    pub query_timeout_secs: u64,
}

impl std::fmt::Debug for ClickHouseClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClickHouseClient")
            .field("query_timeout_secs", &self.query_timeout_secs)
            .finish_non_exhaustive()
    }
}

pub(crate) fn bind_query(mut query: clickhouse::query::Query, binds: &[BoundValue]) -> clickhouse::query::Query {
    for val in binds {
        query = match val {
            BoundValue::Str(s) => query.bind(s.as_str()),
            BoundValue::U64(n) => query.bind(*n),
            BoundValue::I64(n) => query.bind(*n),
            BoundValue::U16(n) => query.bind(*n),
            BoundValue::F64(n) => query.bind(*n),
        };
    }
    query
}

fn strip_format_json(sql: &str) -> &str {
    if sql.trim_end().to_lowercase().ends_with("format json") {
        let sql_trimmed = sql.trim_end();
        &sql_trimmed[..sql_trimmed.len() - 11]
    } else {
        sql
    }
}

impl ClickHouseClient {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn new(config: &ClickHouseConfig) -> Result<Self> {
        let timeout_secs = config.timeout_secs.max(1);
        let mut client = Client::default()
            .with_url(config.http_url.trim_end_matches('/'))
            .with_database(&config.database)
            .with_option("max_execution_time", timeout_secs.to_string());

        if let Some(ref user) = config.user {
            client = client.with_user(user);
        }
        if let Some(ref password) = config.password {
            client = client.with_password(password);
        }

        Ok(Self {
            inner: client,
            query_timeout_secs: timeout_secs,
        })
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn execute_query<T>(&self, sql: &str) -> Result<Vec<T>>
    where
        T: clickhouse::Row + for<'de> serde::Deserialize<'de>,
    {
        self.execute_bound_query(sql, &[]).await
    }

    /// 执行带原生 `.bind(...)` 参数绑定的查询，统一挂载超时、指标与追踪 Span。
    ///
    /// # Errors
    ///
    /// `ClickHouse` 查询或超时失败时返回错误。
    pub async fn execute_bound_query<T>(&self, sql: &str, binds: &[BoundValue]) -> Result<Vec<T>>
    where
        T: clickhouse::Row + for<'de> serde::Deserialize<'de>,
    {
        let clean_sql = strip_format_json(sql);
        let started = std::time::Instant::now();
        let query = bind_query(self.inner.query(clean_sql), binds);
        let result = self
            .with_query_timeout("clickhouse_query", async {
                query
                    .fetch_all::<T>()
                    .await
                    .map_err(|e| AnalyticsError::ClickHouse(format!("ClickHouse error: {e}")))
            })
            .await;
        mazbri_core::metrics::observe_dep("ch", "query", started, result.is_ok());
        record_query_span(
            "clickhouse.query",
            "clickhouse",
            clean_sql,
            started,
            result.as_ref().err(),
        );
        result
    }

    pub(super) async fn execute_count_query_inner(&self, bound: &BoundCond) -> Result<u64> {
        let rows = self
            .execute_bound_query::<TotalRow>(&bound.sql, &bound.binds)
            .await?;
        Ok(rows.first().map_or(0, |r| r.total))
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn test_connection(&self) -> Result<()> {
        // ClickHouse types literal `1` as UInt8; RowBinary requires exact width match for u64.
        let rows = self
            .execute_query::<TotalRow>("SELECT toUInt64(1) AS total")
            .await?;
        if rows.is_empty() {
            Err(AnalyticsError::ClickHouse(
                "ClickHouse connection test failed: no results returned".to_string(),
            ))
        } else {
            tracing::info!("ClickHouse connection test successful");
            Ok(())
        }
    }

    /// 执行不返回行的语句（INSERT / DDL）。
    ///
    /// # Errors
    ///
    /// `ClickHouse` 执行失败时返回错误。
    pub async fn execute_sql(&self, sql: &str) -> Result<()> {
        let started = std::time::Instant::now();
        let result = self
            .with_query_timeout("clickhouse_execute", async {
                self.inner
                    .query(sql)
                    .execute()
                    .await
                    .map_err(|e| AnalyticsError::ClickHouse(format!("ClickHouse error: {e}")))
            })
            .await;
        mazbri_core::metrics::observe_dep("ch", "execute", started, result.is_ok());
        record_query_span(
            "clickhouse.execute",
            "clickhouse",
            sql,
            started,
            result.as_ref().err(),
        );
        result
    }

    /// 使用 `RowBinary` 单次批量写入多行到目标表。
    ///
    /// # Errors
    ///
    /// 序列化、`ClickHouse` 写入或超时失败时返回错误。
    pub async fn insert_rows<T>(&self, table: &str, rows: &[T]) -> Result<()>
    where
        T: clickhouse::Row + serde::Serialize + Send + Sync,
    {
        if rows.is_empty() {
            return Ok(());
        }
        let timeout = Duration::from_secs(self.query_timeout_secs);
        let started = std::time::Instant::now();
        let span_sql = format!("INSERT INTO {table}");
        let result = self
            .with_query_timeout("clickhouse_insert", async {
                let mut insert = self
                    .inner
                    .insert::<T>(table)
                    .map_err(|e| AnalyticsError::ClickHouse(format!("ClickHouse insert init error: {e}")))?
                    .with_timeouts(Some(timeout), Some(timeout));
                for row in rows {
                    insert
                        .write(row)
                        .await
                        .map_err(|e| AnalyticsError::ClickHouse(format!("ClickHouse insert write error: {e}")))?;
                }
                insert
                    .end()
                    .await
                    .map_err(|e| AnalyticsError::ClickHouse(format!("ClickHouse insert end error: {e}")))
            })
            .await;
        mazbri_core::metrics::observe_dep("ch", "insert", started, result.is_ok());
        record_query_span(
            "clickhouse.insert",
            "clickhouse",
            &span_sql,
            started,
            result.as_ref().err(),
        );
        result
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn with_query_timeout<T>(
        &self,
        operation_name: &str,
        future: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        match tokio::time::timeout(
            Duration::from_secs(self.query_timeout_secs),
            future,
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(AnalyticsError::ClickHouse(format!(
                "{} query timed out after {} seconds",
                operation_name, self.query_timeout_secs
            ))),
        }
    }

    /// 按左闭右开 UTC 时间窗 `[start, end)` 与原生参数绑定查询上游旁路汇总。
    ///
    /// # Errors
    ///
    /// 时间格式非法、范围反转或 `ClickHouse` 查询失败时返回错误。
    pub async fn fetch_upstream_usage_summary_window(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        supplier_code: Option<&str>,
        channel_code: Option<&str>,
        supplier_id: Option<&str>,
        channel_id: Option<&str>,
    ) -> Result<Vec<super::types::UpstreamHourlyUsageRow>> {
        if start >= end {
            return Err(AnalyticsError::Config(
                "start_time 必须小于 end_time".to_string(),
            ));
        }
        let bound = super::queries::build_upstream_usage_summary_bound(
            start,
            end,
            supplier_code,
            channel_code,
            supplier_id,
            channel_id,
        )?;
        self.with_query_timeout(
            "upstream_usage_summary",
            self.execute_bound_query::<super::types::UpstreamHourlyUsageRow>(
                &bound.sql,
                &bound.binds,
            ),
        )
        .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn fetch_upstream_usage_summary(
        &self,
        start_time: &str,
        end_time: &str,
        supplier_code: Option<&str>,
        channel_code: Option<&str>,
        supplier_id: Option<&str>,
        channel_id: Option<&str>,
    ) -> Result<Vec<super::types::UpstreamHourlyUsageRow>> {
        let start = super::queries::parse_utc_timestamp(start_time)
            .ok_or_else(|| AnalyticsError::Config("start_time 格式无效".to_string()))?;
        let end = super::queries::parse_utc_timestamp(end_time)
            .ok_or_else(|| AnalyticsError::Config("end_time 格式无效".to_string()))?;
        self.fetch_upstream_usage_summary_window(
            start,
            end,
            supplier_code,
            channel_code,
            supplier_id,
            channel_id,
        )
        .await
    }

    /// 查询窗口 `[start_unix, end_unix)` 内全量有调用去重 `product_id` 列表。
    ///
    /// # Errors
    ///
    /// `ClickHouse` 查询失败时返回错误。
    pub async fn fetch_active_product_ids(
        &self,
        start_unix: i64,
        end_unix: i64,
    ) -> Result<Vec<String>> {
        let sql = super::queries::build_active_products_sql();
        let binds = [BoundValue::I64(start_unix), BoundValue::I64(end_unix)];
        let rows = self
            .with_query_timeout(
                "active_products",
                self.execute_bound_query::<super::types::ActiveProductIdRow>(sql, &binds),
            )
            .await?;
        Ok(rows.into_iter().map(|r| r.product_id).collect())
    }

    /// 按整点小时（UTC）分桶查询左闭右开时间窗 `[start_unix, end_unix)` 的上游旁路调用汇总。
    ///
    /// # Errors
    ///
    /// 参数校验或 `ClickHouse` 查询失败时返回错误。
    pub async fn fetch_upstream_hourly_usage(
        &self,
        start_unix: i64,
        end_unix: i64,
        supplier_id: Option<&str>,
        channel_id: Option<&str>,
    ) -> Result<Vec<super::types::UpstreamHourlyBucketEntry>> {
        let bound = super::queries::build_upstream_hourly_usage_bound(
            start_unix,
            end_unix,
            supplier_id,
            channel_id,
        )?;
        let rows = self
            .with_query_timeout(
                "upstream_hourly_usage",
                self.execute_bound_query::<super::types::UpstreamHourlyBucketWireRow>(
                    &bound.sql,
                    &bound.binds,
                ),
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(super::types::UpstreamHourlyBucketEntry::from)
            .collect())
    }
}

/// 补记一次分析库查询的客户端 span（`db.operation`、`db.sql.table`，不记录 SQL 文本）。
pub(crate) fn record_query_span(
    name: &'static str,
    system: &'static str,
    sql: &str,
    started: std::time::Instant,
    error: Option<&AnalyticsError>,
) {
    let (operation, table) = mazbri_core::trace_spans::sql_operation_and_table(sql);
    let mut span = mazbri_core::trace_spans::CompletedSpan::new(name, started.elapsed())
        .client()
        .attr("db.system", system)
        .attr("db.operation", operation);
    if let Some(table) = table {
        span = span.attr("db.sql.table", table);
    }
    if let Some(error) = error {
        span = span.error(error.to_string());
    }
    span.record_under_current();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, extract::Query as AxumQuery, http::StatusCode, routing::any};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn bound_queries_escape_backslashes_and_question_marks_without_placeholder_errors() {
        let captured_queries: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_for_route = Arc::clone(&captured_queries);

        let app = Router::new().route(
            "/",
            any(
                move |AxumQuery(params): AxumQuery<HashMap<String, String>>, body: String| {
                    let captured = Arc::clone(&captured_for_route);
                    async move {
                        let sql = params
                            .get("query")
                            .cloned()
                            .filter(|s| !s.is_empty())
                            .unwrap_or(body);
                        captured.lock().expect("lock").push(sql);
                        // Empty body = 0 rows in RowBinary!
                        (StatusCode::OK, Vec::<u8>::new())
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let client = ClickHouseClient::new(&ClickHouseConfig {
            database: "mazbri_logs".to_string(),
            http_url: format!("http://{addr}"),
            user: None,
            password: None,
            timeout_secs: 5,
        })
        .expect("client");

        // 1. Upstream summary with trailing backslash + single quote + question mark
        let summary_rows = client
            .fetch_upstream_usage_summary(
                "2026-11-01T00:00:00.125Z",
                "2026-11-01T01:00:00.500Z",
                Some("test\\"),
                Some("ch?a=1' OR 1=1--"),
                None,
                None,
            )
            .await
            .expect("parameterized upstream summary must succeed with 0 rows");
        assert!(summary_rows.is_empty());

        // 2. Request log query with `?a=1&b=2`, `%`, `\`, `'` in `q`
        let req_params = crate::greptimedb::RequestLogQueryParams {
            start_time: chrono::DateTime::from_timestamp(1_700_000_000, 0),
            end_time: chrono::DateTime::from_timestamp(1_700_003_600, 0),
            q: Some("/api/v1/pay?a=1&b=2&pct=50%\\quote'".to_string()),
            request_id: None,
            trace_id: None,
            tenant_id: None,
            tenant_code: None,
            product_id: None,
            supplier_id: None,
            supplier_code: None,
            channel_id: None,
            channel_code: None,
            path: Some("/v1/order?id=1\\2".to_string()),
            route_template: None,
            client_ip: None,
            upstream_host: None,
            upstream_status_code: None,
            error_code: None,
            error_stage: None,
            response_flags: None,
            status_code: None,
            event_target: Some(crate::greptimedb::EventTarget::Gateway),
            filters: Vec::new(),
            latest_universe_limit: None,
            errors_only: false,
            limit: 20,
            offset: 0,
        };
        let res = client
            .query_request_logs(&req_params)
            .await
            .expect("query with ? and \\ must succeed");
        assert_eq!(res.total, 0);
        assert!(res.logs.is_empty());

        let queries = captured_queries.lock().expect("lock").clone();
        assert!(queries.len() >= 3);
        let upstream_q = &queries[0];
        assert!(
            upstream_q.contains("supplier_code = 'test\\\\'"),
            "backslash must be escaped in SQL literal: {upstream_q}"
        );
        assert!(
            upstream_q.contains("timestamp >= toDateTime64('2026-11-01 00:00:00.125000000', 9, 'UTC')")
                && upstream_q.contains("timestamp < toDateTime64('2026-11-01 01:00:00.500000000', 9, 'UTC')"),
            "upstream summary must use half-open UTC fractional window: {upstream_q}"
        );
        let search_q = &queries[1];
        assert!(
            search_q.contains("?a=1&b=2"),
            "question mark in bound LIKE value must be preserved without splitting: {search_q}"
        );

        server.abort();
    }
}
