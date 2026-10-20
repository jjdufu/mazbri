use crate::audit::{AuditAction, AuditLogRequest, AuditResourceType, AuditService};
use crate::error::ApiError;
use crate::handlers::types::ListParams;
use crate::repositories::supplier as repo;
use mazbri_core::{SupplierConfig, mask_config_credentials, merge_auth_credentials};
use sqlx::PgPool;
use tracing::info;

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSupplierRequest {
    pub supplier_code: String,
    pub name: String,
    pub upstream_url: String,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateSupplierRequest {
    #[serde(default)]
    pub supplier_code: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub upstream_url: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

impl CreateSupplierRequest {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_supplier_code(&self.supplier_code)?;
        if self.name.is_empty() || self.name.len() > 255 {
            return Err(ApiError::validation(
                "供应商名称长度必须在 1 到 255 个字符之间".to_string(),
            ));
        }
        validate_upstream_url(&self.upstream_url)?;
        Ok(())
    }

    /// 解析并强校验 config；失败不写库。
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn parse_config(&self) -> Result<SupplierConfig, ApiError> {
        let raw = self.config.clone().unwrap_or_else(|| serde_json::json!({}));
        parse_and_validate_supplier_config(&raw, &self.upstream_url)
    }
}

impl UpdateSupplierRequest {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), ApiError> {
        if let Some(ref supplier_code) = self.supplier_code {
            validate_supplier_code(supplier_code)?;
        }
        if let Some(ref name) = self.name {
            if name.is_empty() || name.len() > 255 {
                return Err(ApiError::validation(
                    "供应商名称长度必须在 1 到 255 个字符之间".to_string(),
                ));
            }
        }
        if let Some(ref upstream_url) = self.upstream_url {
            validate_upstream_url(upstream_url)?;
        }
        if let Some(ref status) = self.status {
            let valid_statuses = ["active", "inactive", "suspended"];
            if !valid_statuses.contains(&status.as_str()) {
                return Err(ApiError::validation("状态值无效".to_string()));
            }
        }
        Ok(())
    }
}

/// 管理端写入路径：反序列化 → validate → transparent scheme 校验。
fn parse_and_validate_supplier_config(
    config: &serde_json::Value,
    upstream_url: &str,
) -> Result<SupplierConfig, ApiError> {
    // 剥离管理端内部字段（disguise 等已废弃）
    let mut cleaned = config.clone();
    if let Some(obj) = cleaned.as_object_mut() {
        obj.remove("disguise");
    }

    let mut cfg = SupplierConfig::parse_and_validate(&cleaned)
        .map_err(|e| ApiError::validation(e.to_string()))?;
    cfg = cfg.normalize_disabled_sections();
    cfg.validate_upstream_url_for_type(upstream_url)
        .map_err(|e| ApiError::validation(e.to_string()))?;
    // G-05：落库前加密 credentials
    cfg = cfg
        .encrypt_credentials_for_storage()
        .map_err(|e| ApiError::internal(format!("加密供应商凭据失败: {e}")))?;
    Ok(cfg)
}

fn validate_supplier_code(supplier_code: &str) -> Result<(), ApiError> {
    if supplier_code.is_empty() || supplier_code.len() > 100 {
        return Err(ApiError::validation(
            "供应商编码长度必须在 1 到 100 个字符之间".to_string(),
        ));
    }
    if !supplier_code
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ApiError::validation(
            "供应商编码只能包含字母、数字、下划线和连字符".to_string(),
        ));
    }
    Ok(())
}

fn validate_upstream_url(upstream_url: &str) -> Result<(), ApiError> {
    let trimmed = upstream_url.trim();
    if trimmed.is_empty() || trimmed.len() > 500 {
        return Err(ApiError::validation(
            "供应商接入地址长度必须在 1 到 500 个字符之间".to_string(),
        ));
    }
    Ok(())
}

