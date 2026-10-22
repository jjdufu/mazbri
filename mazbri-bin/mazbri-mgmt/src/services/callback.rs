use crate::audit::{AuditAction, AuditResourceType, AuditService, create_audit_log};
use crate::error::ApiError;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, QueryBuilder, Row};
use uuid::Uuid;
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSubscriptionRequest {
    pub tenant_code: String,
    pub product_code: String,
    pub callback_url: String,
    pub secret_key: String,
    pub retry_policy: Option<serde_json::Value>,
}
#[derive(Debug, Serialize)]
pub struct SubscriptionResponse {
    pub id: Uuid,
    pub tenant_code: String,
    pub tenant_id: String,
    pub product_code: String,
    pub callback_url: String,
    pub is_active: bool,
}
#[derive(Debug, Serialize)]
pub struct DeliveryLogResponse {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub event_id: String,
    pub callback_url: String,
    pub status: String,
    pub retry_count: i32,
    pub next_retry_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_response_code: Option<i32>,
    pub last_error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
async fn resolve_product_id(pool: &PgPool, code: &str) -> Result<Uuid, ApiError> {
    if let Ok(id) = Uuid::parse_str(code) {
        return Ok(id);
    }
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM products WHERE product_id = $1")
        .bind(code)
        .fetch_optional(pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to resolve product: {e}")))?
        .ok_or_else(|| ApiError::not_found("product".to_string(), code.to_string()))
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallbackListQuery {
    #[serde(default)]
    pub q: Option<String>,
    /// JSON array of `{field,op,value}` list filters.
    #[serde(default)]
    pub filters: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    100
}

#[derive(Debug, Serialize)]
pub struct PaginatedSubscriptions {
    pub data: Vec<SubscriptionResponse>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Serialize)]
pub struct PaginatedDeliveries {
    pub data: Vec<DeliveryLogResponse>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

fn push_subscription_scope(
    builder: &mut QueryBuilder<'_, sqlx::Postgres>,
    pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<(), crate::list_query::ListQueryError> {
    if let Some(p) = pattern {
        let pat = p.to_string();
        builder.push(" AND (t.tenant_code ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR t.tenant_id ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR p.product_id ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR s.callback_url ILIKE ");
        builder.push_bind(pat);
        builder.push(")");
    }
    crate::list_query::apply_pg_filters(builder, filters)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_subscriptions(
    pool: &PgPool,
    q: Option<&str>,
    filters_json: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<PaginatedSubscriptions, ApiError> {
    let page = page.max(1);
    let page_size = page_size.clamp(1, 500);
    let offset = i64::from((page - 1) * page_size);
    let limit = i64::from(page_size);
    let pattern = q
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));
    let filters = crate::list_query::parse_and_validate_filters(
        filters_json,
        &crate::list_query::callback_subscriptions_catalog(),
    )
    .map_err(|e| ApiError::validation(e.message))?;

    let mut count_builder: QueryBuilder<_> = QueryBuilder::new(
        "SELECT COUNT(*) \
         FROM mazbri_platform.callback_subscriptions s \
         JOIN tenants t ON s.tenant_id = t.id \
         JOIN products p ON s.product_id = p.id WHERE 1=1",
    );
    push_subscription_scope(&mut count_builder, pattern.as_deref(), &filters)
        .map_err(|e| ApiError::validation(e.message))?;
    let total: i64 = count_builder
        .build()
        .fetch_one(pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to count subscriptions: {e}")))?
        .get(0);

    let mut list_builder: QueryBuilder<_> = QueryBuilder::new(
        "SELECT s.id, t.tenant_code, t.tenant_id, p.product_id as product_code, s.callback_url, s.is_active \
         FROM mazbri_platform.callback_subscriptions s \
         JOIN tenants t ON s.tenant_id = t.id \
         JOIN products p ON s.product_id = p.id WHERE 1=1",
    );
    push_subscription_scope(&mut list_builder, pattern.as_deref(), &filters)
        .map_err(|e| ApiError::validation(e.message))?;
    list_builder.push(" ORDER BY s.id DESC LIMIT ");
    list_builder.push_bind(limit);
    list_builder.push(" OFFSET ");
    list_builder.push_bind(offset);
    let rows = list_builder
        .build()
        .fetch_all(pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list subscriptions: {e}")))?;

    let data = rows
        .into_iter()
        .map(|row| SubscriptionResponse {
            id: row.get("id"),
            tenant_code: row.get("tenant_code"),
            tenant_id: row.get("tenant_id"),
            product_code: row.get("product_code"),
            callback_url: row.get("callback_url"),
            is_active: row.get("is_active"),
        })
        .collect();
    Ok(PaginatedSubscriptions {
        data,
        total,
        page,
        page_size,
    })
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_subscription(
    pool: &PgPool,
    audit: &AuditService,
    req: CreateSubscriptionRequest,
    user_id: Uuid,
    _username: &str,
    client_ip: Option<String>,
) -> Result<SubscriptionResponse, ApiError> {
    let tenant_id = crate::services::tenant::resolve_tenant_id(pool, &req.tenant_code).await?;
    let product_id = resolve_product_id(pool, &req.product_code).await?;
    let retry_policy = req
        .retry_policy
        .unwrap_or_else(|| serde_json::json!({ "max_retries" : 5, "backoff_multiplier" : 2 }));
    let row = sqlx::query(
            r"
        WITH inserted AS (
            INSERT INTO mazbri_platform.callback_subscriptions (tenant_id, product_id, callback_url, secret_key, retry_policy)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, tenant_id, product_id, callback_url, is_active
        )
        SELECT i.id, t.tenant_code, t.tenant_id, p.product_id as product_code, i.callback_url, i.is_active
        FROM inserted i
        JOIN tenants t ON i.tenant_id = t.id
        JOIN products p ON i.product_id = p.id
        ",
        )
        .bind(tenant_id)
        .bind(product_id)
        .bind(req.callback_url)
        .bind(req.secret_key)
        .bind(retry_policy)
        .fetch_one(pool)
        .await
        .map_err(|e| ApiError::internal(
            format!("Failed to create subscription: {e}"),
        ))?;
    let resp = SubscriptionResponse {
        id: row.get("id"),
        tenant_code: row.get("tenant_code"),
        tenant_id: row.get("tenant_id"),
        product_code: row.get("product_code"),
        callback_url: row.get("callback_url"),
        is_active: row.get("is_active"),
    };
    audit.log(create_audit_log(
        user_id,
        AuditAction::CallbackSubscriptionCreate,
        AuditResourceType::Callback,
        resp.id.to_string(),
        serde_json::json!(
            { "tenant_code" : resp.tenant_code, "product_code" : resp
            .product_code, "callback_url" : resp.callback_url, }
        ),
        client_ip,
    ));
    Ok(resp)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_subscription(
    pool: &PgPool,
    audit: &AuditService,
    id: Uuid,
    user_id: Uuid,
    _username: &str,
    client_ip: Option<String>,
) -> Result<(), ApiError> {
    let result = sqlx::query("DELETE FROM mazbri_platform.callback_subscriptions WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete subscription: {e}")))?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found(
            "subscription".to_string(),
            id.to_string(),
        ));
    }
    audit.log(create_audit_log(
        user_id,
        AuditAction::CallbackSubscriptionDelete,
        AuditResourceType::Callback,
        id.to_string(),
        serde_json::json!({ "action" : "delete" }),
        client_ip,
    ));
    Ok(())
}
fn push_delivery_scope(
    builder: &mut QueryBuilder<'_, sqlx::Postgres>,
    pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<(), crate::list_query::ListQueryError> {
    if let Some(p) = pattern {
        let pat = p.to_string();
        builder.push(" AND (d.event_id ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR s.callback_url ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR d.status ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR COALESCE(d.last_error, '') ILIKE ");
        builder.push_bind(pat);
        builder.push(")");
    }
    crate::list_query::apply_pg_filters(builder, filters)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_delivery_logs(
    pool: &PgPool,
    q: Option<&str>,
    filters_json: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<PaginatedDeliveries, ApiError> {
    let page = page.max(1);
    let page_size = page_size.clamp(1, 500);
    let offset = i64::from((page - 1) * page_size);
    let limit = i64::from(page_size);
    let pattern = q
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));
    let filters = crate::list_query::parse_and_validate_filters(
        filters_json,
        &crate::list_query::callback_deliveries_catalog(),
    )
    .map_err(|e| ApiError::validation(e.message))?;

    let mut count_builder: QueryBuilder<_> = QueryBuilder::new(
        "SELECT COUNT(*) \
         FROM mazbri_platform.callback_delivery_logs d \
         JOIN mazbri_platform.callback_subscriptions s ON s.id = d.subscription_id WHERE 1=1",
    );
    push_delivery_scope(&mut count_builder, pattern.as_deref(), &filters)
        .map_err(|e| ApiError::validation(e.message))?;
    let total: i64 = count_builder
        .build()
        .fetch_one(pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to count callback deliveries: {e}")))?
        .get(0);

    let mut list_builder: QueryBuilder<_> = QueryBuilder::new(
        "SELECT \
            d.id, d.subscription_id, d.event_id, s.callback_url, d.status, \
            d.retry_count, d.next_retry_at, d.last_response_code, d.last_error, \
            d.created_at, d.updated_at \
         FROM mazbri_platform.callback_delivery_logs d \
         JOIN mazbri_platform.callback_subscriptions s ON s.id = d.subscription_id WHERE 1=1",
    );
    push_delivery_scope(&mut list_builder, pattern.as_deref(), &filters)
        .map_err(|e| ApiError::validation(e.message))?;
    list_builder.push(" ORDER BY d.created_at DESC LIMIT ");
    list_builder.push_bind(limit);
    list_builder.push(" OFFSET ");
    list_builder.push_bind(offset);
    let rows = list_builder
        .build()
        .fetch_all(pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list callback deliveries: {e}")))?;

    let data = rows
        .into_iter()
        .map(|row| DeliveryLogResponse {
            id: row.get("id"),
            subscription_id: row.get("subscription_id"),
            event_id: row.get("event_id"),
            callback_url: row.get("callback_url"),
            status: row.get("status"),
            retry_count: row.get("retry_count"),
            next_retry_at: row.get("next_retry_at"),
            last_response_code: row.get("last_response_code"),
            last_error: row.get("last_error"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect();
    Ok(PaginatedDeliveries {
        data,
        total,
        page,
        page_size,
    })
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn retry_delivery(pool: &PgPool, id: Uuid) -> Result<(), ApiError> {
    let result = sqlx::query(
        r"
        UPDATE mazbri_platform.callback_delivery_logs
        SET status = 'retrying',
            next_retry_at = NOW(),
            last_error = NULL,
            updated_at = NOW()
        WHERE id = $1
          AND status IN ('failed', 'retrying', 'pending')
        ",
    )
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to retry callback delivery: {e}")))?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found(
            "callback_delivery".to_string(),
            id.to_string(),
        ));
    }
    Ok(())
}
