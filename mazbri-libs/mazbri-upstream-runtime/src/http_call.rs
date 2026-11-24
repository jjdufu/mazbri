//! Shared HTTP send used by adapters.
//! If the call already has a Content-Type header, `BodyKind` does not overwrite it.

use reqwest::Method;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::auth_digest::{
    DigestAttempt, build_digest_authorization, parse_www_authenticate_digest,
};
use crate::ssrf::{SsrfChecker, extract_ssrf_error};
use crate::types::{BodyKind, ExecuteError, OutboundResponse, WireClock};

pub struct HttpCall<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub headers: HashMap<String, String>,
    pub body: Option<BodyKind>,
    pub timeout: Duration,
}

/// 出站关联头：`traceparent`/`tracestate` 总是按当前 span 覆盖写入（上游看到的父节点
/// 就是本次调用所在 span）；`x-request-id` 仅在缺失时补上，空 id 不补，也不新生成 UUID。
pub(crate) fn inject_correlation(headers: &mut HashMap<String, String>, request_id: &str) {
    mazbri_core::trace_propagation::inject_current_into_map(headers);
    if request_id.is_empty() {
        return;
    }
    if headers
        .keys()
        .any(|k| k.eq_ignore_ascii_case("x-request-id"))
    {
        return;
    }
    headers.insert("x-request-id".into(), request_id.to_string());
}

/// Same rules as [`inject_correlation`] for HTTP/`HeaderMap` (gRPC metadata).
pub(crate) fn inject_correlation_header_map(headers: &mut HeaderMap, request_id: &str) {
    mazbri_core::trace_propagation::inject_current_into_header_map(headers);
    if request_id.is_empty() || headers.contains_key("x-request-id") {
        return;
    }
    if let Ok(hv) = HeaderValue::from_str(request_id) {
        headers.insert(HeaderName::from_static("x-request-id"), hv);
    }
}

#[inline]
fn is_internal_mazbri_header(name: &str) -> bool {
    name.get(..9)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("x-mazbri-"))
}

#[inline]
pub(crate) fn extract_url_path(raw_url: &str) -> String {
    let Some((_, after_scheme)) = raw_url.split_once("://") else {
        return String::new();
    };
    let Some(slash_idx) = after_scheme.find('/') else {
        return "/".to_string();
    };
    let path_and_more = &after_scheme[slash_idx..];
    let end = path_and_more
        .find(['?', '#'])
        .unwrap_or(path_and_more.len());
    path_and_more[..end].to_string()
}

/// # Errors
///
/// 方法非法、SSRF 拦截或上游传输超时/断开时返回 [`ExecuteError`]，并把真实网络耗时累加写入 `wire_clock`。
pub async fn send_http_timed(
    client: &reqwest::Client,
    ssrf: &SsrfChecker,
    call: HttpCall<'_>,
    wire_clock: Option<&WireClock>,
) -> Result<OutboundResponse, ExecuteError> {
    if let Err(e) = ssrf.validate_url_syntax_and_literal(call.url) {
        return Err(ExecuteError::SsrfBlocked(e));
    }
    let method_type = Method::from_bytes(call.method.as_bytes()).map_err(|e| {
        ExecuteError::InvalidMethod(format!("Invalid HTTP method '{}': {}", call.method, e))
    })?;

    let mut header_map = HeaderMap::with_capacity(call.headers.len());
    for (k, v) in &call.headers {
        if is_internal_mazbri_header(k) {
            continue;
        }
        if let (Ok(hn), Ok(hv)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            header_map.insert(hn, hv);
        }
    }

    let has_content_type = header_map.contains_key(reqwest::header::CONTENT_TYPE);
    let mut builder = client
        .request(method_type, call.url)
        .headers(header_map)
        .timeout(call.timeout);

    if let Some(body) = call.body {
        let ct = body.content_type();
        if !has_content_type {
            builder = builder.header(reqwest::header::CONTENT_TYPE, ct);
        }
        builder = builder.body(body.into_bytes());
    }

    let started = Instant::now();
    let resp = match builder.send().await {
        Ok(r) => r,
        Err(e) => {
            if let Some(ssrf_msg) = extract_ssrf_error(&e) {
                return Err(ExecuteError::SsrfBlocked(ssrf_msg));
            }
            if let Some(clock) = wire_clock {
                clock.add(started.elapsed());
            }
            return Err(if e.is_timeout() && e.is_connect() {
                ExecuteError::UpstreamTransport(format!("connect timeout: {e}"))
            } else if e.is_timeout() {
                ExecuteError::UpstreamTransport(format!("read timeout: {e}"))
            } else {
                ExecuteError::UpstreamTransport(e.to_string())
            });
        }
    };
    let status_code = resp.status();
    let protocol = http_version_label(resp.version()).to_string();
    let mut resp_headers = HashMap::with_capacity(resp.headers().len());
    for (key, val) in resp.headers() {
        if let Ok(str_val) = val.to_str() {
            resp_headers.insert(key.as_str().to_string(), str_val.to_string());
        }
    }
    let body_bytes = resp.bytes().await.unwrap_or_default().to_vec();
    let wire_duration = started.elapsed();
    if let Some(clock) = wire_clock {
        clock.add(wire_duration);
    }
    let path = extract_url_path(call.url);
    Ok(OutboundResponse {
        status: status_code.as_u16(),
        headers: resp_headers,
        body: body_bytes,
        final_url: call.url.to_string(),
        method: call.method.to_string(),
        path,
        protocol,
        wire_duration,
    })
}

