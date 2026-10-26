use crate::audit::{AuditAction, AuditLogRequest, AuditResourceType, AuditService};
use crate::error::ApiError;
use crate::handlers::types::ListParams;
use crate::repositories::route_management as repo;
use crate::repositories::supplier as supplier_repo;
use rust_decimal::Decimal;
use sqlx::PgPool;
const MAX_PAGE_SIZE: u32 = 100;
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteRuleListParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    #[serde(default)]
    pub q: Option<String>,
    pub tenant_id: Option<uuid::Uuid>,
    pub product_id: Option<uuid::Uuid>,
    pub status: Option<String>,
    /// JSON array of `{field,op,value}` list filters.
    #[serde(default)]
    pub filters: Option<String>,
}
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateChannelRequest {
    pub supplier_id: uuid::Uuid,
    pub channel_code: String,
    pub name: String,
    pub path_prefix: String,
    #[serde(default)]
    pub cost_price: Decimal,
    #[serde(default)]
    pub qps_limit: i32,
    #[serde(default = "empty_json_object")]
    pub config: serde_json::Value,
}
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateChannelRequest {
    #[serde(default)]
    pub supplier_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub channel_code: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub path_prefix: Option<String>,
    #[serde(default)]
    pub cost_price: Option<Decimal>,
    #[serde(default)]
    pub qps_limit: Option<i32>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRouteRuleRequest {
    #[serde(default)]
    pub tenant_id: Option<uuid::Uuid>,
    pub product_id: uuid::Uuid,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default = "default_active_status")]
    pub status: String,
    #[serde(default = "empty_json_object")]
    pub metadata: serde_json::Value,
    pub channel_id: uuid::Uuid,
    #[serde(default = "default_initial_timeout_ms")]
    pub timeout_ms: i32,
    #[serde(default = "default_initial_max_retries")]
    pub max_retries: i32,
}
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateRouteRuleRequest {
    #[serde(default)]
    pub tenant_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub product_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRouteBindingRequest {
    pub channel_id: uuid::Uuid,
    pub channel_order: i32,
    #[serde(default = "default_switch_condition")]
    pub switch_condition: serde_json::Value,
}
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateRouteBindingRequest {
    #[serde(default)]
    pub channel_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub channel_order: Option<i32>,
    #[serde(default)]
    pub switch_condition: Option<serde_json::Value>,
}
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmergencySwitchRequest {
    pub channel_id: uuid::Uuid,
    pub reason: String,
}
fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    20
}
fn default_priority() -> i32 {
    100
}
fn default_active_status() -> String {
    "active".to_string()
}
fn empty_json_object() -> serde_json::Value {
    serde_json::json!({})
}
fn default_switch_condition() -> serde_json::Value {
    serde_json::json!({ "max_retries" : 1, "timeout_ms" : 3000 })
}
fn default_initial_timeout_ms() -> i32 {
    3000
}
fn default_initial_max_retries() -> i32 {
    1
}
fn validate_code(field: &str, value: &str) -> Result<(), ApiError> {
    if value.is_empty() || value.len() > 100 {
        return Err(ApiError::validation(format!(
            "{field} 长度必须在 1 到 100 个字符之间"
        )));
    }
    if let Some(first_char) = value.chars().next()
        && !first_char.is_ascii_uppercase()
    {
        return Err(ApiError::validation(format!("{field} 必须以大写字母开头")));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_uppercase() || c == '-' || c.is_ascii_digit())
    {
        return Err(ApiError::validation(format!(
            "{field} 只允许包含大写字母、数字和连字符"
        )));
    }
    Ok(())
}
fn validate_name(value: &str) -> Result<(), ApiError> {
    if value.is_empty() || value.len() > 255 {
        return Err(ApiError::validation(
            "名称长度必须在 1 到 255 个字符之间".to_string(),
        ));
    }
    Ok(())
}
fn validate_status(value: &str) -> Result<(), ApiError> {
    let valid_statuses = ["active", "inactive", "suspended"];
    if !valid_statuses.contains(&value) {
        return Err(ApiError::validation("状态值无效".to_string()));
    }
    Ok(())
}
fn validate_channel_values(
    path_prefix: Option<&str>,
    cost_price: Option<Decimal>,
    qps_limit: Option<i32>,
) -> Result<(), ApiError> {
    if let Some(value) = path_prefix {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.len() > 255 {
            return Err(ApiError::validation(
                "Path prefix length must be between 1 and 255 characters".to_string(),
            ));
        }
        if !trimmed.starts_with('/') {
            return Err(ApiError::validation(
                "Path prefix must start with /".to_string(),
            ));
        }
    }
    if let Some(price) = cost_price
        && price < Decimal::ZERO
    {
        return Err(ApiError::validation(
            "Cost price must be greater than or equal to 0".to_string(),
        ));
    }
    if let Some(limit) = qps_limit
        && limit < 0
    {
        return Err(ApiError::validation(
            "QPS limit must be greater than or equal to 0".to_string(),
        ));
    }
    Ok(())
}
fn validate_switch_condition(value: &serde_json::Value) -> Result<(), ApiError> {
    if !value.is_object() {
        return Err(ApiError::validation(
            "Switch condition must be a JSON object".to_string(),
        ));
    }
    if value
        .get("max_retries")
        .and_then(serde_json::Value::as_i64)
        .is_some_and(|v| v < 0)
    {
        return Err(ApiError::validation(
            "Switch condition max_retries must be greater than or equal to 0".to_string(),
        ));
    }
    if value
        .get("timeout_ms")
        .and_then(serde_json::Value::as_i64)
        .is_some_and(|v| v <= 0)
    {
        return Err(ApiError::validation(
            "Switch condition timeout_ms must be greater than 0".to_string(),
        ));
    }
    Ok(())
}
impl CreateChannelRequest {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_code("channel_code", &self.channel_code)?;
        validate_name(&self.name)?;
        validate_channel_values(
            Some(&self.path_prefix),
            Some(self.cost_price),
            Some(self.qps_limit),
        )?;
        mazbri_core::validate_channel_config_if_egress(&self.config)
            .map_err(|e| ApiError::validation(e.to_string()))?;
        Ok(())
    }
}
impl UpdateChannelRequest {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), ApiError> {
        if let Some(value) = &self.channel_code {
            validate_code("channel_code", value)?;
        }
        if let Some(value) = &self.name {
            validate_name(value)?;
        }
        if let Some(value) = &self.status {
            validate_status(value)?;
        }
        validate_channel_values(self.path_prefix.as_deref(), self.cost_price, self.qps_limit)?;
        if let Some(ref cfg) = self.config {
            mazbri_core::validate_channel_config_if_egress(cfg)
                .map_err(|e| ApiError::validation(e.to_string()))?;
        }
        Ok(())
    }
}
impl CreateRouteRuleRequest {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.priority < 0 {
            return Err(ApiError::validation(
                "Priority must be greater than or equal to 0".to_string(),
            ));
        }
        validate_status(&self.status)?;
        if self.timeout_ms <= 0 {
            return Err(ApiError::validation(
                "timeout_ms must be greater than 0".to_string(),
            ));
        }
        if self.max_retries < 0 {
            return Err(ApiError::validation(
                "max_retries must be greater than or equal to 0".to_string(),
            ));
        }
        Ok(())
    }
}
impl UpdateRouteRuleRequest {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), ApiError> {
        if let Some(priority) = self.priority
            && priority < 0
        {
            return Err(ApiError::validation(
                "Priority must be greater than or equal to 0".to_string(),
            ));
        }
        if let Some(value) = &self.status {
            validate_status(value)?;
        }
        Ok(())
    }
}
impl CreateRouteBindingRequest {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.channel_order < 1 {
            return Err(ApiError::validation(
                "Channel order must be greater than or equal to 1".to_string(),
            ));
        }
        validate_switch_condition(&self.switch_condition)
    }
}
impl UpdateRouteBindingRequest {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), ApiError> {
        if let Some(order) = self.channel_order
            && order < 1
        {
            return Err(ApiError::validation(
                "Channel order must be greater than or equal to 1".to_string(),
            ));
        }
        if let Some(value) = &self.switch_condition {
            validate_switch_condition(value)?;
        }
        Ok(())
    }
}
impl EmergencySwitchRequest {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.reason.trim().len() < 3 || self.reason.len() > 500 {
            return Err(ApiError::validation(
                "切换原因长度必须在 3 到 500 个字符之间".to_string(),
            ));
        }
        Ok(())
    }
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_channels(
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
    let filters = crate::list_query::parse_and_validate_filters(
        params.filters.as_deref(),
        &crate::list_query::channels_catalog(),
    )
    .map_err(|e| ApiError::validation(e.message))?;
    let total = repo::count_channels(
        pool,
        params.status.as_deref(),
        search_pattern.as_deref(),
        &filters,
    )
    .await
    .map_err(|e| ApiError::internal(format!("统计通道失败: {e}")))?;
    let channels = repo::list_channels(
        pool,
        params.status.as_deref(),
        search_pattern.as_deref(),
        &filters,
        i64::from(page_size),
        i64::from(offset),
    )
    .await
    .map_err(|e| ApiError::internal(format!("获取通道列表失败: {e}")))?;
    Ok(serde_json::json!(
        { "channels" : channels, "pagination" : pagination(params.page, page_size,
        total), }
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_channel(
    pool: &PgPool,
    audit: &AuditService,
    payload: &CreateChannelRequest,
    user_id: uuid::Uuid,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    payload.validate()?;
    ensure_supplier_active(pool, payload.supplier_id).await?;
    let row = repo::insert_channel(
        pool,
        repo::ChannelInsert {
            supplier_id: payload.supplier_id,
            channel_code: &payload.channel_code,
            name: &payload.name,
            path_prefix: &payload.path_prefix,
            cost_price: payload.cost_price,
            qps_limit: payload.qps_limit,
            config: &payload.config,
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("创建通道失败: {e}")))?;
    let value = serde_json::to_value(&row).unwrap_or_default();
    audit.log(change_audit_log(
        user_id,
        AuditAction::ChannelCreate,
        AuditResourceType::Channel,
        row.id.to_string(),
        None,
        Some(value.clone()),
        client_ip,
    ));
    Ok(value)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_channel(pool: &PgPool, id: &str) -> Result<serde_json::Value, ApiError> {
    let id = parse_uuid(id, "channel")?;
    let row = repo::find_channel_with_supplier_by_id(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("获取通道失败: {e}")))?
        .ok_or_else(|| ApiError::not_found("channel".to_string(), id.to_string()))?;
    Ok(serde_json::to_value(row).unwrap_or_default())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_channel(
    pool: &PgPool,
    audit: &AuditService,
    id: &str,
    payload: &UpdateChannelRequest,
    user_id: uuid::Uuid,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    payload.validate()?;
    let id = parse_uuid(id, "channel")?;
    let old = repo::find_channel_by_id(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("获取通道失败: {e}")))?
        .ok_or_else(|| ApiError::not_found("channel".to_string(), id.to_string()))?;
    if let Some(supplier_id) = payload.supplier_id {
        ensure_supplier_active(pool, supplier_id).await?;
    }
    let row = repo::update_channel(
        pool,
        id,
        repo::ChannelUpdate {
            supplier_id: payload.supplier_id,
            channel_code: payload.channel_code.as_deref(),
            name: payload.name.as_deref(),
            path_prefix: payload.path_prefix.as_deref(),
            cost_price: payload.cost_price,
            qps_limit: payload.qps_limit,
            status: payload.status.as_deref(),
            config: payload.config.as_ref(),
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("更新通道失败: {e}")))?
    .ok_or_else(|| ApiError::not_found("channel".to_string(), id.to_string()))?;
    let old_value = serde_json::to_value(old).unwrap_or_default();
    let new_value = serde_json::to_value(&row).unwrap_or_default();
    audit.log(change_audit_log(
        user_id,
        AuditAction::ChannelUpdate,
        AuditResourceType::Channel,
        row.id.to_string(),
        Some(old_value),
        Some(new_value.clone()),
        client_ip,
    ));
    Ok(new_value)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_channel(
    pool: &PgPool,
    audit: &AuditService,
    id: &str,
    user_id: uuid::Uuid,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    let id = parse_uuid(id, "channel")?;
    let old = repo::find_channel_by_id(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("获取通道失败: {e}")))?
        .ok_or_else(|| ApiError::not_found("channel".to_string(), id.to_string()))?;
    let binding_count = repo::count_route_bindings_for_channel(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("检查路由绑定失败: {e}")))?;
    if binding_count > 0 {
        return Err(ApiError::validation(
            "通道已绑定路由规则，请先解绑后再删除".to_string(),
        ));
    }
    repo::delete_channel(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("删除通道失败: {e}")))?;
    let old_value = serde_json::to_value(&old).unwrap_or_default();
    audit.log(change_audit_log(
        user_id,
        AuditAction::ChannelDelete,
        AuditResourceType::Channel,
        id.to_string(),
        Some(old_value),
        None,
        client_ip,
    ));
    Ok(serde_json::json!({ "message" : "通道删除成功", "id" : id }))
}
async fn ensure_supplier_active(pool: &PgPool, supplier_id: uuid::Uuid) -> Result<(), ApiError> {
    let supplier = supplier_repo::find_by_id(pool, supplier_id)
        .await
        .map_err(|e| ApiError::internal(format!("获取供应商失败: {e}")))?
        .ok_or_else(|| ApiError::not_found("supplier".to_string(), supplier_id.to_string()))?;
    if supplier.status != "active" {
        return Err(ApiError::validation(
            "供应商必须存在且处于启用状态".to_string(),
        ));
    }
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_route_rules(
    pool: &PgPool,
    params: &RouteRuleListParams,
) -> Result<serde_json::Value, ApiError> {
    let page_size = params.page_size.min(MAX_PAGE_SIZE);
    let offset = (params.page.saturating_sub(1)) * page_size;
    let filters = crate::list_query::parse_and_validate_filters(
        params.filters.as_deref(),
        &crate::list_query::routes_catalog(),
    )
    .map_err(|e| ApiError::validation(e.message))?;
    let total = repo::count_route_rules(
        pool,
        params.tenant_id,
        params.product_id,
        params.q.as_deref(),
        params.status.as_deref(),
        &filters,
    )
    .await
    .map_err(|e| ApiError::internal(format!("Failed to count route rules: {e}")))?;
    let route_rules = repo::list_route_rules(
        pool,
        repo::ListRouteRulesQuery {
            tenant_id: params.tenant_id,
            product_id: params.product_id,
            search: params.q.as_deref(),
            status: params.status.as_deref(),
            filters: &filters,
            page_size: i64::from(page_size),
            offset: i64::from(offset),
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("Failed to list route rules: {e}")))?;
    Ok(serde_json::json!(
        { "route_rules" : route_rules, "pagination" : pagination(params.page,
        page_size, total), }
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_route_rule(
    pool: &PgPool,
    audit: &AuditService,
    payload: &CreateRouteRuleRequest,
    user_id: uuid::Uuid,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    payload.validate()?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to begin transaction: {e}")))?;

    let row = sqlx::query_as::<_, repo::RouteRuleRow>(&format!(
        "INSERT INTO route_rules (tenant_id, product_id, priority, status, metadata)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {}",
        repo::ROUTE_RULE_COLUMNS_FOR_SERVICE
    ))
    .bind(payload.tenant_id)
    .bind(payload.product_id)
    .bind(payload.priority)
    .bind(&payload.status)
    .bind(&payload.metadata)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to create route rule: {e}")))?;

    let switch_cond = serde_json::json!({
        "max_retries": payload.max_retries,
        "timeout_ms": payload.timeout_ms,
    });

    sqlx::query_as::<_, repo::RouteChannelBindingRow>(&format!(
        "INSERT INTO route_channel_bindings (route_rule_id, channel_id, channel_order, switch_condition)
         VALUES ($1, $2, $3, $4)
         RETURNING {}",
        repo::ROUTE_BINDING_COLUMNS_FOR_SERVICE
    ))
    .bind(row.id)
    .bind(payload.channel_id)
    .bind(1)
    .bind(&switch_cond)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to bind initial channel: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to commit transaction: {e}")))?;

    let value = serde_json::to_value(&row).unwrap_or_default();
    audit.log(change_audit_log(
        user_id,
        AuditAction::RouteRuleCreate,
        AuditResourceType::RouteRule,
        row.id.to_string(),
        None,
        Some(value.clone()),
        client_ip,
    ));
    Ok(value)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_route_rule(pool: &PgPool, id: &str) -> Result<serde_json::Value, ApiError> {
    let id = parse_uuid(id, "route rule")?;
    let row = repo::find_route_rule_by_id(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get route rule: {e}")))?
        .ok_or_else(|| ApiError::not_found("route_rule".to_string(), id.to_string()))?;
    Ok(serde_json::to_value(row).unwrap_or_default())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_route_rule(
    pool: &PgPool,
    audit: &AuditService,
    id: &str,
    payload: &UpdateRouteRuleRequest,
    user_id: uuid::Uuid,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    payload.validate()?;
    let id = parse_uuid(id, "route rule")?;
    let old = repo::find_route_rule_by_id(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get route rule: {e}")))?
        .ok_or_else(|| ApiError::not_found("route_rule".to_string(), id.to_string()))?;
    let row = repo::update_route_rule(
        pool,
        id,
        repo::RouteRuleUpdate {
            tenant_id: payload.tenant_id,
            product_id: payload.product_id,
            priority: payload.priority,
            status: payload.status.as_deref(),
            metadata: payload.metadata.as_ref(),
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("Failed to update route rule: {e}")))?
    .ok_or_else(|| ApiError::not_found("route_rule".to_string(), id.to_string()))?;
    let old_value = serde_json::to_value(old).unwrap_or_default();
    let new_value = serde_json::to_value(&row).unwrap_or_default();
    audit.log(change_audit_log(
        user_id,
        AuditAction::RouteRuleUpdate,
        AuditResourceType::RouteRule,
        row.id.to_string(),
        Some(old_value),
        Some(new_value.clone()),
        client_ip,
    ));
    Ok(new_value)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_route_rule(
    pool: &PgPool,
    audit: &AuditService,
    id: &str,
    user_id: uuid::Uuid,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    let id = parse_uuid(id, "route rule")?;
    let old = repo::find_route_rule_by_id(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get route rule: {e}")))?
        .ok_or_else(|| ApiError::not_found("route_rule".to_string(), id.to_string()))?;
    repo::delete_route_rule(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete route rule: {e}")))?;
    let old_value = serde_json::to_value(old).unwrap_or_default();
    audit.log(change_audit_log(
        user_id,
        AuditAction::RouteRuleDelete,
        AuditResourceType::RouteRule,
        id.to_string(),
        Some(old_value),
        None,
        client_ip,
    ));
    Ok(serde_json::json!({ "message" : "Route rule deleted successfully", "id" : id }))
}
/// 锁定路由规则及其全部渠道绑定（`FOR UPDATE`）。
async fn lock_route_for_switch(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    route_rule_id: uuid::Uuid,
) -> Result<(repo::RouteRuleRow, Vec<repo::RouteChannelBindingRow>), ApiError> {
    let old_rule = sqlx::query_as::<_, repo::RouteRuleRow>(&format!(
        "SELECT {} FROM route_rules WHERE id = $1 FOR UPDATE",
        repo::ROUTE_RULE_COLUMNS_FOR_SERVICE
    ))
    .bind(route_rule_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to lock route rule: {e}")))?
    .ok_or_else(|| ApiError::not_found("route_rule".to_string(), route_rule_id.to_string()))?;
    let old_bindings = sqlx::query_as::<
        _,
        repo::RouteChannelBindingRow,
    >(
            &format!(
                "SELECT {} FROM route_channel_bindings WHERE route_rule_id = $1 ORDER BY channel_order ASC FOR UPDATE",
                repo::ROUTE_BINDING_COLUMNS_FOR_SERVICE
            ),
        )
        .bind(route_rule_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| ApiError::internal(
            format!("Failed to lock route bindings: {e}"),
        ))?;
    Ok((old_rule, old_bindings))
}

/// 紧急切换：选中渠道置首并标记 fail-fast，其余渠道顺延。
async fn apply_emergency_binding_order(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    old_bindings: &[repo::RouteChannelBindingRow],
    payload: &EmergencySwitchRequest,
) -> Result<(), ApiError> {
    for binding in old_bindings {
        let new_order = if binding.channel_id == payload.channel_id {
            1
        } else {
            binding.channel_order.saturating_add(100)
        };
        let switch_condition = if binding.channel_id == payload.channel_id {
            merge_route_metadata(
                &binding.switch_condition,
                &serde_json::json!(
                    { "emergency" : true, "fail_fast" : true, "reason" : payload.reason
                    .trim() }
                ),
            )
        } else {
            binding.switch_condition.clone()
        };
        sqlx::query(
            "UPDATE route_channel_bindings
             SET channel_order = $2, switch_condition = $3, updated_at = NOW()
             WHERE id = $1",
        )
        .bind(binding.id)
        .bind(new_order)
        .bind(&switch_condition)
        .execute(&mut **tx)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update route binding: {e}")))?;
    }
    Ok(())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn emergency_switch_route(
    pool: &PgPool,
    audit: &AuditService,
    route_rule_id: &str,
    payload: &EmergencySwitchRequest,
    user_id: uuid::Uuid,
    actor_name: &str,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    payload.validate()?;
    let route_rule_id = parse_uuid(route_rule_id, "route rule")?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to start emergency switch: {e}")))?;
    let (old_rule, old_bindings) = lock_route_for_switch(&mut tx, route_rule_id).await?;
    let target_binding = old_bindings
        .iter()
        .find(|binding| binding.channel_id == payload.channel_id)
        .ok_or_else(|| {
            ApiError::validation("channel_id is not bound to this route rule".to_string())
        })?;
    let emergency_metadata = serde_json::json!(
        { "emergency_switch" : { "enabled" : true, "channel_id" : payload.channel_id,
        "binding_id" : target_binding.id, "reason" : payload.reason.trim(), "operator" :
        actor_name, "operator_id" : user_id, "switched_at" : chrono::Utc::now(), } }
    );
    let merged_metadata = merge_route_metadata(&old_rule.metadata, &emergency_metadata);
    let new_rule = sqlx::query_as::<_, repo::RouteRuleRow>(&format!(
        "UPDATE route_rules
         SET status = 'active', metadata = $2, updated_at = NOW()
         WHERE id = $1
         RETURNING {}",
        repo::ROUTE_RULE_COLUMNS_FOR_SERVICE
    ))
    .bind(route_rule_id)
    .bind(&merged_metadata)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to update route rule: {e}")))?;
    apply_emergency_binding_order(&mut tx, &old_bindings, payload).await?;
    let new_bindings = sqlx::query_as::<_, repo::RouteChannelBindingRow>(&format!(
        "SELECT {} FROM route_channel_bindings WHERE route_rule_id = $1 ORDER BY channel_order ASC",
        repo::ROUTE_BINDING_COLUMNS_FOR_SERVICE
    ))
    .bind(route_rule_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to load updated route bindings: {e}")))?;
    tx.commit()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to commit emergency switch: {e}")))?;
    let old_value = serde_json::json!(
        { "route_rule" : old_rule, "bindings" : old_bindings, }
    );
    let new_value = serde_json::json!(
        { "route_rule" : new_rule, "bindings" : new_bindings, }
    );
    audit.log(AuditLogRequest {
        user_id: Some(user_id),
        action: AuditAction::RouteEmergencySwitch,
        resource_type: AuditResourceType::RouteRule,
        resource_id: Some(route_rule_id.to_string()),
        details: Some(serde_json::json!(
            { "reason" : payload.reason.trim(), "operator" : actor_name,
            "operator_id" : user_id, "ip_address" : client_ip,
            "selected_channel_id" : payload.channel_id, "old_route" : old_value,
            "new_route" : new_value, }
        )),
        ip_address: client_ip,
        old_value: Some(old_value),
        new_value: Some(new_value.clone()),
        ..Default::default()
    });
    Ok(serde_json::json!(
        { "message" : "Emergency switch applied", "route_rule_id" : route_rule_id,
        "selected_channel_id" : payload.channel_id, "route" : new_value, }
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_route_bindings(
    pool: &PgPool,
    route_rule_id: &str,
) -> Result<serde_json::Value, ApiError> {
    let route_rule_id = parse_uuid(route_rule_id, "route rule")?;
    let bindings = repo::list_route_bindings(pool, route_rule_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list route bindings: {e}")))?;
    Ok(serde_json::json!({ "bindings" : bindings }))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_route_binding(
    pool: &PgPool,
    audit: &AuditService,
    route_rule_id: &str,
    payload: &CreateRouteBindingRequest,
    user_id: uuid::Uuid,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    payload.validate()?;
    let route_rule_id = parse_uuid(route_rule_id, "route rule")?;
    let row = repo::insert_route_binding(
        pool,
        repo::RouteBindingInsert {
            route_rule_id,
            channel_id: payload.channel_id,
            channel_order: payload.channel_order,
            switch_condition: &payload.switch_condition,
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("Failed to create route binding: {e}")))?;
    let value = serde_json::to_value(&row).unwrap_or_default();
    audit.log(change_audit_log(
        user_id,
        AuditAction::RouteChannelBindingCreate,
        AuditResourceType::RouteChannelBinding,
        row.id.to_string(),
        None,
        Some(value.clone()),
        client_ip,
    ));
    Ok(value)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_route_binding(
    pool: &PgPool,
    audit: &AuditService,
    id: &str,
    payload: &UpdateRouteBindingRequest,
    user_id: uuid::Uuid,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    payload.validate()?;
    let id = parse_uuid(id, "route binding")?;
    let old = repo::find_route_binding_by_id(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get route binding: {e}")))?
        .ok_or_else(|| ApiError::not_found("route_channel_binding".to_string(), id.to_string()))?;
    let row = repo::update_route_binding(
        pool,
        id,
        repo::RouteBindingUpdate {
            channel_id: payload.channel_id,
            channel_order: payload.channel_order,
            switch_condition: payload.switch_condition.as_ref(),
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("Failed to update route binding: {e}")))?
    .ok_or_else(|| ApiError::not_found("route_channel_binding".to_string(), id.to_string()))?;
    let old_value = serde_json::to_value(old).unwrap_or_default();
    let new_value = serde_json::to_value(&row).unwrap_or_default();
    audit.log(change_audit_log(
        user_id,
        AuditAction::RouteChannelBindingUpdate,
        AuditResourceType::RouteChannelBinding,
        row.id.to_string(),
        Some(old_value),
        Some(new_value.clone()),
        client_ip,
    ));
    Ok(new_value)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_route_binding(
    pool: &PgPool,
    audit: &AuditService,
    id: &str,
    user_id: uuid::Uuid,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    let id = parse_uuid(id, "route binding")?;
    let old = repo::find_route_binding_by_id(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get route binding: {e}")))?
        .ok_or_else(|| ApiError::not_found("route_channel_binding".to_string(), id.to_string()))?;
    repo::delete_route_binding(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete route binding: {e}")))?;
    let old_value = serde_json::to_value(old).unwrap_or_default();
    audit.log(change_audit_log(
        user_id,
        AuditAction::RouteChannelBindingDelete,
        AuditResourceType::RouteChannelBinding,
        id.to_string(),
        Some(old_value),
        None,
        client_ip,
    ));
    Ok(serde_json::json!(
        { "message" : "Route binding deleted successfully", "id" : id }
    ))
}
fn pagination(page: u32, page_size: u32, total: i64) -> serde_json::Value {
    let total_pages = mazbri_core::num::f64_to_u64(
        (mazbri_core::num::i64_to_f64(total) / f64::from(page_size)).ceil(),
    );
    serde_json::json!(
        { "page" : page, "page_size" : page_size, "total" : total, "total_pages" :
        total_pages, }
    )
}
fn parse_uuid(id: &str, resource: &str) -> Result<uuid::Uuid, ApiError> {
    uuid::Uuid::parse_str(id)
        .map_err(|e| ApiError::validation(format!("Invalid {resource} ID format: {e}")))
}
fn merge_route_metadata(
    existing: &serde_json::Value,
    patch: &serde_json::Value,
) -> serde_json::Value {
    let mut merged = existing.clone();
    if !merged.is_object() {
        merged = serde_json::json!({});
    }
    if let (Some(target), Some(source)) = (merged.as_object_mut(), patch.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    merged
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
        user_id: Some(user_id),
        action,
        resource_type,
        resource_id: Some(resource_id),
        details: Some(serde_json::json!(
            { "change_summary" : { "has_old_value" : old_value.is_some(),
            "has_new_value" : new_value.is_some(), } }
        )),
        ip_address,
        old_value,
        new_value,
        ..Default::default()
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn get_next_channel_code(_pool: &PgPool) -> Result<serde_json::Value, ApiError> {
    use rand::RngExt;
    let num: u32 = rand::rng().random_range(1000..=9999);
    Ok(serde_json::json!({
        "channel_code": format!("CH-{}", num)
    }))
}
