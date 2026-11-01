use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
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
        event_target: &str,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "{}/api/v1/analytics/requests?start_time={}&end_time={}&event_target={}",
            self.base_url,
            urlencoding::encode(start_time),
            urlencoding::encode(end_time),
            urlencoding::encode(event_target),
        );
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
    pub async fn metrics_services_health(&self) -> Result<serde_json::Value> {
        self.get_json("/api/v1/metrics/services/health", &[]).await
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn metric_timeseries(
        &self,
        metric: &str,
        start: i64,
        end: i64,
        step: Option<u64>,
        service: Option<&str>,
    ) -> Result<serde_json::Value> {
        let mut params = vec![
            ("metric", metric.to_string()),
            ("start", start.to_string()),
            ("end", end.to_string()),
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
    pub async fn request_histogram(&self, params: &[(&str, String)]) -> Result<serde_json::Value> {
        self.get_json("/api/v1/request/histogram", params).await
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn request_stages(&self, params: &[(&str, String)]) -> Result<serde_json::Value> {
        self.get_json("/api/v1/request/stages", params).await
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

    /// 转发 OLAP 模板查询，返回 analytics 的 HTTP 状态与 JSON 体（4xx 原样透传给调用方）。
    ///
    /// # Errors
    ///
    /// 网络失败或响应不是 JSON 时返回错误。
    pub async fn olap_query(
        &self,
        body: &serde_json::Value,
    ) -> Result<(reqwest::StatusCode, serde_json::Value)> {
        let path = "/api/v1/olap/query";
        let resp = self
            .client
            .post(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await
            .with_context(|| format!("调用 analytics {path} 失败"))?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.context("解析 analytics 响应 JSON 失败")?;
        Ok((status, body))
    }

    /// trace 检索、详情与服务依赖转发：返回 analytics 的 HTTP 状态与 JSON 体（4xx 由调用方透传）。
    ///
    /// # Errors
    ///
    /// 网络失败或响应不是 JSON 时返回错误。
    pub async fn traces(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<(reqwest::StatusCode, serde_json::Value)> {
        let resp = self
            .client
            .get(self.url_with_query(&format!("/api/v1/traces{path}"), params))
            .send()
            .await
            .with_context(|| format!("调用 analytics /api/v1/traces{path} 失败"))?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.context("解析 analytics 响应 JSON 失败")?;
        Ok((status, body))
    }

    fn url_with_query(&self, path: &str, params: &[(&str, String)]) -> String {
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
        if query.is_empty() {
            format!("{}{}", self.base_url, path)
        } else {
            format!("{}{}?{}", self.base_url, path, query)
        }
    }

    async fn get_json(&self, path: &str, params: &[(&str, String)]) -> Result<serde_json::Value> {
        let url = self.url_with_query(path, params);
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
