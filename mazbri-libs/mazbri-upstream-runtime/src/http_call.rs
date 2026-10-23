//! Shared HTTP send used by adapters.
//! If the call already has a Content-Type header, `BodyKind` does not overwrite it.

use reqwest::Method;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::auth_digest::{
    DigestAttempt, build_digest_authorization, parse_www_authenticate_digest,
};
use crate::ssrf::SsrfChecker;
use crate::types::{BodyKind, ExecuteError, OutboundResponse};

pub struct HttpCall<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub headers: HashMap<String, String>,
    pub body: Option<BodyKind>,
    pub timeout: Duration,
}

/// Best-effort: insert `x-request-id` when missing. Never overwrites. Empty id is a no-op.
/// Does not mint a UUID — callers that have no serial skip this.
pub(crate) fn inject_x_request_id(headers: &mut HashMap<String, String>, request_id: &str) {
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

/// Same rules as [`inject_x_request_id`] for HTTP/`HeaderMap` (gRPC metadata).
pub(crate) fn inject_x_request_id_header_map(headers: &mut HeaderMap, request_id: &str) {
    if request_id.is_empty() || headers.contains_key("x-request-id") {
        return;
    }
    if let Ok(hv) = HeaderValue::from_str(request_id) {
        headers.insert(HeaderName::from_static("x-request-id"), hv);
    }
}

pub async fn send_http(
    client: &reqwest::Client,
    ssrf: &SsrfChecker,
    call: HttpCall<'_>,
) -> Result<OutboundResponse, ExecuteError> {
    if let Err(e) = ssrf.validate_url(call.url).await {
        return Err(ExecuteError::SsrfBlocked(e));
    }
    let method_type = Method::from_bytes(call.method.as_bytes()).map_err(|e| {
        ExecuteError::InvalidMethod(format!("Invalid HTTP method '{}': {}", call.method, e))
    })?;

    let mut header_map = HeaderMap::new();
    for (k, v) in &call.headers {
        if k.to_lowercase().starts_with("x-mazbri-") {
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
        let ct = body.content_type().to_string();
        let bytes = body.into_bytes();
        if !has_content_type {
            builder = builder.header("content-type", ct);
        }
        builder = builder.body(bytes);
    }

    let started = Instant::now();
    let resp = builder.send().await.map_err(|e| {
        if e.is_timeout() {
            ExecuteError::UpstreamTransport(format!("timeout: {e}"))
        } else {
            ExecuteError::UpstreamTransport(e.to_string())
        }
    })?;
    let _elapsed = started.elapsed();
    let status_code = resp.status();
    let mut resp_headers = HashMap::new();
    for (key, val) in resp.headers() {
        if let Ok(str_val) = val.to_str() {
            resp_headers.insert(key.to_string(), str_val.to_string());
        }
    }
    let body_bytes = resp.bytes().await.unwrap_or_default().to_vec();
    let path = url::Url::parse(call.url)
        .ok()
        .map(|u| u.path().to_string())
        .unwrap_or_default();
    Ok(OutboundResponse {
        status: status_code.as_u16(),
        headers: resp_headers,
        body: body_bytes,
        final_url: call.url.to_string(),
        method: call.method.to_string(),
        path,
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
/// Non-digest callers pass `None` (same path as `send_http`).
pub async fn send_http_with_digest(
    client: &reqwest::Client,
    ssrf: &SsrfChecker,
    call: HttpCall<'_>,
    digest: Option<DigestAttempt>,
) -> Result<OutboundResponse, ExecuteError> {
    let Some(digest) = digest else {
        return send_http(client, ssrf, call).await;
    };
    let method = call.method;
    let url = call.url;
    let timeout = call.timeout;
    let headers = call.headers.clone();
    let body = call.body.clone();
    let first = send_http(client, ssrf, call).await?;
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
    send_http(
        client,
        ssrf,
        HttpCall {
            method,
            url,
            headers: headers2,
            body,
            timeout,
        },
    )
    .await
}

pub fn auth_only_response(call: &HttpCall<'_>) -> OutboundResponse {
    let path = url::Url::parse(call.url)
        .ok()
        .map(|u| u.path().to_string())
        .unwrap_or_default();
    OutboundResponse {
        status: 204,
        headers: call.headers.clone(),
        body: Vec::new(),
        final_url: call.url.to_string(),
        method: call.method.to_string(),
        path,
    }
}

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

pub fn append_query(url: &str, query: &[(String, String)]) -> String {
    if query.is_empty() {
        return url.to_string();
    }
    let mut parts = Vec::new();
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
        send_http(&client, &ssrf, call).await.unwrap();
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
    async fn digest_replays_once_on_401() {
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
        let resp = send_http_with_digest(
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
        )
        .await
        .unwrap();
        assert_eq!(resp.status, 200);
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
    fn inject_x_request_id_inserts_when_missing() {
        let mut headers = HashMap::new();
        inject_x_request_id(&mut headers, "11111111-1111-1111-1111-111111111111");
        assert_eq!(
            headers.get("x-request-id").map(String::as_str),
            Some("11111111-1111-1111-1111-111111111111")
        );
    }

    #[test]
    fn inject_x_request_id_does_not_overwrite() {
        let mut headers = HashMap::new();
        headers.insert("X-Request-Id".into(), "kept".into());
        inject_x_request_id(&mut headers, "new");
        assert_eq!(
            headers.get("X-Request-Id").map(String::as_str),
            Some("kept")
        );
        assert!(!headers.keys().any(|k| k == "x-request-id"));
    }

    #[test]
    fn inject_x_request_id_skips_empty() {
        let mut headers = HashMap::new();
        inject_x_request_id(&mut headers, "");
        assert!(headers.is_empty());
    }

    #[test]
    fn inject_x_request_id_header_map_grpc_rules() {
        let mut headers = HeaderMap::new();
        inject_x_request_id_header_map(&mut headers, "");
        assert!(!headers.contains_key("x-request-id"));

        inject_x_request_id_header_map(&mut headers, "rid-a");
        assert_eq!(
            headers.get("x-request-id").and_then(|v| v.to_str().ok()),
            Some("rid-a")
        );

        headers.clear();
        headers.insert("X-Request-Id", HeaderValue::from_static("kept"));
        inject_x_request_id_header_map(&mut headers, "new");
        assert_eq!(
            headers.get("x-request-id").and_then(|v| v.to_str().ok()),
            Some("kept")
        );
    }
}
