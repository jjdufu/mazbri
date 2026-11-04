use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use reqwest::Client;
use serde::Deserialize;
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct AnalyticsClientConfig {
    pub url: String,
    pub connect_timeout_secs: u64,
    pub read_timeout_secs: u64,
    pub write_timeout_secs: u64,
    /// analytics 不可达时返回给前端的 `Retry-After`（秒）。
    pub retry_after_secs: u64,
}
impl Default for AnalyticsClientConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            connect_timeout_secs: 5,
            read_timeout_secs: 10,
            write_timeout_secs: 5,
            retry_after_secs: 5,
        }
    }
}

/// 调用 analytics 的失败分类。`Display` 不含内部 URL，可安全写入日志与持久化字段。
#[derive(Debug, thiserror::Error)]
pub enum AnalyticsError {
    /// 连接失败、超时、DNS 等：analytics 未就绪。
    #[error("analytics 不可达: {source}")]
    Unavailable {
        #[source]
        source: reqwest::Error,
        retry_after_secs: u64,
    },
    /// 响应体不是 JSON（启动中、反代页、空体等）。
    #[error("analytics 响应不是 JSON（HTTP {status}）")]
    BadResponse { status: StatusCode },
    /// analytics 返回了 JSON 错误体（4xx / 5xx）。
    #[error("analytics 返回 HTTP {status}")]
    Upstream {
        status: StatusCode,
        body: serde_json::Value,
    },
}

pub type AnalyticsResult<T> = Result<T, AnalyticsError>;

/// 前端可见的错误码：analytics 不可达。
pub const ANALYTICS_UNAVAILABLE_CODE: &str = "ANALYTICS_UNAVAILABLE";
/// 前端可见的错误码：analytics 响应异常（非 JSON 或 5xx）。
pub const ANALYTICS_BAD_GATEWAY_CODE: &str = "ANALYTICS_BAD_GATEWAY";

impl AnalyticsError {
    /// 映射到 mgmt 对前端的 HTTP 状态。
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::Upstream { status, .. } if status.is_client_error() => *status,
            Self::BadResponse { .. } | Self::Upstream { .. } => StatusCode::BAD_GATEWAY,
        }
    }

    /// 统一的前端响应：不可达 → 503 + `Retry-After`；非 JSON / 5xx → 502；4xx 原样透传。
    /// 只返回中文提示，不带内部 URL 与错误链（细节写日志）。
    #[must_use]
    pub fn into_response(self, what: &str) -> Response {
        let status = self.status();
        match &self {
            Self::Upstream { status: up, body } if up.is_client_error() => {
                tracing::info!(%up, what, "analytics 拒绝请求，透传 4xx");
                let message = body
                    .get("message")
                    .or_else(|| body.get("error"))
                    .and_then(serde_json::Value::as_str)
                    .map_or_else(|| format!("{what}参数无效"), str::to_string);
                crate::response::ApiResponse::err(status, message)
            }
            Self::Unavailable {
                retry_after_secs, ..
            } => {
                tracing::warn!(error = %self, what, "analytics 未就绪");
                let mut response = crate::response::coded_err(
                    status,
                    ANALYTICS_UNAVAILABLE_CODE,
                    format!("{what}失败：analytics 未就绪，请稍后重试"),
                    &serde_json::json!({ "retry_after_secs": retry_after_secs }),
                );
                if let Ok(value) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                    response.headers_mut().insert(header::RETRY_AFTER, value);
                }
                response
            }
            Self::BadResponse { .. } | Self::Upstream { .. } => {
                tracing::warn!(error = %self, what, "analytics 响应异常");
                crate::response::coded_err(
                    status,
                    ANALYTICS_BAD_GATEWAY_CODE,
                    format!("{what}失败：analytics 响应异常"),
                    &serde_json::json!({}),
                )
            }
        }
    }
}

