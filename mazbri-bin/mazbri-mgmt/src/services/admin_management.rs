mod helpers;
use crate::audit::{AuditAction, AuditLogRequest, AuditResourceType, AuditService};
use crate::error::ApiError;
use crate::handlers::types::{
    AdminListParams, CreateAdminRequest, ForcePasswordResetRequest, ResetPasswordRequest,
    SetAdminStatusRequest, UpdateAdminRequest,
};
use crate::middleware::auth::{AdminClaims, can_manage_admins};
use crate::repositories::admin_management as repo;
use crate::services::PasswordPolicyCache;
use sqlx::PgPool;
use tracing::{info, warn};
fn admin_row_to_json(row: &repo::AdminRow) -> serde_json::Value {
    serde_json::json!(
        { "id" : row.id, "username" : row.username, "display_name" : row.display_name,
        "email" : row.email, "role" : row.role, "is_active" : row.is_active,
        "password_changed_at" : row.password_changed_at, "force_password_reset" : row
        .force_password_reset, "mfa_enabled" : row.mfa.mfa_enabled, "mfa_required" : row.mfa.mfa_required, "created_at" : row.created_at, "updated_at" : row.updated_at,
        "last_login_at" : row.last_login_at, }
    )
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_admins(
    pool: &PgPool,
    claims: &AdminClaims,
    params: &AdminListParams,
) -> Result<serde_json::Value, ApiError> {
    if !can_manage_admins(claims) {
        return Err(ApiError::Forbidden);
    }
    let page = params.page.max(1);
    let page_size = params.page_size.clamp(1, helpers::MAX_PAGE_SIZE);
    let offset = (page - 1) * page_size;
    let search_pattern = params
        .q
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));
    let search_ref = search_pattern.as_deref();
    let filters = crate::list_query::parse_and_validate_filters(
        params.filters.as_deref(),
        &crate::list_query::admins_catalog(),
    )
    .map_err(|e| ApiError::validation(e.message))?;
    let total = repo::count_admins(
        pool,
        params.role.as_deref(),
        params.status.as_deref(),
        search_ref,
        &filters,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to count admins: {}", e);
        ApiError::internal("统计管理员数量失败".to_string())
    })?;
    let rows = repo::list_admins(
        pool,
        params.role.as_deref(),
        params.status.as_deref(),
        search_ref,
        &filters,
        i64::from(page_size),
        i64::from(offset),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list admins: {}", e);
        ApiError::internal("获取管理员列表失败".to_string())
    })?;
    let items: Vec<serde_json::Value> = rows.iter().map(admin_row_to_json).collect();
    let total_pages = mazbri_core::num::f64_to_u64(
        (mazbri_core::num::i64_to_f64(total) / f64::from(page_size)).ceil(),
    );
    Ok(serde_json::json!(
        { "items" : items, "total" : total, "page" : page, "page_size" : page_size,
        "total_pages" : total_pages, }
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_admin(
    pool: &PgPool,
    audit: &AuditService,
    password_policy_cache: &PasswordPolicyCache,
    claims: &AdminClaims,
    req: &CreateAdminRequest,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    helpers::validate_manage_admins(claims)?;
    helpers::validate_assignable_role(claims, &req.role)?;
    helpers::validate_password_strength(password_policy_cache, &req.password).await?;
    if req.username.is_empty() || req.username.len() > 100 {
        return Err(ApiError::validation(
            "用户名必须是 1-100 个字符".to_string(),
        ));
    }
    let exists = repo::username_exists(pool, &req.username)
        .await
        .unwrap_or(true);
    if exists {
        info!("Username already exists: {}", req.username);
        return Err(ApiError::validation("用户名已存在".to_string()));
    }
    let password_hash = helpers::hash_password(&req.password).map_err(|e| {
        tracing::error!("Failed to hash password: {}", e);
        ApiError::internal("处理密码失败".to_string())
    })?;
    let row = repo::insert_admin(
        pool,
        repo::InsertAdminParams {
            username: &req.username,
            display_name: &req.display_name,
            email: req.email.as_deref(),
            password_hash: &password_hash,
            role: &req.role,
            force_password_reset: req.force_password_reset,
            mfa_required: req.mfa_required,
        },
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to create admin: {}", e);
        ApiError::internal("创建管理员失败".to_string())
    })?;
    let policy = password_policy_cache.get().await;
    if let Err(e) = policy
        .record_password_history(pool, row.id, &password_hash)
        .await
    {
        warn!("Failed to record password history: {}", e);
    }
    info!(
        "Admin created: {} (role: {}) by {}",
        row.username, row.role, claims.sub
    );
    let audit_user_id = helpers::extract_audit_user_id(claims)?;
    audit.log(AuditLogRequest {
        user_id: Some(audit_user_id),
        action: AuditAction::AdminCreate,
        resource_type: AuditResourceType::Admin,
        resource_id: Some(row.id.to_string()),
        details: Some(serde_json::json!({
            "username": row.username,
            "role": row.role,
            "mfa_required": row.mfa.mfa_required,
        })),
        ip_address: client_ip,
        ..Default::default()
    });
    Ok(admin_row_to_json(&row))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_admin(
    pool: &PgPool,
    claims: &AdminClaims,
    id: uuid::Uuid,
) -> Result<serde_json::Value, ApiError> {
    if !can_manage_admins(claims) {
        return Err(ApiError::Forbidden);
    }
    let row = repo::find_by_id(pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get admin: {}", e);
            ApiError::internal("获取管理员失败".to_string())
        })?
        .ok_or_else(|| ApiError::not_found("admin".to_string(), id.to_string()))?;
    Ok(admin_row_to_json(&row))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
/// 只判断这次保存是否要把 `mfa_required` 从 true 改为 false。校验失败时与更新接口相同。
///
/// # Errors
///
/// 无权限、目标不存在或目标是超级管理员时返回错误。
pub async fn admin_update_lifts_mfa_requirement(
    pool: &PgPool,
    claims: &AdminClaims,
    id: uuid::Uuid,
    req: &UpdateAdminRequest,
) -> Result<bool, ApiError> {
    if !can_manage_admins(claims) {
        return Err(ApiError::Forbidden);
    }
    let current = repo::find_edit_state(pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load admin edit state: {}", e);
            ApiError::internal("检查管理员失败".to_string())
        })?
        .ok_or_else(|| ApiError::not_found("admin".to_string(), id.to_string()))?;
    if req.role.as_ref().is_some_and(|role| role != &current.role)
        || req.force_password_reset.is_some()
        || req.mfa_required.is_some()
    {
        helpers::ensure_not_super_admin_target(&current.role)?;
    }
    if let Some(role) = req.role.as_deref()
        && role != current.role
    {
        helpers::validate_assignable_role(claims, role)?;
    }
    Ok(current.mfa_required && req.mfa_required == Some(false))
}

/// 这次编辑是否会递增 `auth_epoch`。未变化的标记和已经为 true 的强制改密不加。
#[must_use]
pub fn auth_epoch_should_bump(
    current_role: &str,
    current_mfa_required: bool,
    current_force_password_reset: bool,
    requested_role: Option<&str>,
    requested_mfa_required: Option<bool>,
    requested_force_password_reset: Option<bool>,
) -> bool {
    let role_changed = requested_role.is_some_and(|role| role != current_role);
    let mfa_required_changed =
        requested_mfa_required.is_some_and(|value| value != current_mfa_required);
    let force_reset_raised =
        requested_force_password_reset == Some(true) && !current_force_password_reset;
    role_changed || mfa_required_changed || force_reset_raised
}

/// # Errors
///
/// 无权限、校验失败或数据库更新失败时返回错误。
pub async fn update_admin(
    pool: &PgPool,
    audit: &AuditService,
    claims: &AdminClaims,
    id: uuid::Uuid,
    req: &UpdateAdminRequest,
    client_ip: Option<String>,
    fresh_for_lift: bool,
) -> Result<serde_json::Value, ApiError> {
    if !can_manage_admins(claims) {
        return Err(ApiError::Forbidden);
    }
    let current = repo::find_role_by_id(pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check admin: {}", e);
            ApiError::internal("检查管理员失败".to_string())
        })?
        .ok_or_else(|| ApiError::not_found("admin".to_string(), id.to_string()))?;
    // 超级管理员：只允许修改显示名 / 邮箱；角色与强制改密等安全属性不可经由账号管理修改。
    if req.role.as_ref().is_some_and(|role| role != &current.role)
        || req.force_password_reset.is_some()
        || req.mfa_required.is_some()
    {
        helpers::ensure_not_super_admin_target(&current.role)?;
    }
    if let Some(ref role) = req.role
        && role != &current.role
    {
        helpers::validate_assignable_role(claims, role)?;
    }
    let has_updates = req.display_name.is_some()
        || req.email.is_some()
        || req.role.is_some()
        || req.mfa_required.is_some()
        || req.force_password_reset.is_some();
    if !has_updates {
        return Err(ApiError::validation("没有要更新的字段".to_string()));
    }
    let current_flags = repo::find_edit_state(pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load admin edit state: {}", e);
            ApiError::internal("检查管理员失败".to_string())
        })?
        .ok_or_else(|| ApiError::not_found("admin".to_string(), id.to_string()))?;
    let bump_epoch = auth_epoch_should_bump(
        &current_flags.role,
        current_flags.mfa_required,
        current_flags.force_password_reset,
        req.role.as_deref(),
        req.mfa_required,
        req.force_password_reset,
    );
    let lifts_mfa_requirement = current_flags.mfa_required && req.mfa_required == Some(false);
    if lifts_mfa_requirement && !fresh_for_lift {
        return Err(ApiError::validation(
            "解除强制 MFA 需要重新验证".to_string(),
        ));
    }
    let row = repo::update_admin(
        pool,
        repo::AdminUpdate {
            id,
            display_name: req.display_name.as_deref(),
            email: req.email.as_deref(),
            role: req.role.as_deref(),
            mfa_required: req.mfa_required,
            force_password_reset: req.force_password_reset,
            bump_epoch,
        },
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to update admin: {}", e);
        ApiError::internal("更新管理员失败".to_string())
    })?;
    info!("Admin updated: {} by {}", current.username, claims.sub);
    let audit_user_id = helpers::extract_audit_user_id(claims)?;
    audit.log(AuditLogRequest {
        user_id: Some(audit_user_id),
        action: AuditAction::AdminUpdate,
        resource_type: AuditResourceType::Admin,
        resource_id: Some(id.to_string()),
        details: Some(serde_json::json!(
            { "username" : current.username, "auth_epoch_bumped" : bump_epoch, "updated_fields" : { "display_name"
            : req.display_name.is_some(), "email" : req.email.is_some(), "role" :
            req.role.is_some(), "mfa_required" : req.mfa_required.is_some(), "force_password_reset" : req.force_password_reset.is_some() } }
        )),
        ip_address: client_ip,
        ..Default::default()
    });
    Ok(admin_row_to_json(&row))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_admin(
    pool: &PgPool,
    audit: &AuditService,
    claims: &AdminClaims,
    id: uuid::Uuid,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    if !can_manage_admins(claims) {
        return Err(ApiError::Forbidden);
    }
    if claims.user_id == id.to_string() {
        return Err(ApiError::validation("不能删除自己的账户".to_string()));
    }
    let current = repo::find_role_by_id(pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check admin: {}", e);
            ApiError::internal("检查管理员失败".to_string())
        })?
        .ok_or_else(|| ApiError::not_found("admin".to_string(), id.to_string()))?;
    helpers::ensure_not_super_admin_target(&current.role)?;
    repo::delete_admin(pool, id).await.map_err(|e| {
        tracing::error!("Failed to delete admin: {}", e);
        ApiError::internal("删除管理员失败".to_string())
    })?;
    info!("Admin deleted: {} by {}", current.username, claims.sub);
    let audit_user_id = helpers::extract_audit_user_id(claims)?;
    audit.log(AuditLogRequest {
        user_id: Some(audit_user_id),
        action: AuditAction::AdminDelete,
        resource_type: AuditResourceType::Admin,
        resource_id: Some(id.to_string()),
        details: Some(serde_json::json!(
            { "username" : current.username, "role" : current.role, }
        )),
        ip_address: client_ip,
        ..Default::default()
    });
    Ok(serde_json::json!({ "message" : "管理员已删除" }))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