fn header_ci<'a>(headers: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    headers.iter().find_map(|(k, v)| {
        if k.eq_ignore_ascii_case(name) {
            Some(v.as_str())
        } else {
            None
        }
    })
}

/// Digest: first send without Digest Authorization; on 401 + WWW-Authenticate Digest, replay once.
/// Non-digest callers pass `None` (same path as [`send_http_timed`]).
///
/// # Errors
///
/// 同 [`send_http_timed`]，并把全部网络往返耗时累加写入 `wire_clock`。
pub async fn send_http_with_digest_timed(
    client: &reqwest::Client,
    ssrf: &SsrfChecker,
    call: HttpCall<'_>,
    digest: Option<DigestAttempt>,
    wire_clock: Option<&WireClock>,
) -> Result<OutboundResponse, ExecuteError> {
    let Some(digest) = digest else {
        return send_http_timed(client, ssrf, call, wire_clock).await;
    };
    let method = call.method;
    let url = call.url;
    let timeout = call.timeout;
    let headers = call.headers.clone();
    let body = call.body.clone();
    let first = send_http_timed(client, ssrf, call, wire_clock).await?;
    if first.status != 401 {
        return Ok(first);
    }
    let Some(www) = header_ci(&first.headers, "www-authenticate") else {
        return Ok(first);
    };
    let Some(challenge) = parse_www_authenticate_digest(www) else {
        return Ok(first);
    };
    let authz = build_digest_authorization(&digest, &challenge, method, url)?;
    let mut headers2 = headers;
    headers2.insert("Authorization".into(), authz);
    let mut second = send_http_timed(
        client,
        ssrf,
        HttpCall {
            method,
            url,
            headers: headers2,
            body,
            timeout,
        },
        wire_clock,
    )
    .await?;
    second.wire_duration = second.wire_duration.saturating_add(first.wire_duration);
    Ok(second)
}

#[must_use]
pub fn auth_only_response(call: &HttpCall<'_>) -> OutboundResponse {
    OutboundResponse {
        status: 204,
        headers: call.headers.clone(),
        body: Vec::new(),
        final_url: call.url.to_string(),
        method: call.method.to_string(),
        path: extract_url_path(call.url),
        protocol: String::new(),
        wire_duration: Duration::ZERO,
    }
}

/// reqwest 响应的 HTTP 版本标签（访问日志 `protocol`）。
#[must_use]
pub fn http_version_label(version: reqwest::Version) -> &'static str {
    match version {
        reqwest::Version::HTTP_09 => "HTTP/0.9",
        reqwest::Version::HTTP_10 => "HTTP/1.0",
        reqwest::Version::HTTP_2 => "HTTP/2",
        reqwest::Version::HTTP_3 => "HTTP/3",
        _ => "HTTP/1.1",
    }
}