const MAX_PAGE_SIZE: u32 = 100;

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_suppliers(
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
        &crate::list_query::suppliers_catalog(),
    )
    .map_err(|e| ApiError::validation(e.message))?;
    let total = repo::count_suppliers(pool, params.status.as_deref(), search_ref, &filters)
        .await
        .map_err(|e| ApiError::internal(format!("统计供应商失败: {e}")))?;
    let suppliers = repo::list_suppliers(
        pool,
        params.status.as_deref(),
        search_ref,
        &filters,
        i64::from(page_size),
        i64::from(offset),
    )
    .await
    .map_err(|e| ApiError::internal(format!("获取供应商列表失败: {e}")))?;

    let total_pages = mazbri_core::num::f64_to_u64((mazbri_core::num::i64_to_f64(total) / f64::from(page_size)).ceil());

    let masked_suppliers: Vec<serde_json::Value> = suppliers
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "supplier_code": row.supplier_code,
                "name": row.name,
                "upstream_url": row.upstream_url,
                "status": row.status,
                "config": mask_config_credentials(row.config),
                "created_at": row.created_at,
                "updated_at": row.updated_at,
            })
        })
        .collect();

    Ok(serde_json::json!(
        {
            "suppliers": masked_suppliers,
            "pagination": {
                "page": params.page,
                "page_size": page_size,
                "total": total,
                "total_pages": total_pages
            }
        }
    ))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_supplier(
    pool: &PgPool,
    audit: &AuditService,
    payload: &CreateSupplierRequest,
    user_id: uuid::Uuid,
    username: &str,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    info!(
        "Creating supplier: {} ({}) (by {})",
        payload.name, payload.supplier_code, username
    );
    payload.validate()?;

    let parsed = payload.parse_config()?;
    let config_val = parsed
        .to_json_value()
        .map_err(|e| ApiError::validation(e.to_string()))?;

    let row = repo::insert_supplier(
        pool,
        &payload.supplier_code,
        &payload.name,
        &payload.upstream_url,
        config_val,
    )
    .await
    .map_err(|e| ApiError::internal(format!("创建供应商失败: {e}")))?;

    let mut new_value = serde_json::to_value(&row).unwrap_or_default();
    if let Some(cfg) = new_value.get("config").cloned() {
        new_value["config"] = mask_config_credentials(cfg);
    }
    audit.log(change_audit_log(
        user_id,
        AuditAction::SupplierCreate,
        AuditResourceType::Supplier,
        row.id.to_string(),
        None,
        Some(new_value),
        client_ip,
    ));
    info!("Created supplier: {} ({})", row.name, row.id);

    Ok(serde_json::json!(
        {
            "id": row.id,
            "supplier_code": row.supplier_code,
            "name": row.name,
            "upstream_url": row.upstream_url,
            "status": row.status,
            "config": mask_config_credentials(row.config),
            "created_at": row.created_at,
            "updated_at": row.updated_at,
        }
    ))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_supplier(pool: &PgPool, id: &str) -> Result<serde_json::Value, ApiError> {
    let supplier_id = uuid::Uuid::parse_str(id)
        .map_err(|e| ApiError::validation(format!("供应商 ID 格式无效: {e}")))?;
    let row = repo::find_by_id(pool, supplier_id)
        .await
        .map_err(|e| ApiError::internal(format!("获取供应商失败: {e}")))?
        .ok_or_else(|| ApiError::not_found("supplier".to_string(), supplier_id.to_string()))?;

    Ok(serde_json::json!(
        {
            "id": row.id,
            "supplier_code": row.supplier_code,
            "name": row.name,
            "upstream_url": row.upstream_url,
            "status": row.status,
            "config": mask_config_credentials(row.config),
            "created_at": row.created_at,
            "updated_at": row.updated_at,
        }
    ))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_supplier(
    pool: &PgPool,
    audit: &AuditService,
    id: &str,
    payload: &UpdateSupplierRequest,
    user_id: uuid::Uuid,
    username: &str,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    info!("Updating supplier: {} (by {})", id, username);
    let supplier_id = uuid::Uuid::parse_str(id)
        .map_err(|e| ApiError::validation(format!("供应商 ID 格式无效: {e}")))?;
    payload.validate()?;

    let old = repo::find_by_id(pool, supplier_id)
        .await
        .map_err(|e| ApiError::internal(format!("获取供应商失败: {e}")))?
        .ok_or_else(|| ApiError::not_found("supplier".to_string(), supplier_id.to_string()))?;

    let effective_upstream_url = payload
        .upstream_url
        .as_deref()
        .unwrap_or(old.upstream_url.as_str());

    let mut updated_config = None;
    if let Some(new_cfg) = payload.config.clone() {
        let mut merged = new_cfg;
        merge_auth_credentials(&mut merged, &old.config);
        let parsed = parse_and_validate_supplier_config(&merged, effective_upstream_url)?;
        let config_val = parsed
            .to_json_value()
            .map_err(|e| ApiError::validation(e.to_string()))?;
        updated_config = Some(config_val);
    } else if payload.upstream_url.is_some() {
        // 仅改 URL：存量可能含已裁剪字段，用 lenient 解析
        let existing = SupplierConfig::parse_and_validate_lenient(&old.config).map_err(|e| {
            ApiError::validation(format!("现有 config 不合法，请先修复 config: {e}"))
        })?;
        existing
            .validate_upstream_url_for_type(effective_upstream_url)
            .map_err(|e| ApiError::validation(e.to_string()))?;
    }

    let row = repo::update_supplier(
        pool,
        repo::UpdateSupplierParams {
            id: supplier_id,
            supplier_code: payload.supplier_code.as_deref(),
            name: payload.name.as_deref(),
            upstream_url: payload.upstream_url.as_deref(),
            status: payload.status.as_deref(),
            config: updated_config,
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("更新供应商失败: {e}")))?
    .ok_or_else(|| ApiError::not_found("supplier".to_string(), supplier_id.to_string()))?;

    let mut old_value = serde_json::to_value(old).unwrap_or_default();
    let mut new_value = serde_json::to_value(&row).unwrap_or_default();
    if let Some(cfg) = old_value.get("config").cloned() {
        old_value["config"] = mask_config_credentials(cfg);
    }
    if let Some(cfg) = new_value.get("config").cloned() {
        new_value["config"] = mask_config_credentials(cfg);
    }
    audit.log(change_audit_log(
        user_id,
        AuditAction::SupplierUpdate,
        AuditResourceType::Supplier,
        row.id.to_string(),
        Some(old_value),
        Some(new_value),
        client_ip,
    ));

    Ok(serde_json::json!(
        {
            "id": row.id,
            "supplier_code": row.supplier_code,
            "name": row.name,
            "upstream_url": row.upstream_url,
            "status": row.status,
            "config": mask_config_credentials(row.config),
            "created_at": row.created_at,
            "updated_at": row.updated_at,
        }
    ))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_supplier(
    pool: &PgPool,
    audit: &AuditService,
    id: &str,
    user_id: uuid::Uuid,
    username: &str,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    info!("Deleting supplier: {} (by {})", id, username);
    let supplier_id = uuid::Uuid::parse_str(id)
        .map_err(|e| ApiError::validation(format!("供应商 ID 格式无效: {e}")))?;
    let row = repo::find_by_id(pool, supplier_id)
        .await
        .map_err(|e| ApiError::internal(format!("获取供应商失败: {e}")))?
        .ok_or_else(|| ApiError::not_found("supplier".to_string(), supplier_id.to_string()))?;

    let channel_count = repo::count_channels_for_supplier(pool, supplier_id)
        .await
        .map_err(|e| ApiError::internal(format!("检查供应商通道失败: {e}")))?;
    if channel_count > 0 {
        return Err(ApiError::validation(
            "供应商下存在通道，请先停用或迁移后再删除".to_string(),
        ));
    }

    repo::delete_supplier(pool, supplier_id)
        .await
        .map_err(|e| ApiError::internal(format!("删除供应商失败: {e}")))?;
    info!("Deleted supplier: {} ({})", row.name, id);

    let old_value = serde_json::to_value(&row).unwrap_or_default();
    audit.log(change_audit_log(
        user_id,
        AuditAction::SupplierDelete,
        AuditResourceType::Supplier,
        supplier_id.to_string(),
        Some(old_value),
        None,
        client_ip,
    ));

    Ok(serde_json::json!(
        {
            "message": "供应商删除成功",
            "id": id,
            "supplier_code": row.supplier_code,
            "name": row.name,
            "upstream_url": row.upstream_url,
            "status": row.status,
        }
    ))
}

fn change_audit_log(
    user_id: uuid::Uuid,
    action: AuditAction,
    resource_type: AuditResourceType,
    resource_id: String,
    old_value: Option<serde_json::Value>,
    new_value: Option<serde_json::Value>,
    ip_address: Option<String>,
) -> AuditLogRequest {
    AuditLogRequest {
        user_id,
        action,
        resource_type,
        resource_id: Some(resource_id),
        details: Some(serde_json::json!(
            {
                "change_summary": {
                    "has_old_value": old_value.is_some(),
                    "has_new_value": new_value.is_some(),
                }
            }
        )),
        ip_address,
        old_value,
        new_value,
        ..Default::default()
    }
}
