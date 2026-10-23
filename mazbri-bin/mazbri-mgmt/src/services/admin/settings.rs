use crate::error::ApiError;
use crate::handlers::types::{SystemSettingsUpdate, UserSettingsUpdate};
use crate::repositories::admin as repo;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::info;
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_system_settings(db: &PgPool) -> Result<serde_json::Value, ApiError> {
    let mut settings = repo::get_system_settings(db)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch settings: {e}")))?;

    let allowed_keys = [
        "site_name",
        "site_url",
        "tenant_gateway_url",
        "enable_audit_log",
        "session_timeout",
        "max_upload_size",
    ];
    settings.retain(|key, _| allowed_keys.contains(&key.as_str()));

    serde_json::to_value(settings)
        .map_err(|e| ApiError::internal(format!("Failed to serialize settings: {e}")))
}

/// 规范化租户网关 URL：去空白、去尾 `/`，强制 http(s)。
fn normalize_tenant_gateway_url(raw: &str) -> Result<String, ApiError> {
    let url = raw.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return Err(ApiError::validation(
            "tenant_gateway_url 不能为空".to_string(),
        ));
    }
    if url.len() > 500 {
        return Err(ApiError::validation(
            "tenant_gateway_url 长度不得超过 500".to_string(),
        ));
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(ApiError::validation(
            "tenant_gateway_url 必须以 http:// 或 https:// 开头".to_string(),
        ));
    }
    // 粗粒度 host 检查：禁止明显空白/控制字符
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(ApiError::validation(
            "tenant_gateway_url 不能包含空白或控制字符".to_string(),
        ));
    }
    Ok(url)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_system_settings(
    db: Arc<PgPool>,
    req: &SystemSettingsUpdate,
) -> Result<serde_json::Value, ApiError> {
    info!("Updating system settings");
    let mut tx = db
        .begin()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to begin transaction: {e}")))?;
    let mut errors = Vec::new();
    if let Some(ref site_name) = req.site_name
        && let Err(e) = repo::upsert_system_setting(
            &mut tx,
            "site_name",
            &serde_json::json!(site_name),
            "general",
        )
        .await
    {
        errors.push(format!("site_name: {e}"));
    }
    if let Some(ref site_url) = req.site_url
        && let Err(e) = repo::upsert_system_setting(
            &mut tx,
            "site_url",
            &serde_json::json!(site_url),
            "general",
        )
        .await
    {
        errors.push(format!("site_url: {e}"));
    }
    if let Some(ref tenant_gateway_url) = req.tenant_gateway_url {
        let normalized = normalize_tenant_gateway_url(tenant_gateway_url)?;
        if let Err(e) = repo::upsert_system_setting(
            &mut tx,
            "tenant_gateway_url",
            &serde_json::json!(normalized),
            "integration",
        )
        .await
        {
            errors.push(format!("tenant_gateway_url: {e}"));
        }
    }
    if let Some(enable_audit_log) = req.enable_audit_log
        && let Err(e) = repo::upsert_system_setting(
            &mut tx,
            "enable_audit_log",
            &serde_json::json!(enable_audit_log),
            "general",
        )
        .await
    {
        errors.push(format!("enable_audit_log: {e}"));
    }
    if let Some(session_timeout) = req.session_timeout
        && let Err(e) = repo::upsert_system_setting(
            &mut tx,
            "session_timeout",
            &serde_json::json!(session_timeout),
            "security",
        )
        .await
    {
        errors.push(format!("session_timeout: {e}"));
    }
    if let Some(max_upload_size) = req.max_upload_size
        && let Err(e) = repo::upsert_system_setting(
            &mut tx,
            "max_upload_size",
            &serde_json::json!(max_upload_size),
            "security",
        )
        .await
    {
        errors.push(format!("max_upload_size: {e}"));
    }
    if !errors.is_empty() {
        let _ = tx.rollback().await;
        return Err(ApiError::internal(format!(
            "Failed to update some settings: {}",
            errors.join(", ")
        )));
    }
    if let Err(e) = tx.commit().await {
        return Err(ApiError::internal(format!(
            "Failed to commit transaction: {e}"
        )));
    }
    info!("System settings updated successfully");
    Ok(serde_json::json!({ "message" : "设置已更新" }))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_user_settings(db: &PgPool) -> Result<serde_json::Value, ApiError> {
    let mut settings = repo::get_user_settings(db)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch user settings: {e}")))?;
    if !settings.contains_key("language") {
        settings.insert("language".to_string(), serde_json::json!("zh"));
    }
    Ok(serde_json::json!({ "settings" : settings }))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_user_settings(
    db: Arc<PgPool>,
    req: &UserSettingsUpdate,
) -> Result<serde_json::Value, ApiError> {
    info!("Updating user settings");
    let user_id = repo::get_admin_user_id_by_username(&db)
        .await
        .map_err(|e| ApiError::internal(format!("Admin user not found: {e}")))?;
    let mut tx = db
        .begin()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to begin transaction: {e}")))?;
    let mut errors = Vec::new();
    if let Some(ref language) = req.language
        && let Err(e) =
            repo::upsert_user_setting(&mut tx, user_id, "language", &serde_json::json!(language))
                .await
    {
        errors.push(format!("language: {e}"));
    }
    if let Some(ref email) = req.email
        && let Err(e) =
            repo::upsert_user_setting(&mut tx, user_id, "email", &serde_json::json!(email)).await
    {
        errors.push(format!("email: {e}"));
    }
    if !errors.is_empty() {
        let _ = tx.rollback().await;
        return Err(ApiError::internal(format!(
            "Failed to update some settings: {}",
            errors.join(", ")
        )));
    }
    if let Err(e) = tx.commit().await {
        return Err(ApiError::internal(format!(
            "Failed to commit transaction: {e}"
        )));
    }
    info!("User settings updated successfully");
    Ok(serde_json::json!({ "message" : "用户设置已更新" }))
}
