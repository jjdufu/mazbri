//! `ws_unary`：经 [`crate::ws_pool::WsPool`] 借还连接的一元请求-响应。
//!
//! - **无**每请求必 `connect_async` 主路径；**无**池满回退短连。
//! - Binary 帧：**严格失败**（`binary ws frame not supported`），不半支持。
//! - 握手 headers 为稳定鉴权头；`request_id` 只写入帧 `id`，不进池键。

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use crate::http_call::{append_query, join_url};
use crate::types::{AuthRejectTrigger, AuthRejection, BizError, ExecuteError, OutboundResponse};
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
            if matches!(e, WsWaitError::ClosedBeforeReply(_))
                && !guard.reused()
                && ctx.invalidation.ws_close_before_reply
                && let Some(token) = ctx.auth.token.as_deref()
            {
                // 新握手的连接未回任何帧即被关闭：按规则判定为认证被拒，交给引擎换票重试。
                return Ok(AdapterOutcome {
                    auth_rejection: Some(AuthRejection {
                        trigger: AuthRejectTrigger::WsClosedBeforeReply,
                        token: token.to_string(),
                    }),
                    payload: serde_json::Value::Null,
                    biz_error: None,
                    response: closed_response(&url),
                });
            }
            return Err(e.into());
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
            auth_rejection: None,
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
        auth_rejection: None,
        payload,
        biz_error: None,
        response: synthetic_response(&url, &json),
    })
}

/// 发送并等待回复的失败分类（决定是否属于「回复前被关闭」）。
#[derive(Debug)]
enum WsWaitError {
    /// 写帧失败。
    SendFailed(String),
    /// 收到 Close / EOF / 连接被重置，且此前未收到任何业务帧。
    ClosedBeforeReply(String),
    /// 其余错误（超时、帧解析失败、已收到过帧后断开等），一律不重试。
    Other(ExecuteError),
}

impl From<WsWaitError> for ExecuteError {
    fn from(e: WsWaitError) -> Self {
        match e {
            WsWaitError::SendFailed(m) => ExecuteError::UpstreamTransport(format!("ws send: {m}")),
            WsWaitError::ClosedBeforeReply(m) => ExecuteError::UpstreamTransport(m),
            WsWaitError::Other(e) => e,
        }
    }
}

async fn send_and_wait(
    guard: &mut crate::ws_pool::WsConnGuard,
    request_id: &str,
    text: String,
    wait_timeout: std::time::Duration,
) -> Result<serde_json::Value, WsWaitError> {
    {
        let ws = guard.stream().map_err(WsWaitError::Other)?;
        ws.send(Message::Text(text.into()))
            .await
            .map_err(|e| WsWaitError::SendFailed(e.to_string()))?;
    }

    let mut frames_received = 0usize;
    let closed = |frames: usize, msg: String| {
        if frames == 0 {
            WsWaitError::ClosedBeforeReply(msg)
        } else {
            WsWaitError::Other(ExecuteError::UpstreamTransport(msg))
        }
    };
    tokio::time::timeout(wait_timeout, async {
        loop {
            let next = {
                let ws = guard.stream().map_err(WsWaitError::Other)?;
                ws.next().await
            };
            match next {
                Some(Ok(Message::Text(t))) => {
                    frames_received += 1;
                    let v: serde_json::Value = serde_json::from_str(&t).map_err(|e| {
                        WsWaitError::Other(ExecuteError::AdapterFailed(format!(
                            "ws frame json: {e}"
                        )))
                    })?;
                    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
                    if id == request_id || id.is_empty() {
                        return Ok(v);
                    }
                }
                Some(Ok(Message::Binary(_))) => {
                    return Err(WsWaitError::Other(ExecuteError::AdapterFailed(
                        "binary ws frame not supported".into(),
                    )));
                }
                Some(Ok(Message::Close(_))) | None => {
                    return Err(closed(frames_received, "ws closed before reply".into()));
                }
                Some(Ok(Message::Ping(p))) => {
                    let ws = guard.stream().map_err(WsWaitError::Other)?;
                    ws.send(Message::Pong(p)).await.map_err(|e| {
                        WsWaitError::Other(ExecuteError::UpstreamTransport(format!("ws pong: {e}")))
                    })?;
                }
                Some(Ok(_)) => {}
                Some(Err(e)) if is_disconnect(&e) => {
                    return Err(closed(
                        frames_received,
                        format!("ws closed before reply: {e}"),
                    ));
                }
                Some(Err(e)) => {
                    return Err(WsWaitError::Other(ExecuteError::UpstreamTransport(
                        format!("ws: {e}"),
                    )));
                }
            }
        }
    })
    .await
    .map_err(|_| {
        WsWaitError::Other(ExecuteError::UpstreamTransport(
            "ws wait exceeded timeout_ms".into(),
        ))
    })?
}