#[must_use]
pub fn join_url(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    let base = base.trim_end_matches('/');
    if path.is_empty() || path == "/" {
        return base.to_string();
    }
    if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

#[must_use]
pub fn append_query(url: &str, query: &[(String, String)]) -> String {
    if query.is_empty() {
        return url.to_string();
    }
    let mut parts = Vec::with_capacity(query.len());
    for (k, v) in query {
        parts.push(format!(
            "{}={}",
            crate::funcs::url_encode(k),
            crate::funcs::url_encode(v)
        ));
    }
    if url.contains('?') {
        format!("{url}&{}", parts.join("&"))
    } else {
        format!("{url}?{}", parts.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BodyKind;
    use reqwest::header::{HeaderMap, HeaderValue};
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn capture_request(call: HttpCall<'_>) -> String {
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
        let url = format!("http://{addr}/");
        let call = HttpCall { url: &url, ..call };
        send_http_timed(&client, &ssrf, call, None).await.unwrap();
        rx.await.unwrap()
    }

    #[tokio::test]
    async fn none_body_has_no_forced_content_type() {
        let raw = capture_request(HttpCall {
            method: "POST",
            url: "http://127.0.0.1/",
            headers: HashMap::new(),
            body: None,
            timeout: std::time::Duration::from_secs(2),
        })
        .await;
        assert!(!raw.to_ascii_lowercase().contains("content-type"));
    }

    #[tokio::test]
    async fn json_body_sets_application_json() {
        let raw = capture_request(HttpCall {
            method: "POST",
            url: "http://127.0.0.1/",
            headers: HashMap::new(),
            body: Some(BodyKind::Json("{}".into())),
            timeout: std::time::Duration::from_secs(2),
        })
        .await;
        assert!(
            raw.to_ascii_lowercase()
                .contains("content-type: application/json")
        );
        assert!(raw.contains("\r\n\r\n{}"));
    }

    #[tokio::test]
    async fn existing_content_type_is_not_overwritten() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".into(), "application/vnd.api+json".into());
        let raw = capture_request(HttpCall {
            method: "POST",
            url: "http://127.0.0.1/",
            headers,
            body: Some(BodyKind::Json("{\"a\":1}".into())),
            timeout: std::time::Duration::from_secs(2),
        })
        .await;
        let lower = raw.to_ascii_lowercase();
        assert!(lower.contains("content-type: application/vnd.api+json"));
        assert!(!lower.contains("content-type: application/json"));
    }

    #[tokio::test]
    async fn digest_replays_once_on_401_and_accumulates_wire_clock() {
        use crate::auth_digest::DigestAttempt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<Vec<String>>();
        tokio::spawn(async move {
            let mut seen = Vec::new();
            for i in 0..2 {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap();
                seen.push(String::from_utf8_lossy(&buf[..n]).into_owned());
                if i == 0 {
                    let body = b"auth";
                    let www =
                        "WWW-Authenticate: Digest realm=\"r\", nonce=\"abc\", qop=\"auth\"\r\n";
                    let resp = format!(
                        "HTTP/1.1 401 Unauthorized\r\n{www}Content-Length: {}\r\n\r\n{}",
                        body.len(),
                        std::str::from_utf8(body).unwrap()
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                } else {
                    let _ = sock
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
                        .await;
                }
            }
            let _ = tx.send(seen);
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let ssrf = SsrfChecker::with_allow_private(true);
        let url = format!("http://{addr}/");
        let clock = WireClock::new();
        let resp = send_http_with_digest_timed(
            &client,
            &ssrf,
            HttpCall {
                method: "GET",
                url: &url,
                headers: HashMap::new(),
                body: None,
                timeout: std::time::Duration::from_secs(2),
            },
            Some(DigestAttempt {
                username: "u".into(),
                password: "p".into(),
                algorithm: "MD5".into(),
                realm: None,
            }),
            Some(&clock),
        )
        .await
        .unwrap();
        assert_eq!(resp.status, 200);
        assert!(!clock.total().is_zero());
        let seen = rx.await.unwrap();
        assert_eq!(seen.len(), 2);
        assert!(
            !seen[0]
                .to_ascii_lowercase()
                .contains("authorization: digest")
        );
        assert!(
            seen[1]
                .to_ascii_lowercase()
                .contains("authorization: digest")
        );
    }

    #[test]
    fn extract_url_path_handles_query_and_root() {
        assert_eq!(extract_url_path("https://example.com/a/b?x=1#f"), "/a/b");
        assert_eq!(extract_url_path("http://127.0.0.1:8080"), "/");
    }

    #[test]
    fn inject_correlation_inserts_when_missing() {
        let mut headers = HashMap::new();
        inject_correlation(&mut headers, "11111111-1111-1111-1111-111111111111");
        assert_eq!(
            headers.get("x-request-id").map(String::as_str),
            Some("11111111-1111-1111-1111-111111111111")
        );
    }

    #[test]
    fn inject_correlation_does_not_overwrite() {
        let mut headers = HashMap::new();
        headers.insert("X-Request-Id".into(), "kept".into());
        inject_correlation(&mut headers, "new");
        assert_eq!(
            headers.get("X-Request-Id").map(String::as_str),
            Some("kept")
        );
        assert!(!headers.keys().any(|k| k == "x-request-id"));
    }

    #[test]
    fn inject_correlation_skips_empty() {
        let mut headers = HashMap::new();
        inject_correlation(&mut headers, "");
        assert!(headers.is_empty());
    }

    #[test]
    fn inject_correlation_header_map_grpc_rules() {
        let mut headers = HeaderMap::new();
        inject_correlation_header_map(&mut headers, "");
        assert!(!headers.contains_key("x-request-id"));

        inject_correlation_header_map(&mut headers, "rid-a");
        assert_eq!(
            headers.get("x-request-id").and_then(|v| v.to_str().ok()),
            Some("rid-a")
        );

        headers.clear();
        headers.insert("X-Request-Id", HeaderValue::from_static("kept"));
        inject_correlation_header_map(&mut headers, "new");
        assert_eq!(
            headers.get("x-request-id").and_then(|v| v.to_str().ok()),
            Some("kept")
        );
    }
}
