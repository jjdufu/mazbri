//! 隔离管理探针口：仅供 mgmt 工作台 business/live 试调。
//!
//! - 明文 HTTP（docker 内网），`Authorization: Bearer <mgmt_probe_token>`
//! - 只暴露 `POST /v1/mgmt-probe/invoke`，复用与 gRPC Execute 同一 `execute_invoke`
//! - 不改配置、不发证、不碰租户 8888 mTLS 数据面

use crate::config::MgmtProbeConfig;
use crate::service::MazbriBypassServiceImpl;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use mazbri_upstream_contracts::shared::{
    InvokeBizError, InvokeRequest, InvokeResponse, InvokeUpstreamMeta, TraceContext,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tracing::info;

const INVOKE_PATH: &str = "/v1/mgmt-probe/invoke";

#[derive(Clone)]
struct ProbeState {
    service: MazbriBypassServiceImpl,
    token: Arc<str>,
}

/// 与 gRPC `InvokeRequest` 字段对齐的 JSON 体（mgmt 探针专用）。
#[derive(Debug, Deserialize)]
pub struct ProbeInvokeJson {
    #[serde(default)]
    pub product_id: String,
    #[serde(default)]
    pub tenant_id: String,
    pub channel_id: String,
    pub request_id: String,
    #[serde(default)]
    pub trace: Option<ProbeTraceJson>,
    #[serde(default)]
    pub timeout_ms: u64,
    #[serde(default)]
    pub payload_json: String,
    #[serde(default)]
    pub egress_spec_hash: String,
    #[serde(default)]
    pub egress_spec_json: String,
    #[serde(default)]
    pub supplier_id: String,
    #[serde(default)]
    pub supplier_code: String,
    #[serde(default)]
    pub supplier_config_hash: String,
    #[serde(default)]
    pub supplier_config_json: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProbeTraceJson {
    #[serde(rename = "request_id", default)]
    pub request: String,
    #[serde(rename = "trace_id", default)]
    pub trace: String,
    #[serde(rename = "span_id", default)]
    pub span: String,
    #[serde(rename = "parent_span_id", default)]
    pub parent_span: String,
}

#[derive(Debug, Serialize)]
pub struct ProbeInvokeResponseJson {
    pub success: bool,
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biz_error: Option<ProbeBizErrorJson>,
    pub payload_json: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_meta: Option<ProbeUpstreamMetaJson>,
}

#[derive(Debug, Serialize)]
pub struct ProbeBizErrorJson {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub details_json: String,
}

#[derive(Debug, Serialize)]
pub struct ProbeUpstreamMetaJson {
    pub status: u32,
    pub url: String,
    pub adapter: String,
    pub latency_ms: u64,
}

impl From<ProbeInvokeJson> for InvokeRequest {
    fn from(v: ProbeInvokeJson) -> Self {
        Self {
            product_id: v.product_id,
            tenant_id: v.tenant_id,
            channel_id: v.channel_id,
            request_id: v.request_id,
            trace: v.trace.map(|t| TraceContext {
                request_id: t.request,
                trace_id: t.trace,
                span_id: t.span,
                parent_span_id: t.parent_span,
            }),
            timeout_ms: v.timeout_ms,
            payload_json: v.payload_json,
            egress_spec_hash: v.egress_spec_hash,
            egress_spec_json: v.egress_spec_json,
            supplier_id: v.supplier_id,
            supplier_code: v.supplier_code,
            supplier_config_hash: v.supplier_config_hash,
            supplier_config_json: v.supplier_config_json,
        }
    }
}

impl From<InvokeResponse> for ProbeInvokeResponseJson {
    fn from(r: InvokeResponse) -> Self {
        Self {
            success: r.success,
            error: r.error,
            biz_error: r.biz_error.map(|b: InvokeBizError| ProbeBizErrorJson {
                code: b.code,
                message: b.message,
                details_json: b.details_json,
            }),
            payload_json: r.payload_json,
            upstream_meta: r.upstream_meta.map(|m: InvokeUpstreamMeta| ProbeUpstreamMetaJson {
                status: m.status,
                url: m.url,
                adapter: m.adapter,
                latency_ms: m.latency_ms,
            }),
        }
    }
}

fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

fn token_matches(expected: &str, presented: &str) -> bool {
    let a = expected.as_bytes();
    let b = presented.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    bool::from(a.ct_eq(b))
}

async fn invoke_handler(
    State(state): State<ProbeState>,
    headers: HeaderMap,
    Json(body): Json<ProbeInvokeJson>,
) -> impl IntoResponse {
    let Some(presented) = extract_bearer(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "missing_bearer_token"})),
        )
            .into_response();
    };
    if !token_matches(state.token.as_ref(), presented) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid_token"})),
        )
            .into_response();
    }
    let response = state.service.execute_invoke(body.into()).await;
    (StatusCode::OK, Json(ProbeInvokeResponseJson::from(response))).into_response()
}

/// 在独立端口启动管理探针 HTTP 服务；与 8888 mTLS gRPC 并行。
///
/// # Errors
///
/// 绑定失败或服务异常退出时返回错误。
pub async fn serve(cfg: &MgmtProbeConfig, service: MazbriBypassServiceImpl) -> anyhow::Result<()> {
    let addr = SocketAddr::new(cfg.host.parse()?, cfg.port);
    let state = ProbeState {
        service,
        token: Arc::from(cfg.mgmt_probe_token.as_str()),
    };
    let app = Router::new()
        .route(INVOKE_PATH, post(invoke_handler))
        .with_state(state);
    info!(
        %addr,
        path = INVOKE_PATH,
        "mazbri-bypass mgmt probe HTTP listening (token auth, internal only)"
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("mgmt probe server error: {e}"))
}
