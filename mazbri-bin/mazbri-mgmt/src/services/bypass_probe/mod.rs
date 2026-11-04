//! C1：mgmt → bypass gRPC `Invoke`（工作台 business/live 试调；不经 gateway 计费）。
//!
//! # 威胁建模（草稿未发布试跑）
//!
//! - **SSRF**：调用前 mgmt 用 `[ssrf]` 预检 upstream / `token_url`；bypass 进程内
//!   `RuntimeEngine` 再次拦截。禁止「未校验就 Invoke」。
//! - **密钥**：`supplier_config_json` 保持库内密文形态发往 bypass，仅在 bypass 解密；
//!   mgmt 试跑路径不再为 business/live 解密出站凭据。
//! - **租户隔离**：`tenant_id`/`product_id` 固定为合成值 `mgmt-probe`，不经 gateway
//!   鉴权/计费/路由；仍受 `connector_probe` 进程内滑动窗口限流。
//! - **无引擎兜底**：business/live **只**走本客户端；失败即失败，不得回退
//!   `RuntimeEngine::invoke`。

use mazbri_core::hash_egress_spec_json;
use mazbri_tls::{ManagedCertificate, resolve_connect_target};
use mazbri_upstream_contracts::shared::{InvokeRequest, InvokeResponse, TraceContext};
use mazbri_upstream_contracts::BYPASS_EXECUTE_PATH;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tonic::Request;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};
use tonic_prost::ProstCodec;

/// 发布 `require_gate` 唯一认的试跑拓扑标记（写入 `upstream_connector_runs.probe_path`）。
pub const PROBE_PATH_BYPASS_GRPC: &str = "bypass_grpc";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// 合成租户/产品：标明管理面探针，避免与真实租户调用混淆；**不产生计费**。
pub const PROBE_TENANT_ID: &str = "mgmt-probe";
pub const PROBE_PRODUCT_ID: &str = "mgmt-probe";

fn hash_config_json(config_json: &str) -> String {
    let digest = Sha256::digest(config_json.as_bytes());
    hex::encode(digest)
}

/// mgmt 侧 bypass 探针客户端（单 endpoint 缓存 channel；mTLS）。
#[derive(Clone)]
pub struct BypassProbeClient {
    /// `host:port`（无 scheme），与 gateway `[services].bypass_url` 同形。
    endpoint: String,
    cert: ManagedCertificate,
    channel: Arc<RwLock<Option<Channel>>>,
}

impl BypassProbeClient {
    #[must_use]
    pub fn new(endpoint: impl Into<String>, cert: ManagedCertificate) -> Self {
        Self {
            endpoint: endpoint.into(),
            cert,
            channel: Arc::new(RwLock::new(None)),
        }
    }

    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    async fn get_channel(&self) -> Result<Channel, String> {
        if let Some(ch) = self.channel.read().await.clone() {
            return Ok(ch);
        }
        let mut guard = self.channel.write().await;
        if let Some(ch) = guard.clone() {
            return Ok(ch);
        }
        let ch = self.connect().await?;
        *guard = Some(ch.clone());
        Ok(ch)
    }

    async fn connect(&self) -> Result<Channel, String> {
        let uri = format!("https://{}", self.endpoint.trim());
        let target = resolve_connect_target(&uri)
            .await
            .map_err(|e| format!("BYPASS_CONNECT: resolve {}: {e}", self.endpoint))?;
        let server_name = target.server_name.clone().ok_or_else(|| {
            format!(
                "BYPASS_CONNECT: endpoint {} 缺少 DNS server name（mTLS SNI）",
                self.endpoint
            )
        })?;
        let identity = Identity::from_pem(
            self.cert.cert_pem.as_bytes(),
            self.cert.key_pem.as_bytes(),
        );
        let ca = Certificate::from_pem(self.cert.ca_cert_pem.as_bytes());
        let tls = ClientTlsConfig::new()
            .identity(identity)
            .ca_certificate(ca)
            .domain_name(server_name)
            .assume_http2(true)
            .timeout(CONNECT_TIMEOUT);
        let endpoint_uri = format!("https://{}", target.display_addr);
        let ep = Endpoint::from_shared(endpoint_uri.clone())
            .map_err(|e| format!("BYPASS_CONNECT: bad endpoint URI: {e}"))?
            .connect_timeout(CONNECT_TIMEOUT)
            .http2_keep_alive_interval(Duration::from_secs(20))
            .keep_alive_timeout(Duration::from_secs(10))
            .keep_alive_while_idle(true)
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .tls_config(tls)
            .map_err(|e| format!("BYPASS_CONNECT: tls: {e}"))?;
        ep.connect()
            .await
            .map_err(|e| format!("BYPASS_CONNECT: {e}"))
    }

