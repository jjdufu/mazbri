use super::{
    AppState, CertProxyConfig, Client, IntoResponse, Json, MAX_PROXY_BODY_BYTES, Method, Request,
    Response, StatusCode, Value, error, extract_client_ip, header, json,
};

pub(crate) fn get_cert_proxy_config(state: &AppState) -> CertProxyConfig {
    CertProxyConfig {
        cert_service_url: state.cert_service.url.clone(),
        timeout_secs: 30,
        cert_service_token: state.cert_service.token.clone(),
    }
}
/// 透传 `x-request-id`，便于跨服务关联同一请求的审计与日志。
pub(crate) fn forward_request_id(
    req: reqwest::RequestBuilder,
    headers: &axum::http::HeaderMap,
) -> reqwest::RequestBuilder {
    match headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 128)
    {
        Some(request_id) => req.header("x-request-id", request_id),
        None => req,
    }
}
fn invalid_proxy_path_response() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": "invalid_proxy_path",
            "message": "非法的证书代理目标路径"
        })),
    )
        .into_response()
}

/// 严格断言代理转发的 `path`：必须以单 `/` 开头，严禁包含 `..`、`\`、`?`、`#`、连续 `//`
/// 或百分号编码后的路径穿越 / 段内分隔符。
pub(crate) fn validate_proxy_path(path: &str) -> Result<(), Box<Response>> {
    if !path.starts_with('/')
        || path.contains("..")
        || path.contains('\\')
        || path.contains('?')
        || path.contains('#')
        || path.contains("//")
    {
        return Err(Box::new(invalid_proxy_path_response()));
    }
    let Ok(decoded) = urlencoding::decode(path) else {
        return Err(Box::new(invalid_proxy_path_response()));
    };
    if decoded.contains("..")
        || decoded.contains('\\')
        || decoded.contains('?')
        || decoded.contains('#')
        || decoded.contains("//")
        || decoded.split('/').count() != path.split('/').count()
    {
        return Err(Box::new(invalid_proxy_path_response()));
    }
    Ok(())
}

/// 读取代理请求体并限制在 `MAX_PROXY_BODY_BYTES`（1 MiB）以内；超限时直接返回 `413 Payload Too Large`，
/// 绝不静默降级为空请求体。
pub(crate) async fn read_proxy_body(
    body: axum::body::Body,
) -> Result<axum::body::Bytes, Box<Response>> {
    axum::body::to_bytes(body, MAX_PROXY_BODY_BYTES)
        .await
        .map_err(|err| {
            error!("Proxy request body rejected (max {MAX_PROXY_BODY_BYTES} bytes): {err}");
            Box::new(
                (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    Json(serde_json::json!({
                        "error": "payload_too_large",
                        "message": "请求体超过最大限制（1 MiB）"
                    })),
                )
                    .into_response(),
            )
        })
}

pub(crate) async fn proxy_request(
    client: &Client,
    config: &CertProxyConfig,
    request: Request,
    path: &str,
    method: Method,
    peer_addr: Option<std::net::SocketAddr>,
) -> Response {
    if let Err(response) = validate_proxy_path(path) {
        return *response;
    }
    let client_ip = extract_client_ip(request.headers(), peer_addr);
    let forwarded_headers = request.headers().clone();
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(std::string::ToString::to_string);
    let mut url = format!("{}{}", config.cert_service_url.trim_end_matches('/'), path);
    if let Some(query) = request.uri().query()
        && !query.is_empty()
    {
        if query.contains('#') {
            return invalid_proxy_path_response();
        }
        url = format!("{url}?{query}");
    }
    let body_bytes = match read_proxy_body(request.into_body()).await {
        Ok(bytes) => bytes,
        Err(response) => return *response,
    };
    let mut req = match method {
        Method::GET => client.get(&url),
        Method::POST => client.post(&url),
        Method::PUT => client.put(&url),
        Method::DELETE => client.delete(&url),
        _ => {
            error!("Unsupported method for proxy: {}", method);
            return (
                StatusCode::METHOD_NOT_ALLOWED,
                Json(serde_json::json!(
                    { "error" : "method_not_allowed", "message" : "方法不允许" }
                )),
            )
                .into_response();
        }
    };
    if let Some(ip) = client_ip {
        req = req.header("x-forwarded-for", ip);
    }
    req = forward_request_id(req, &forwarded_headers);
    req = req.bearer_auth(&config.cert_service_token);
    if method == Method::POST || method == Method::PUT {
        req = req.body(body_bytes);
        if let Some(content_type) = content_type.as_deref() {
            req = req.header(header::CONTENT_TYPE, content_type);
        } else {
            req = req.header(header::CONTENT_TYPE, "application/json");
        }
    }
    match crate::audit::actor_header::send_to_cert(req).await {
        Ok(response) => forward_upstream_response(response).await,
        Err(e) => {
            error!("Failed to connect to cert-service: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!(
                    { "error" : "cert_service_unavailable", "message" :
                    "证书服务不可用" }
                )),
            )
                .into_response()
        }
    }
}

