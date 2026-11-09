//! 引擎「认证被拒 → 作废令牌 → 换票 → 只重试一次」与重试预算的端到端测试。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use mazbri_core::channel_egress::ADAPTER_JSONRPC;
use mazbri_core::{
    ADAPTER_HTTP_JSON, AUTH_API_KEY, AUTH_OAUTH_CC_FORM, AuthInvalidationSpec, AuthSpec,
    ChannelEgressSpec, EgressRequestSpec,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    ExecuteMode, ExecuteOptions, InvokeInput, InvokeResult, RuntimeEngine, RuntimeOptions,
    SsrfChecker,
};

/// 模拟上游：换票接口第 n 次签发 `T{n}`；业务接口只接受编号 ≥ `valid_from` 的令牌。
#[derive(Default)]
struct Upstream {
    token_hits: AtomicUsize,
    api_hits: AtomicUsize,
    valid_from: AtomicUsize,
    token_fails: std::sync::atomic::AtomicBool,
    /// true：以 JSON-RPC `error.code=-32001`（HTTP 200）拒绝；false：HTTP 401。
    jsonrpc: bool,
    last_token: std::sync::Mutex<String>,
}

impl Upstream {
    fn new(valid_from: usize, jsonrpc: bool) -> Arc<Self> {
        Arc::new(Self {
            valid_from: AtomicUsize::new(valid_from),
            jsonrpc,
            ..Self::default()
        })
    }

