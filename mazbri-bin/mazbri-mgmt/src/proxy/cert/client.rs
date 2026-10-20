use super::{error, json, AppState, CertProxyConfig, Client, Request, Method, Response, extract_client_ip, header, MAX_PROXY_BODY_BYTES, IntoResponse, StatusCode, Json, Value};

pub(crate) fn get_cert_proxy_config(state: &AppState) -> CertProxyConfig {
    CertProxyConfig {
        cert_service_url: state.cert_service.url.clone(),
        timeout_secs: 30,
        cert_service_token: state.cert_service.token.clone(),
    }
}
pub(crate) async fn proxy_request(
    client: &Client,
    config: &CertProxyConfig,
    request: Request,
    path: &str,
    method: Method,
    peer_addr: Option<std::net::SocketAddr>,
) -> Response {
    let client_ip = extract_client_ip(request.headers(), peer_addr);
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(std::string::ToString::to_string);
    let mut url = format!("{}{}", config.cert_service_url, path);
    if let Some(query) = request.uri().query() {
        if !query.is_empty() {
            url = format!("{url}?{query}");
        }
    }
    let body_bytes = axum::body::to_bytes(request.into_body(), MAX_PROXY_BODY_BYTES)
        .await
        .unwrap_or_default();
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
    req = req.bearer_auth(&config.cert_service_token);
    if method == Method::POST || method == Method::PUT {
        req = req.body(body_bytes);
        if let Some(content_type) = content_type.as_deref() {
            req = req.header(header::CONTENT_TYPE, content_type);
        } else {
            req = req.header(header::CONTENT_TYPE, "application/json");
        }
    }
    match req.send().await {
        Ok(response) => {
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