async fn forward_upstream_response(response: reqwest::Response) -> Response {
    let status = StatusCode::from_u16(response.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();
    match response.text().await {
        Ok(body) => (status, [(header::CONTENT_TYPE, content_type)], body).into_response(),
        Err(e) => {
            error!("Failed to read cert-service response: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!(
                    { "error" : "proxy_error", "message" :
                    "读取上游响应失败" }
                )),
            )
                .into_response()
        }
    }
}
pub(crate) enum CertificateLookup {
    Present,
    Missing,
    Unavailable,
}

/// 只确认证书存在。打 `GET /certificates/{id}`，不打 `/full`，因此不会取出 `key_pem`。
pub(crate) async fn lookup_certificate(
    client: &Client,
    config: &CertProxyConfig,
    id: uuid::Uuid,
) -> CertificateLookup {
    let url = format!(
        "{}/certificates/{id}",
        config.cert_service_url.trim_end_matches('/')
    );
    let req = client.get(url).bearer_auth(&config.cert_service_token);
    match crate::audit::actor_header::send_to_cert(req).await {
        Ok(response) if response.status().is_success() => CertificateLookup::Present,
        Ok(response) if response.status().as_u16() == 404 => CertificateLookup::Missing,
        Ok(response) => {
            error!(
                "cert-service existence check returned {}",
                response.status()
            );
            CertificateLookup::Unavailable
        }
        Err(error) => {
            error!("cert-service existence check failed: {error}");
            CertificateLookup::Unavailable
        }
    }
}

