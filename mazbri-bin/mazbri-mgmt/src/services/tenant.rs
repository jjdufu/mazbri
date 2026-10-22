use crate::audit::{AuditAction, AuditService, tenant_audit};
use crate::error::ApiError;
use crate::handlers::types::{CreateTenantRequest, ListParams, SetTenantEnabledRequest};
use crate::repositories::tenant as repo;
use sqlx::PgPool;
use tracing::info;
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateTenantRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub contact_person: Option<String>,
    #[serde(default)]
    pub contact_phone: Option<String>,
    #[serde(default)]
    pub contact_email: Option<String>,
    #[serde(default)]
    pub token_expiration_secs: Option<i32>,
}
impl UpdateTenantRequest {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), ApiError> {
        if let Some(ref name) = self.name {
            if name.len() > 255 {
                return Err(ApiError::validation(
                    "name must not exceed 255 characters".to_string(),
                ));
            }
        }
        if let Some(ref email) = self.contact_email {
            if !email.contains('@') {
                return Err(ApiError::validation(
                    "contact_email must be a valid email address".to_string(),
                ));
            }
        }
        if let Some(secs) = self.token_expiration_secs {
            if secs < 60 {
                return Err(ApiError::validation(
                    "token_expiration_secs must be at least 60 seconds".to_string(),
                ));
            }
            if secs > 31_536_000 {
                return Err(ApiError::validation(
                    "token_expiration_secs must not exceed 31536000 seconds (1 year)".to_string(),
                ));
            }
        }
        Ok(())
    }
}
const MAX_PAGE_SIZE: u32 = 100;
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_tenants(
    pool: &PgPool,
    params: &ListParams,
) -> Result<serde_json::Value, ApiError> {
    let page_size = params.page_size.min(MAX_PAGE_SIZE);
    let offset = (params.page.saturating_sub(1)) * page_size;
    let search_pattern = params
        .q
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));
    let search_ref = search_pattern.as_deref();
    let filters = crate::list_query::parse_and_validate_filters(
        params.filters.as_deref(),
        &crate::list_query::tenants_catalog(),
    )
    .map_err(|e| ApiError::validation(e.message))?;
    if params.status.is_some() {
        return Err(ApiError::validation(
            "tenants list: use filters field `enabled`".to_string(),
        ));
    }
    let total = repo::count_tenants(pool, None, search_ref, &filters)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to count tenants: {e}")))?;
    let tenants = repo::list_tenants(
        pool,
        None,
        search_ref,
        &filters,
        i64::from(page_size),
        i64::from(offset),
    )
    .await
    .map_err(|e| ApiError::internal(format!("Failed to list tenants: {e}")))?;
    let total_pages = mazbri_core::num::f64_to_u64(
        (mazbri_core::num::i64_to_f64(total) / f64::from(page_size)).ceil(),
    );
    Ok(serde_json::json!(
        { "tenants" : tenants, "pagination" : { "page" : params.page, "page_size" :
        page_size, "total" : total, "total_pages" : total_pages, } }
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_tenant(
    pool: &PgPool,
    audit: &AuditService,
    payload: &CreateTenantRequest,
    user_id: uuid::Uuid,
    username: &str,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    info!(
        "Creating tenant: [{}] {} ({}) (by {})",
        payload.tenant_code, payload.name, payload.tenant_id, username
    );
    let tenant_code = payload.tenant_code.to_uppercase();
    if tenant_code.len() != 8
        || !tenant_code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err(ApiError::validation(
            "Tenant code must be 8 characters, uppercase letters and numbers only".to_string(),
        ));
    }
    let existing_code = repo::count_by_tenant_code(pool, &tenant_code)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check tenant_code: {e}")))?;
    if existing_code > 0 {
        return Err(ApiError::validation(format!(
            "Tenant code '{tenant_code}' already exists"
        )));
    }
    let existing_tenant_id = repo::count_by_tenant_id(pool, &payload.tenant_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check tenant_id: {e}")))?;
    if existing_tenant_id > 0 {
        return Err(ApiError::validation(format!(
            "Tenant ID '{}' already exists",
            payload.tenant_id
        )));
    }
    let row = repo::insert_tenant(
        pool,
        repo::InsertTenantParams {
            tenant_code: &tenant_code,
            tenant_id: &payload.tenant_id,
            name: &payload.name,
            contact_person: payload.contact_person.as_deref(),
            contact_phone: payload.contact_phone.as_deref(),
            contact_email: payload.contact_email.as_deref(),
            token_expiration_secs: payload.token_expiration_secs,
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("Failed to create tenant: {e}")))?;
    audit.log(tenant_audit(
        user_id,
        AuditAction::TenantCreate,
        row.id,
        Some(serde_json::json!(
            { "tenant_code" : row.tenant_code, "tenant_id" : row.tenant_id,
            "name" : row.name, "contact_person" : row.contact_person,
            "contact_phone" : row.contact_phone, "contact_email" : row
            .contact_email, "token_expiration_secs" : row
            .token_expiration_secs, }
        )),
        client_ip,
    ));
    info!(
        "Created tenant: [{}] {} ({})",
        row.tenant_code, row.name, row.tenant_id
    );
    Ok(serde_json::json!(
        { "tenant_code" : row.tenant_code, "tenant_id" : row.tenant_id, "name" : row
        .name, "contact_person" : row.contact_person, "contact_phone" : row
        .contact_phone, "contact_email" : row.contact_email, "enabled" : row.enabled,
        "token_expiration_secs" : row.token_expiration_secs, "created_at" : row
        .created_at, }
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn resolve_tenant_id(pool: &PgPool, code_or_id: &str) -> Result<uuid::Uuid, ApiError> {
    if let Ok(id) = uuid::Uuid::parse_str(code_or_id) {
        return Ok(id);
    }
    repo::get_id_by_code(pool, code_or_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to resolve tenant code: {e}")))?
        .ok_or_else(|| ApiError::not_found("tenant".to_string(), code_or_id.to_string()))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_tenant(pool: &PgPool, id_or_code: &str) -> Result<serde_json::Value, ApiError> {
    let tenant_id = resolve_tenant_id(pool, id_or_code).await?;
    let row = repo::find_by_id(pool, tenant_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get tenant: {e}")))?
        .ok_or_else(|| ApiError::not_found("tenant".to_string(), tenant_id.to_string()))?;
    Ok(serde_json::json!(
        { "tenant_code" : row.tenant_code, "tenant_id" : row.tenant_id, "name" : row
        .name, "contact_person" : row.contact_person, "contact_phone" : row
        .contact_phone, "contact_email" : row.contact_email, "enabled" : row.enabled,
        "token_expiration_secs" : row.token_expiration_secs, "created_at" : row
        .created_at, "updated_at" : row.updated_at, }
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_tenant(
    pool: &PgPool,
    audit: &AuditService,
    id_or_code: &str,
    payload: &UpdateTenantRequest,
    user_id: uuid::Uuid,
    username: &str,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    info!("Updating tenant: {} (by {})", id_or_code, username);
    let tenant_id = resolve_tenant_id(pool, id_or_code).await?;
    payload.validate()?;
    let has_updates = payload.name.is_some()
        || payload.contact_person.is_some()
        || payload.contact_phone.is_some()
        || payload.contact_email.is_some()
        || payload.token_expiration_secs.is_some();
    if !has_updates {
        return Err(ApiError::validation("No fields to update".to_string()));
    }
    let row = repo::update_tenant(
        pool,
        tenant_id,
        payload.name.as_deref(),
        payload.contact_person.as_deref(),
        payload.contact_phone.as_deref(),
        payload.contact_email.as_deref(),
        payload.token_expiration_secs,
    )
    .await
    .map_err(|e| ApiError::internal(format!("Failed to update tenant: {e}")))?
    .ok_or_else(|| ApiError::not_found("tenant".to_string(), tenant_id.to_string()))?;
    audit.log(tenant_audit(
        user_id,
        AuditAction::TenantUpdate,
        row.id,
        Some(serde_json::json!(
            { "name" : row.name, "contact_email" : row.contact_email,
            "token_expiration_secs" : row.token_expiration_secs, }
        )),
        client_ip,
    ));
    Ok(serde_json::json!(
        { "tenant_code" : row.tenant_code, "tenant_id" : row.tenant_id, "name" : row
        .name, "enabled" : row.enabled, "contact_email" : row.contact_email,
        "token_expiration_secs" : row.token_expiration_secs, "updated_at" : row
        .updated_at, }
    ))
}
/// Hard-delete tenant (`R_ADMIN` only at handler). Cleans platform row (CASCADE children)
/// and auth credentials for the same tenant UUID.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_tenant(
    pool: &PgPool,
    auth_db: &PgPool,
    audit: &AuditService,
    id_or_code: &str,
    user_id: uuid::Uuid,
    username: &str,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    info!("Hard-deleting tenant: {} (by {})", id_or_code, username);
    let tenant_id = resolve_tenant_id(pool, id_or_code).await?;
    let row = repo::get_identity_row(pool, tenant_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load tenant: {e}")))?
        .ok_or_else(|| ApiError::not_found("tenant".to_string(), tenant_id.to_string()))?;

    // Auth DB: credentials (api_keys CASCADE on credential_id)
    sqlx::query("DELETE FROM auth_credentials WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(auth_db)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete auth credentials: {e}")))?;

    let affected = repo::delete_by_id(pool, tenant_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete tenant: {e}")))?;
    if affected == 0 {
        return Err(ApiError::not_found(
            "tenant".to_string(),
            tenant_id.to_string(),
        ));
    }

    info!(
        "Deleted tenant: [{}] {} ({})",
        row.tenant_code, row.name, id_or_code
    );
    audit.log(tenant_audit(
        user_id,
        AuditAction::TenantDelete,
        tenant_id,
        Some(serde_json::json!({
            "tenant_code": row.tenant_code,
            "name": row.name,
            "prior_enabled": row.enabled,
            "hard_delete": true,
        })),
        client_ip,
    ));
    Ok(serde_json::json!({
        "message": "Tenant deleted successfully",
        "tenant_code": row.tenant_code,
        "name": row.name,
    }))
}

/// 启用 / 停用租户。
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn set_tenant_enabled(
    pool: &PgPool,
    audit: &AuditService,
    id_or_code: &str,
    payload: &SetTenantEnabledRequest,
    user_id: uuid::Uuid,
    username: &str,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    info!(
        "Setting tenant {} enabled={} (by {})",
        id_or_code, payload.enabled, username
    );
    let tenant_id = resolve_tenant_id(pool, id_or_code).await?;
    let row = repo::set_enabled(pool, tenant_id, payload.enabled)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update tenant enabled: {e}")))?
        .ok_or_else(|| ApiError::not_found("tenant".to_string(), tenant_id.to_string()))?;
    info!(
        "Tenant [{}] {} enabled={}",
        row.tenant_code, row.name, row.enabled
    );
    let audit_action = if row.enabled {
        AuditAction::TenantActivate
    } else {
        AuditAction::TenantSuspend
    };
    audit.log(tenant_audit(
        user_id,
        audit_action,
        tenant_id,
        Some(serde_json::json!({
            "tenant_code": row.tenant_code,
            "name": row.name,
            "reason": payload.reason,
            "enabled": row.enabled,
        })),
        client_ip,
    ));
    Ok(serde_json::json!({
        "tenant_code": row.tenant_code,
        "name": row.name,
        "enabled": row.enabled,
        "reason": payload.reason,
    }))
}
