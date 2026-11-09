//! 工作台 business/live 试调：mgmt → bypass **隔离管理探针 HTTP 口**（不经 gateway 计费）。
//!
//! # 威胁建模（草稿未发布试跑）
//!
//! - **SSRF**：调用前 mgmt 用 `[ssrf]` 预检 upstream / `token_url`；bypass 进程内
//!   `RuntimeEngine` 再次拦截。禁止「未校验就 Invoke」。
//! - **密钥**：`supplier_config_json` 保持库内密文形态发往 bypass，仅在 bypass 解密；
//!   mgmt 试跑路径不再为 business/live 解密出站凭据。
//! - **租户隔离**：`tenant_id`/`product_id` 固定为合成值 `mgmt-probe`，不经 gateway
//!   鉴权/计费/路由；仍受 `connector_probe` 进程内滑动窗口限流。
//! - **鉴权**：共享 `mgmt_probe_token`（Bearer），**不**使用 cert 主令牌 / mTLS 客户端证。
//! - **无引擎兜底**：business/live **只**走本客户端；失败即失败，不得回退
//!   `RuntimeEngine::invoke`。

use mazbri_core::hash_egress_spec_json;
use mazbri_upstream_contracts::mgmt_probe::{
    MGMT_PROBE_INVOKE_PATH, ProbeInvokeRequest, ProbeInvokeResponse, ProbeTrace,
};
use mazbri_upstream_runtime::redact_body_preview;
use sha2::{Digest, Sha256};
use std::fmt;
use std::time::Duration;

/// 发布 `require_gate` 唯一认的试跑拓扑标记（写入 `upstream_connector_runs.probe_path`）。
pub const PROBE_PATH_BYPASS_MGMT_PROBE: &str = "bypass_mgmt_probe";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// 错误文案/日志里保留的 bypass 原始响应体上限（脱敏后截断）。
const BODY_PREVIEW_BYTES: usize = 512;

/// 合成租户/产品：标明管理面探针，避免与真实租户调用混淆；**不产生计费**。
pub const PROBE_TENANT_ID: &str = "mgmt-probe";
pub const PROBE_PRODUCT_ID: &str = "mgmt-probe";

fn hash_config_json(config_json: &str) -> String {
    let digest = Sha256::digest(config_json.as_bytes());
    hex::encode(digest)
}

/// 调用 bypass 管理探针失败的分类；`code()` 即写入试运行记录的 `error_code`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// HTTP 客户端构建失败（仅启动期）。
    ClientBuild(String),
    /// 连接失败、超时、读响应体失败或非 2xx：bypass 不可用。
    Unavailable(String),
    /// `mgmt_probe_token` 被 bypass 拒绝（401）：两侧令牌配置不一致。
    Unauthorized,
    /// 2xx 但响应体不符合共享契约：两侧版本或契约不一致。
    Decode(String),
}

impl ProbeError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::ClientBuild(_) => "BYPASS_PROBE_CLIENT",
            Self::Unavailable(_) => "BYPASS_UNAVAILABLE",
            Self::Unauthorized => "BYPASS_PROBE_UNAUTHORIZED",
            Self::Decode(_) => "BYPASS_PROBE_DECODE",
        }
    }
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClientBuild(e) => write!(f, "bypass probe client build failed: {e}"),
            Self::Unavailable(e) => write!(f, "bypass probe unavailable: {e}"),
            Self::Unauthorized => f.write_str("bypass probe rejected mgmt_probe_token"),
            Self::Decode(e) => write!(f, "bypass probe response decode failed: {e}"),
        }
    }
}

impl std::error::Error for ProbeError {}

