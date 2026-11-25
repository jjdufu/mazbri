use crate::audit::{AuditAction, AuditService, tenant_audit};
use crate::error::ApiError;
use crate::handlers::types::{CreateTenantRequest, ListParams, SetTenantEnabledRequest};
use crate::repositories::tenant as repo;
use sqlx::PgPool;
use tracing::info;
pub const MIN_TENANT_TOKEN_EXPIRATION_SECS: i32 = 60;
pub const MAX_TENANT_TOKEN_EXPIRATION_SECS: i32 = 86_400;
pub const DEFAULT_TENANT_TOKEN_EXPIRATION_SECS: i32 = 1_800;

fn validate_token_expiration_secs(secs: i32) -> Result<i32, ApiError> {
    if !(MIN_TENANT_TOKEN_EXPIRATION_SECS..=MAX_TENANT_TOKEN_EXPIRATION_SECS).contains(&secs) {
        return Err(ApiError::validation(format!(
            "token_expiration_secs must be between {MIN_TENANT_TOKEN_EXPIRATION_SECS} and {MAX_TENANT_TOKEN_EXPIRATION_SECS} seconds"
        )));
    }
    Ok(secs)
}

async fn notify_tenant_auth_invalidation(pool: &PgPool) {
    if let Err(error) = mazbri_core::config_watch::bump_config_version_and_notify(
        pool,
        mazbri_core::config_watch::AUTH_API_KEY_VERSION_KEY,
    )
    .await
    {
        tracing::warn!(
            error = %error,
            "Failed to bump auth_api_key_version after tenant mutation"
        );
    }
}

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
        if let Some(ref name) = self.name
            && name.len() > 255
        {
            return Err(ApiError::validation(
                "name must not exceed 255 characters".to_string(),
            ));
        }
        if let Some(ref email) = self.contact_email
            && !email.contains('@')
        {
            return Err(ApiError::validation(
                "contact_email must be a valid email address".to_string(),
            ));
        }
        if let Some(secs) = self.token_expiration_secs {
            validate_token_expiration_secs(secs)?;
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
        .map(mazbri_core::config::contains_ilike_pattern);
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
    let token_expiration_secs = match payload.token_expiration_secs {
        Some(secs) => Some(validate_token_expiration_secs(secs)?),
        None => Some(DEFAULT_TENANT_TOKEN_EXPIRATION_SECS),
    };
    let row = repo::insert_tenant(
        pool,
        repo::InsertTenantParams {
            tenant_code: &tenant_code,
            tenant_id: &payload.tenant_id,
            name: &payload.name,
            contact_person: payload.contact_person.as_deref(),
            contact_phone: payload.contact_phone.as_deref(),
            contact_email: payload.contact_email.as_deref(),
            token_expiration_secs,
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
    tenant_json(&row)
}
/// 租户行统一序列化为响应 JSON（含 `uuid` = tenants.id）。
fn tenant_json<T: serde::Serialize>(row: &T) -> Result<serde_json::Value, ApiError> {
    serde_json::to_value(row)
        .map_err(|e| ApiError::internal(format!("Failed to serialize tenant: {e}")))
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
    tenant_json(&row)
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
    notify_tenant_auth_invalidation(pool).await;
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
    tenant_json(&row)
}

pub struct DeleteTenantParams<'a> {
    pub pool: &'a PgPool,
    pub auth_db: &'a PgPool,
    pub ratelimit_db: &'a PgPool,
    pub cache_db: &'a PgPool,
    pub audit: &'a AuditService,
    pub id_or_code: &'a str,
    pub user_id: uuid::Uuid,
    pub username: &'a str,
    pub client_ip: Option<String>,
}

/// Hard-delete tenant (`R_ADMIN` only at handler).
///
/// Order of operations (cross-DB consistency):
/// 1. Soft-disable (`enabled = false`) in platform DB + bump `AUTH_API_KEY_VERSION_KEY` to cut traffic immediately.
/// 2. Idempotently clean `auth_db`, `ratelimit_db`, and `cache_db` rows for this `tenant_id`.
/// 3. Delete the platform `tenants` row (CASCADE child tables).
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_tenant(params: DeleteTenantParams<'_>) -> Result<serde_json::Value, ApiError> {
    let DeleteTenantParams {
        pool,
        auth_db,
        ratelimit_db,
        cache_db,
        audit,
        id_or_code,
        user_id,
        username,
        client_ip,
    } = params;
    info!("Hard-deleting tenant: {} (by {})", id_or_code, username);
    let tenant_id = resolve_tenant_id(pool, id_or_code).await?;
    let row = repo::get_identity_row(pool, tenant_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load tenant: {e}")))?
        .ok_or_else(|| ApiError::not_found("tenant".to_string(), tenant_id.to_string()))?;

    // Step 1: Soft-disable tenant + notify gateway/auth immediately so in-flight requests fail closed.
    let _ = repo::set_enabled(pool, tenant_id, false)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to soft-disable tenant before delete: {e}")))?;
    notify_tenant_auth_invalidation(pool).await;

    // Step 2a: Auth DB credentials (api_keys CASCADE on credential_id).
    sqlx::query("DELETE FROM auth_credentials WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(auth_db)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete auth credentials: {e}")))?;

    // Step 2b: Ratelimit DB commands & policies (idempotent).
    sqlx::query("DELETE FROM rate_limit_control_commands WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(ratelimit_db)
        .await
        .map_err(|e| {
            ApiError::internal(format!(
                "Failed to delete tenant rate-limit control commands: {e}"
            ))
        })?;
    sqlx::query("DELETE FROM rate_limit_tenant_policies WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(ratelimit_db)
        .await
        .map_err(|e| {
            ApiError::internal(format!("Failed to delete tenant rate-limit policies: {e}"))
        })?;

    // Step 2c: Cache DB tenant cache configs (idempotent).
    sqlx::query("DELETE FROM cache_configs WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(cache_db)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete tenant cache configs: {e}")))?;

    // Step 3: Physical delete on platform DB.
    let affected = repo::delete_by_id(pool, tenant_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete tenant: {e}")))?;
    if affected == 0 {
        return Err(ApiError::not_found(
            "tenant".to_string(),
            tenant_id.to_string(),
        ));
    }
    notify_tenant_auth_invalidation(pool).await;

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
    notify_tenant_auth_invalidation(pool).await;
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
        "uuid": row.id,
        "tenant_code": row.tenant_code,
        "name": row.name,
        "enabled": row.enabled,
        "reason": payload.reason,
    }))
}
