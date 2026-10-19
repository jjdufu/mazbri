use super::*;

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
proxy_handler! {
    name = list_presets_proxy, method = Method::GET, path = "/presets", forbidden =
    FORBIDDEN_CERT,
}
pub async fn get_preset_proxy(
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
        &format!("/presets/{id}"),
        Method::GET,
        Some(addr),
    )
    .await
}
proxy_handler! {
    name = list_service_names_proxy, method = Method::GET, path = "/services", forbidden
    = FORBIDDEN_SVC,
}
proxy_handler! {
    name = get_service_name_proxy, method = Method::GET, path = "/services/{}",
    path_param = name, forbidden = FORBIDDEN_SVC,
}
proxy_handler! {
    name = validate_service_name_proxy, method = Method::GET, path =
    "/services/{}/validate", path_param = name, forbidden = FORBIDDEN_SVC,
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
proxy_handler_with_log! {
    name = create_service_name_proxy, method = Method::POST, path = "/services",
    forbidden = FORBIDDEN_SVC, log = "Admin {} creating service name",
}
proxy_handler_with_log! {
    name = update_service_name_proxy, method = Method::PUT, path = "/services/{}",
    path_param = name, forbidden = FORBIDDEN_SVC, log =
    "Admin {} updating service name {}",
}
proxy_handler_with_log! {
    name = delete_service_name_proxy, method = Method::DELETE, path = "/services/{}",
    path_param = name, forbidden = FORBIDDEN_SVC, log =
    "Admin {} deleting service name {}",
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
            audit_details,
            client_ip,
        ));
    }
    Ok(response)
}
pub async fn create_preset_proxy(
    State(state): State<AppState>,
    claims: axum::Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    request: Request,
) -> Result<Response, ApiError> {
    if !can_manage_certificates(&claims) {
        return Err(ApiError::Forbidden);
    }
    require_license_issuance(&state).await?;
    let (parts, body) = request.into_parts();
    let body_bytes = axum::body::to_bytes(body, MAX_PROXY_BODY_BYTES)
        .await
        .unwrap_or_default();
    let is_import = parts
        .headers
        .get(PRESET_IMPORT_AUDIT_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case(PRESET_IMPORT_AUDIT_VALUE));
    let client_ip = extract_client_ip(&parts.headers, Some(addr));
    let audit_details = enrich_audit_details(
        parse_json_body(&body_bytes),
        client_ip.as_deref(),
        |details| {
            details.insert(
                "operation".to_string(),
                json!(if is_import { "import" } else { "create" }),
            );
        },
    );
    info!(
        "Admin {} {} preset DNSName",
        claims.sub,
        if is_import { "importing" } else { "creating" }
    );
    let config = get_cert_proxy_config(&state);
    let response = proxy_request(
        &state.cert_http_client,
        &config,
        Request::from_parts(parts, axum::body::Body::from(body_bytes)),
        "/presets",
        Method::POST,
        Some(addr),
    )
    .await;
    if response.status().is_success() {
        let audit_user_id = extract_user_id(&claims)?;
        state.audit.log(create_audit_log(
            audit_user_id,
            if is_import {
                AuditAction::CertificatePresetImport
            } else {
                AuditAction::CertificatePresetCreate
            },
            AuditResourceType::CertificatePreset,
            None::<String>,
            audit_details,
            client_ip,
        ));
    }
    Ok(response)
}
pub async fn update_preset_proxy(
    State(state): State<AppState>,
    claims: axum::Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Path(id): Path<String>,
    request: Request,
) -> Result<Response, ApiError> {
    if !can_manage_certificates(&claims) {
        return Err(ApiError::Forbidden);
    }
    info!("Admin {} updating preset DNSName {}", claims.sub, id);
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
            details.insert("preset_id".to_string(), json!(id.clone()));
            details.insert("operation".to_string(), json!("update"));
        },
    );
    let config = get_cert_proxy_config(&state);
    let resolved_path = format!("/presets/{id}");
    let response = proxy_request(
        &state.cert_http_client,
        &config,
        Request::from_parts(parts, axum::body::Body::from(body_bytes)),
        &resolved_path,
        Method::PUT,
        Some(addr),
    )
    .await;
    if response.status().is_success() {
        let audit_user_id = extract_user_id(&claims)?;
        state.audit.log(create_audit_log(
            audit_user_id,
            AuditAction::CertificatePresetUpdate,
            AuditResourceType::CertificatePreset,
            id.clone(),
            audit_details,
            client_ip,
        ));
    }
    Ok(response)
}
pub async fn delete_preset_proxy(
    State(state): State<AppState>,
    claims: axum::Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Path(id): Path<String>,
    request: Request,
) -> Result<Response, ApiError> {
    if !can_manage_certificates(&claims) {
        return Err(ApiError::Forbidden);
    }
    info!("Admin {} deleting preset DNSName {}", claims.sub, id);
    require_license_issuance(&state).await?;
    let client_ip = extract_client_ip(request.headers(), Some(addr));
    let config = get_cert_proxy_config(&state);
    let resolved_path = format!("/presets/{id}");
    let response = proxy_request(
        &state.cert_http_client,
        &config,
        request,
        &resolved_path,
        Method::DELETE,
        Some(addr),
    )
    .await;
    if response.status().is_success() {
        let audit_user_id = extract_user_id(&claims)?;
        state.audit.log(create_audit_log(
            audit_user_id,
            AuditAction::CertificatePresetDelete,
            AuditResourceType::CertificatePreset,
            id.clone(),
            Some(json!(
                { "preset_id" : id, "operation" : "delete", "client_ip" :
                client_ip.clone(), }
            )),
            client_ip,
        ));
    }
    Ok(response)
}