/// 重置密码的校验。通过后调用方再消费 fresh，失败不会消耗授权。
///
/// # Errors
///
/// 权限、复杂度、目标保护或历史密码校验失败时返回错误。
pub async fn precheck_password_reset(
    pool: &PgPool,
    password_policy_cache: &PasswordPolicyCache,
    claims: &AdminClaims,
    id: uuid::Uuid,
    req: &ResetPasswordRequest,
) -> Result<(), ApiError> {
    if !can_manage_admins(claims) {
        return Err(ApiError::Forbidden);
    }
    helpers::validate_password_strength(password_policy_cache, &req.new_password).await?;
    let admin = repo::find_brief_by_id(pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check admin: {}", e);
            ApiError::internal("检查管理员失败".to_string())
        })?
        .ok_or_else(|| ApiError::not_found("admin".to_string(), id.to_string()))?;
    helpers::ensure_not_super_admin_target(&admin.role)?;
    let password_hash = helpers::hash_password(&req.new_password).map_err(|e| {
        tracing::error!("Failed to hash password: {}", e);
        ApiError::internal("处理密码失败".to_string())
    })?;
    let policy = password_policy_cache.get().await;
    if let Some(reuse_error) = policy.check_password_reuse(pool, id, &password_hash).await {
        return Err(ApiError::validation(reuse_error));
    }
    Ok(())
}