pub(crate) fn parse_json_body(body_bytes: &[u8]) -> Option<Value> {
    if body_bytes.is_empty() {
        return None;
    }
    serde_json::from_slice(body_bytes).ok()
}
pub(crate) fn enrich_audit_details(
    details: Option<Value>,
    client_ip: Option<&str>,
    mutate: impl FnOnce(&mut serde_json::Map<String, Value>),
) -> Value {
    let mut details = details.unwrap_or_else(|| json!({}));
    let Some(object) = details.as_object_mut() else {
        return details;
    };
    if let Some(ip) = client_ip {
        object.insert("client_ip".to_string(), json!(ip));
    }
    mutate(object);
    details
}
pub(crate) fn forbidden(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({ "error" : "forbidden", "message" : message })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::cert::encode_cert_path_segment;
    use axum::Router;
    use axum::body::Body;
    use axum::extract::{Path, State};
    use axum::routing::{get, post, put};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;

    #[derive(Clone)]
    struct TestProxyState {
        client: Client,
        config: CertProxyConfig,
    }

    async fn spawn_mock_cert_upstream() -> (String, Arc<AtomicUsize>) {
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_clone = Arc::clone(&hits);
        let upstream = Router::new().fallback(move |req: Request| {
            let hits = Arc::clone(&hits_clone);
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                (
                    StatusCode::OK,
                    Json(serde_json::json!({ "forwarded_path": req.uri().path() })),
                )
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, upstream).await;
        });
        (format!("http://{addr}"), hits)
    }

    async fn forward_uuid(
        State(st): State<TestProxyState>,
        Path(id): Path<uuid::Uuid>,
        req: Request,
    ) -> Response {
        let method = req.method().clone();
        let target = format!("/certificates/{id}");
        proxy_request(&st.client, &st.config, req, &target, method, None).await
    }

    async fn forward_subject(
        State(st): State<TestProxyState>,
        Path(subject): Path<String>,
        req: Request,
    ) -> Response {
        let encoded = match encode_cert_path_segment(&subject) {
            Ok(seg) => seg,
            Err(err) => return err.into_response(),
        };
        let target = format!("/certificates/by-subject/{encoded}");
        proxy_request(&st.client, &st.config, req, &target, Method::GET, None).await
    }

    fn build_typed_proxy_router(state: TestProxyState) -> Router {
        let inner = Router::new()
            .route("/{id}", get(forward_uuid))
            .route("/{id}/full", get(forward_uuid))
            .route("/{id}/auto-renewal", put(forward_uuid))
            .route("/renew/{id}", post(forward_uuid))
            .route("/revoke/{id}", post(forward_uuid))
            .route("/by-subject/{subject}", get(forward_subject))
            .with_state(state);
        Router::new().nest("/api/v1/mgmt/certificates", inner)
    }

    #[tokio::test]
    async fn blocks_encoded_path_traversal_on_router_before_reaching_cert_service() {
        let (upstream_url, hits) = spawn_mock_cert_upstream().await;
        let state = TestProxyState {
            client: Client::new(),
            config: CertProxyConfig {
                cert_service_url: upstream_url,
                timeout_secs: 5,
                cert_service_token: "test-master".to_string(),
            },
        };
        let app = build_typed_proxy_router(state);

        for (method, uri) in [
            (
                "GET",
                "/api/v1/mgmt/certificates/..%2Fauthorizations%2Froot%2Fcurrent%2Fmaterials",
            ),
            (
                "GET",
                "/api/v1/mgmt/certificates/..%2Fconfigs%2Fmazbri-gateway%3Fx",
            ),
            (
                "PUT",
                "/api/v1/mgmt/certificates/..%2Fconfigs%2Fmazbri-gateway%3Fx/auto-renewal",
            ),
            (
                "GET",
                "/api/v1/mgmt/certificates/..%2Fauthorizations%2Froot%2Fcurrent%2Fmaterials/full",
            ),
            ("POST", "/api/v1/mgmt/certificates/renew/..%2Fissue"),
            (
                "POST",
                "/api/v1/mgmt/certificates/revoke/..%2Fconfigs%2Fglobal",
            ),
            (
                "GET",
                "/api/v1/mgmt/certificates/by-subject/..%2Fauthorizations%2Froot%2Fcurrent%2Fmaterials",
            ),
            (
                "GET",
                "/api/v1/mgmt/certificates/by-subject/..%2Fconfigs%2Fmazbri-gateway%3Fx",
            ),
        ] {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("req"),
                )
                .await
                .expect("resp");
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{method} {uri}");
        }
        assert_eq!(
            hits.load(Ordering::SeqCst),
            0,
            "path traversal attempts must never reach cert-service"
        );

        for uri in [
            "/api/v1/mgmt/certificates/00000000-0000-0000-0000-000000000001",
            "/api/v1/mgmt/certificates/by-subject/mazbri-gateway",
        ] {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(uri)
                        .body(Body::empty())
                        .expect("req"),
                )
                .await
                .expect("resp");
            assert_eq!(resp.status(), StatusCode::OK, "{uri}");
        }
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn proxy_request_path_assertion_rejects_unsafe_paths_without_forwarding() {
        let (upstream_url, hits) = spawn_mock_cert_upstream().await;
        let client = Client::new();
        let config = CertProxyConfig {
            cert_service_url: upstream_url,
            timeout_secs: 5,
            cert_service_token: "test-master".to_string(),
        };
        for bad_path in [
            "/certificates/..%2Fauthorizations%2Froot%2Fcurrent%2Fmaterials",
            "/certificates/..%2Fconfigs%2Fmazbri-gateway%3Fx",
            "/certificates/../authorizations/root/current/materials",
            "/configs/mazbri-gateway?dry_run=1",
            "/configs/mazbri-gateway#frag",
            "/configs//mazbri-gateway",
            "/configs\\mazbri-gateway",
            "certificates/00000000-0000-0000-0000-000000000001",
        ] {
            let req = Request::builder()
                .method(Method::GET)
                .uri("/test")
                .body(Body::empty())
                .expect("req");
            let resp = proxy_request(&client, &config, req, bad_path, Method::GET, None).await;
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{bad_path}");
        }
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn oversized_body_returns_413_payload_too_large_without_forwarding() {
        let (upstream_url, hits) = spawn_mock_cert_upstream().await;
        let client = Client::new();
        let config = CertProxyConfig {
            cert_service_url: upstream_url,
            timeout_secs: 5,
            cert_service_token: "test-master".to_string(),
        };
        let oversized = vec![b'a'; MAX_PROXY_BODY_BYTES + 1];
        let req = Request::builder()
            .method(Method::POST)
            .uri("/issue")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(oversized))
            .expect("req");
        let resp = proxy_request(&client, &config, req, "/issue", Method::POST, None).await;
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            hits.load(Ordering::SeqCst),
            0,
            "oversized body must not be coerced to empty body or forwarded"
        );
    }

    #[test]
    fn cert_proxy_handlers_and_segment_encoder_enforce_strong_types() {
        assert_eq!(
            encode_cert_path_segment("mazbri-gateway").expect("gateway"),
            "mazbri-gateway"
        );
        assert_eq!(
            encode_cert_path_segment("global").expect("global"),
            "global"
        );
        assert!(encode_cert_path_segment("..%2Fauthorizations%2Froot%2Fcurrent%2Fmaterials").is_err());
        assert!(encode_cert_path_segment("..%2Fconfigs%2Fmazbri-gateway%3Fx").is_err());
        assert!(encode_cert_path_segment("mazbri-cert-service").is_err());

        let certs_src = include_str!("certificates.rs");
        let common_src = include_str!("common.rs");
        assert!(
            !certs_src.contains("Path<String>"),
            "certificates.rs must not accept untyped Path<String>"
        );
        assert!(
            !common_src.contains("Path<String>"),
            "common.rs macros must not accept untyped Path<String>"
        );
    }
}
