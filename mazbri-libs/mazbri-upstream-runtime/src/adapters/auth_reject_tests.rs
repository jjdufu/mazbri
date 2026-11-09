//! 认证失效判定：规则命中矩阵与适配器端到端行为。

use super::*;
use crate::ssrf::SsrfChecker;
use crate::types::{ExecuteMode, ExecuteOptions, InvokeInput};
use mazbri_core::channel_egress::ADAPTER_JSONRPC;
use mazbri_core::{
    ADAPTER_HTTP_JSON, AuthInvalidationSpec, BodyMatchRule, ChannelEgressSpec, SupplierConfig,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const TOKEN: &str = "tok-1";

fn outcome(status: u16, body: &[u8], biz_error: Option<BizError>) -> AdapterOutcome {
    AdapterOutcome {
        response: OutboundResponse {
            status,
            headers: HashMap::new(),
            body: body.to_vec(),
            final_url: "http://u/x".into(),
            method: "POST".into(),
            path: "/x".into(),
        },
        payload: serde_json::Value::Null,
        biz_error,
        auth_rejection: None,
    }
}

fn rules(json: serde_json::Value) -> AuthInvalidationSpec {
    serde_json::from_value(json).expect("rules")
}

fn trigger(
    rules: &AuthInvalidationSpec,
    kind: AdapterKind,
    out: &AdapterOutcome,
    token: Option<&str>,
) -> Option<AuthRejectTrigger> {
    detect_auth_rejection(rules, kind, out, token).map(|r| r.trigger)
}

#[test]
fn default_rule_401_with_token_only() {
    let rules = AuthInvalidationSpec::default();
    let out = outcome(401, b"", None);
    assert_eq!(
        trigger(&rules, AdapterKind::HttpJson, &out, Some(TOKEN)),
        Some(AuthRejectTrigger::HttpStatus(401))
    );
    assert_eq!(trigger(&rules, AdapterKind::HttpJson, &out, None), None);
    let rejection = detect_auth_rejection(&rules, AdapterKind::HttpJson, &out, Some(TOKEN));
    assert_eq!(rejection.map(|r| r.token), Some(TOKEN.to_string()));
}

#[test]
fn empty_status_rule_ignores_401() {
    let rules = rules(serde_json::json!({"http_status": []}));
    let out = outcome(401, b"", None);
    assert_eq!(
        trigger(&rules, AdapterKind::HttpJson, &out, Some(TOKEN)),
        None
    );
}

#[test]
fn jsonrpc_code_rule() {
    let err = serde_json::json!({"code": -32001, "message": "token expired"});
    let out = outcome(200, b"{}", Some(map_jsonrpc_error(&err)));
    let on = rules(serde_json::json!({"jsonrpc_codes": [-32001]}));
    assert_eq!(
        trigger(&on, AdapterKind::Jsonrpc, &out, Some(TOKEN)),
        Some(AuthRejectTrigger::JsonRpcCode(-32001))
    );
    let off = AuthInvalidationSpec::default();
    assert_eq!(trigger(&off, AdapterKind::Jsonrpc, &out, Some(TOKEN)), None);
    assert_eq!(
        out.biz_error.as_ref().map(|b| b.code.as_str()),
        Some("FORBIDDEN")
    );
}

#[test]
fn body_match_rule() {
    let rules = rules(serde_json::json!({
        "body_match": [{"path": "$.code", "equals": ["TOKEN_EXPIRED"]}]
    }));
    let hit = outcome(200, br#"{"code":"TOKEN_EXPIRED"}"#, None);
    assert_eq!(
        trigger(&rules, AdapterKind::HttpJson, &hit, Some(TOKEN)),
        Some(AuthRejectTrigger::BodyMatch)
    );
    let miss = outcome(200, br#"{"code":"OTHER"}"#, None);
    assert_eq!(
        trigger(&rules, AdapterKind::HttpJson, &miss, Some(TOKEN)),
        None
    );
    let not_json = outcome(200, b"<html>oops", None);
    assert_eq!(
        trigger(&rules, AdapterKind::HttpJson, &not_json, Some(TOKEN)),
        None
    );
}

#[test]
fn body_match_compares_json_values_strictly() {
    let rules = AuthInvalidationSpec {
        body_match: vec![BodyMatchRule {
            path: "$.code".into(),
            equals: vec![serde_json::json!(40101)],
        }],
        ..AuthInvalidationSpec::default()
    };
    let as_string = outcome(200, br#"{"code":"40101"}"#, None);
    assert_eq!(
        trigger(&rules, AdapterKind::HttpJson, &as_string, Some(TOKEN)),
        None
    );
    let as_number = outcome(200, br#"{"code":40101}"#, None);
    assert_eq!(
        trigger(&rules, AdapterKind::HttpJson, &as_number, Some(TOKEN)),
        Some(AuthRejectTrigger::BodyMatch)
    );
}

#[test]
fn grpc_unauthenticated_follows_401_rule() {
    let biz = BizError {
        code: "UNAUTHORIZED".into(),
        message: "grpc-status 16".into(),
        details: None,
    };
    let out = outcome(200, b"", Some(biz));
    assert_eq!(
        trigger(
            &AuthInvalidationSpec::default(),
            AdapterKind::Grpc,
            &out,
            Some(TOKEN)
        ),
        Some(AuthRejectTrigger::HttpStatus(401))
    );
    let off = rules(serde_json::json!({"http_status": []}));
    assert_eq!(trigger(&off, AdapterKind::Grpc, &out, Some(TOKEN)), None);
}

fn invoke_input(adapter: &str, base_url: &str) -> InvokeInput {
    InvokeInput {
        supplier_id: "s".into(),
        supplier_code: "s".into(),
        channel_id: "c".into(),
        request_id: "rid-1".into(),
        payload: serde_json::json!({}),
        spec: ChannelEgressSpec {
            adapter: adapter.into(),
            base_url: base_url.into(),
            ..Default::default()
        },
        config: None,
        timeout: Duration::from_secs(2),
        mode: ExecuteMode::Business,
        options: ExecuteOptions::default(),
        credentials: HashMap::new(),
        pipeline_keys: None,
        artifact_bytes: None,
        cache_key: None,
    }
}

#[test]
fn effective_rules_prefer_channel_then_enabled_supplier() {
    let supplier = |enabled: bool| -> Arc<SupplierConfig> {
        Arc::new(
            serde_json::from_value(serde_json::json!({
                "auth_flow": {
                    "enabled": enabled,
                    "profile": "oauth_cc",
                    "invalidation": {"http_status": [401, 403], "jsonrpc_codes": [-32001]}
                }
            }))
            .expect("supplier config"),
        )
    };
    let mut input = invoke_input(ADAPTER_HTTP_JSON, "http://127.0.0.1");
    assert_eq!(
        effective_invalidation(&input),
        AuthInvalidationSpec::default()
    );

    input.config = Some(supplier(true));
    let inherited = effective_invalidation(&input);
    assert_eq!(inherited.http_status, vec![401, 403]);
    assert_eq!(
        inherited.jsonrpc_codes,
        Vec::<i64>::new(),
        "trimmed for http_json"
    );

    input.config = Some(supplier(false));
    assert_eq!(
        effective_invalidation(&input),
        AuthInvalidationSpec::default()
    );

    input.config = Some(supplier(true));
    input.spec.auth = Some(
        serde_json::from_value(serde_json::json!({
            "profile": "oauth_cc",
            "invalidation": {"http_status": [419]}
        }))
        .expect("auth spec"),
    );
    assert_eq!(effective_invalidation(&input).http_status, vec![419]);
}

/// 单次应答的 HTTP 服务。
async fn serve_once(status_line: &'static str, body: &'static str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 8192];
        let _ = sock.read(&mut buf).await;
        let resp = format!(
            "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let _ = sock.write_all(resp.as_bytes()).await;
    });
    format!("http://{addr}")
}

async fn invoke_against(
    adapter: &str,
    base: &str,
    rules: &AuthInvalidationSpec,
) -> Result<AdapterOutcome, ExecuteError> {
    let kind = AdapterKind::parse(adapter).unwrap();
    let mut input = invoke_input(adapter, base);
    input.spec.request.method = "POST".into();
    input.spec.request.path = "/rpc".into();
    input.spec.operation = Some("demo.op".into());
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let ssrf = SsrfChecker::with_allow_private(true);
    let auth = AppliedAuth {
        headers: HashMap::new(),
        query: Vec::new(),
        metadata: HashMap::new(),
        token: Some(TOKEN.into()),
        stale_evicted: None,
    };
    let pipeline_keys = crate::crypto_pipeline::PipelineKeys::empty();
    let registry = mazbri_crypto::AlgorithmRegistry::default_registry();
    let ws_pool = crate::ws_pool::WsPool::new(crate::ws_pool::WsPoolConfig::default());
    let ctx = AdapterCtx {
        client: &client,
        ssrf: &ssrf,
        input: &input,
        base_url: base,
        auth: &auth,
        timeout: Duration::from_secs(2),
        ws_pool: &ws_pool,
        pipeline_keys: &pipeline_keys,
        registry: &registry,
        invalidation: rules,
    };
    invoke_adapter(&ctx, kind).await
}

#[tokio::test]
async fn http_custom_reject_status_returns_raw_response() {
    let base = serve_once("419 Session Expired", "<html>not json</html>").await;
    let rules = rules(serde_json::json!({"http_status": [401, 419]}));
    let out = invoke_against(ADAPTER_HTTP_JSON, &base, &rules)
        .await
        .expect("reject status must not fail parsing");
    assert_eq!(out.response.status, 419);
    assert_eq!(
        out.auth_rejection.map(|r| r.trigger),
        Some(AuthRejectTrigger::HttpStatus(419))
    );
}

#[tokio::test]
async fn jsonrpc_error_code_detected_end_to_end() {
    let body =
        r#"{"jsonrpc":"2.0","id":"rid-1","error":{"code":-32001,"message":"token expired"}}"#;
    let rules_on = rules(serde_json::json!({"jsonrpc_codes": [-32001]}));
    let base = serve_once("200 OK", body).await;
    let out = invoke_against(ADAPTER_JSONRPC, &base, &rules_on)
        .await
        .expect("ok");
    assert_eq!(
        out.auth_rejection.map(|r| r.trigger),
        Some(AuthRejectTrigger::JsonRpcCode(-32001))
    );

    let base = serve_once("200 OK", body).await;
    let out = invoke_against(ADAPTER_JSONRPC, &base, &AuthInvalidationSpec::default())
        .await
        .expect("ok");
    assert!(out.auth_rejection.is_none());
    assert_eq!(out.biz_error.map(|b| b.code), Some("FORBIDDEN".to_string()));
}
