use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{PgPool, Row};
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TenantAccountRow {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub balance: Decimal,
    pub credit_limit: Decimal,
    pub available_balance: Decimal,
    pub status: String,
    pub credit_tier: String,
    pub billing_cycle: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RechargeResultRow {
    pub transaction_id: uuid::Uuid,
    pub account_id: uuid::Uuid,
    pub balance_before: Decimal,
    pub balance_after: Decimal,
    pub status: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct RefundResultRow {
    pub transaction_id: uuid::Uuid,
    pub account_id: uuid::Uuid,
    pub balance_before: Decimal,
    pub balance_after: Decimal,
}
#[derive(Debug, Clone, Serialize)]
pub struct TransactionRow {
    pub id: uuid::Uuid,
    pub transaction_type: String,
    pub amount: Decimal,
    pub balance_before: Decimal,
    pub balance_after: Decimal,
    pub description: Option<String>,
    pub payment_method: Option<String>,
    pub operator_id: Option<uuid::Uuid>,
    pub operator_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub total_count: i64,
}
pub use super::tenant_helpers::tenant_exists;
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_tenant_account(
    billing_pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<Option<TenantAccountRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantAccountRow>(
        r"
        SELECT id, tenant_id, balance, credit_limit, available_balance,
               status, credit_tier, billing_cycle, created_at, updated_at
        FROM tenant_accounts
        WHERE tenant_id = $1
    ",
    )
    .bind(tenant_id)
    .fetch_optional(billing_pool)
    .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_tenant_account(
    billing_pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<TenantAccountRow, sqlx::Error> {
    sqlx::query_as::<_, TenantAccountRow>(
        r"
        INSERT INTO tenant_accounts (tenant_id, balance, credit_limit, status, credit_tier)
        VALUES ($1, 0, 10.0000, 'active', 'standard')
        RETURNING id, tenant_id, balance, credit_limit, available_balance,
                  status, credit_tier, billing_cycle, created_at, updated_at
    ",
    )
    .bind(tenant_id)
    .fetch_one(billing_pool)
    .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn ensure_tenant_account(
    billing_pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<uuid::Uuid, sqlx::Error> {
    let query = "SELECT ensure_tenant_account($1, 'standard')";
    let account_id: uuid::Uuid = sqlx::query_scalar(query)
        .bind(tenant_id)
        .fetch_one(billing_pool)
        .await?;
    Ok(account_id)
}
pub struct ProcessRechargeParams<'a> {
    pub tenant_id: uuid::Uuid,
    pub amount: Decimal,
    pub payment_method: &'a str,
    pub payment_reference: &'a Option<String>,
    pub description: &'a str,
    pub operator_id: uuid::Uuid,
    pub operator_name: &'a str,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn process_recharge(
    billing_pool: &PgPool,
    params: ProcessRechargeParams<'_>,
) -> Result<RechargeResultRow, sqlx::Error> {
    let query = "SELECT * FROM process_recharge($1, $2, $3, $4, $5, $6, $7)";
    let row = sqlx::query(query)
        .bind(params.tenant_id)
        .bind(params.amount)
        .bind(params.payment_method)
        .bind(params.payment_reference)
        .bind(params.description)
        .bind(params.operator_id)
        .bind(params.operator_name)
        .fetch_one(billing_pool)
        .await?;
    Ok(RechargeResultRow {
        transaction_id: row.get::<uuid::Uuid, _>(0),
        account_id: row.get::<uuid::Uuid, _>(1),
        balance_before: row.get::<Decimal, _>(2),
        balance_after: row.get::<Decimal, _>(3),
        status: row.get::<String, _>(4),
    })
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn process_refund(
    billing_pool: &PgPool,
    tenant_id: uuid::Uuid,
    amount: Decimal,
    reason: &Option<String>,
    bill_id: &Option<uuid::Uuid>,
    operator_id: uuid::Uuid,
    operator_name: &str,
) -> Result<RefundResultRow, sqlx::Error> {
    let query = "SELECT * FROM process_refund($1, $2, $3, $4, $5, $6)";
    let row = sqlx::query(query)
        .bind(tenant_id)
        .bind(amount)
        .bind(reason)
        .bind(operator_id)
        .bind(operator_name)
        .bind(bill_id)
        .fetch_one(billing_pool)
        .await?;
    Ok(RefundResultRow {
        transaction_id: row.get::<uuid::Uuid, _>(0),
        account_id: row.get::<uuid::Uuid, _>(1),
        balance_before: row.get::<Decimal, _>(2),
        balance_after: row.get::<Decimal, _>(3),
    })
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_tenant_transactions(
    billing_pool: &PgPool,
    tenant_id: uuid::Uuid,
    page: i32,
    limit: i32,
    transaction_type: &Option<String>,
) -> Result<Vec<TransactionRow>, sqlx::Error> {
    let offset = (page - 1).max(0) * limit.max(1);
    let rows = sqlx::query(
        r"
        SELECT
            t.id,
            t.transaction_type,
            t.amount,
            t.balance_before,
            t.balance_after,
            t.description,
            t.payment_method,
            t.operator_id,
            t.operator_name,
            t.created_at,
            COUNT(*) OVER() AS total_count
        FROM transactions t
        WHERE t.tenant_id = $1
          AND (
            ($2::text IS NULL AND t.transaction_type IN ('credit', 'refund'))
            OR ($2::text IS NOT NULL AND t.transaction_type = $2::text)
          )
          AND t.status = 'completed'
        ORDER BY t.created_at DESC
        LIMIT $3
        OFFSET $4
        ",
    )
    .bind(tenant_id)
    .bind(transaction_type.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(billing_pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| TransactionRow {
            id: row.get::<uuid::Uuid, _>(0),
            transaction_type: row.get::<String, _>(1),
            amount: row.get::<Decimal, _>(2),
            balance_before: row.get::<Decimal, _>(3),
            balance_after: row.get::<Decimal, _>(4),
            description: row.get::<Option<String>, _>(5),
            payment_method: row.get::<Option<String>, _>(6),
            operator_id: row.get::<Option<uuid::Uuid>, _>(7),
            operator_name: row.get::<Option<String>, _>(8),
            created_at: row.get::<DateTime<Utc>, _>(9),
            total_count: row.get::<i64, _>(10),
        })
        .collect())
}