    /// 丢弃缓存 channel（下次 Invoke 重连）。
    pub async fn invalidate(&self) {
        *self.channel.write().await = None;
    }

    /// 一元 Execute；错误透传 bypass / 传输层文案（无静默吞掉）。
    ///
    /// # Errors
    ///
    /// 连接或 gRPC 调用失败时返回可读错误串。
    pub async fn execute_invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, String> {
        let channel = self.get_channel().await?;
        let path = http::uri::PathAndQuery::from_static(BYPASS_EXECUTE_PATH);
        let mut client = tonic::client::Grpc::new(channel);
        client
            .ready()
            .await
            .map_err(|e| format!("BYPASS_NOT_READY: {e}"))?;
        let codec = ProstCodec::<InvokeRequest, InvokeResponse>::default();
        let response = client
            .unary(Request::new(request), path, codec)
            .await
            .map_err(|status| {
                // 连接级失败时清缓存，避免粘住坏 channel
                let msg = status.to_string();
                if msg.contains("transport") || msg.contains("connection") {
                    let this = self.clone();
                    tokio::spawn(async move {
                        this.invalidate().await;
                    });
                }
                format!("BYPASS_GRPC: {status}")
            })?;
        Ok(response.into_inner())
    }
}

/// 组装 bypass `InvokeRequest`（始终带全量 spec/config，避免探针 `CONFIG_CACHE_MISS` 往返）。
#[must_use]
pub fn build_probe_invoke_request(
    params: &ProbeInvokeParams<'_>,
) -> InvokeRequest {
    let egress_spec_json = params.egress_spec_json.to_owned();
    let supplier_config_json = params.supplier_config_json.to_owned();
    let request_id = params.request_id.to_owned();
    InvokeRequest {
        product_id: PROBE_PRODUCT_ID.into(),
        tenant_id: PROBE_TENANT_ID.into(),
        channel_id: params.channel_id.into(),
        request_id: request_id.clone(),
        trace: Some(TraceContext {
            request_id: request_id.clone(),
            trace_id: request_id.clone(),
            span_id: uuid::Uuid::new_v4().to_string(),
            parent_span_id: String::new(),
        }),
        timeout_ms: params.timeout_ms,
        payload_json: params.payload_json.into(),
        egress_spec_hash: hash_egress_spec_json(&egress_spec_json),
        egress_spec_json,
        supplier_id: params.supplier_id.into(),
        supplier_code: params.supplier_code.into(),
        supplier_config_hash: hash_config_json(&supplier_config_json),
        supplier_config_json,
    }
}

/// [`build_probe_invoke_request`] 入参。
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

/// 将 [`InvokeResponse`] 映射为工作台结果；`success` 且无 `biz_error` 才算业务 ok。
#[must_use]
pub fn outcome_from_invoke_response(response: &InvokeResponse) -> ProbeOutcome {
    let meta = response.upstream_meta.as_ref();
    let latency_ms = meta.map_or(0, |m| m.latency_ms);
    let status = meta.and_then(|m| u16::try_from(m.status).ok()).filter(|s| *s > 0);
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
        (Some(classify_bypass_error(&response.error)), Some(response.error.clone()))
    } else {
        (Some("BYPASS_INVOKE_FAILED".into()), Some("bypass invoke failed".into()))
    };

    let stages = serde_json::json!([{
        "name": "bypass_grpc",
        "ok": ok,
        "latency_ms": latency_ms,
        "response_summary": {
            "probe_path": PROBE_PATH_BYPASS_GRPC,
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
