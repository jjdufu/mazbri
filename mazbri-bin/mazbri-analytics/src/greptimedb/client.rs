use crate::config::GreptimeDBConfig;
use crate::error::{AnalyticsError, Result};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
#[derive(Debug, Deserialize)]
struct GreptimeDBSqlResponse {
    output: Option<Vec<GreptimeDBOutput>>,
}
#[derive(Debug, Deserialize)]
struct GreptimeDBOutput {
    records: Option<GreptimeDBRecords>,
}
#[derive(Debug, Deserialize)]
struct GreptimeDBRecords {
    schema: Option<GreptimeDBSchema>,
    rows: Option<Vec<Vec<serde_json::Value>>>,
}
#[derive(Debug, Deserialize)]
struct GreptimeDBSchema {
    column_schemas: Option<Vec<GreptimeDBColumn>>,
}
#[derive(Debug, Deserialize)]
struct GreptimeDBColumn {
    name: String,
}
#[derive(Debug, Clone)]
pub struct GreptimeDBClient {
    pub(super) http_url: String,
    pub(super) database: String,
    pub(super) user: String,
    pub(super) password: String,
    pub(super) client: Client,
    pub query_timeout_secs: u64,
    pub(super) service_targets: Vec<crate::greptimedb::ServiceMetricTarget>,
}
impl GreptimeDBClient {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn new(config: &GreptimeDBConfig, service_targets: Vec<crate::greptimedb::ServiceMetricTarget>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .gzip(config.compression)
            .build()
            .map_err(|e| AnalyticsError::Config(format!("Failed to create HTTP client: {e}")))?;
        let user = config.user.clone().unwrap_or_else(|| "default".to_string());
        let password = config
            .password
            .clone()
            .unwrap_or_else(|| {
                tracing::warn!(
                    "GreptimeDB password not configured, using empty string -- local deployments may not require a password"
                );
                String::new()
            });
        Ok(Self {
            http_url: config.http_url.trim_end_matches('/').to_string(),
            database: config.database.clone(),
            user,
            password,
            client,
            query_timeout_secs: config.query_timeout_secs,
            service_targets,
        })
    }
    async fn execute_query_columns_rows(
        &self,
        sql: &str,
        error_context: &str,
    ) -> Result<(Vec<String>, Vec<Vec<serde_json::Value>>)> {
        let url = format!("{}/v1/sql?db={}", self.http_url, self.database);
        let response = self
            .client
            .post(&url)
            .basic_auth(&self.user, Some(&self.password))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(format!("sql={}", urlencoding::encode(sql)))
            .send()
            .await
            .map_err(|e| AnalyticsError::GreptimeDB(format!("{error_context} failed: {e}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            return Err(AnalyticsError::GreptimeDB(format!(
                "{error_context} failed: {status} - {error_body}"
            )));
        }
        let body = response.text().await.map_err(|e| {
            AnalyticsError::GreptimeDB(format!("{error_context} response read failed: {e}"))
        })?;
        let resp: GreptimeDBSqlResponse = serde_json::from_str(&body).map_err(|e| {
            AnalyticsError::GreptimeDB(format!("{error_context} JSON parse failed: {e}"))
        })?;
        let output = resp
            .output
            .and_then(|mut o| o.pop())
            .and_then(|o| o.records);
        match output {
            Some(records) => {
                let columns: Vec<String> = records
                    .schema
                    .and_then(|s| s.column_schemas)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|c| c.name)
                    .collect();
                let rows = records.rows.unwrap_or_default();
                Ok((columns, rows))
            }
            None => Ok((vec![], vec![])),
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn execute_query_rows(
        &self,
        sql: &str,
        error_context: &str,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        let (columns, rows) = self.execute_query_columns_rows(sql, error_context).await?;
        let results: Vec<HashMap<String, serde_json::Value>> = rows
            .into_iter()
            .map(|row| {
                columns
                    .iter()
                    .zip(row.iter())
                    .map(|(col, val)| (col.clone(), val.clone()))
                    .collect()
            })
            .collect();
        Ok(results)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn test_connection(&self) -> Result<()> {
        let rows = self
            .execute_query_rows("SELECT 1", "GreptimeDB connection test")
            .await?;
        if rows.is_empty() {
            Err(AnalyticsError::GreptimeDB(
                "GreptimeDB connection test failed: no results returned".to_string(),
            ))
        } else {
            tracing::info!("GreptimeDB connection test successful");
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
            Err(_) => Err(AnalyticsError::GreptimeDB(format!(
                "{} query timed out after {} seconds / \
                 {} 查询在 {} 秒后超时",
                operation_name, self.query_timeout_secs, operation_name, self.query_timeout_secs
            ))),
        }
    }
}
