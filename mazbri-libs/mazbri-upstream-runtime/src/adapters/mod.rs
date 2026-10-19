//! Closed adapter set. Each adapter maps tenant JSON → upstream wire → tenant JSON.

mod graphql;
mod grpc;
mod http_envelope;
mod http_json;
mod job_poll;
mod jsonrpc;
mod sign_form;
mod soap_xml;
mod ws_unary;

use mazbri_core::{AdapterKind, ChannelEgressSpec, WRAP_METHOD_BIZ_CONTENT};
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;

use crate::auth::AppliedAuth;
use crate::http_call::{HttpCall, auth_only_response, send_http_with_digest};
use crate::ssrf::SsrfChecker;
use crate::template::extract_by_path;
use crate::types::{BizError, ExecuteError, ExecuteMode, InvokeInput, OutboundResponse};

pub struct AdapterCtx<'a> {
    pub client: &'a Client,
    pub ssrf: &'a SsrfChecker,
    pub input: &'a InvokeInput,
    pub base_url: &'a str,
    pub auth: &'a AppliedAuth,
    pub timeout: Duration,
    pub ws_handshake_timeout: Duration,
    pub job_poll_min_interval: Duration,
}

pub struct AdapterOutcome {
    pub response: OutboundResponse,
    pub payload: serde_json::Value,
    pub biz_error: Option<BizError>,
}

