//! `ws_unary`：经 [`crate::ws_pool::WsPool`] 借还连接的一元请求-响应。
//!
//! - **无**每请求必 `connect_async` 主路径；**无**池满回退短连。
//! - Binary 帧：**严格失败**（`binary ws frame not supported`），不半支持。
//! - 握手 headers 为稳定鉴权头；`request_id` 只写入帧 `id`，不进池键。

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use crate::http_call::{append_query, join_url};
use crate::types::{BizError, ExecuteError, OutboundResponse};
use crate::ws_pool::{auth_fingerprint, PoolKey};

use super::{AdapterCtx, AdapterOutcome, wrap_by_name};

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &ctx.input.spec;
    let operation = spec
        .operation
        .as_deref()
        .ok_or_else(|| ExecuteError::SpecInvalid("ws_unary operation missing".into()))?;
    let path = ws_path(ctx);
    let url = ws_url(ctx, &path).await?;

    // 稳定握手头：auth + spec.request；故意不注入 per-request correlation（帧 id 承担）。
    let headers_map = super::merge_request_headers(ctx.auth, spec);
    let query = super::merge_request_query(ctx.auth, spec);
    let header_pairs: Vec<(String, String)> = headers_map.into_iter().collect();
    let auth_fp = auth_fingerprint(&header_pairs, &query);
    let key = PoolKey {
        base_url: ctx.base_url.to_string(),
        path,
        auth_fp,
    };

    let mut guard = ctx.ws_pool.checkout(key, &url, &header_pairs).await?;

    let mut frame = wrap_by_name(spec, &ctx.input.payload)?;
    if let Some(obj) = frame.as_object_mut() {
        obj.entry("id")
            .or_insert_with(|| serde_json::Value::String(ctx.input.request_id.clone()));
        obj.entry("method")
            .or_insert_with(|| serde_json::Value::String(operation.to_string()));
    }
    let text = serde_json::to_string(&frame)
        .map_err(|e| ExecuteError::SpecInvalid(format!("ws frame: {e}")))?;

    let json = match send_and_wait(&mut guard, &ctx.input.request_id, text, ctx.timeout).await {
        Ok(v) => v,
        Err(e) => {
            guard.mark_broken();
            return Err(e);
        }
    };

    let code = json
        .get("code")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    if code != 0 {
        let message = json
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("ws business error")
            .to_string();
        return Ok(AdapterOutcome {
            payload: serde_json::Value::Null,
            biz_error: Some(BizError {
                code: "UPSTREAM_BIZ".into(),
                message,
                details: Some(json.clone()),
            }),
            response: synthetic_response(&url, &json),
        });
    }
    let payload = json.get("data").cloned().unwrap_or(json.clone());
    Ok(AdapterOutcome {
        payload,
        biz_error: None,
        response: synthetic_response(&url, &json),
    })
}

