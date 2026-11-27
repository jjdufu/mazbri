use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::handlers::mfa_guard::require_category;
use crate::middleware::auth::{
    AdminClaims, can_manage_tenants, can_write_business, extract_user_id, is_admin,
};
use crate::response::ApiResponse;
use crate::services::mfa::Category;
use crate::services::tenant_crypto as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
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

/// 传输加密页统一读取：`{ profile, key, license_status, actions }`。
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_tenant_crypto(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::get_crypto_bundle(
        &state.db,
        &state.cert_http_client,
        &state.cert_service.url,
        &state.cert_service.token,
        &code,
        can_write_business(&claims),
        is_admin(&claims),
    )
    .await?;
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
    Json(mut payload): Json<svc::UpdateCryptoProfileRequest>,
) -> Result<Response, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code).await?;
    // allow_insecure 仅超级管理员可改写；非管理员强制沿用库中现值。
    if !is_admin(&claims) {
        let current = svc::get_profile(&state.db, &code).await?;
        let current_allow = current
            .get("allow_insecure")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if payload.allow_insecure && !current_allow {
            return Err(ApiError::Forbidden);
        }
        payload.allow_insecure = current_allow;
    }
    if let Err(response) = require_category(&state, &claims, Category::Key).await {
        return Ok(*response);
    }
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
        serde_json::json!({
            "tenant_code": code,
            "tenant_id": tenant_id,
            "encryption_mode": payload.encryption_mode,
            "suite": payload.suite,
            "data_alg": payload.data_alg,
            "mac_alg": payload.mac_alg,
            "require_signature": payload.require_signature,
            "allow_insecure": payload.allow_insecure,
            "nonce_ttl_seconds": payload.nonce_ttl_seconds,
            "clock_skew_seconds": payload.clock_skew_seconds,
        }),
        client_ip,
    )?;
    Ok(ApiResponse::ok(data).into_response())
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
) -> Result<Response, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code).await?;
    if let Err(response) = require_category(&state, &claims, Category::Key).await {
        return Ok(*response);
    }
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
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::TenantCryptoKeyCreate,
        tenant_id.to_string(),
        serde_json::json!({
            "tenant_code": code,
            "tenant_id": tenant_id,
            "key_id": data.get("id"),
            "rotate": false
        }),
        client_ip,
    )?;
    Ok(crate::response::created(data).into_response())
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
) -> Result<Response, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code).await?;
    if let Err(response) = require_category(&state, &claims, Category::Key).await {
        return Ok(*response);
    }
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
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::TenantCryptoKeyRotate,
        tenant_id.to_string(),
        serde_json::json!({
            "tenant_code": code,
            "tenant_id": tenant_id,
            "key_id": data.get("id"),
            "rotate": true
        }),
        client_ip,
    )?;
    Ok(crate::response::created(data).into_response())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_crypto_key(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Result<Response, ApiError> {
    if !can_manage_tenants(&claims) {
        return Err(ApiError::Forbidden);
    }
    let actor_id = extract_user_id(&claims)?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code).await?;
    if let Err(response) = require_category(&state, &claims, Category::Key).await {
        return Ok(*response);
    }
    let data = svc::delete_crypto_key(&state.db, &code, actor_id).await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    log_tenant_crypto_audit(
        &state,
        &claims,
        AuditAction::TenantCryptoKeyDelete,
        tenant_id.to_string(),
        serde_json::json!({
            "tenant_code": code,
            "tenant_id": tenant_id,
            "status": "deleted"
        }),
        client_ip,
    )?;
    Ok(ApiResponse::ok(data).into_response())
}

/// 导出授权：常规导出属于业务权限；迁移模式（含加密后的私钥材料与密钥事件）仅超级管理员。
fn authorize_crypto_export(claims: &AdminClaims, mode: &str) -> Result<(), ApiError> {
    if !can_manage_tenants(claims) {
        return Err(ApiError::Forbidden);
    }
    if svc::is_migration_export(mode) && !is_admin(claims) {
        return Err(ApiError::Forbidden);
    }
    Ok(())
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
) -> Result<axum::response::Response, ApiError> {
    authorize_crypto_export(&claims, &query.mode)?;
    if svc::is_migration_export(&query.mode)
        && let Err(response) = require_category(&state, &claims, Category::Key).await
    {
        return Ok(*response);
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
    Ok(ApiResponse::ok(data).into_response())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(role: &str) -> AdminClaims {
        AdminClaims {
            iss: "test".into(),
            aud: None,
            sub: "tester".into(),
            user_id: uuid::Uuid::nil().to_string(),
            role: role.into(),
            iat: 0,
            exp: i64::MAX,
            jti: None,
            token_type: None,
            sid: uuid::Uuid::nil().to_string(),
            sxp: None,
            auth_epoch: 0,
        }
    }

    fn authorize_allow_insecure(
        claims: &AdminClaims,
        allow_insecure: bool,
    ) -> Result<(), ApiError> {
        if allow_insecure && !is_admin(claims) {
            return Err(ApiError::Forbidden);
        }
        Ok(())
    }

    #[test]
    fn allow_insecure_requires_super_admin() {
        assert!(authorize_allow_insecure(&claims("R_ADMIN"), true).is_ok());
        assert!(authorize_allow_insecure(&claims("R_ADMIN"), false).is_ok());
        assert!(authorize_allow_insecure(&claims("R_FINANCE"), false).is_ok());
        assert!(matches!(
            authorize_allow_insecure(&claims("R_FINANCE"), true),
            Err(ApiError::Forbidden)
        ));
    }

    fn crypto_write_actions(claims: &AdminClaims) -> (bool, bool) {
        (can_write_business(claims), is_admin(claims))
    }

    #[test]
    fn write_actions_are_biz_and_allow_insecure_is_super_admin() {
        let (biz_admin, super_admin) = crypto_write_actions(&claims("R_ADMIN"));
        assert!(biz_admin);
        assert!(super_admin);
        let (biz_finance, super_finance) = crypto_write_actions(&claims("R_FINANCE"));
        assert!(biz_finance);
        assert!(!super_finance);
        let (biz_viewer, super_viewer) = crypto_write_actions(&claims("R_VIEWER"));
        assert!(!biz_viewer);
        assert!(!super_viewer);
    }

    #[test]
    fn migration_export_is_super_admin_only() {
        assert!(authorize_crypto_export(&claims("R_ADMIN"), "migration").is_ok());
        assert!(authorize_crypto_export(&claims("R_ADMIN"), "regular").is_ok());
        assert!(authorize_crypto_export(&claims("R_FINANCE"), "regular").is_ok());
        assert!(matches!(
            authorize_crypto_export(&claims("R_FINANCE"), "migration"),
            Err(ApiError::Forbidden)
        ));
        for mode in ["regular", "migration"] {
            assert!(matches!(
                authorize_crypto_export(&claims("R_VIEWER"), mode),
                Err(ApiError::Forbidden)
            ));
        }
    }
}