/// 解析探针 HTTP 响应（状态码 + 原始体）；与传输层解耦便于单测。
///
/// # Errors
///
/// 401 → [`ProbeError::Unauthorized`]；其它非 2xx → [`ProbeError::Unavailable`]；
/// 2xx 但体不符合契约 → [`ProbeError::Decode`]（脱敏截断原始体记 warn 日志）。
pub fn decode_probe_response(
    status: reqwest::StatusCode,
    text: &str,
) -> Result<ProbeInvokeResponse, ProbeError> {
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ProbeError::Unauthorized);
    }
    if !status.is_success() {
        return Err(ProbeError::Unavailable(format!(
            "HTTP {status}: {}",
            redact_body_preview(text, BODY_PREVIEW_BYTES, true)
        )));
    }
    serde_json::from_str(text).map_err(|e| {
        tracing::warn!(
            error = %e,
            body_preview = %redact_body_preview(text, BODY_PREVIEW_BYTES, true),
            "bypass mgmt probe response does not match shared contract"
        );
        ProbeError::Decode(e.to_string())
    })
}

/// mgmt 侧 bypass 管理探针客户端（HTTP + Bearer token；启动不拉证）。
#[derive(Clone)]
pub struct BypassProbeClient {
    /// 完整 URL 前缀，如 `http://mazbri-bypass:8890`。
    base_url: String,
    token: String,
    http: reqwest::Client,
}

impl BypassProbeClient {
    /// # Errors
    ///
    /// HTTP 客户端构建失败时返回 [`ProbeError::ClientBuild`]。
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Result<Self, ProbeError> {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| ProbeError::ClientBuild(e.to_string()))?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            token: token.into(),
            http,
        })
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// `POST` [`MGMT_PROBE_INVOKE_PATH`]；请求/响应均为共享契约类型。
    ///
    /// # Errors
    ///
    /// 见 [`ProbeError`] 各分类。
    pub async fn execute_invoke(
        &self,
        params: &ProbeInvokeParams<'_>,
    ) -> Result<ProbeInvokeResponse, ProbeError> {
        let request_id = params.request_id;
        let body = ProbeInvokeRequest {
            product_id: PROBE_PRODUCT_ID.to_owned(),
            tenant_id: PROBE_TENANT_ID.to_owned(),
            channel_id: params.channel_id.to_owned(),
            request_id: request_id.to_owned(),
            trace: ProbeTrace {
                request_id: request_id.to_owned(),
                trace_id: request_id.to_owned(),
                span_id: uuid::Uuid::new_v4().to_string(),
                parent_span_id: String::new(),
            },
            timeout_ms: params.timeout_ms,
            payload_json: params.payload_json.to_owned(),
            egress_spec_hash: hash_egress_spec_json(params.egress_spec_json),
            egress_spec_json: params.egress_spec_json.to_owned(),
            supplier_id: params.supplier_id.to_owned(),
            supplier_code: params.supplier_code.to_owned(),
            supplier_config_hash: hash_config_json(params.supplier_config_json),
            supplier_config_json: params.supplier_config_json.to_owned(),
        };
        let url = format!("{}{MGMT_PROBE_INVOKE_PATH}", self.base_url);
        let response = self
            .http
            .post(&url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.token),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| ProbeError::Unavailable(e.to_string()))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| ProbeError::Unavailable(format!("read body: {e}")))?;
        decode_probe_response(status, &text)
    }
}

/// [`BypassProbeClient::execute_invoke`] / 组装入参。
pub struct ProbeInvokeParams<'a> {
    pub channel_id: &'a str,
    pub request_id: &'a str,
    pub timeout_ms: u64,
    pub payload_json: &'a str,
    pub egress_spec_json: &'a str,
    pub supplier_id: &'a str,
    pub supplier_code: &'a str,
    pub supplier_config_json: &'a str,
}

/// 从 bypass 响应提炼门控/UI 用字段。
#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    pub ok: bool,
    /// 纯上游耗时（bypass `upstream_meta.upstream_wire_ms`）；无上游元信息时为 0。
    pub upstream_wire_ms: u64,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub status: Option<u16>,
    pub url: Option<String>,
    pub adapter: Option<String>,
    pub biz_error: Option<serde_json::Value>,
    pub stages: serde_json::Value,
}

impl ProbeOutcome {
    /// 调用探针失败（未拿到合法响应）时的结果。
    #[must_use]
    pub fn from_probe_error(err: &ProbeError) -> Self {
        Self {
            ok: false,
            upstream_wire_ms: 0,
            error_code: Some(err.code().to_owned()),
            error_message: Some(err.to_string()),
            payload: None,
            status: None,
            url: None,
            adapter: None,
            biz_error: None,
            stages: serde_json::json!([{
                "name": PROBE_PATH_BYPASS_MGMT_PROBE,
                "ok": false,
                "latency_ms": 0,
                "response_summary": {"probe_path": PROBE_PATH_BYPASS_MGMT_PROBE},
                "error": err.code(),
            }]),
        }
    }
}

