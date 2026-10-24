use super::types::TotalRow;
use crate::config::ClickHouseConfig;
use crate::error::{AnalyticsError, Result};
use clickhouse::Client;

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

impl ClickHouseClient {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn new(config: &ClickHouseConfig) -> Result<Self> {
        let mut client = Client::default()
            .with_url(config.http_url.trim_end_matches('/'))
            .with_database(&config.database);

        if let Some(ref user) = config.user {
            client = client.with_user(user);
        }
        if let Some(ref password) = config.password {
            client = client.with_password(password);
        }

        Ok(Self {
            inner: client,
            query_timeout_secs: config.timeout_secs,
        })
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn execute_query<T>(&self, sql: &str) -> Result<Vec<T>>
    where
        T: clickhouse::Row + for<'de> serde::Deserialize<'de>,
    {
        // 剥离 SQL 尾部的 FORMAT JSON（因为 RowBinary 不需要，且追加会导致语法错误）
        let clean_sql = if sql.trim_end().to_lowercase().ends_with("format json") {
            let sql_trimmed = sql.trim_end();
            &sql_trimmed[..sql_trimmed.len() - 11]
        } else {
            sql
        };

        self.inner
            .query(clean_sql)
            .fetch_all::<T>()
            .await
            .map_err(|e| AnalyticsError::ClickHouse(format!("ClickHouse error: {e}")))
    }

    pub(super) async fn execute_count_query_inner(&self, sql: &str) -> Result<u64> {
        let rows = self.execute_query::<TotalRow>(sql).await?;
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

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn with_query_timeout<T>(
        &self,
        operation_name: &str,
        future: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        match tokio::time::timeout(
            std::time::Duration::from_secs(self.query_timeout_secs),
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
        let mut conditions = vec![
            "timestamp >= parseDateTimeBestEffortOrNull(?)".to_string(),
            "timestamp <= parseDateTimeBestEffortOrNull(?)".to_string(),
        ];
        if let Some(sc) = supplier_code
            && !sc.trim().is_empty()
        {
            conditions.push(format!("supplier_code = '{}'", sc.replace('\'', "''")));
        }
        if let Some(cc) = channel_code
            && !cc.trim().is_empty()
        {
            conditions.push(format!("channel_code = '{}'", cc.replace('\'', "''")));
        }
        if let Some(si) = supplier_id
            && !si.trim().is_empty()
        {
            conditions.push(format!("supplier_id = '{}'", si.replace('\'', "''")));
        }
        if let Some(ci) = channel_id
            && !ci.trim().is_empty()
        {
            conditions.push(format!("channel_id = '{}'", ci.replace('\'', "''")));
        }
        let sql = super::queries::build_upstream_usage_summary_sql(&conditions);
        self.inner
            .query(&sql)
            .bind(start_time)
            .bind(end_time)
            .fetch_all::<super::types::UpstreamHourlyUsageRow>()
            .await
            .map_err(|e| {
                AnalyticsError::ClickHouse(format!("ClickHouse upstream summary query error: {e}"))
            })
    }
}