/// Auth headers first; spec `request.headers` fill missing keys (`or_insert`).
/// Content-Type override is applied afterwards via `apply_json_content_type_override`.
pub(crate) fn merge_request_headers(
    auth: &AppliedAuth,
    spec: &ChannelEgressSpec,
) -> HashMap<String, String> {
    let mut headers = auth.headers.clone();
    if let Some(ref req) = spec.request {
        if let Some(ref spec_h) = req.headers {
            for (k, v) in spec_h {
                headers.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
    }
    headers
}

fn strip_content_type(headers: &mut HashMap<String, String>) {
    headers.retain(|k, _| !k.eq_ignore_ascii_case("content-type"));
}

/// Insert `spec.request.content_type` over any existing Content-Type (JSON encoding only).
pub(crate) fn apply_json_content_type_override(
    headers: &mut HashMap<String, String>,
    spec: &ChannelEgressSpec,
) {
    let Some(ct) = spec
        .request
        .as_ref()
        .and_then(|r| r.content_type.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    strip_content_type(headers);
    headers.insert("Content-Type".into(), ct.to_string());
}

/// Form encoding ignores Override; wire MIME is always urlencoded.
pub(crate) fn force_urlencoded_content_type(headers: &mut HashMap<String, String>) {
    strip_content_type(headers);
    headers.insert(
        "Content-Type".into(),
        "application/x-www-form-urlencoded".into(),
    );
}

pub(crate) fn request_body_encoding(spec: &ChannelEgressSpec) -> Option<&str> {
    spec.request
        .as_ref()
        .and_then(|r| r.body_encoding.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn reject_incompatible_body_encoding(
    kind: AdapterKind,
    spec: &ChannelEgressSpec,
) -> Result<(), ExecuteError> {
    let Some(enc) = request_body_encoding(spec) else {
        return Ok(());
    };
    if enc.eq_ignore_ascii_case("json") {
        return Ok(());
    }
    if enc.eq_ignore_ascii_case("none") {
        if matches!(kind, AdapterKind::HttpJson | AdapterKind::HttpEnvelope) {
            return Ok(());
        }
        return Err(ExecuteError::SpecInvalid(format!(
            "adapter {} requires json body",
            kind.as_str()
        )));
    }
    if enc.eq_ignore_ascii_case("form") {
        if matches!(kind, AdapterKind::HttpJson) {
            return Ok(());
        }
        return Err(ExecuteError::SpecInvalid(format!(
            "adapter {} forbids form body_encoding",
            kind.as_str()
        )));
    }
    Err(ExecuteError::SpecInvalid(format!(
        "unknown body_encoding '{enc}'"
    )))
}

/// Auth query first; spec `request.query` appended after.
pub(crate) fn merge_request_query(
    auth: &AppliedAuth,
    spec: &ChannelEgressSpec,
) -> Vec<(String, String)> {
    let mut query = auth.query.clone();
    if let Some(ref req) = spec.request {
        if let Some(ref spec_q) = req.query {
            for (k, v) in spec_q {
                query.push((k.clone(), v.clone()));
            }
        }
    }
    query
}

pub async fn dispatch_http(
    ctx: &AdapterCtx<'_>,
    mut call: HttpCall<'_>,
) -> Result<OutboundResponse, ExecuteError> {
    crate::http_call::inject_x_request_id(&mut call.headers, &ctx.input.request_id);
    let kind = ctx
        .input
        .spec
        .adapter_kind()
        .map_err(|e| ExecuteError::SpecInvalid(e.to_string()))?;
    crate::auth::apply_request_signature(&mut call, kind, &ctx.input.spec, &ctx.input.credentials)?;
    if matches!(ctx.input.mode, ExecuteMode::AuthOnly) {
        return Ok(auth_only_response(&call));
    }
    let digest = crate::auth::digest_attempt_from(&ctx.input.spec, &ctx.input.credentials);
    send_http_with_digest(ctx.client, ctx.ssrf, call, digest).await
}

pub async fn invoke_adapter(
    ctx: &AdapterCtx<'_>,
    kind: AdapterKind,
) -> Result<AdapterOutcome, ExecuteError> {
    reject_incompatible_body_encoding(kind, &ctx.input.spec)?;
    match kind {
        AdapterKind::HttpJson => http_json::invoke(ctx).await,
        AdapterKind::HttpEnvelope => http_envelope::invoke(ctx).await,
        AdapterKind::SignForm => sign_form::invoke(ctx).await,
        AdapterKind::Jsonrpc => jsonrpc::invoke(ctx).await,
        AdapterKind::Graphql => graphql::invoke(ctx).await,
        AdapterKind::Grpc => grpc::invoke(ctx).await,
        AdapterKind::SoapXml => soap_xml::invoke(ctx).await,
        AdapterKind::WsUnary => ws_unary::invoke(ctx).await,
        AdapterKind::JobPoll => job_poll::invoke(ctx).await,
    }
}

pub fn require_object(
    payload: &serde_json::Value,
) -> Result<&serde_json::Map<String, serde_json::Value>, ExecuteError> {
    payload
        .as_object()
        .ok_or_else(|| ExecuteError::SpecInvalid("tenant payload_json 必须是 JSON 对象".into()))
}

pub fn wrap_method_biz(operation: &str, payload: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "method": operation,
        "biz_content": payload,
    })
}

pub fn wrap_by_name(
    spec: &ChannelEgressSpec,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, ExecuteError> {
    let wrap = spec.wrap_name();
    match wrap {
        WRAP_METHOD_BIZ_CONTENT => {
            let op = spec.operation.as_deref().unwrap_or("");
            Ok(wrap_method_biz(op, payload))
        }
        "none" => Ok(payload.clone()),
        other => Err(ExecuteError::SpecInvalid(format!("unknown wrap '{other}'"))),
    }
}

pub fn apply_ok_and_unwrap(
    raw: &serde_json::Value,
    spec: &ChannelEgressSpec,
) -> Result<(serde_json::Value, Option<BizError>), ExecuteError> {
    if let Some(ref resp) = spec.response {
        if let Some(ref ok) = resp.ok {
            let got = extract_by_path(raw, &ok.path);
            let matches = got.as_ref().is_some_and(|v| json_eq(v, &ok.eq));
            if !matches {
                let message = got
                    .as_ref()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "upstream business not ok".into());
                return Ok((
                    raw.clone(),
                    Some(BizError {
                        code: "UPSTREAM_BIZ".into(),
                        message,
                        details: Some(raw.clone()),
                    }),
                ));
            }
        }
        if let Some(ref path) = resp.unwrap {
            if let Some(inner) = extract_by_path(raw, path) {
                return Ok((inner, None));
            }
            return Err(ExecuteError::AdapterFailed(format!(
                "response.unwrap path not found: {path}"
            )));
        }
    }
    Ok((raw.clone(), None))
}

fn json_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    if a == b {
        return true;
    }
    match (a, b) {
        (serde_json::Value::Number(x), serde_json::Value::Number(y)) => x.as_f64() == y.as_f64(),
        (serde_json::Value::String(s), serde_json::Value::Number(n)) => {
            s.parse::<f64>().ok() == n.as_f64()
        }
        (serde_json::Value::Number(n), serde_json::Value::String(s)) => {
            n.as_f64() == s.parse::<f64>().ok()
        }
        _ => false,
    }
}

pub fn parse_json_body(bytes: &[u8]) -> Result<serde_json::Value, ExecuteError> {
    serde_json::from_slice(bytes)
        .map_err(|e| ExecuteError::AdapterFailed(format!("upstream body is not JSON: {e}")))
}

pub fn map_jsonrpc_error(err: &serde_json::Value) -> BizError {
    let code = err.get("code").and_then(|v| v.as_i64()).unwrap_or(-32603);
    let message = err
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("jsonrpc error")
        .to_string();
    let mapped = match code {
        -32600 => "INVALID_PARAMETER",
        -32601 => "NOT_FOUND",
        -32602 => "INVALID_PARAMETER",
        -32700 => "INVALID_PARAMETER",
        -32001 | -32003 => "FORBIDDEN",
        -32002 => "UNAUTHORIZED",
        _ => "UPSTREAM_BIZ",
    };
    BizError {
        code: mapped.into(),
        message,
        details: Some(err.clone()),
    }
}

pub fn require_artifact(input: &InvokeInput) -> Result<&[u8], ExecuteError> {
    input
        .artifact_bytes
        .as_deref()
        .filter(|b| !b.is_empty())
        .ok_or_else(|| ExecuteError::ArtifactMissing("grpc artifact_id has no bytes".into()))
}

pub fn snake_to_camel(s: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in s.chars() {
        if c == '_' {
            up = true;
            continue;
        }
        if up {
            for u in c.to_uppercase() {
                out.push(u);
            }
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_call::HttpCall;
    use crate::ssrf::SsrfChecker;
    use crate::types::{ExecuteMode, ExecuteOptions, InvokeInput};
    use mazbri_core::{ADAPTER_HTTP_JSON, ChannelEgressSpec, EgressRequestSpec};
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn auth_with(headers: HashMap<String, String>, query: Vec<(String, String)>) -> AppliedAuth {
        AppliedAuth {
            headers,
            query,
            metadata: HashMap::new(),
            token: None,
        }
    }

    fn spec_hq(
        headers: HashMap<String, String>,
        query: HashMap<String, String>,
    ) -> ChannelEgressSpec {
        ChannelEgressSpec {
            adapter: ADAPTER_HTTP_JSON.into(),
            request: Some(EgressRequestSpec {
                headers: if headers.is_empty() {
                    None
                } else {
                    Some(headers)
                },
                query: if query.is_empty() { None } else { Some(query) },
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn merge_headers_auth_first_spec_or_insert() {
        let auth = auth_with(
            [("Authorization".into(), "Bearer t".into())]
                .into_iter()
                .collect(),
            vec![],
        );
        let spec = spec_hq(
            [
                ("Authorization".into(), "Bearer hand".into()),
                ("X-Version".into(), "1".into()),
            ]
            .into_iter()
            .collect(),
            HashMap::new(),
        );
        let merged = merge_request_headers(&auth, &spec);
        assert_eq!(
            merged.get("Authorization").map(String::as_str),
            Some("Bearer t")
        );
        assert_eq!(merged.get("X-Version").map(String::as_str), Some("1"));
    }

    #[test]
    fn merge_query_appends_spec_after_auth() {
        let auth = auth_with(HashMap::new(), vec![("token".into(), "abc".into())]);
        let spec = spec_hq(
            HashMap::new(),
            [("appId".into(), "1".into())].into_iter().collect(),
        );
        let q = merge_request_query(&auth, &spec);
        assert_eq!(
            q,
            vec![("token".into(), "abc".into()), ("appId".into(), "1".into()),]
        );
    }

    #[test]
    fn content_type_override_wins_over_kv_header() {
        let auth = auth_with(HashMap::new(), vec![]);
        let spec = ChannelEgressSpec {
            adapter: ADAPTER_HTTP_JSON.into(),
            request: Some(EgressRequestSpec {
                headers: Some(
                    [("Content-Type".into(), "text/plain".into())]
                        .into_iter()
                        .collect(),
                ),
                content_type: Some("application/vnd.api+json".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut merged = merge_request_headers(&auth, &spec);
        apply_json_content_type_override(&mut merged, &spec);
        let ct = merged
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str());
        assert_eq!(ct, Some("application/vnd.api+json"));
    }

    #[test]
    fn form_encoding_forces_urlencoded_ignoring_override() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".into(), "application/json".into());
        let spec = ChannelEgressSpec {
            adapter: ADAPTER_HTTP_JSON.into(),
            request: Some(EgressRequestSpec {
                content_type: Some("application/vnd.api+json".into()),
                body_encoding: Some("form".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        apply_json_content_type_override(&mut headers, &spec);
        force_urlencoded_content_type(&mut headers);
        let ct = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str());
        assert_eq!(ct, Some("application/x-www-form-urlencoded"));
    }

    fn invoke_input(request_id: &str) -> InvokeInput {
        InvokeInput {
            supplier_id: "s".into(),
            supplier_code: "s".into(),
            channel_id: "c".into(),
            request_id: request_id.into(),
            payload: serde_json::json!({}),
            spec: ChannelEgressSpec {
                adapter: ADAPTER_HTTP_JSON.into(),
                ..Default::default()
            },
            config: None,
            fallback_base_url: "http://127.0.0.1".into(),
            path_prefix: "/".into(),
            timeout: Duration::from_secs(2),
            mode: ExecuteMode::Business,
            options: ExecuteOptions::default(),
            credentials: HashMap::new(),
            artifact_bytes: None,
            cache_key: None,
        }
    }

    async fn capture_dispatch(headers: HashMap<String, String>, request_id: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap();
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
                .await;
            let _ = tx.send(String::from_utf8_lossy(&buf[..n]).into_owned());
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let ssrf = SsrfChecker::with_allow_private(true);
        let input = invoke_input(request_id);
        let auth = auth_with(HashMap::new(), vec![]);
        let ctx = AdapterCtx {
            client: &client,
            ssrf: &ssrf,
            input: &input,
            base_url: "http://127.0.0.1",
            auth: &auth,
            timeout: Duration::from_secs(2),
            ws_handshake_timeout: Duration::from_secs(2),
            job_poll_min_interval: Duration::from_millis(10),
        };
        let url = format!("http://{addr}/");
        dispatch_http(
            &ctx,
            HttpCall {
                method: "GET",
                url: &url,
                headers,
                body: None,
                timeout: Duration::from_secs(2),
            },
        )
        .await
        .unwrap();
        rx.await.unwrap()
    }

    fn header_line<'a>(raw: &'a str, name: &str) -> Option<&'a str> {
        let want = format!("{name}:");
        raw.lines().find_map(|line| {
            let lower = line.to_ascii_lowercase();
            if lower.starts_with(&want) {
                Some(line.split_once(':').map(|(_, v)| v.trim()).unwrap_or(""))
            } else {
                None
            }
        })
    }

    #[tokio::test]
    async fn dispatch_http_inserts_x_request_id_when_missing() {
        let raw = capture_dispatch(HashMap::new(), "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").await;
        assert_eq!(
            header_line(&raw.to_ascii_lowercase(), "x-request-id"),
            Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
        );
    }

    #[tokio::test]
    async fn dispatch_http_does_not_overwrite_existing_x_request_id() {
        let mut headers = HashMap::new();
        headers.insert("X-Request-Id".into(), "already-set".into());
        let raw = capture_dispatch(headers, "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").await;
        assert_eq!(
            header_line(&raw.to_ascii_lowercase(), "x-request-id"),
            Some("already-set")
        );
    }

    #[tokio::test]
    async fn dispatch_http_skips_empty_request_id() {
        let raw = capture_dispatch(HashMap::new(), "").await;
        assert!(header_line(&raw.to_ascii_lowercase(), "x-request-id").is_none());
    }
}
