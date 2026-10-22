use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::fmt::Write as _;
use tracing::info;
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct AnalyticsClientConfig {
    pub url: String,
    pub connect_timeout_secs: u64,
    pub read_timeout_secs: u64,
    pub write_timeout_secs: u64,
}
impl Default for AnalyticsClientConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            connect_timeout_secs: 5,
            read_timeout_secs: 10,
            write_timeout_secs: 5,
        }
    }
}
pub struct AnalyticsHttpClient {
    client: Client,
    base_url: String,
}
impl AnalyticsHttpClient {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn new(config: &AnalyticsClientConfig) -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(config.connect_timeout_secs))
            .timeout(std::time::Duration::from_secs(config.read_timeout_secs))
            .build()
            .context("创建 AnalyticsHttpClient 失败")?;
        Ok(Self {
            client,
            base_url: config.url.trim_end_matches('/').to_string(),
        })
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn health_check(&self) -> Result<()> {
        let url = format!("{}/health", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("analytics 健康检查请求失败")?;
        if resp.status().is_success() {
            info!("Analytics 服务健康检查通过: {}", self.base_url);
            Ok(())
        } else {
            anyhow::bail!("analytics 健康检查返回非成功状态: {}", resp.status())
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn aggregate_requests(
        &self,
        start_time: &str,
        end_time: &str,
        service: Option<&str>,
    ) -> Result<serde_json::Value> {
        let mut url = format!(
            "{}/api/v1/analytics/requests?start_time={}&end_time={}",
            self.base_url,
            urlencoding::encode(start_time),
            urlencoding::encode(end_time),
        );
        if let Some(svc) = service.map(str::trim).filter(|s| !s.is_empty()) {
            let _ = write!(url, "&service={}", urlencoding::encode(svc));
        }
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("调用 analytics aggregate_requests 失败")?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.context("解析 analytics 响应 JSON 失败")?;
        if status.is_success() {
            Ok(body)
        } else {
            anyhow::bail!("analytics aggregate_requests 返回错误 {status}: {body}")
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn metrics_overview(&self) -> Result<serde_json::Value> {
        self.get_json("/api/v1/metrics/overview", &[]).await
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn metrics_catalog(&self) -> Result<serde_json::Value> {
        self.get_json("/api/v1/metrics/catalog", &[]).await
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn metric_timeseries(
        &self,
        metric: &str,
        start_time: i64,
        end_time: i64,
        step: Option<u64>,
        service: Option<&str>,
    ) -> Result<serde_json::Value> {
        let mut params = vec![
            ("metric", metric.to_string()),
            ("start_time", start_time.to_string()),
            ("end_time", end_time.to_string()),
        ];
        if let Some(step) = step {
            params.push(("step", step.to_string()));
        }
        if let Some(service) = service {
            params.push(("service", service.to_string()));
        }
        self.get_json("/api/v1/metrics/timeseries", &params).await
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn query_request_logs(&self, params: &[(&str, String)]) -> Result<serde_json::Value> {
        self.get_json("/api/v1/request/query", params).await
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
    ) -> Result<serde_json::Value> {
        let mut params = vec![
            ("start_time", start_time.to_string()),
            ("end_time", end_time.to_string()),
        ];
        if let Some(sc) = supplier_code {
            params.push(("supplier_code", sc.to_string()));
        }
        if let Some(cc) = channel_code {
            params.push(("channel_code", cc.to_string()));
        }
        if let Some(si) = supplier_id {
            params.push(("supplier_id", si.to_string()));
        }
        if let Some(ci) = channel_id {
            params.push(("channel_id", ci.to_string()));
        }
        self.get_json("/api/v1/analytics/upstream-billing", &params)
            .await
    }

    async fn get_json(&self, path: &str, params: &[(&str, String)]) -> Result<serde_json::Value> {
        let query = params
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    urlencoding::encode(key),
                    urlencoding::encode(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let url = if query.is_empty() {
            format!("{}{}", self.base_url, path)
        } else {
            format!("{}{}?{}", self.base_url, path, query)
        };
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("调用 analytics {path} 失败"))?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.context("解析 analytics 响应 JSON 失败")?;
        if status.is_success() {
            Ok(body)
        } else {
            anyhow::bail!("analytics {path} 返回错误 {status}: {body}")
        }
    }
}