    fn respond(&self, path: &str, headers: &HashMap<String, String>) -> (u16, String) {
        if path.starts_with("/oauth/token") {
            if self.token_fails.load(Ordering::SeqCst) {
                return (500, r#"{"error":"down"}"#.into());
            }
            let n = self.token_hits.fetch_add(1, Ordering::SeqCst) + 1;
            return (
                200,
                format!(r#"{{"access_token":"T{n}","expires_in":3600}}"#),
            );
        }
        self.api_hits.fetch_add(1, Ordering::SeqCst);
        let token = headers
            .get("authorization")
            .and_then(|v| v.rsplit(' ').next())
            .unwrap_or_default()
            .to_string();
        let accepted = token
            .strip_prefix('T')
            .and_then(|n| n.parse::<usize>().ok())
            .is_some_and(|n| n >= self.valid_from.load(Ordering::SeqCst));
        *self.last_token.lock().unwrap() = token;
        match (accepted, self.jsonrpc) {
            (true, true) => (
                200,
                r#"{"jsonrpc":"2.0","id":"1","result":{"ok":true}}"#.into(),
            ),
            (true, false) => (200, r#"{"ok":true}"#.into()),
            (false, true) => (
                200,
                r#"{"jsonrpc":"2.0","id":"1","error":{"code":-32001,"message":"token expired"}}"#
                    .into(),
            ),
            (false, false) => (401, r#"{"error":"invalid_token"}"#.into()),
        }
    }

    fn api_hits(&self) -> usize {
        self.api_hits.load(Ordering::SeqCst)
    }

    fn token_hits(&self) -> usize {
        self.token_hits.load(Ordering::SeqCst)
    }
}

async fn read_request(
    sock: &mut tokio::net::TcpStream,
) -> Option<(String, HashMap<String, String>)> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let n = sock.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = head.lines();
    let path = lines.next()?.split(' ').nth(1)?.to_string();
    let headers: HashMap<String, String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let len: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    while buf.len() < header_end + len {
        let n = sock.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Some((path, headers))
}

async fn spawn(upstream: Arc<Upstream>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let upstream = upstream.clone();
            tokio::spawn(async move {
                let Some((path, headers)) = read_request(&mut sock).await else {
                    return;
                };
                let (status, body) = upstream.respond(&path, &headers);
                let resp = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}")
}

fn engine(options: RuntimeOptions) -> RuntimeEngine {
    let client = Arc::new(reqwest::Client::builder().no_proxy().build().unwrap());
    RuntimeEngine::new(client, SsrfChecker::with_allow_private(true)).with_options(options)
}

fn ticket_auth(base: &str) -> AuthSpec {
    AuthSpec {
        profile: AUTH_OAUTH_CC_FORM.into(),
        token_url: Some(format!("{base}/oauth/token")),
        ..Default::default()
    }
}

fn input(base: &str, adapter: &str, auth: AuthSpec) -> InvokeInput {
    InvokeInput {
        supplier_id: "s".into(),
        supplier_code: "s".into(),
        channel_id: "c".into(),
        request_id: "1".into(),
        payload: serde_json::json!({"k": "v"}),
        spec: ChannelEgressSpec {
            adapter: adapter.into(),
            base_url: base.into(),
            operation: Some("demo.op".into()),
            auth: Some(auth),
            request: EgressRequestSpec {
                method: "POST".into(),
                path: "/api".into(),
                ..Default::default()
            },
            ..Default::default()
        },
        config: None,
        timeout: Duration::from_secs(3),
        mode: ExecuteMode::Production,
        options: ExecuteOptions::default(),
        credentials: HashMap::new(),
        pipeline_keys: None,
        artifact_bytes: None,
        cache_key: None,
    }
}

fn status(result: &InvokeResult) -> Option<u16> {
    result.upstream_meta.as_ref().map(|m| m.status)
}

#[tokio::test]
async fn rejected_then_refreshed_token_succeeds() {
    let up = Upstream::new(2, false);
    let base = spawn(up.clone()).await;
    let result = engine(RuntimeOptions::default())
        .invoke(input(&base, ADAPTER_HTTP_JSON, ticket_auth(&base)))
        .await;
    assert!(result.ok, "{:?}", result.error_message);
    assert_eq!(result.error_code, None);
    assert_eq!(status(&result), Some(200));
    assert_eq!(up.api_hits(), 2);
    assert_eq!(up.token_hits(), 2, "initial fetch + one refresh");
    assert_eq!(*up.last_token.lock().unwrap(), "T2");
}

#[tokio::test]
async fn rejected_twice_returns_upstream_auth_rejected() {
    let up = Upstream::new(usize::MAX, false);
    let base = spawn(up.clone()).await;
    let result = engine(RuntimeOptions::default())
        .invoke(input(&base, ADAPTER_HTTP_JSON, ticket_auth(&base)))
        .await;
    assert!(!result.ok);
    assert_eq!(result.error_code.as_deref(), Some("UPSTREAM_AUTH_REJECTED"));
    let msg = result.error_message.clone().unwrap_or_default();
    assert!(
        msg.contains("upstream rejected credentials after token refresh (trigger=http_status=401)"),
        "{msg}"
    );
    assert_eq!(status(&result), Some(401));
    assert_eq!(up.api_hits(), 2, "exactly one retry");
}

#[tokio::test]
async fn jsonrpc_code_rule_triggers_refresh() {
    let up = Upstream::new(2, true);
    let base = spawn(up.clone()).await;
    let mut auth = ticket_auth(&base);
    auth.invalidation = Some(AuthInvalidationSpec {
        jsonrpc_codes: vec![-32001],
        ..AuthInvalidationSpec::default()
    });
    let result = engine(RuntimeOptions::default())
        .invoke(input(&base, ADAPTER_JSONRPC, auth))
        .await;
    assert!(
        result.ok,
        "{:?} {:?}",
        result.error_message, result.biz_error
    );
    assert!(result.biz_error.is_none());
    assert_eq!(up.api_hits(), 2);
}

#[tokio::test]
async fn retry_disabled_still_reports_auth_rejected() {
    let up = Upstream::new(usize::MAX, false);
    let base = spawn(up.clone()).await;
    let options = RuntimeOptions {
        auth_retry_enabled: false,
        ..RuntimeOptions::default()
    };
    let result = engine(options)
        .invoke(input(&base, ADAPTER_HTTP_JSON, ticket_auth(&base)))
        .await;
    assert_eq!(
        result.error_code.as_deref(),
        Some("UPSTREAM_AUTH_REJECTED"),
        "same outcome as rejected-after-retry"
    );
    let msg = result.error_message.clone().unwrap_or_default();
    assert!(
        msg.contains("retry skipped (trigger=http_status=401, reason=disabled)"),
        "{msg}"
    );
    assert_eq!(status(&result), Some(401));
    assert_eq!(up.api_hits(), 1);
}

async fn rejected_storm(ratio: f64, min: u32, requests: usize) -> usize {
    let up = Upstream::new(usize::MAX, false);
    let base = spawn(up.clone()).await;
    let engine = engine(RuntimeOptions {
        retry_budget_ratio: ratio,
        retry_budget_min_per_window: min,
        retry_budget_window: Duration::from_mins(1),
        ..RuntimeOptions::default()
    });
    for _ in 0..requests {
        let _ = engine
            .invoke(input(&base, ADAPTER_HTTP_JSON, ticket_auth(&base)))
            .await;
    }
    up.api_hits() - requests
}

#[tokio::test]
async fn retry_budget_caps_retries_by_ratio() {
    assert_eq!(rejected_storm(0.2, 0, 100).await, 20);
}

#[tokio::test]
async fn retry_budget_minimum_covers_low_traffic() {
    assert_eq!(rejected_storm(0.2, 3, 2).await, 2);
}

/// 一个渠道鉴权键的预算耗尽不影响另一个键的认证重试（预算按令牌缓存键分桶）。
#[tokio::test]
async fn retry_budget_is_per_token_key() {
    use crate::RetrySkip;
    let up = Upstream::new(usize::MAX, false);
    let base = spawn(up.clone()).await;
    let engine = engine(RuntimeOptions {
        retry_budget_ratio: 0.2,
        retry_budget_min_per_window: 1,
        retry_budget_window: Duration::from_mins(1),
        ..RuntimeOptions::default()
    });
    let for_supplier = |id: &str| {
        let mut i = input(&base, ADAPTER_HTTP_JSON, ticket_auth(&base));
        i.supplier_id = id.into();
        i
    };
    let a1 = engine.invoke(for_supplier("a")).await;
    let a2 = engine.invoke(for_supplier("a")).await;
    let b1 = engine.invoke(for_supplier("b")).await;
    assert_eq!(
        a1.internal_retries, 1,
        "supplier a: first retry within minimum"
    );
    assert_eq!(
        a2.retry_skipped,
        Some(RetrySkip::Budget),
        "supplier a exhausted"
    );
    assert_eq!(b1.internal_retries, 1, "supplier b has its own budget");
}

#[tokio::test]
async fn concurrent_rejections_share_one_refresh() {
    let up = Upstream::new(1, false);
    let base = spawn(up.clone()).await;
    let engine = engine(RuntimeOptions {
        retry_budget_ratio: 1.0,
        retry_budget_min_per_window: 64,
        ..RuntimeOptions::default()
    });
    let warm = engine
        .invoke(input(&base, ADAPTER_HTTP_JSON, ticket_auth(&base)))
        .await;
    assert!(warm.ok);
    assert_eq!(up.token_hits(), 1);
    // 上游重启：已缓存的 T1 失效。
    up.valid_from.store(2, Ordering::SeqCst);
    let mut joins = Vec::new();
    for _ in 0..16 {
        let engine = engine.clone();
        let input = input(&base, ADAPTER_HTTP_JSON, ticket_auth(&base));
        joins.push(tokio::spawn(async move { engine.invoke(input).await }));
    }
    for j in joins {
        let result = j.await.unwrap();
        assert!(result.ok, "{:?}", result.error_message);
        assert_eq!(status(&result), Some(200));
    }
    assert_eq!(up.token_hits(), 2, "evict once, refresh once");
}

/// 上游重启后同一渠道的并发被拒只由首个被拒者支付预算：预算只够 1 次也全部恢复。
#[tokio::test]
async fn followers_of_refreshed_token_do_not_consume_budget() {
    let up = Upstream::new(1, false);
    let base = spawn(up.clone()).await;
    let engine = engine(RuntimeOptions {
        retry_budget_ratio: 0.05,
        retry_budget_min_per_window: 1,
        retry_budget_window: Duration::from_mins(1),
        ..RuntimeOptions::default()
    });
    assert!(
        engine
            .invoke(input(&base, ADAPTER_HTTP_JSON, ticket_auth(&base)))
            .await
            .ok
    );
    up.valid_from.store(2, Ordering::SeqCst);
    let mut joins = Vec::new();
    for _ in 0..16 {
        let engine = engine.clone();
        let input = input(&base, ADAPTER_HTTP_JSON, ticket_auth(&base));
        joins.push(tokio::spawn(async move { engine.invoke(input).await }));
    }
    for j in joins {
        let result = j.await.unwrap();
        assert!(result.ok, "{:?}", result.error_message);
        assert_eq!(result.retry_skipped, None);
    }
    assert_eq!(up.token_hits(), 2, "one refresh for the whole burst");
}

#[tokio::test]
async fn static_credentials_are_not_retried() {
    let up = Upstream::new(usize::MAX, false);
    let base = spawn(up.clone()).await;
    let auth = AuthSpec {
        profile: AUTH_API_KEY.into(),
        ..Default::default()
    };
    let mut input = input(&base, ADAPTER_HTTP_JSON, auth);
    input
        .credentials
        .insert("secret".into(), "static-key".into());
    let result = engine(RuntimeOptions::default()).invoke(input).await;
    assert_eq!(result.error_code, None);
    assert_eq!(status(&result), Some(401));
    assert_eq!(up.api_hits(), 1);
}

#[tokio::test]
async fn token_endpoint_failure_is_auth_token_failed() {
    let up = Upstream::new(1, false);
    up.token_fails.store(true, Ordering::SeqCst);
    let base = spawn(up.clone()).await;
    let result = engine(RuntimeOptions::default())
        .invoke(input(&base, ADAPTER_HTTP_JSON, ticket_auth(&base)))
        .await;
    assert_eq!(result.error_code.as_deref(), Some("AUTH_TOKEN_FAILED"));
    assert_eq!(up.api_hits(), 0);
}

#[derive(Default)]
struct Recorder(std::sync::Mutex<Vec<crate::observe::RuntimeEvent>>);

impl crate::observe::RuntimeObserver for Recorder {
    fn on_event(&self, event: crate::observe::RuntimeEvent) {
        self.0.lock().unwrap().push(event);
    }
}

#[derive(Clone, Default)]
struct LogBuf(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for LogBuf {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn retry_is_reported_in_result_events_and_logs() {
    use crate::observe::{RetryOutcome, RuntimeEvent};
    use crate::{AuthRejectTrigger, RetryReason};
    use mazbri_core::AdapterKind;

    let buf = LogBuf::default();
    let writer = buf.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let up = Upstream::new(2, true);
    let base = spawn(up.clone()).await;
    let mut auth = ticket_auth(&base);
    auth.invalidation = Some(AuthInvalidationSpec {
        jsonrpc_codes: vec![-32001],
        ..AuthInvalidationSpec::default()
    });
    let recorder = Arc::new(Recorder::default());
    let engine = engine(RuntimeOptions::default()).with_observer(recorder.clone());
    let mut input = input(&base, ADAPTER_JSONRPC, auth);
    input.mode = ExecuteMode::Business;
    let result = engine.invoke(input).await;
    assert!(result.ok);
    assert_eq!(result.internal_retries, 1);
    let trigger = AuthRejectTrigger::JsonRpcCode(-32001);
    assert_eq!(
        result.retry_reasons,
        vec![RetryReason::AuthRejected(trigger)]
    );
    assert_eq!(
        result.retry_reasons[0].label(),
        "auth_rejected:jsonrpc_code"
    );
    assert_eq!(result.retry_skipped, None);
    let names: Vec<&str> = result.stages.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"upstream") && names.contains(&"upstream#retry"),
        "{names:?}"
    );

    let events = recorder.0.lock().unwrap().clone();
    assert_eq!(
        events,
        vec![
            RuntimeEvent::AuthRejected {
                adapter: AdapterKind::Jsonrpc,
                trigger
            },
            RuntimeEvent::TokenEvict { evicted: true },
            RuntimeEvent::AuthRetry {
                adapter: AdapterKind::Jsonrpc,
                result: RetryOutcome::Ok
            },
        ]
    );

    let logs = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("upstream.auth_rejected"), "{logs}");
    assert!(
        logs.contains(&format!(
            "token_fp={}",
            crate::observe::token_fingerprint("T1")
        )),
        "{logs}"
    );
    assert!(!logs.contains("T1 ") && !logs.contains("Bearer"), "{logs}");
}

#[tokio::test]
async fn skipped_retry_is_reported() {
    use crate::RetrySkip;
    let up = Upstream::new(usize::MAX, false);
    let base = spawn(up.clone()).await;
    let disabled = engine(RuntimeOptions {
        auth_retry_enabled: false,
        ..RuntimeOptions::default()
    })
    .invoke(input(&base, ADAPTER_HTTP_JSON, ticket_auth(&base)))
    .await;
    assert_eq!(disabled.retry_skipped, Some(RetrySkip::Disabled));
    assert_eq!(disabled.internal_retries, 0);

    let recorder = Arc::new(Recorder::default());
    let exhausted = engine(RuntimeOptions {
        retry_budget_min_per_window: 0,
        ..RuntimeOptions::default()
    })
    .with_observer(recorder.clone())
    .invoke(input(&base, ADAPTER_HTTP_JSON, ticket_auth(&base)))
    .await;
    assert_eq!(exhausted.retry_skipped, Some(RetrySkip::Budget));
    assert_eq!(
        exhausted.error_code.as_deref(),
        Some("UPSTREAM_AUTH_REJECTED")
    );
    assert!(
        recorder
            .0
            .lock()
            .unwrap()
            .contains(&crate::observe::RuntimeEvent::BudgetExhausted {
                kind: crate::observe::RetryKind::Auth
            })
    );
}
