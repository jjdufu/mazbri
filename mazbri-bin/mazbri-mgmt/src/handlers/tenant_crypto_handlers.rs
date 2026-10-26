use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::middleware::auth::{AdminClaims, can_manage_tenants, extract_user_id};
use crate::response::ApiResponse;
use crate::services::tenant_crypto as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};

fn log_tenant_crypto_audit(
    state: &AppState,
    claims: &AdminClaims,
    action: AuditAction,
    resource_id: impl Into<String>,
    details: serde_json::Value,
    client_ip: Option<String>,
) -> Result<(), ApiError> {
    let user_id = extract_user_id(claims)?;
    state.audit.log(create_audit_log(
        user_id,
        action,
        AuditResourceType::Tenant,
        resource_id.into(),
        Some(details),
        client_ip,
    ));
    Ok(())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_crypto_profile(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::get_profile(&state.db, &code).await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_crypto_profile(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
    Json(payload): Json<svc::UpdateCryptoProfileRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code).await?;
    let data = svc::update_profile(
        &state.db,
        &state.cert_http_client,
        &state.cert_service.url,
        &state.cert_service.token,
        &code,
        &payload,
        actor_id,
    )
    .await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::TenantCryptoProfileUpdate,
        tenant_id.to_string(),
        serde_json::json!(
            { "tenant_code" : code, "tenant_id" : tenant_id, "encryption_mode" :
            payload.encryption_mode, "require_signature" : payload
            .require_signature, "require_request_binding" : payload.require_request_binding,
            "signature_version" : payload.signature_version, "nonce_ttl_seconds" : payload
            .nonce_ttl_seconds, "clock_skew_seconds" : payload
            .clock_skew_seconds, "current_key_version" : payload
            .current_key_version }
        ),
        client_ip,
    )?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_crypto_key(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code).await?;
    let data = svc::create_key(
        &state.db,
        &state.cert_http_client,
        &state.cert_service.url,
        &state.cert_service.token,
        &code,
        actor_id,
        false,
    )
    .await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    let key_version = data
        .get("keyVersion")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::TenantCryptoKeyCreate,
        tenant_id.to_string(),
        serde_json::json!(
            { "tenant_code" : code, "tenant_id" : tenant_id, "key_version" :
            key_version, "status" : data.get("status").cloned(), "rotate" :
            false }
        ),
        client_ip,
    )?;
    Ok(crate::response::created(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn rotate_crypto_key(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code).await?;
    let data = svc::create_key(
        &state.db,
        &state.cert_http_client,
        &state.cert_service.url,
        &state.cert_service.token,
        &code,
        actor_id,
        true,
    )
    .await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    let key_version = data
        .get("keyVersion")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::TenantCryptoKeyRotate,
        tenant_id.to_string(),
        serde_json::json!(
            { "tenant_code" : code, "tenant_id" : tenant_id, "key_version" :
            key_version, "status" : data.get("status").cloned(), "rotate" :
            true }
        ),
        client_ip,
    )?;
    Ok(crate::response::created(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_crypto_keys(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::list_keys(&state.db, &code).await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn activate_crypto_key(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path((code, version)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code).await?;
    let data = svc::set_key_status(svc::SetKeyStatusArgs {
        pool: &state.db,
        cert_client: &state.cert_http_client,
        cert_service_url: &state.cert_service.url,
        cert_service_token: state.cert_service.token.as_str(),
        code: &code,
        version: &version,
        status: "active",
        actor_id,
    })
    .await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::TenantCryptoKeyActivate,
        tenant_id.to_string(),
        serde_json::json!(
            { "tenant_code" : code, "tenant_id" : tenant_id, "key_version" :
            version, "status" : data.get("status").cloned() }
        ),
        client_ip,
    )?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn deprecate_crypto_key(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path((code, version)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code).await?;
    let data = svc::set_key_status(svc::SetKeyStatusArgs {
        pool: &state.db,
        cert_client: &state.cert_http_client,
        cert_service_url: &state.cert_service.url,
        cert_service_token: state.cert_service.token.as_str(),
        code: &code,
        version: &version,
        status: "deprecated",
        actor_id,
    })
    .await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::TenantCryptoKeyDeprecate,
        tenant_id.to_string(),
        serde_json::json!(
            { "tenant_code" : code, "tenant_id" : tenant_id, "key_version" :
            version, "status" : data.get("status").cloned() }
        ),
        client_ip,
    )?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn disable_crypto_key(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path((code, version)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code).await?;
    let data = svc::set_key_status(svc::SetKeyStatusArgs {
        pool: &state.db,
        cert_client: &state.cert_http_client,
        cert_service_url: &state.cert_service.url,
        cert_service_token: state.cert_service.token.as_str(),
        code: &code,
        version: &version,
        status: "disabled",
        actor_id,
    })
    .await?;

    let client_ip = extract_client_ip(&headers, Some(addr));
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::TenantCryptoKeyDisable,
        tenant_id.to_string(),
        serde_json::json!(
            { "tenant_code" : code, "tenant_id" : tenant_id, "key_version" :
            version, "status" : data.get("status").cloned() }
        ),
        client_ip,
    )?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_crypto_key(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path((code, version)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code).await?;
    let data = svc::delete_crypto_key(&state.db, &code, &version, actor_id).await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::TenantCryptoKeyDelete,
        tenant_id.to_string(),
        serde_json::json!(
            { "tenant_code" : code, "tenant_id" : tenant_id, "key_version" : version, "status" : "deleted" }
        ),
        client_ip,
    )?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn export_crypto_data(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Query(query): Query<svc::CryptoExportQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let data = svc::export_crypto_data(&state.db, &query, actor_id).await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::TenantCryptoExport,
        "all".to_string(),
        serde_json::json!(
            { "mode" : query.mode, "manifest" : data.get("manifest").cloned() }
        ),
        client_ip,
    )?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn verify_crypto_keys(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let data = svc::verify_crypto_keys(
        &state.db,
        &state.cert_http_client,
        &state.cert_service.url,
        &state.cert_service.token,
        actor_id,
    )
    .await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::TenantCryptoVerify,
        "all".to_string(),
        serde_json::json!(
            { "checked" : data.get("checked").cloned(), "verified" :
            data.get("verified").cloned(), "failed_count" : data
            .get("failed").and_then(|failed| failed.as_array()).map(std::vec::Vec::len) }
        ),
        client_ip,
    )?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_request_keys(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::list_request_keys(&state.db, &code).await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_request_key(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code).await?;
    let data = svc::create_request_key(
        &state.db,
        &state.cert_http_client,
        &state.cert_service.url,
        &state.cert_service.token,
        &code,
        actor_id,
    )
    .await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::RequestKeyCreate,
        tenant_id.to_string(),
        serde_json::json!({
            "tenant_code": code,
            "kid": data.get("kid"),
            "kind": "request_binding"
        }),
        client_ip,
    )?;
    Ok(crate::response::created(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn retire_request_key(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path((code, kid)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code).await?;
    let data = svc::retire_request_key(&state.db, &code, &kid, actor_id).await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::RequestKeyRetire,
        tenant_id.to_string(),
        serde_json::json!({
            "tenant_code": code,
            "kid": kid,
            "kind": "request_binding",
            "status": "retiring"
        }),
        client_ip,
    )?;
    Ok(ApiResponse::ok(data))
}