/// 重置 MFA 前的目标校验。未启用时不算失败，调用方不应再要 fresh。
///
/// # Errors
///
/// 无权限、目标不存在或目标是超级管理员时返回错误。
pub async fn precheck_mfa_reset(
    pool: &PgPool,
    claims: &AdminClaims,
    id: uuid::Uuid,
) -> Result<(), ApiError> {
    if !can_manage_admins(claims) {
        return Err(ApiError::Forbidden);
    }
    let admin = repo::find_mfa_by_id(pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check admin: {}", e);
            ApiError::internal("检查管理员失败".to_string())
        })?
        .ok_or_else(|| ApiError::not_found("admin".to_string(), id.to_string()))?;
    helpers::ensure_not_super_admin_target(&admin.role)?;
    if !admin.mfa_enabled {
        return Err(ApiError::validation("该用户未启用 MFA".to_string()));
    }
    Ok(())
}

/// # Errors
///
/// 无权限、校验失败或数据库更新失败时返回错误。
pub async fn reset_password(
    pool: &PgPool,
    audit: &AuditService,
    password_policy_cache: &PasswordPolicyCache,
    claims: &AdminClaims,
    id: uuid::Uuid,
    req: &ResetPasswordRequest,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    if !can_manage_admins(claims) {
        return Err(ApiError::Forbidden);
    }
    helpers::validate_password_strength(password_policy_cache, &req.new_password).await?;
    let admin = repo::find_brief_by_id(pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check admin: {}", e);
            ApiError::internal("检查管理员失败".to_string())
        })?
        .ok_or_else(|| ApiError::not_found("admin".to_string(), id.to_string()))?;
    helpers::ensure_not_super_admin_target(&admin.role)?;
    let password_hash = helpers::hash_password(&req.new_password).map_err(|e| {
        tracing::error!("Failed to hash password: {}", e);
        ApiError::internal("处理密码失败".to_string())
    })?;
    let policy = password_policy_cache.get().await;
    if let Some(reuse_error) = policy.check_password_reuse(pool, id, &password_hash).await {
        return Err(ApiError::validation(reuse_error));
    }
    repo::update_password(pool, id, &password_hash)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update password: {}", e);
            ApiError::internal("更新密码失败".to_string())
        })?;
    if let Err(e) = policy
        .record_password_history(pool, id, &password_hash)
        .await
    {
        warn!("Failed to record password history: {}", e);
    }
    info!(
        "Password reset for admin: {} by {}",
        admin.username, claims.sub
    );
    let audit_user_id = helpers::extract_audit_user_id(claims)?;
    audit.log(AuditLogRequest {
        user_id: Some(audit_user_id),
        action: AuditAction::AdminPasswordReset,
        resource_type: AuditResourceType::Admin,
        resource_id: Some(id.to_string()),
        details: Some(serde_json::json!({
            "username": admin.username,
            "auth_epoch_bumped": true,
        })),
        ip_address: client_ip,
        ..Default::default()
    });
    Ok(serde_json::json!({ "message" : "密码已重置" }))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn force_password_reset(
    pool: &PgPool,
    audit: &AuditService,
    claims: &AdminClaims,
    id: uuid::Uuid,
    req: &ForcePasswordResetRequest,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    if !can_manage_admins(claims) {
        return Err(ApiError::Forbidden);
    }
    let admin = repo::find_edit_state(pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check admin: {}", e);
            ApiError::internal("检查管理员失败".to_string())
        })?
        .ok_or_else(|| ApiError::not_found("admin".to_string(), id.to_string()))?;
    helpers::ensure_not_super_admin_target(&admin.role)?;
    let bumped = auth_epoch_should_bump(
        &admin.role,
        admin.mfa_required,
        admin.force_password_reset,
        None,
        None,
        Some(req.force_password_reset),
    );
    repo::update_force_password_reset(pool, id, req.force_password_reset)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update force_password_reset: {}", e);
            ApiError::internal("更新强制重置密码标志失败".to_string())
        })?;
    info!(
        "Force password reset {} for admin: {} by {}",
        if req.force_password_reset {
            "enabled"
        } else {
            "disabled"
        },
        admin.username,
        claims.sub
    );
    let audit_user_id = helpers::extract_audit_user_id(claims)?;
    audit.log(AuditLogRequest {
        user_id: Some(audit_user_id),
        action: AuditAction::AdminForcePasswordReset,
        resource_type: AuditResourceType::Admin,
        resource_id: Some(id.to_string()),
        details: Some({
            let mut details = serde_json::json!({
                "username": admin.username,
                "force_password_reset": req.force_password_reset,
            });
            if bumped && let Some(obj) = details.as_object_mut() {
                obj.insert("auth_epoch_bumped".to_string(), serde_json::json!(true));
            }
            details
        }),
        ip_address: client_ip,
        ..Default::default()
    });
    Ok(serde_json::json!(
        { "message" : if req.force_password_reset { "已启用强制重置密码" } else {
        "已禁用强制重置密码" } }
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn set_status(
    pool: &PgPool,
    audit: &AuditService,
    claims: &AdminClaims,
    id: uuid::Uuid,
    req: &SetAdminStatusRequest,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    if !can_manage_admins(claims) {
        return Err(ApiError::Forbidden);
    }
    if claims.user_id == id.to_string() && !req.is_active {
        return Err(ApiError::validation("不能禁用自己的账户".to_string()));
    }
    let admin = repo::find_status_by_id(pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check admin: {}", e);
            ApiError::internal("检查管理员失败".to_string())
        })?
        .ok_or_else(|| ApiError::not_found("admin".to_string(), id.to_string()))?;
    helpers::ensure_not_super_admin_target(&admin.role)?;
    if admin.is_active == req.is_active {
        return Ok(serde_json::json!({ "message" : "状态未变化" }));
    }
    repo::update_status(pool, id, req.is_active)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update status: {}", e);
            ApiError::internal("更新状态失败".to_string())
        })?;
    let status_text = if req.is_active { "enabled" } else { "disabled" };
    info!(
        "Admin {}: {} by {}",
        status_text, admin.username, claims.sub
    );
    let audit_user_id = helpers::extract_audit_user_id(claims)?;
    audit.log(AuditLogRequest {
        user_id: Some(audit_user_id),
        action: AuditAction::AdminStatusChange,
        resource_type: AuditResourceType::Admin,
        resource_id: Some(id.to_string()),
        details: Some(serde_json::json!({
            "username": admin.username,
            "auth_epoch_bumped": true,
        })),
        ip_address: client_ip,
        ..Default::default()
    });
    Ok(serde_json::json!(
        { "message" : if req.is_active { "管理员已启用" } else {
        "管理员已禁用" } }
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn reset_mfa(
    pool: &PgPool,
    audit: &AuditService,
    claims: &AdminClaims,
    id: uuid::Uuid,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    if !can_manage_admins(claims) {
        return Err(ApiError::Forbidden);
    }
    let admin = repo::find_mfa_by_id(pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check admin: {}", e);
            ApiError::internal("检查管理员失败".to_string())
        })?
        .ok_or_else(|| ApiError::not_found("admin".to_string(), id.to_string()))?;
    helpers::ensure_not_super_admin_target(&admin.role)?;
    if !admin.mfa_enabled {
        return Ok(serde_json::json!({ "message" : "该用户未启用 MFA" }));
    }
    repo::reset_mfa(pool, id).await.map_err(|e| {
        tracing::error!("Failed to reset MFA: {}", e);
        ApiError::internal("重置 MFA 失败".to_string())
    })?;
    info!("MFA reset for admin: {} by {}", admin.username, claims.sub);
    let audit_user_id = helpers::extract_audit_user_id(claims)?;
    audit.log(AuditLogRequest {
        user_id: Some(audit_user_id),
        action: AuditAction::AdminMfaReset,
        resource_type: AuditResourceType::Admin,
        resource_id: Some(id.to_string()),
        details: Some(serde_json::json!({
            "username": admin.username,
            "action": "mfa_reset",
            "auth_epoch_bumped": true,
        })),
        ip_address: client_ip,
        ..Default::default()
    });
    Ok(serde_json::json!({ "message" : "MFA 已重置" }))
}

#[cfg(test)]
mod epoch_bump_tests {
    use super::auth_epoch_should_bump;

    #[test]
    fn unchanged_mfa_required_does_not_bump() {
        assert!(!auth_epoch_should_bump(
            "R_FINANCE",
            true,
            false,
            None,
            Some(true),
            None,
        ));
    }

    #[test]
    fn force_reset_already_true_does_not_bump() {
        assert!(!auth_epoch_should_bump(
            "R_FINANCE",
            false,
            true,
            None,
            None,
            Some(true),
        ));
    }

    #[test]
    fn email_only_does_not_bump() {
        assert!(!auth_epoch_should_bump(
            "R_FINANCE",
            false,
            false,
            None,
            None,
            None,
        ));
    }

    #[test]
    fn raising_force_reset_or_changing_role_bumps() {
        assert!(auth_epoch_should_bump(
            "R_FINANCE",
            false,
            false,
            None,
            None,
            Some(true),
        ));
        assert!(auth_epoch_should_bump(
            "R_FINANCE",
            false,
            false,
            Some("R_VIEWER"),
            None,
            None,
        ));
    }
}