/// 对端断开类错误（关闭、重置、IO 中断），区别于协议/解析错误。
fn is_disconnect(e: &tokio_tungstenite::tungstenite::Error) -> bool {
    use tokio_tungstenite::tungstenite::Error as WsErr;
    use tokio_tungstenite::tungstenite::error::ProtocolError;
    matches!(
        e,
        WsErr::ConnectionClosed
            | WsErr::AlreadyClosed
            | WsErr::Io(_)
            | WsErr::Protocol(ProtocolError::ResetWithoutClosingHandshake)
    )
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

/// 回复前被关闭时的占位响应：没有 HTTP 状态（0）、没有响应体。
fn closed_response(url: &str) -> OutboundResponse {
    OutboundResponse {
        status: 0,
        headers: std::collections::HashMap::default(),
        body: Vec::new(),
        final_url: url.to_string(),
        method: "WS".into(),
        path: url.to_string(),
    }
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
        let invalidation = mazbri_core::AuthInvalidationSpec::default();
        run_with(pool, base, request_id, &auth_empty(), &invalidation).await
    }

    async fn run_with(
        pool: &WsPool,
        base: &str,
        request_id: &str,
        auth: &AppliedAuth,
        invalidation: &mazbri_core::AuthInvalidationSpec,
    ) -> Result<AdapterOutcome, ExecuteError> {
        let mut input = invoke_input(request_id, base_spec("/ws"));
        input.spec.base_url = base.to_string();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let ssrf = SsrfChecker::with_allow_private(true);
        let pipeline_keys = crate::crypto_pipeline::PipelineKeys::empty();
        let registry = mazbri_crypto::AlgorithmRegistry::default_registry();
        let ctx = AdapterCtx {
            client: &client,
            ssrf: &ssrf,
            input: &input,
            base_url: base,
            auth,
            timeout: Duration::from_secs(3),
            ws_pool: pool,
            pipeline_keys: &pipeline_keys,
            registry: &registry,
            invalidation,
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

    /// 脚本化服务端：每条连接先正常回复 `replies` 次；之后收到请求时，
    /// 可先回一帧 `id` 不匹配的业务帧（`stray_frame`），再关闭（`graceful` 发 Close，否则直接断 TCP）。
    #[derive(Clone, Copy)]
    struct CloseScript {
        replies: usize,
        stray_frame: bool,
        graceful: bool,
    }

    async fn spawn_closing_ws(script: CloseScript) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let Ok(mut ws) = accept_async(sock).await else {
                        return;
                    };
                    let mut served = 0usize;
                    while let Some(Ok(msg)) = ws.next().await {
                        let Message::Text(t) = msg else { continue };
                        let v: serde_json::Value = serde_json::from_str(&t).unwrap_or_default();
                        if served < script.replies {
                            served += 1;
                            let id = v.get("id").cloned().unwrap_or(serde_json::Value::Null);
                            let reply = serde_json::json!({"id": id, "code": 0, "data": {}});
                            let _ = ws.send(Message::Text(reply.to_string().into())).await;
                            continue;
                        }
                        if script.stray_frame {
                            let stray = serde_json::json!({"id": "someone-else", "code": 0});
                            let _ = ws.send(Message::Text(stray.to_string().into())).await;
                        }
                        if script.graceful {
                            let _ = ws.close(None).await;
                        }
                        return;
                    }
                });
            }
        });
        format!("http://{addr}")
    }

    fn auth_token() -> AppliedAuth {
        AppliedAuth {
            token: Some("tok-ws".into()),
            ..auth_empty()
        }
    }

    fn close_rule(on: bool) -> mazbri_core::AuthInvalidationSpec {
        mazbri_core::AuthInvalidationSpec {
            ws_close_before_reply: on,
            ..mazbri_core::AuthInvalidationSpec::default()
        }
    }

    fn pool() -> WsPool {
        WsPool::new(WsPoolConfig {
            handshake_timeout: Duration::from_secs(2),
            max_concurrent: 8,
            max_concurrent_per_key: 4,
            max_idle_per_key: 2,
            idle_ttl: Duration::from_secs(30),
        })
    }

    const CLOSE_NOW: CloseScript = CloseScript {
        replies: 0,
        stray_frame: false,
        graceful: true,
    };

    #[tokio::test]
    async fn fresh_conn_closed_before_reply_is_auth_rejection_when_enabled() {
        for graceful in [true, false] {
            let base = spawn_closing_ws(CloseScript {
                graceful,
                ..CLOSE_NOW
            })
            .await;
            let out = run_with(&pool(), &base, "r1", &auth_token(), &close_rule(true))
                .await
                .expect("closed-before-reply must surface as auth rejection");
            let rejection = out.auth_rejection.expect("rejection");
            assert_eq!(rejection.trigger, AuthRejectTrigger::WsClosedBeforeReply);
            assert_eq!(rejection.token, "tok-ws");
            assert_eq!(out.response.status, 0);
        }
    }

    #[tokio::test]
    async fn closed_before_reply_is_transport_error_when_rule_off_or_no_token() {
        let base = spawn_closing_ws(CLOSE_NOW).await;
        let err = run_with(&pool(), &base, "r1", &auth_token(), &close_rule(false))
            .await
            .err()
            .expect("rule off");
        assert_eq!(err.code(), "UPSTREAM_TRANSPORT");

        let err = run_with(&pool(), &base, "r2", &auth_empty(), &close_rule(true))
            .await
            .err()
            .expect("no token");
        assert_eq!(err.code(), "UPSTREAM_TRANSPORT");
    }

    #[tokio::test]
    async fn close_after_stray_frame_is_not_auth_rejection() {
        let base = spawn_closing_ws(CloseScript {
            stray_frame: true,
            ..CLOSE_NOW
        })
        .await;
        let err = run_with(&pool(), &base, "r1", &auth_token(), &close_rule(true))
            .await
            .err()
            .expect("frame was received, so close is plain transport");
        assert_eq!(err.code(), "UPSTREAM_TRANSPORT");
    }

    #[tokio::test]
    async fn reused_conn_closed_before_reply_is_transport_error() {
        let base = spawn_closing_ws(CloseScript {
            replies: 1,
            ..CLOSE_NOW
        })
        .await;
        let pool = pool();
        let first = run_with(&pool, &base, "r1", &auth_token(), &close_rule(true))
            .await
            .expect("first reply");
        assert!(first.auth_rejection.is_none());
        let err = run_with(&pool, &base, "r2", &auth_token(), &close_rule(true))
            .await
            .err()
            .expect("reused connection close is not an auth signal");
        assert_eq!(err.code(), "UPSTREAM_TRANSPORT");
        assert_eq!(pool.reuses(), 1);
    }
}
