//! 隔离管理探针口：仅供 mgmt 工作台 business/live 试调。
//!
//! - 明文 HTTP（docker 内网），`Authorization: Bearer <mgmt_probe_token>`
//! - 只暴露 [`MGMT_PROBE_INVOKE_PATH`]，复用与 gRPC Execute 同一 `execute_invoke`
//! - 请求/响应 JSON 只用 `mazbri_upstream_contracts::mgmt_probe` 共享类型
//! - 不改配置、不发证、不碰租户 8888 mTLS 数据面

use crate::config::MgmtProbeConfig;
use crate::service::MazbriBypassServiceImpl;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use mazbri_upstream_contracts::mgmt_probe::{
    MGMT_PROBE_INVOKE_PATH, ProbeInvokeRequest, ProbeInvokeResponse,
};
use mazbri_upstream_contracts::shared::{InvokeRequest, InvokeResponse};
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tracing::info;

/// 探针 handler 背后的 invoke 执行面；生产实现为 [`MazbriBypassServiceImpl`]。
pub trait ProbeExecutor: Clone + Send + Sync + 'static {
    fn execute_invoke(&self, req: InvokeRequest) -> impl Future<Output = InvokeResponse> + Send;
}

impl ProbeExecutor for MazbriBypassServiceImpl {
    fn execute_invoke(&self, req: InvokeRequest) -> impl Future<Output = InvokeResponse> + Send {
        MazbriBypassServiceImpl::execute_invoke(self, req)
    }
}

#[derive(Clone)]
struct ProbeState<E> {
    executor: E,
    token: Arc<str>,
}

fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
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

async fn invoke_handler<E: ProbeExecutor>(
    State(state): State<ProbeState<E>>,
    headers: HeaderMap,
    Json(body): Json<ProbeInvokeRequest>,
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
    let response = state.executor.execute_invoke(body.into()).await;
    (StatusCode::OK, Json(ProbeInvokeResponse::from(response))).into_response()
}

/// 探针路由（token 鉴权 + 单一 invoke 路径）。
fn router<E: ProbeExecutor>(token: &str, executor: E) -> Router {
    let state = ProbeState {
        executor,
        token: Arc::from(token),
    };
    Router::new()
        .route(MGMT_PROBE_INVOKE_PATH, post(invoke_handler::<E>))
        .with_state(state)
}

/// 在独立端口启动管理探针 HTTP 服务；与 8888 mTLS gRPC 并行。
///
/// # Errors
///
/// 绑定失败或服务异常退出时返回错误。
pub async fn serve(cfg: &MgmtProbeConfig, service: MazbriBypassServiceImpl) -> anyhow::Result<()> {
    let addr = SocketAddr::new(cfg.host.parse()?, cfg.port);
    let app = router(&cfg.mgmt_probe_token, service);
    info!(
        %addr,
        path = MGMT_PROBE_INVOKE_PATH,
        "mazbri-bypass mgmt probe HTTP listening (token auth, internal only)"
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("mgmt probe server error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use mazbri_upstream_contracts::mgmt_probe::{ProbeInvokeRequest, ProbeTrace};
    use mazbri_upstream_contracts::shared::InvokeUpstreamMeta;
    use tower::ServiceExt;

    const TEST_TOKEN: &str = "unit-test-probe-token";

    #[derive(Clone)]
    struct FakeExecutor;

    impl ProbeExecutor for FakeExecutor {
        fn execute_invoke(
            &self,
            req: InvokeRequest,
        ) -> impl Future<Output = InvokeResponse> + Send {
            std::future::ready(InvokeResponse {
                success: true,
                error: String::new(),
                biz_error: None,
                payload_json: format!("{{\"channel\":\"{}\"}}", req.channel_id),
                upstream_meta: Some(InvokeUpstreamMeta {
                    status: 200,
                    url: "http://upstream.invalid/x".into(),
                    adapter: "http_json".into(),
                    upstream_wire_ms: 42,
                }),
                internal_retries: 0,
                retry_reasons: Vec::new(),
            })
        }
    }

    fn probe_request() -> ProbeInvokeRequest {
        ProbeInvokeRequest {
            product_id: "mgmt-probe".into(),
            tenant_id: "mgmt-probe".into(),
            channel_id: "CH-1".into(),
            request_id: "req-1".into(),
            trace: ProbeTrace {
                request_id: "req-1".into(),
                trace_id: "req-1".into(),
                span_id: "span-1".into(),
                parent_span_id: String::new(),
            },
            timeout_ms: 1000,
            payload_json: "{}".into(),
            egress_spec_hash: String::new(),
            egress_spec_json: "{}".into(),
            supplier_id: "s".into(),
            supplier_code: "S".into(),
            supplier_config_hash: String::new(),
            supplier_config_json: "{}".into(),
        }
    }

    async fn call(auth: Option<&str>, body: String) -> (StatusCode, Vec<u8>) {
        let mut builder = Request::post(MGMT_PROBE_INVOKE_PATH)
            .header(axum::http::header::CONTENT_TYPE, "application/json");
        if let Some(a) = auth {
            builder = builder.header(axum::http::header::AUTHORIZATION, a);
        }
        let resp = router(TEST_TOKEN, FakeExecutor)
            .oneshot(builder.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        (status, bytes.to_vec())
    }

    #[tokio::test]
    async fn handler_body_decodes_as_shared_contract() {
        let body = serde_json::to_string(&probe_request()).unwrap();
        let (status, bytes) = call(Some(&format!("Bearer {TEST_TOKEN}")), body).await;
        assert_eq!(status, StatusCode::OK);
        let parsed: ProbeInvokeResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(parsed.success);
        assert_eq!(parsed.payload_json, "{\"channel\":\"CH-1\"}");
        assert_eq!(parsed.upstream_meta.expect("meta").upstream_wire_ms, 42);
    }

    #[tokio::test]
    async fn handler_rejects_missing_or_wrong_token() {
        let body = serde_json::to_string(&probe_request()).unwrap();
        let (status, _) = call(None, body.clone()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = call(Some("Bearer wrong"), body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn handler_rejects_request_outside_contract() {
        let mut value = serde_json::to_value(probe_request()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), serde_json::Value::Bool(true));
        let (status, _) = call(Some(&format!("Bearer {TEST_TOKEN}")), value.to_string()).await;
        assert!(status.is_client_error(), "{status}");
    }
}