pub struct AnalyticsHttpClient {
    client: Client,
    base_url: String,
    retry_after_secs: u64,
}
impl AnalyticsHttpClient {
    /// # Errors
    ///
    /// reqwest 客户端构建失败时返回错误。
    pub fn new(config: &AnalyticsClientConfig) -> anyhow::Result<Self> {
        use anyhow::Context;
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(config.connect_timeout_secs))
            .timeout(std::time::Duration::from_secs(config.read_timeout_secs))
            .build()
            .context("创建 AnalyticsHttpClient 失败")?;
        Ok(Self {
            client,
            base_url: config.url.trim_end_matches('/').to_string(),
            retry_after_secs: config.retry_after_secs,
        })
    }
    /// # Errors
    ///
    /// 见 [`AnalyticsError`]。
    pub async fn aggregate_requests(
        &self,
        start_time: &str,
        end_time: &str,
        event_target: &str,
    ) -> AnalyticsResult<serde_json::Value> {
        self.get_json(
            "/api/v1/analytics/requests",
            &[
                ("start_time", start_time.to_string()),
                ("end_time", end_time.to_string()),
                ("event_target", event_target.to_string()),
            ],
        )
        .await
    }
    /// # Errors
    ///
    /// 见 [`AnalyticsError`]。
    pub async fn metrics_overview(
        &self,
        params: &[(&str, String)],
    ) -> AnalyticsResult<serde_json::Value> {
        self.get_json("/api/v1/metrics/overview", params).await
    }
    /// # Errors
    ///
    /// 见 [`AnalyticsError`]。
    pub async fn slo_status(&self) -> AnalyticsResult<serde_json::Value> {
        self.get_json("/api/v1/slo/status", &[]).await
    }
    /// # Errors
    ///
    /// 见 [`AnalyticsError`]。
    pub async fn metrics_catalog(&self) -> AnalyticsResult<serde_json::Value> {
        self.get_json("/api/v1/metrics/catalog", &[]).await
    }
    /// # Errors
    ///
    /// 见 [`AnalyticsError`]。
    pub async fn metrics_services_health(&self) -> AnalyticsResult<serde_json::Value> {
        self.get_json("/api/v1/metrics/services/health", &[]).await
    }
    /// # Errors
    ///
    /// 见 [`AnalyticsError`]。
    pub async fn metrics_topology(
        &self,
        start: Option<i64>,
        end: Option<i64>,
    ) -> AnalyticsResult<serde_json::Value> {
        let mut params = Vec::new();
        if let Some(start) = start {
            params.push(("start", start.to_string()));
        }
        if let Some(end) = end {
            params.push(("end", end.to_string()));
        }
        self.get_json("/api/v1/metrics/topology", &params).await
    }
    /// # Errors
    ///
    /// 见 [`AnalyticsError`]。
    pub async fn metric_timeseries(
        &self,
        metric: &str,
        start: i64,
        end: i64,
        step: Option<u64>,
        service: Option<&str>,
    ) -> AnalyticsResult<serde_json::Value> {
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
    /// 见 [`AnalyticsError`]。
    pub async fn query_request_logs(
        &self,
        params: &[(&str, String)],
    ) -> AnalyticsResult<serde_json::Value> {
        self.get_json("/api/v1/request/query", params).await
    }
    /// # Errors
    ///
    /// 见 [`AnalyticsError`]。
    pub async fn request_histogram(
        &self,
        params: &[(&str, String)],
    ) -> AnalyticsResult<serde_json::Value> {
        self.get_json("/api/v1/request/histogram", params).await
    }
    /// # Errors
    ///
    /// 见 [`AnalyticsError`]。
    pub async fn request_stages(
        &self,
        params: &[(&str, String)],
    ) -> AnalyticsResult<serde_json::Value> {
        self.get_json("/api/v1/request/stages", params).await
    }
    /// # Errors
    ///
    /// 见 [`AnalyticsError`]。
    pub async fn fetch_upstream_usage_summary(
        &self,
        start_time: &str,
        end_time: &str,
        supplier_code: Option<&str>,
        channel_code: Option<&str>,
        supplier_id: Option<&str>,
        channel_id: Option<&str>,
    ) -> AnalyticsResult<serde_json::Value> {
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

    /// 转发 OLAP 模板查询。
    ///
    /// # Errors
    ///
    /// 见 [`AnalyticsError`]。
    pub async fn olap_query(&self, body: &serde_json::Value) -> AnalyticsResult<serde_json::Value> {
        let req = self
            .client
            .post(format!("{}/api/v1/olap/query", self.base_url))
            .json(body);
        self.execute(req).await
    }

    /// trace 检索、详情与服务依赖转发。
    ///
    /// # Errors
    ///
    /// 见 [`AnalyticsError`]。
    pub async fn traces(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> AnalyticsResult<serde_json::Value> {
        self.get_json(&format!("/api/v1/traces{path}"), params)
            .await
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

    async fn get_json(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> AnalyticsResult<serde_json::Value> {
        let req = self.client.get(self.url_with_query(path, params));
        self.execute(req).await
    }

    /// 先读状态码，再解析 JSON：传输失败 → `Unavailable`；非 JSON → `BadResponse`；
    /// 非 2xx → `Upstream`。
    async fn execute(&self, req: reqwest::RequestBuilder) -> AnalyticsResult<serde_json::Value> {
        let unavailable = |e: reqwest::Error| AnalyticsError::Unavailable {
            source: e.without_url(),
            retry_after_secs: self.retry_after_secs,
        };
        let resp = req.send().await.map_err(unavailable)?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(unavailable)?;
        classify_response(status, &bytes)
    }
}

fn classify_response(status: StatusCode, bytes: &[u8]) -> AnalyticsResult<serde_json::Value> {
    let body: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| AnalyticsError::BadResponse { status })?;
    if status.is_success() {
        Ok(body)
    } else {
        Err(AnalyticsError::Upstream { status, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap_or_default();
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    #[test]
    fn classify_success_and_errors() {
        assert!(classify_response(StatusCode::OK, br#"{"a":1}"#).is_ok());
        assert!(matches!(
            classify_response(StatusCode::OK, b"<html>"),
            Err(AnalyticsError::BadResponse { .. })
        ));
        assert!(matches!(
            classify_response(StatusCode::SERVICE_UNAVAILABLE, b""),
            Err(AnalyticsError::BadResponse { status }) if status == StatusCode::SERVICE_UNAVAILABLE
        ));
        assert!(matches!(
            classify_response(StatusCode::NOT_FOUND, br#"{"message":"x"}"#),
            Err(AnalyticsError::Upstream { status, .. }) if status == StatusCode::NOT_FOUND
        ));
    }

    #[tokio::test]
    async fn client_error_is_passed_through() {
        let err = AnalyticsError::Upstream {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            body: serde_json::json!({"message": "模板不存在"}),
        };
        let resp = err.into_response("OLAP 查询");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body_json(resp).await["error"], "模板不存在");
    }

    #[tokio::test]
    async fn server_error_and_non_json_become_502() {
        let err = AnalyticsError::Upstream {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: serde_json::json!({"message": "db down at http://internal:1"}),
        };
        let resp = err.into_response("指标总览");
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
        let body = body_json(resp).await;
        assert_eq!(body["code"], ANALYTICS_BAD_GATEWAY_CODE);
        assert!(!body.to_string().contains("internal"));

        let resp = AnalyticsError::BadResponse {
            status: StatusCode::OK,
        }
        .into_response("指标总览");
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn unreachable_becomes_503_with_retry_after() {
        // 绑定临时端口后立即释放：该端口无监听，连接被拒绝。
        let addr = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .and_then(|l| l.local_addr())
            .expect("ephemeral port");
        let client = AnalyticsHttpClient::new(&AnalyticsClientConfig {
            url: format!("http://{addr}"),
            connect_timeout_secs: 1,
            read_timeout_secs: 1,
            write_timeout_secs: 1,
            retry_after_secs: 7,
        })
        .expect("client");
        let err = client.metrics_overview(&[]).await.expect_err("must fail");
        assert!(matches!(err, AnalyticsError::Unavailable { .. }));
        assert!(!err.to_string().contains(&addr.to_string()));
        let resp = err.into_response("指标总览");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            resp.headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()),
            Some("7")
        );
        let body = body_json(resp).await;
        assert_eq!(body["code"], ANALYTICS_UNAVAILABLE_CODE);
        assert!(
            body["error"]
                .as_str()
                .unwrap_or_default()
                .contains("analytics 未就绪")
        );
    }
}