/// 将探针 HTTP 响应映射为工作台结果；`success` 且无 `biz_error` 才算业务 ok。
#[must_use]
pub fn outcome_from_invoke_response(response: &ProbeInvokeResponse) -> ProbeOutcome {
    let meta = response.upstream_meta.as_ref();
    let upstream_wire_ms = meta.map_or(0, |m| m.upstream_wire_ms);
    let status = meta
        .and_then(|m| u16::try_from(m.status).ok())
        .filter(|s| *s > 0);
    let url = meta.map(|m| m.url.clone());
    let adapter = meta.map(|m| m.adapter.clone());

    let biz_error = response.biz_error.as_ref().map(|b| {
        serde_json::json!({
            "code": b.code,
            "message": b.message,
            "details": if b.details_json.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_str(&b.details_json).unwrap_or(serde_json::Value::String(b.details_json.clone()))
            }
        })
    });

    let payload = if response.payload_json.is_empty() {
        None
    } else {
        Some(
            serde_json::from_str(&response.payload_json)
                .unwrap_or(serde_json::Value::String(response.payload_json.clone())),
        )
    };

    let transport_ok = response.success;
    let ok = transport_ok && biz_error.is_none();

    let (error_code, error_message) = if ok {
        (None, None)
    } else if let Some(ref b) = response.biz_error {
        (Some(b.code.clone()), Some(b.message.clone()))
    } else if !response.error.is_empty() {
        (
            Some(classify_bypass_error(&response.error)),
            Some(response.error.clone()),
        )
    } else {
        (
            Some("BYPASS_INVOKE_FAILED".into()),
            Some("bypass invoke failed".into()),
        )
    };

    // 阶段结构沿用工作台通用 StageResult（`latency_ms`）；探针阶段的取值即纯上游耗时。
    let stages = serde_json::json!([{
        "name": PROBE_PATH_BYPASS_MGMT_PROBE,
        "ok": ok,
        "latency_ms": upstream_wire_ms,
        "response_summary": {
            "probe_path": PROBE_PATH_BYPASS_MGMT_PROBE,
            "status": status,
            "url": url,
            "adapter": adapter,
        },
        "error": error_message.clone(),
    }]);

    ProbeOutcome {
        ok,
        upstream_wire_ms,
        error_code,
        error_message,
        payload,
        status,
        url,
        adapter,
        biz_error,
        stages,
    }
}

