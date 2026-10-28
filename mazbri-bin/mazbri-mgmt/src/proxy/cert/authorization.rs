use super::{
    AdminClaims, ApiError, AppState, ConnectInfo, FORBIDDEN_CERT, HeaderMap, IntoResponse, Json,
    Method, Multipart, Request, ReqwestMultipartForm, ReqwestMultipartPart, Response, State,
    StatusCode, can_manage_certificates, error, extract_client_ip, forbidden,
    get_cert_proxy_config, header, proxy_request,
};

pub async fn export_license_request_proxy(
    State(state): State<AppState>,
    claims: axum::Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    request: Request,
) -> Result<Response, ApiError> {
    if !can_manage_certificates(&claims) {
        return Err(ApiError::Forbidden);
    }
    let config = get_cert_proxy_config(&state);
    Ok(proxy_request(
        &state.cert_http_client,
        &config,
        request,
        "/authorizations/license/request",
        Method::POST,
        Some(addr),
    )
    .await)
}

pub async fn get_license_status_proxy(
    State(state): State<AppState>,
    claims: axum::Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    request: Request,
) -> Result<Response, ApiError> {
    if !can_manage_certificates(&claims) {
        return Err(ApiError::Forbidden);
    }
    let config = get_cert_proxy_config(&state);
    Ok(proxy_request(
        &state.cert_http_client,
        &config,
        request,
        "/authorizations/license/status",
        Method::GET,
        Some(addr),
    )
    .await)
}

pub async fn import_license_proxy(
    State(state): State<AppState>,
    claims: axum::Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Response, ApiError> {
    if !can_manage_certificates(&claims) {
        return Err(ApiError::Forbidden);
    }
    let mut license_file: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| ApiError::validation(format!("Failed to read multipart form: {error}")))?
    {
        if field.name() == Some("license_file") {
            let bytes = field.bytes().await.map_err(|error| {
                ApiError::validation(format!("Failed to read license file: {error}"))
            })?;
            if bytes.len() > 64 * 1024 {
                return Err(ApiError::validation("许可证文件不能超过 64 KiB"));
            }
            license_file = Some(bytes.to_vec());
        }
    }
    let Some(license_file) = license_file else {
        return Err(ApiError::validation("license_file is required"));
    };
    if let Err(response) = crate::handlers::mfa_guard::require_step_up(&state, &claims).await {
        return Ok(*response);
    }
    let config = get_cert_proxy_config(&state);
    let url = format!(
        "{}{}",
        config.cert_service_url, "/authorizations/license/import"
    );
    let mut req = state.cert_http_client.post(url);
    if let Some(ip) = extract_client_ip(&headers, Some(addr)) {
        req = req.header("x-forwarded-for", ip);
    }
    if let Some(admin_id) = headers
        .get("x-mazbri-admin-id")
        .and_then(|value| value.to_str().ok())
    {
        req = req.header("x-mazbri-admin-id", admin_id);
    }
    req = super::client::forward_request_id(req, &headers);
    req = req.bearer_auth(&config.cert_service_token);
    let form = ReqwestMultipartForm::new().part(
        "license_file",
        ReqwestMultipartPart::bytes(license_file)
            .file_name("license.jws")
            .mime_str("application/jose")
            .map_err(|error| {
                ApiError::validation(format!("Invalid license content type: {error}"))
            })?,
    );
    match crate::audit::actor_header::send_to_cert(req.multipart(form)).await {
        Ok(response) => {
            let status = StatusCode::from_u16(response.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let body = response.text().await.unwrap_or_default();
            Ok((status, [(header::CONTENT_TYPE, "application/json")], body).into_response())
        }
        Err(error) => {
            error!("Failed to connect to cert-service: {}", error);
            Ok((
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": "cert_service_unavailable",
                    "message": "证书服务不可用"
                })),
            )
                .into_response())
        }
    }
}

proxy_handler! {
    name = list_certificates_proxy, method = Method::GET, path = "/certificates",
    forbidden = FORBIDDEN_CERT,
}
