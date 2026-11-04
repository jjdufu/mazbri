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
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

/// 发布 `require_gate` 唯一认的试跑拓扑标记（写入 `upstream_connector_runs.probe_path`）。
pub const PROBE_PATH_BYPASS_MGMT_PROBE: &str = "bypass_mgmt_probe";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// 合成租户/产品：标明管理面探针，避免与真实租户调用混淆；**不产生计费**。
pub const PROBE_TENANT_ID: &str = "mgmt-probe";
pub const PROBE_PRODUCT_ID: &str = "mgmt-probe";

fn hash_config_json(config_json: &str) -> String {
    let digest = Sha256::digest(config_json.as_bytes());
    hex::encode(digest)
}

#[derive(Debug, Serialize)]
struct ProbeHttpRequest<'a> {
    product_id: &'a str,
    tenant_id: &'a str,
    channel_id: &'a str,
    request_id: &'a str,
    trace: ProbeHttpTrace<'a>,
    timeout_ms: u64,
    payload_json: &'a str,
    egress_spec_hash: String,
    egress_spec_json: &'a str,
    supplier_id: &'a str,
    supplier_code: &'a str,
    supplier_config_hash: String,
    supplier_config_json: &'a str,
}

#[derive(Debug, Serialize)]
struct ProbeHttpTrace<'a> {
    #[serde(rename = "request_id")]
    request: &'a str,
    #[serde(rename = "trace_id")]
    trace: &'a str,
    #[serde(rename = "span_id")]
    span: String,
    #[serde(rename = "parent_span_id")]
    parent_span: &'a str,
}

#[derive(Debug, Deserialize)]
struct ProbeHttpResponse {
    success: bool,
    #[serde(default)]
    error: String,
    biz_error: Option<ProbeHttpBizError>,
    #[serde(default)]
    payload_json: String,
    upstream_meta: Option<ProbeHttpUpstreamMeta>,
}

#[derive(Debug, Deserialize)]
struct ProbeHttpBizError {
    code: String,
    message: String,
    #[serde(default)]
    details_json: String,
}

#[derive(Debug, Deserialize)]
struct ProbeHttpUpstreamMeta {
    status: u32,
    url: String,
    adapter: String,
    latency_ms: u64,
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
    /// HTTP 客户端构建失败时返回错误。
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| format!("BYPASS_PROBE_HTTP_CLIENT: {e}"))?;
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

    /// `POST /v1/mgmt-probe/invoke`；错误透传 bypass / 传输层文案（无静默吞掉）。
    ///
    /// # Errors
    ///
    /// 连接或 HTTP 调用失败时返回可读错误串。
    pub async fn execute_invoke(
        &self,
        params: &ProbeInvokeParams<'_>,
    ) -> Result<ProbeHttpInvokeResult, String> {
        let request_id = params.request_id;
        let body = ProbeHttpRequest {
            product_id: PROBE_PRODUCT_ID,
            tenant_id: PROBE_TENANT_ID,
            channel_id: params.channel_id,
            request_id,
            trace: ProbeHttpTrace {
                request: request_id,
                trace: request_id,
                span: uuid::Uuid::new_v4().to_string(),
                parent_span: "",
            },
            timeout_ms: params.timeout_ms,
            payload_json: params.payload_json,
            egress_spec_hash: hash_egress_spec_json(params.egress_spec_json),
            egress_spec_json: params.egress_spec_json,
            supplier_id: params.supplier_id,
            supplier_code: params.supplier_code,
            supplier_config_hash: hash_config_json(params.supplier_config_json),
            supplier_config_json: params.supplier_config_json,
        };
        let url = format!("{}/v1/mgmt-probe/invoke", self.base_url);
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
            .map_err(|e| format!("BYPASS_PROBE_HTTP: {e}"))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| format!("BYPASS_PROBE_HTTP_BODY: {e}"))?;
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err("BYPASS_PROBE_UNAUTHORIZED: mgmt_probe_token rejected".into());
        }
        if !status.is_success() {
            return Err(format!("BYPASS_PROBE_HTTP_{status}: {text}"));
        }
        let parsed: ProbeHttpResponse = serde_json::from_str(&text)
            .map_err(|e| format!("BYPASS_PROBE_HTTP_JSON: {e}; body={text}"))?;
        Ok(ProbeHttpInvokeResult {
            success: parsed.success,
            error: parsed.error,
            biz_error: parsed.biz_error.map(|b| ProbeBizError {
                code: b.code,
                message: b.message,
                details_json: b.details_json,
            }),
            payload_json: parsed.payload_json,
            upstream_meta: parsed.upstream_meta.map(|m| ProbeUpstreamMeta {
                status: m.status,
                url: m.url,
                adapter: m.adapter,
                latency_ms: m.latency_ms,
            }),
        })
    }
}

/// HTTP 探针原始结果（对应 bypass JSON 响应）。
#[derive(Debug, Clone)]
pub struct ProbeHttpInvokeResult {
    pub success: bool,
    pub error: String,
    pub biz_error: Option<ProbeBizError>,
    pub payload_json: String,
    pub upstream_meta: Option<ProbeUpstreamMeta>,
}

#[derive(Debug, Clone)]
pub struct ProbeBizError {
    pub code: String,
    pub message: String,
    pub details_json: String,
}

#[derive(Debug, Clone)]
pub struct ProbeUpstreamMeta {
    pub status: u32,
    pub url: String,
    pub adapter: String,
    pub latency_ms: u64,
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
    pub latency_ms: u64,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub status: Option<u16>,
    pub url: Option<String>,
    pub adapter: Option<String>,
    pub biz_error: Option<serde_json::Value>,
    pub stages: serde_json::Value,
}

/// 将探针 HTTP 响应映射为工作台结果；`success` 且无 `biz_error` 才算业务 ok。
#[must_use]
pub fn outcome_from_invoke_response(response: &ProbeHttpInvokeResult) -> ProbeOutcome {
    let meta = response.upstream_meta.as_ref();
    let latency_ms = meta.map_or(0, |m| m.latency_ms);
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

    let stages = serde_json::json!([{
        "name": "bypass_mgmt_probe",
        "ok": ok,
        "latency_ms": latency_ms,
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
        latency_ms,
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
        !s.is_empty()
            && s.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
            && s.len() < 64
    }) {
        code.to_string()
    } else {
        "BYPASS_UPSTREAM_ERROR".into()
    }
}