fn classify_bypass_error(error: &str) -> String {
    if error.contains("SSRF") {
        "SSRF_BLOCKED".into()
    } else if error.contains("AUTH_TOKEN") || error.contains("Dynamic Auth") {
        "AUTH_TOKEN_FAILED".into()
    } else if error.contains("timeout") || error.contains("Timeout") {
        "UPSTREAM_TIMEOUT".into()
    } else if error.contains("CONFIG_CACHE") {
        "CONFIG_CACHE_MISS".into()
    } else if let Some(code) = error.split(':').next().filter(|s| {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_uppercase() || b == b'_') && s.len() < 64
    }) {
        code.to_string()
    } else {
        "BYPASS_UPSTREAM_ERROR".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::routing::post;
    use mazbri_upstream_contracts::shared::{InvokeResponse, InvokeUpstreamMeta};
    use reqwest::StatusCode;

    const TEST_TOKEN: &str = "unit-test-probe-token";

    /// 与 bypass handler 相同的产出方式：proto `InvokeResponse` → 共享契约 JSON。
    fn bypass_body(upstream_wire_ms: u64) -> String {
        let resp = ProbeInvokeResponse::from(InvokeResponse {
            success: true,
            error: String::new(),
            biz_error: None,
            payload_json: "{\"items\":[]}".into(),
            upstream_meta: Some(InvokeUpstreamMeta {
                status: 200,
                url: "http://upstream.invalid/v3/gateway".into(),
                adapter: "http_envelope".into(),
                upstream_wire_ms,
            }),
        });
        serde_json::to_string(&resp).unwrap()
    }

    fn params() -> ProbeInvokeParams<'static> {
        ProbeInvokeParams {
            channel_id: "CH-1",
            request_id: "req-1",
            timeout_ms: 1000,
            payload_json: "{}",
            egress_spec_json: "{}",
            supplier_id: "sid",
            supplier_code: "S",
            supplier_config_json: "{}",
        }
    }

    async fn serve_stub(status: StatusCode, body: String) -> String {
        let app = Router::new().route(
            MGMT_PROBE_INVOKE_PATH,
            post(
                move |axum::Json(req): axum::Json<ProbeInvokeRequest>| async move {
                    assert_eq!(req.tenant_id, PROBE_TENANT_ID);
                    (status, body)
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    #[test]
    fn decode_accepts_contract_body() {
        let parsed = decode_probe_response(StatusCode::OK, &bypass_body(59)).unwrap();
        let outcome = outcome_from_invoke_response(&parsed);
        assert!(outcome.ok);
        assert_eq!(outcome.upstream_wire_ms, 59);
        assert_eq!(outcome.status, Some(200));
        assert_eq!(outcome.stages[0]["latency_ms"], 59);
    }

    #[test]
    fn decode_failure_is_probe_decode() {
        let legacy = bypass_body(59).replace("upstream_wire_ms", "latency_ms");
        let err = decode_probe_response(StatusCode::OK, &legacy).unwrap_err();
        assert!(matches!(err, ProbeError::Decode(_)), "{err:?}");
        assert_eq!(err.code(), "BYPASS_PROBE_DECODE");
        let err = decode_probe_response(StatusCode::OK, "not json").unwrap_err();
        assert_eq!(err.code(), "BYPASS_PROBE_DECODE");
    }

    #[test]
    fn non_success_status_is_unavailable() {
        for status in [
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::NOT_FOUND,
        ] {
            let err = decode_probe_response(status, "{}").unwrap_err();
            assert_eq!(err.code(), "BYPASS_UNAVAILABLE", "{status}");
        }
        let err = decode_probe_response(StatusCode::UNAUTHORIZED, "{}").unwrap_err();
        assert_eq!(err, ProbeError::Unauthorized);
        assert_eq!(err.code(), "BYPASS_PROBE_UNAUTHORIZED");
    }

    #[test]
    fn error_outcome_carries_code() {
        let outcome = ProbeOutcome::from_probe_error(&ProbeError::Decode("x".into()));
        assert!(!outcome.ok);
        assert_eq!(outcome.error_code.as_deref(), Some("BYPASS_PROBE_DECODE"));
        assert_eq!(outcome.upstream_wire_ms, 0);
    }

    #[tokio::test]
    async fn client_parses_bypass_contract_over_http() {
        let base = serve_stub(StatusCode::OK, bypass_body(42)).await;
        let client = BypassProbeClient::new(base, TEST_TOKEN).unwrap();
        let resp = client.execute_invoke(&params()).await.unwrap();
        assert_eq!(resp.upstream_meta.expect("meta").upstream_wire_ms, 42);
    }

    #[tokio::test]
    async fn client_reports_decode_over_http() {
        let legacy = bypass_body(42).replace("upstream_wire_ms", "latency_ms");
        let base = serve_stub(StatusCode::OK, legacy).await;
        let client = BypassProbeClient::new(base, TEST_TOKEN).unwrap();
        let err = client.execute_invoke(&params()).await.unwrap_err();
        assert_eq!(err.code(), "BYPASS_PROBE_DECODE");
    }

    #[tokio::test]
    async fn client_reports_unavailable_when_refused() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let client = BypassProbeClient::new(format!("http://{addr}"), TEST_TOKEN).unwrap();
        let err = client.execute_invoke(&params()).await.unwrap_err();
        assert!(matches!(err, ProbeError::Unavailable(_)), "{err:?}");
        assert_eq!(err.code(), "BYPASS_UNAVAILABLE");
    }
}
