//! `ws_unary`：经 [`crate::ws_pool::WsPool`] 借还连接的一元请求-响应。
//!
//! - **无**每请求必 `connect_async` 主路径；**无**池满回退短连。
//! - Binary 帧：**严格失败**（`binary ws frame not supported`），不半支持。
//! - 握手 headers 为稳定鉴权头；`request_id` 只写入帧 `id`，不进池键。

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use crate::http_call::{append_query, join_url};
use crate::retry_budget::RetryBudget;
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

    let mut frame = wrap_by_name(spec, &ctx.input.payload)?;
    if let Some(obj) = frame.as_object_mut() {
        obj.entry("id")
            .or_insert_with(|| serde_json::Value::String(ctx.input.request_id.clone()));
        obj.entry("method")
            .or_insert_with(|| serde_json::Value::String(operation.to_string()));
    }
    let text = serde_json::to_string(&frame)
        .map_err(|e| ExecuteError::SpecInvalid(format!("ws frame: {e}")))?;

    let json = match exchange(ctx, &key, &url, &header_pairs, text).await? {
        Exchange::Reply(json) => json,
        Exchange::Rejected(outcome) => return Ok(*outcome),
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

enum Exchange {
    Reply(serde_json::Value),
    /// 新握手的连接回复前被关闭且命中 `ws_close_before_reply` 规则。
    Rejected(Box<AdapterOutcome>),
}

/// 借连接、发帧、等回复。复用连接在回复前断开（写失败 / Close / EOF，且未收到任何帧）
/// 时，在预算允许下换一条新握手的连接重试一次；超时、已收到帧、新连接失败均不重试。
async fn exchange(
    ctx: &AdapterCtx<'_>,
    key: &PoolKey,
    url: &str,
    header_pairs: &[(String, String)],
    text: String,
) -> Result<Exchange, ExecuteError> {
    let rid = &ctx.input.request_id;
    let mut guard = ctx.ws_pool.checkout(key.clone(), url, header_pairs).await?;
    let err = match send_and_wait(&mut guard, rid, text.clone(), ctx.timeout).await {
        Ok(json) => return Ok(Exchange::Reply(json)),
        Err(e) => e,
    };
    guard.mark_broken();
    let reused = guard.reused();
    drop(guard);
    let stale = reused
        && matches!(
            err,
            WsWaitError::SendFailed(_) | WsWaitError::ClosedBeforeReply(_)
        );
    if !(stale && ctx.ws_stale_retry.is_some_and(RetryBudget::try_acquire)) {
        return closed_or_error(ctx, err, reused, url);
    }
    // 重试路径单独装箱：不让很少走到的第二轮握手与收发撑大每次调用的 future。
    Box::pin(retry_on_fresh(ctx, key, url, header_pairs, text)).await
}

async fn retry_on_fresh(
    ctx: &AdapterCtx<'_>,
    key: &PoolKey,
    url: &str,
    header_pairs: &[(String, String)],
    text: String,
) -> Result<Exchange, ExecuteError> {
    let mut fresh = ctx
        .ws_pool
        .checkout_fresh(key.clone(), url, header_pairs)
        .await?;
    match send_and_wait(&mut fresh, &ctx.input.request_id, text, ctx.timeout).await {
        Ok(json) => Ok(Exchange::Reply(json)),
        Err(e) => {
            fresh.mark_broken();
            closed_or_error(ctx, e, false, url)
        }
    }
}

/// 失败收尾：新握手连接回复前被关闭、规则开启且带令牌时判定为认证被拒（交给引擎换票重试），
/// 其余情况返回传输错误。
fn closed_or_error(
    ctx: &AdapterCtx<'_>,
    err: WsWaitError,
    reused: bool,
    url: &str,
) -> Result<Exchange, ExecuteError> {
    if matches!(err, WsWaitError::ClosedBeforeReply(_))
        && !reused
        && ctx.invalidation.ws_close_before_reply
        && let Some(token) = ctx.auth.token.as_deref()
    {
        return Ok(Exchange::Rejected(Box::new(AdapterOutcome {
            auth_rejection: Some(AuthRejection {
                trigger: AuthRejectTrigger::WsClosedBeforeReply,
                token: token.to_string(),
            }),
            payload: serde_json::Value::Null,
            biz_error: None,
            response: closed_response(url),
        })));
    }
    Err(err.into())
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
        run_with(pool, base, request_id, &RunOpts::plain()).await
    }

    /// 单次调用的可变条件：令牌、失效规则、陈旧连接重试预算、等待超时。
    struct RunOpts<'a> {
        auth: AppliedAuth,
        invalidation: mazbri_core::AuthInvalidationSpec,
        stale_retry: Option<&'a RetryBudget>,
        timeout: Duration,
    }

    impl RunOpts<'_> {
        fn plain() -> Self {
            Self {
                auth: auth_empty(),
                invalidation: mazbri_core::AuthInvalidationSpec::default(),
                stale_retry: None,
                timeout: Duration::from_secs(3),
            }
        }
    }

    async fn run_with(
        pool: &WsPool,
        base: &str,
        request_id: &str,
        opts: &RunOpts<'_>,
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
            auth: &opts.auth,
            timeout: opts.timeout,
            ws_pool: pool,
            pipeline_keys: &pipeline_keys,
            registry: &registry,
            invalidation: &opts.invalidation,
            ws_stale_retry: opts.stale_retry,
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
            liveness_check_on_checkout: true,
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
            liveness_check_on_checkout: true,
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

    /// 正常回复额度用完后，再收到请求时的动作。
    #[derive(Clone, Copy, PartialEq)]
    enum Then {
        /// 发 Close 帧后断开。
        Close,
        /// 不发 Close 直接断 TCP。
        Drop,
        /// 先回一帧 `id` 不匹配的业务帧再 Close。
        StrayThenClose,
        /// 不回复也不关闭。
        Silent,
    }

    /// 每次正常回复后的空闲期动作。
    #[derive(Clone, Copy)]
    enum Idle {
        Keep,
        Close,
        Ping,
    }

    /// 脚本化服务端：每条连接先正常回复 `replies` 次，之后按 `then` 处理；
    /// `kill` 触发时所有现存连接立即关闭（模拟上游重启）。
    #[derive(Clone, Copy)]
    struct Script {
        replies: usize,
        then: Then,
        idle: Idle,
        reply_delay: Duration,
    }

    const CLOSE_NOW: Script = Script {
        replies: 0,
        then: Then::Close,
        idle: Idle::Keep,
        reply_delay: Duration::ZERO,
    };

    const ALWAYS_REPLY: Script = Script {
        replies: usize::MAX,
        ..CLOSE_NOW
    };

    struct ScriptedWs {
        base: String,
        handshakes: Arc<AtomicUsize>,
        kill: tokio::sync::broadcast::Sender<()>,
    }

    async fn spawn_script_ws(script: Script) -> ScriptedWs {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handshakes = Arc::new(AtomicUsize::new(0));
        let (kill, _) = tokio::sync::broadcast::channel::<()>(4);
        let (hs, kill_tx) = (handshakes.clone(), kill.clone());
        tokio::spawn(async move {
            while let Ok((sock, _)) = listener.accept().await {
                let hs = hs.clone();
                let kill_rx = kill_tx.subscribe();
                tokio::spawn(serve_script(sock, script, hs, kill_rx));
            }
        });
        ScriptedWs {
            base: format!("http://{addr}"),
            handshakes,
            kill,
        }
    }

    async fn serve_script(
        sock: tokio::net::TcpStream,
        script: Script,
        handshakes: Arc<AtomicUsize>,
        mut kill: tokio::sync::broadcast::Receiver<()>,
    ) {
        let Ok(mut ws) = accept_async(sock).await else {
            return;
        };
        handshakes.fetch_add(1, Ordering::SeqCst);
        let mut served = 0usize;
        loop {
            let msg = tokio::select! {
                _ = kill.recv() => {
                    let _ = ws.close(None).await;
                    return;
                }
                msg = ws.next() => msg,
            };
            let Some(Ok(msg)) = msg else { return };
            let Message::Text(t) = msg else { continue };
            let v: serde_json::Value = serde_json::from_str(&t).unwrap_or_default();
            if served < script.replies {
                served += 1;
                tokio::time::sleep(script.reply_delay).await;
                let id = v.get("id").cloned().unwrap_or(serde_json::Value::Null);
                let reply = serde_json::json!({"id": id, "code": 0, "data": {}});
                let _ = ws.send(Message::Text(reply.to_string().into())).await;
                match script.idle {
                    Idle::Keep => {}
                    Idle::Ping => {
                        let _ = ws.send(Message::Ping(b"hb".to_vec().into())).await;
                    }
                    Idle::Close => {
                        let _ = ws.close(None).await;
                        return;
                    }
                }
                continue;
            }
            match script.then {
                Then::Silent => continue,
                Then::Drop => return,
                Then::StrayThenClose => {
                    let stray = serde_json::json!({"id": "someone-else", "code": 0});
                    let _ = ws.send(Message::Text(stray.to_string().into())).await;
                }
                Then::Close => {}
            }
            let _ = ws.close(None).await;
            return;
        }
    }

    fn auth_token() -> AppliedAuth {
        AppliedAuth {
            token: Some("tok-ws".into()),
            ..auth_empty()
        }
    }

    fn with_token(close_rule: bool) -> RunOpts<'static> {
        RunOpts {
            auth: auth_token(),
            invalidation: mazbri_core::AuthInvalidationSpec {
                ws_close_before_reply: close_rule,
                ..mazbri_core::AuthInvalidationSpec::default()
            },
            ..RunOpts::plain()
        }
    }

    fn with_stale_retry(budget: &RetryBudget) -> RunOpts<'_> {
        RunOpts {
            stale_retry: Some(budget),
            ..RunOpts::plain()
        }
    }

    fn generous_budget() -> RetryBudget {
        RetryBudget::new(1.0, 100, Duration::from_mins(1))
    }

    fn pool_with(liveness_check_on_checkout: bool) -> WsPool {
        WsPool::new(WsPoolConfig {
            handshake_timeout: Duration::from_secs(2),
            max_concurrent: 8,
            max_concurrent_per_key: 4,
            max_idle_per_key: 4,
            idle_ttl: Duration::from_secs(30),
            liveness_check_on_checkout,
        })
    }

    fn pool() -> WsPool {
        pool_with(true)
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    #[tokio::test]
    async fn fresh_conn_closed_before_reply_is_auth_rejection_when_enabled() {
        for then in [Then::Close, Then::Drop] {
            let srv = spawn_script_ws(Script { then, ..CLOSE_NOW }).await;
            let out = run_with(&pool(), &srv.base, "r1", &with_token(true))
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
        let srv = spawn_script_ws(CLOSE_NOW).await;
        let err = run_with(&pool(), &srv.base, "r1", &with_token(false))
            .await
            .err()
            .expect("rule off");
        assert_eq!(err.code(), "UPSTREAM_TRANSPORT");

        let no_token = RunOpts {
            auth: auth_empty(),
            ..with_token(true)
        };
        let err = run_with(&pool(), &srv.base, "r2", &no_token)
            .await
            .err()
            .expect("no token");
        assert_eq!(err.code(), "UPSTREAM_TRANSPORT");
    }

    #[tokio::test]
    async fn idle_close_is_discarded_on_checkout() {
        let srv = spawn_script_ws(Script {
            idle: Idle::Close,
            ..ALWAYS_REPLY
        })
        .await;
        let pool = pool();
        run_with(&pool, &srv.base, "r1", &RunOpts::plain())
            .await
            .expect("first");
        settle().await;
        run_with(&pool, &srv.base, "r2", &RunOpts::plain())
            .await
            .expect("dead idle conn discarded, new handshake succeeds");
        assert_eq!(pool.stale_discards(), 1);
        assert_eq!(pool.reuses(), 0);
        assert_eq!(srv.handshakes.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn idle_ping_is_answered_and_conn_reused() {
        let srv = spawn_script_ws(Script {
            idle: Idle::Ping,
            ..ALWAYS_REPLY
        })
        .await;
        let pool = pool();
        run_with(&pool, &srv.base, "r1", &RunOpts::plain())
            .await
            .expect("first");
        settle().await;
        run_with(&pool, &srv.base, "r2", &RunOpts::plain())
            .await
            .expect("second");
        assert_eq!(pool.stale_discards(), 0);
        assert_eq!(pool.reuses(), 1);
        assert_eq!(srv.handshakes.load(Ordering::SeqCst), 1);
    }

    /// 复用连接在收到请求后才断开（存活检查与发送之间的竞态）。
    const RACE: Script = Script {
        replies: 1,
        ..CLOSE_NOW
    };

    async fn race(
        pool: &WsPool,
        opts: &RunOpts<'_>,
    ) -> (Result<AdapterOutcome, ExecuteError>, usize) {
        let srv = spawn_script_ws(RACE).await;
        run_with(pool, &srv.base, "r1", opts)
            .await
            .expect("first reply");
        let second = run_with(pool, &srv.base, "r2", opts).await;
        (second, srv.handshakes.load(Ordering::SeqCst))
    }

    #[tokio::test]
    async fn reused_conn_closed_before_reply_retries_on_fresh_conn() {
        for liveness in [true, false] {
            let budget = generous_budget();
            let pool = pool_with(liveness);
            let (second, handshakes) = race(&pool, &with_stale_retry(&budget)).await;
            let out = second.expect("retried on fresh connection");
            assert!(out.biz_error.is_none());
            assert_eq!(handshakes, 2, "liveness={liveness}");
            assert_eq!(pool.reuses(), 1);
        }
    }

    #[tokio::test]
    async fn reused_conn_close_not_retried_when_disabled_or_budget_exhausted() {
        let (second, handshakes) = race(&pool(), &RunOpts::plain()).await;
        assert_eq!(second.err().map(|e| e.code()), Some("UPSTREAM_TRANSPORT"));
        assert_eq!(handshakes, 1);

        let empty = RetryBudget::new(0.2, 0, Duration::from_mins(1));
        let (second, handshakes) = race(&pool(), &with_stale_retry(&empty)).await;
        assert_eq!(second.err().map(|e| e.code()), Some("UPSTREAM_TRANSPORT"));
        assert_eq!(handshakes, 1);
    }

    #[tokio::test]
    async fn reused_conn_close_is_never_auth_rejection() {
        let (second, _) = race(&pool(), &with_token(true)).await;
        assert_eq!(second.err().map(|e| e.code()), Some("UPSTREAM_TRANSPORT"));
    }

    #[tokio::test]
    async fn timeout_and_received_frame_are_not_retried() {
        for script in [
            Script {
                replies: 1,
                then: Then::Silent,
                ..CLOSE_NOW
            },
            Script {
                replies: 1,
                then: Then::StrayThenClose,
                ..CLOSE_NOW
            },
        ] {
            let budget = generous_budget();
            let srv = spawn_script_ws(script).await;
            let pool = pool();
            let opts = RunOpts {
                timeout: Duration::from_millis(300),
                ..with_stale_retry(&budget)
            };
            run_with(&pool, &srv.base, "r1", &opts)
                .await
                .expect("first");
            let err = run_with(&pool, &srv.base, "r2", &opts)
                .await
                .err()
                .expect("not retried");
            assert_eq!(err.code(), "UPSTREAM_TRANSPORT");
            if script.then == Then::Silent {
                assert!(
                    err.to_string().contains("ws wait exceeded timeout_ms"),
                    "{err}"
                );
            }
            assert_eq!(srv.handshakes.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn fresh_conn_close_after_stray_frame_is_not_auth_rejection() {
        let srv = spawn_script_ws(Script {
            then: Then::StrayThenClose,
            ..CLOSE_NOW
        })
        .await;
        let err = run_with(&pool(), &srv.base, "r1", &with_token(true))
            .await
            .err()
            .expect("frame was received, so close is plain transport");
        assert_eq!(err.code(), "UPSTREAM_TRANSPORT");
    }

    #[tokio::test]
    async fn first_request_after_upstream_restart_succeeds() {
        let srv = spawn_script_ws(Script {
            reply_delay: Duration::from_millis(100),
            ..ALWAYS_REPLY
        })
        .await;
        let pool = pool();
        let mut joins = Vec::new();
        for i in 0..4 {
            let (pool, base) = (pool.clone(), srv.base.clone());
            joins.push(tokio::spawn(async move {
                run_with(&pool, &base, &format!("w{i}"), &RunOpts::plain()).await
            }));
        }
        for j in joins {
            j.await.unwrap().expect("warm-up");
        }
        assert_eq!(srv.handshakes.load(Ordering::SeqCst), 4);
        srv.kill.send(()).unwrap();
        settle().await;
        run_with(&pool, &srv.base, "after-restart", &RunOpts::plain())
            .await
            .expect("first request after restart succeeds");
        assert_eq!(pool.stale_discards(), 4);
        assert_eq!(srv.handshakes.load(Ordering::SeqCst), 5);
    }
}
