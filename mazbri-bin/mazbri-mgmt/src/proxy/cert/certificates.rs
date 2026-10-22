use super::{
    AdminClaims, ApiError, AppState, AuditAction, AuditResourceType, ConnectInfo, FORBIDDEN_CERT,
    IntoResponse, MAX_PROXY_BODY_BYTES, Method, Path, Request, Response, State,
    can_manage_certificates, create_audit_log, enrich_audit_details, extract_client_ip,
    extract_user_id, forbidden, get_cert_proxy_config, info, json, parse_json_body, proxy_request,
    require_license_issuance,
};

pub async fn get_certificate_proxy(
    State(state): State<AppState>,
    claims: axum::Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Path(id): Path<String>,
    request: Request,
) -> impl IntoResponse {
    if !can_manage_certificates(&claims) {
        return forbidden(FORBIDDEN_CERT);
    }
    if let Err(error) = require_license_issuance(&state).await {
        return error.into_response();
    }
    let config = get_cert_proxy_config(&state);
    proxy_request(
        &state.cert_http_client,
        &config,
        request,
        &format!("/certificates/{id}"),
        Method::GET,
        Some(addr),
    )
    .await
}
pub async fn get_auto_renewal_config_proxy(
    State(state): State<AppState>,
    claims: axum::Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Path(id): Path<String>,
    request: Request,
) -> impl IntoResponse {
    if !can_manage_certificates(&claims) {
        return forbidden(FORBIDDEN_CERT);
    }
    if let Err(error) = require_license_issuance(&state).await {
        return error.into_response();
    }
    let config = get_cert_proxy_config(&state);
    proxy_request(
        &state.cert_http_client,
        &config,
        request,
        &format!("/certificates/{id}/auto-renewal"),
        Method::GET,
        Some(addr),
    )
    .await
}
pub async fn get_full_certificate_proxy(
    State(state): State<AppState>,
    claims: axum::Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Path(id): Path<String>,
    request: Request,
) -> impl IntoResponse {
    if !can_manage_certificates(&claims) {
        return forbidden(FORBIDDEN_CERT);
    }
    if let Err(error) = require_license_issuance(&state).await {
        return error.into_response();
    }
    let config = get_cert_proxy_config(&state);
    proxy_request(
        &state.cert_http_client,
        &config,
        request,
        &format!("/certificates/{id}/full"),
        Method::GET,
        Some(addr),
    )
    .await
}
pub async fn get_ca_certificate_proxy(
    State(state): State<AppState>,
    claims: axum::Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    request: Request,
) -> impl IntoResponse {
    if !can_manage_certificates(&claims) {
        return forbidden(FORBIDDEN_CERT);
    }
    if let Err(error) = require_license_issuance(&state).await {
        return error.into_response();
    }
    let config = get_cert_proxy_config(&state);
    proxy_request(
        &state.cert_http_client,
        &config,
        request,
        "/ca",
        Method::GET,
        Some(addr),
    )
    .await
}
pub async fn configure_auto_renewal_proxy(
    State(state): State<AppState>,
    claims: axum::Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Path(id): Path<String>,
    request: Request,
) -> impl IntoResponse {
    if !can_manage_certificates(&claims) {
        return forbidden(FORBIDDEN_CERT);
    }
    info!(
        "Admin {} configuring auto-renewal for certificate {}",
        claims.sub, id
    );
    if let Err(error) = require_license_issuance(&state).await {
        return error.into_response();
    }
    let config = get_cert_proxy_config(&state);
    proxy_request(
        &state.cert_http_client,
        &config,
        request,
        &format!("/certificates/{id}/auto-renewal"),
        Method::PUT,
        Some(addr),
    )
    .await
}
proxy_handler_with_audit! {
    name = renew_certificate_proxy, method = Method::POST, path = "/renew/{}", path_param
    = id, audit_action = AuditAction::CertificateRenew, resource_id_fn = id.clone(),
    audit_details = Some(serde_json::json!({ "certificate_id" : id })), log =
    "Admin {} renewing certificate {}",
}
proxy_handler_with_audit! {
    name = revoke_certificate_proxy, method = Method::POST, path = "/revoke/{}",
    path_param = id, audit_action = AuditAction::CertificateRevoke, resource_id_fn = id
    .clone(), audit_details = Some(serde_json::json!({ "certificate_id" : id })), log =
    "Admin {} revoking certificate {}",
}
pub async fn issue_certificate_proxy(
    State(state): State<AppState>,
    claims: axum::Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    request: Request,
) -> Result<Response, ApiError> {
    if !can_manage_certificates(&claims) {
        return Err(ApiError::Forbidden);
    }
    info!("Admin {} issuing certificate", claims.sub);
    require_license_issuance(&state).await?;
    let (parts, body) = request.into_parts();
    let body_bytes = axum::body::to_bytes(body, MAX_PROXY_BODY_BYTES)
        .await
        .unwrap_or_default();
    let client_ip = extract_client_ip(&parts.headers, Some(addr));
    let audit_details = enrich_audit_details(
        parse_json_body(&body_bytes),
        client_ip.as_deref(),
        |details| {
            details
                .entry("action".to_string())
                .or_insert_with(|| json!("issue"));
        },
    );
    let config = get_cert_proxy_config(&state);
    let response = proxy_request(
        &state.cert_http_client,
        &config,
        Request::from_parts(parts, axum::body::Body::from(body_bytes)),
        "/issue",
        Method::POST,
        Some(addr),
    )
    .await;
    if response.status().is_success() {
        let audit_user_id = extract_user_id(&claims)?;
        state.audit.log(create_audit_log(
            audit_user_id,
            AuditAction::CertificateIssue,
            AuditResourceType::Certificate,
            None::<String>,
            Some(audit_details),
            client_ip,
        ));
    }
    Ok(response)
}