async fn send_and_wait(
    guard: &mut crate::ws_pool::WsConnGuard,
    request_id: &str,
    text: String,
    wait_timeout: std::time::Duration,
) -> Result<serde_json::Value, ExecuteError> {
    {
        let ws = guard.stream()?;
        ws.send(Message::Text(text.into()))
            .await
            .map_err(|e| ExecuteError::UpstreamTransport(format!("ws send: {e}")))?;
    }

    tokio::time::timeout(wait_timeout, async {
        loop {
            let next = {
                let ws = guard.stream()?;
                ws.next().await
            };
            match next {
                Some(Ok(Message::Text(t))) => {
                    let v: serde_json::Value = serde_json::from_str(&t)
                        .map_err(|e| ExecuteError::AdapterFailed(format!("ws frame json: {e}")))?;
                    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
                    if id == request_id || id.is_empty() {
                        return Ok(v);
                    }
                }
                Some(Ok(Message::Binary(_))) => {
                    return Err(ExecuteError::AdapterFailed(
                        "binary ws frame not supported".into(),
                    ));
                }
                Some(Ok(Message::Close(_))) | None => {
                    return Err(ExecuteError::UpstreamTransport(
                        "ws closed before reply".into(),
                    ));
                }
                Some(Ok(Message::Ping(p))) => {
                    let ws = guard.stream()?;
                    ws.send(Message::Pong(p))
                        .await
                        .map_err(|e| ExecuteError::UpstreamTransport(format!("ws pong: {e}")))?;
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => {
                    return Err(ExecuteError::UpstreamTransport(format!("ws: {e}")));
                }
            }
        }
    })
    .await
    .map_err(|_| ExecuteError::UpstreamTransport("ws wait exceeded timeout_ms".into()))?
}

fn ws_path(ctx: &AdapterCtx<'_>) -> String {
    let spec = &ctx.input.spec;
    spec.ws
        .as_ref()
        .and_then(|w| w.path.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or(spec.request.path.as_str())
        .to_string()
}

async fn ws_url(ctx: &AdapterCtx<'_>, path: &str) -> Result<String, ExecuteError> {
    let mut url = join_url(ctx.base_url, path);
    if url.starts_with("http://") {
        url = format!("ws://{}", url.trim_start_matches("http://"));
    } else if url.starts_with("https://") {
        url = format!("wss://{}", url.trim_start_matches("https://"));
    }
    url = append_query(&url, &super::merge_request_query(ctx.auth, &ctx.input.spec));
    if let Err(e) = ctx
        .ssrf
        .validate_url(
            &url.replace("ws://", "http://")
                .replace("wss://", "https://"),
        )
        .await
    {
        return Err(ExecuteError::SsrfBlocked(e));
    }
    Ok(url)
}

fn synthetic_response(url: &str, json: &serde_json::Value) -> OutboundResponse {
    OutboundResponse {
        status: 200,
        headers: std::collections::HashMap::default(),
        body: serde_json::to_vec(json).unwrap_or_default(),
        final_url: url.to_string(),
        method: "WS".into(),
        path: url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AppliedAuth;
    use crate::ssrf::SsrfChecker;
    use crate::types::{ExecuteMode, ExecuteOptions, InvokeInput};
    use crate::ws_pool::{WsPool, WsPoolConfig};
    use futures_util::{SinkExt, StreamExt};
    use mazbri_core::channel_egress::{ADAPTER_WS_UNARY, ChannelEgressSpec, WsUnarySpec};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::accept_async;

    fn auth_empty() -> AppliedAuth {
        AppliedAuth {
            headers: HashMap::new(),
            query: vec![],
            metadata: HashMap::new(),
            token: None,
            stale_evicted: None,
        }
    }

    fn base_spec(path: &str) -> ChannelEgressSpec {
        ChannelEgressSpec {
            adapter: ADAPTER_WS_UNARY.into(),
            base_url: "http://127.0.0.1".into(),
            operation: Some("demo.op".into()),
            ws: Some(WsUnarySpec {
                path: Some(path.into()),
                auth_in: None,
            }),
            request: mazbri_core::channel_egress::EgressRequestSpec {
                method: "GET".into(),
                path: path.into(),
                wrap: Some("none".into()),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn invoke_input(request_id: &str, spec: ChannelEgressSpec) -> InvokeInput {
        InvokeInput {
            supplier_id: "s".into(),
            supplier_code: "s".into(),
            channel_id: "c".into(),
            request_id: request_id.into(),
            payload: serde_json::json!({"k": "v"}),
            spec,
            config: None,
            timeout: Duration::from_secs(3),
            mode: ExecuteMode::Business,
            options: ExecuteOptions::default(),
            credentials: HashMap::new(),
            pipeline_keys: None,
            artifact_bytes: None,
            cache_key: None,
        }
    }

    /// 计数握手次数的 unary 回显服务：每条 TCP/WS 升级 +1。
    async fn spawn_counting_ws(handshakes: Arc<AtomicUsize>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    break;
                };
                let handshakes = handshakes.clone();
                tokio::spawn(async move {
                    let Ok(mut ws) = accept_async(sock).await else {
                        return;
                    };
                    handshakes.fetch_add(1, Ordering::SeqCst);
                    while let Some(Ok(msg)) = ws.next().await {
                        match msg {
                            Message::Text(t) => {
                                let v: serde_json::Value =
                                    serde_json::from_str(&t).unwrap_or_default();
                                let id = v.get("id").cloned().unwrap_or(serde_json::Value::Null);
                                let reply = serde_json::json!({
                                    "id": id,
                                    "code": 0,
                                    "data": {"ok": true}
                                });
                                if ws
                                    .send(Message::Text(reply.to_string().into()))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Message::Ping(p) => {
                                let _ = ws.send(Message::Pong(p)).await;
                            }
                            Message::Close(_) => break,
                            _ => {}
                        }
                    }
                });
            }
        });
        format!("http://{addr}")
    }

    /// 慢响应服务：握手后 sleep，用于打满并发。
    async fn spawn_slow_ws(handshakes: Arc<AtomicUsize>, delay: Duration) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    break;
                };
                let handshakes = handshakes.clone();
                tokio::spawn(async move {
                    let Ok(mut ws) = accept_async(sock).await else {
                        return;
                    };
                    handshakes.fetch_add(1, Ordering::SeqCst);
                    while let Some(Ok(msg)) = ws.next().await {
                        if let Message::Text(t) = msg {
                            tokio::time::sleep(delay).await;
                            let v: serde_json::Value =
                                serde_json::from_str(&t).unwrap_or_default();
                            let id = v.get("id").cloned().unwrap_or(serde_json::Value::Null);
                            let reply = serde_json::json!({"id": id, "code": 0, "data": {}});
                            let _ = ws.send(Message::Text(reply.to_string().into())).await;
                        }
                    }
                });
            }
        });
        format!("http://{addr}")
    }

    async fn run_once(
        pool: &WsPool,
        base: &str,
        request_id: &str,
    ) -> Result<AdapterOutcome, ExecuteError> {
        let mut input = invoke_input(request_id, base_spec("/ws"));
        input.spec.base_url = base.to_string();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let ssrf = SsrfChecker::with_allow_private(true);
        let auth = auth_empty();
        let pipeline_keys = crate::crypto_pipeline::PipelineKeys::empty();
        let registry = mazbri_crypto::AlgorithmRegistry::default_registry();
        let ctx = AdapterCtx {
            client: &client,
            ssrf: &ssrf,
            input: &input,
            base_url: base,
            auth: &auth,
            timeout: Duration::from_secs(3),
            ws_pool: pool,
            pipeline_keys: &pipeline_keys,
            registry: &registry,
        };
        invoke(&ctx).await
    }

    #[tokio::test]
    async fn sequential_invokes_reuse_connection() {
        let hits = Arc::new(AtomicUsize::new(0));
        let base = spawn_counting_ws(hits.clone()).await;
        let pool = WsPool::new(WsPoolConfig {
            handshake_timeout: Duration::from_secs(2),
            max_concurrent: 8,
            max_concurrent_per_key: 4,
            max_idle_per_key: 2,
            idle_ttl: Duration::from_secs(30),
        });

        let n = 12usize;
        for i in 0..n {
            let out = run_once(&pool, &base, &format!("rid-{i}"))
                .await
                .expect("invoke");
            assert!(out.biz_error.is_none());
            assert_eq!(
                out.payload.get("ok").and_then(serde_json::Value::as_bool),
                Some(true)
            );
        }

        let handshakes = hits.load(Ordering::SeqCst);
        assert!(
            handshakes < n,
            "handshakes={handshakes} must be << N={n} (pool reuse)"
        );
        assert_eq!(handshakes, 1, "sequential should reuse single connection");
        assert!(pool.reuses() >= n - 1, "reuses={}", pool.reuses());
        assert_eq!(pool.handshakes(), 1);
    }

    #[tokio::test]
    async fn exceeding_global_concurrency_fails_fast() {
        let hits = Arc::new(AtomicUsize::new(0));
        let base = spawn_slow_ws(hits.clone(), Duration::from_millis(800)).await;
        let pool = WsPool::new(WsPoolConfig {
            handshake_timeout: Duration::from_secs(2),
            max_concurrent: 2,
            max_concurrent_per_key: 8,
            max_idle_per_key: 2,
            idle_ttl: Duration::from_secs(30),
        });

        let mut joins = Vec::new();
        for i in 0..8 {
            let pool = pool.clone();
            let base = base.clone();
            joins.push(tokio::spawn(async move {
                run_once(&pool, &base, &format!("c-{i}")).await
            }));
        }
        let mut ok = 0usize;
        let mut limited = 0usize;
        for j in joins {
            match j.await.expect("join") {
                Ok(_) => ok += 1,
                Err(e) => {
                    let msg = e.to_string();
                    assert!(
                        msg.contains("ws concurrency limit exceeded"),
                        "unexpected err: {msg}"
                    );
                    limited += 1;
                }
            }
        }
        assert!(ok >= 1, "at least some should run");
        assert!(limited >= 1, "excess must fail fast; ok={ok} limited={limited}");
        assert!(pool.rejects() >= 1);
    }
}
