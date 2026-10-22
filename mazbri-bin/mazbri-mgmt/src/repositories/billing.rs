use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{PgPool, QueryBuilder, Row};
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BillRow {
    pub id: uuid::Uuid,
    pub tenant_id: Option<uuid::Uuid>,
    pub tenant_name: String,
    pub tenant_code: String,
    pub bill_kind: String,
    pub party_type: String,
    pub party_id: Option<uuid::Uuid>,
    pub party_code: String,
    pub party_name: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub status: String,
    pub subtotal: Decimal,
    pub total: Decimal,
    pub currency: String,
    pub total_calls: i64,
    pub total_data_bytes: i64,
    pub due_date: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
}
#[derive(Debug, Clone, Serialize)]
pub struct BillLineItemRow {
    pub item_id: String,
    pub description: String,
    pub quantity: i64,
    pub total_units: i64,
    pub unit_price: Decimal,
    pub total: Decimal,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AccountRow {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub status: String,
    pub balance: Decimal,
    pub credit_limit: Decimal,
    pub available_balance: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
const BILL_COLUMNS: &str = "\
    b.id, b.tenant_id, \
    b.party_name AS tenant_name, \
    b.party_code AS tenant_code, \
    b.bill_kind, \
    b.party_type, \
    b.party_id, \
    b.party_code, \
    b.party_name, \
    b.billing_period_start AS period_start, \
    b.billing_period_end AS period_end, \
    b.status, \
    b.base_amount AS subtotal, \
    b.total_amount AS total, \
    'CNY'::text AS currency, \
    b.total_calls, \
    b.total_data_bytes, \
    (b.billing_period_end + INTERVAL '15 days') AS due_date, \
    b.created_at, \
    b.updated_at, \
    b.paid_at";
fn map_bill_row(row: &sqlx::postgres::PgRow) -> BillRow {
    BillRow {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        tenant_name: row.get("tenant_name"),
        tenant_code: row.get("tenant_code"),
        bill_kind: row.get("bill_kind"),
        party_type: row.get("party_type"),
        party_id: row.get("party_id"),
        party_code: row.get("party_code"),
        party_name: row.get("party_name"),
        period_start: row.get("period_start"),
        period_end: row.get("period_end"),
        status: row.get("status"),
        subtotal: row.get("subtotal"),
        total: row.get("total"),
        currency: row.get("currency"),
        total_calls: row.get("total_calls"),
        total_data_bytes: row.get("total_data_bytes"),
        due_date: row.get("due_date"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        paid_at: row.get("paid_at"),
    }
}
fn push_bill_search_clause(
    builder: &mut QueryBuilder<'_, sqlx::Postgres>,
    pattern: &str,
    matched_tenant_ids: &[uuid::Uuid],
) {
    // Customer bills often leave party_name/code empty; list UI shows enriched
    // tenant name from platform DB. Match party fields + bill meta + resolved tenants.
    let pat = pattern.to_string();
    builder.push(" AND (b.id::text ILIKE ");
    builder.push_bind(pat.clone());
    builder.push(" OR COALESCE(b.tenant_id::text, '') ILIKE ");
    builder.push_bind(pat.clone());
    builder.push(" OR b.party_name ILIKE ");
    builder.push_bind(pat.clone());
    builder.push(" OR b.party_code ILIKE ");
    builder.push_bind(pat.clone());
    builder.push(" OR b.bill_kind ILIKE ");
    builder.push_bind(pat.clone());
    builder.push(" OR b.status ILIKE ");
    builder.push_bind(pat);
    if !matched_tenant_ids.is_empty() {
        builder.push(" OR b.tenant_id = ANY(");
        builder.push_bind(matched_tenant_ids.to_vec());
        builder.push(")");
    }
    builder.push(")");
}

fn push_bill_scope(
    builder: &mut QueryBuilder<'_, sqlx::Postgres>,
    tenant_id: Option<uuid::Uuid>,
    search: Option<&str>,
    matched_tenant_ids: &[uuid::Uuid],
    status: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<(), crate::list_query::ListQueryError> {
    if let Some(id) = tenant_id {
        builder.push(" AND b.tenant_id = ");
        builder.push_bind(id);
    }
    if let Some(pattern) = search {
        push_bill_search_clause(builder, pattern, matched_tenant_ids);
    }
    if let Some(s) = status {
        builder.push(" AND b.status = ");
        builder.push_bind(s.to_string());
    }
    crate::list_query::apply_pg_filters(builder, filters)
}

/// `list_bills` 的查询条件。
pub struct ListBillsQuery<'a> {
    pub tenant_id: Option<uuid::Uuid>,
    pub search: Option<&'a str>,
    pub matched_tenant_ids: &'a [uuid::Uuid],
    pub status: Option<&'a str>,
    pub filters: &'a [crate::list_query::ValidatedFilter],
    pub limit: i64,
    pub offset: i64,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_bills(
    pool: &PgPool,
    params: ListBillsQuery<'_>,
) -> Result<Vec<BillRow>, sqlx::Error> {
    let ListBillsQuery {
        tenant_id,
        search,
        matched_tenant_ids,
        status,
        filters,
        limit,
        offset,
    } = params;
    let mut builder: QueryBuilder<_> =
        QueryBuilder::new(format!("SELECT {BILL_COLUMNS} FROM bills b WHERE 1=1"));
    push_bill_scope(
        &mut builder,
        tenant_id,
        search,
        matched_tenant_ids,
        status,
        filters,
    )
    .map_err(|e| sqlx::Error::Protocol(e.message))?;
    builder.push(" ORDER BY b.created_at DESC, b.id DESC LIMIT ");
    builder.push_bind(limit);
    builder.push(" OFFSET ");
    builder.push_bind(offset);
    let rows = builder.build().fetch_all(pool).await?;
    Ok(rows.iter().map(map_bill_row).collect())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn count_bills(
    pool: &PgPool,
    tenant_id: Option<uuid::Uuid>,
    search: Option<&str>,
    matched_tenant_ids: &[uuid::Uuid],
    status: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<i64, sqlx::Error> {
    let mut builder: QueryBuilder<_> = QueryBuilder::new("SELECT COUNT(*) FROM bills b WHERE 1=1");
    push_bill_scope(
        &mut builder,
        tenant_id,
        search,
        matched_tenant_ids,
        status,
        filters,
    )
    .map_err(|e| sqlx::Error::Protocol(e.message))?;
    let row = builder.build().fetch_one(pool).await?;
    Ok(row.get(0))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_bill(pool: &PgPool, bill_id: uuid::Uuid) -> Result<Option<BillRow>, sqlx::Error> {
    let row = sqlx::query(&format!(
        "SELECT {BILL_COLUMNS} FROM bills b WHERE b.id = $1"
    ))
    .bind(bill_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(map_bill_row))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_bill(
    pool: &PgPool,
    bill_id: uuid::Uuid,
) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, uuid::Uuid>("DELETE FROM bills WHERE id = $1 RETURNING id")
        .bind(bill_id)
        .fetch_optional(pool)
        .await
}
/// Returns true if a row was updated.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_bill_status(
    pool: &PgPool,
    bill_id: uuid::Uuid,
    status: &str,
    paid_at: Option<DateTime<Utc>>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE bills
         SET status = $2,
             paid_at = $3,
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(bill_id)
    .bind(status)
    .bind(paid_at)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_bill_items(
    pool: &PgPool,
    bill: &BillRow,
) -> Result<Vec<BillLineItemRow>, sqlx::Error> {
    let snapshot_rows = sqlx::query(
        "SELECT item_key, description, quantity, total_units, unit_price, total
         FROM bill_items
         WHERE bill_id = $1
         ORDER BY id ASC",
    )
    .bind(bill.id)
    .fetch_all(pool)
    .await?;
    if !snapshot_rows.is_empty() {
        return Ok(snapshot_rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| BillLineItemRow {
                item_id: row
                    .get::<String, _>("item_key")
                    .if_empty(format!("item-{}", index + 1)),
                description: row.get("description"),
                quantity: row.get("quantity"),
                total_units: row.get("total_units"),
                unit_price: row.get("unit_price"),
                total: row.get("total"),
            })
            .collect());
    }
    if bill.bill_kind == "upstream" {
        return Ok(Vec::new());
    }
    let Some(tenant_id) = bill.tenant_id else {
        return Ok(Vec::new());
    };
    let rows = sqlx::query(
        "SELECT
            api_endpoint,
            COUNT(*) as quantity,
            COALESCE(SUM(billable_units), 0) as total_units,
            COALESCE(MIN(unit_price), 0) as unit_price,
            COALESCE(SUM(billable_amount), 0) as total
         FROM usage_records
         WHERE tenant_id = $1
           AND recorded_at >= $2
           AND recorded_at <= $3
         GROUP BY api_endpoint
         ORDER BY total DESC, api_endpoint ASC",
    )
    .bind(tenant_id)
    .bind(bill.period_start)
    .bind(bill.period_end)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| BillLineItemRow {
            item_id: format!("item-{}", index + 1),
            description: row.get("api_endpoint"),
            quantity: row.get("quantity"),
            total_units: row.get("total_units"),
            unit_price: row.get("unit_price"),
            total: row.get("total"),
        })
        .collect())
}
trait EmptyFallback {
    fn if_empty(self, fallback: String) -> String;
}
impl EmptyFallback for String {
    fn if_empty(self, fallback: String) -> String {
        if self.is_empty() { fallback } else { self }
    }
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_accounts(
    pool: &PgPool,
    tenant_id: Option<uuid::Uuid>,
    status: Option<&str>,
    cursor_created_at: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<Vec<AccountRow>, sqlx::Error> {
    sqlx::query_as(
            "SELECT id, tenant_id, status, balance, credit_limit, available_balance, created_at, updated_at \
         FROM tenant_accounts WHERE ($1::uuid IS NULL OR tenant_id = $1) AND ($2::text IS NULL OR status = $2) \
         AND ($3::timestamptz IS NULL OR created_at < $3) ORDER BY created_at DESC LIMIT $4",
        )
        .bind(tenant_id)
        .bind(status)
        .bind(cursor_created_at)
        .bind(limit)
        .fetch_all(pool)
        .await
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UpstreamHourlySettlementRow {
    pub id: uuid::Uuid,
    pub window_start: DateTime<Utc>,
    pub supplier_code: String,
    pub channel_code: String,
    pub total_calls: i64,
    pub successful_calls: i64,
    pub failed_calls: i64,
    pub total_bytes: i64,
    pub unit_price: Decimal,
    pub settlement_amount: Decimal,
    pub created_at: DateTime<Utc>,
}

pub struct UpsertHourlySettlementParams<'a> {
    pub window_start: DateTime<Utc>,
    pub supplier_code: &'a str,
    pub channel_code: &'a str,
    pub total_calls: i64,
    pub successful_calls: i64,
    pub failed_calls: i64,
    pub total_bytes: i64,
    pub unit_price: Decimal,
    pub settlement_amount: Decimal,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn upsert_hourly_settlement(
    pool: &PgPool,
    params: UpsertHourlySettlementParams<'_>,
) -> Result<uuid::Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO upstream_hourly_settlements (
            window_start, supplier_code, channel_code,
            total_calls, successful_calls, failed_calls, total_bytes,
            unit_price, settlement_amount
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (window_start, supplier_code, channel_code) DO UPDATE SET
            total_calls = EXCLUDED.total_calls,
            successful_calls = EXCLUDED.successful_calls,
            failed_calls = EXCLUDED.failed_calls,
            total_bytes = EXCLUDED.total_bytes,
            unit_price = EXCLUDED.unit_price,
            settlement_amount = EXCLUDED.settlement_amount
        RETURNING id",
    )
    .bind(params.window_start)
    .bind(params.supplier_code)
    .bind(params.channel_code)
    .bind(params.total_calls)
    .bind(params.successful_calls)
    .bind(params.failed_calls)
    .bind(params.total_bytes)
    .bind(params.unit_price)
    .bind(params.settlement_amount)
    .fetch_one(pool)
    .await
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BillingAdjustmentRow {
    pub id: uuid::Uuid,
    pub bill_id: Option<uuid::Uuid>,
    pub tenant_id: Option<uuid::Uuid>,
    pub adjustment_type: String,
    pub amount: Decimal,
    pub reason: String,
    pub reference_period_start: DateTime<Utc>,
    pub reference_period_end: DateTime<Utc>,
    pub status: String,
    pub operator_id: Option<uuid::Uuid>,
    pub created_at: DateTime<Utc>,
}

pub struct InsertBillingAdjustmentParams<'a> {
    pub bill_id: Option<uuid::Uuid>,
    pub tenant_id: Option<uuid::Uuid>,
    pub adjustment_type: &'a str,
    pub amount: Decimal,
    pub reason: &'a str,
    pub reference_period_start: DateTime<Utc>,
    pub reference_period_end: DateTime<Utc>,
    pub status: &'a str,
    pub operator_id: Option<uuid::Uuid>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_billing_adjustment(
    pool: &PgPool,
    params: InsertBillingAdjustmentParams<'_>,
) -> Result<uuid::Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO billing_adjustments (
            bill_id, tenant_id, adjustment_type, amount, reason,
            reference_period_start, reference_period_end, status, operator_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id",
    )
    .bind(params.bill_id)
    .bind(params.tenant_id)
    .bind(params.adjustment_type)
    .bind(params.amount)
    .bind(params.reason)
    .bind(params.reference_period_start)
    .bind(params.reference_period_end)
    .bind(params.status)
    .bind(params.operator_id)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_billing_adjustments_for_bill(
    pool: &PgPool,
    bill_id: uuid::Uuid,
) -> Result<Vec<BillingAdjustmentRow>, sqlx::Error> {
    sqlx::query_as::<_, BillingAdjustmentRow>(
        "SELECT id, bill_id, tenant_id, adjustment_type, amount, reason,
                reference_period_start, reference_period_end, status, operator_id, created_at
         FROM billing_adjustments
         WHERE bill_id = $1
         ORDER BY created_at DESC",
    )
    .bind(bill_id)
    .fetch_all(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_billing_adjustment(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<Option<BillingAdjustmentRow>, sqlx::Error> {
    sqlx::query_as::<_, BillingAdjustmentRow>(
        "SELECT id, bill_id, tenant_id, adjustment_type, amount, reason,
                reference_period_start, reference_period_end, status, operator_id, created_at
         FROM billing_adjustments
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn void_billing_adjustment(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<Option<BillingAdjustmentRow>, sqlx::Error> {
    sqlx::query_as::<_, BillingAdjustmentRow>(
        "UPDATE billing_adjustments
         SET status = 'void'
         WHERE id = $1 AND status = 'posted'
         RETURNING id, bill_id, tenant_id, adjustment_type, amount, reason,
                   reference_period_start, reference_period_end, status, operator_id, created_at",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub struct ListBillingAdjustmentsParams<'a> {
    pub bill_id: Option<uuid::Uuid>,
    pub tenant_id: Option<uuid::Uuid>,
    pub status: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn count_billing_adjustments(
    pool: &PgPool,
    params: &ListBillingAdjustmentsParams<'_>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM billing_adjustments
         WHERE ($1::uuid IS NULL OR bill_id = $1)
           AND ($2::uuid IS NULL OR tenant_id = $2)
           AND ($3::text IS NULL OR status = $3)",
    )
    .bind(params.bill_id)
    .bind(params.tenant_id)
    .bind(params.status)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_billing_adjustments(
    pool: &PgPool,
    params: ListBillingAdjustmentsParams<'_>,
) -> Result<Vec<BillingAdjustmentRow>, sqlx::Error> {
    sqlx::query_as::<_, BillingAdjustmentRow>(
        "SELECT id, bill_id, tenant_id, adjustment_type, amount, reason,
                reference_period_start, reference_period_end, status, operator_id, created_at
         FROM billing_adjustments
         WHERE ($1::uuid IS NULL OR bill_id = $1)
           AND ($2::uuid IS NULL OR tenant_id = $2)
           AND ($3::text IS NULL OR status = $3)
         ORDER BY created_at DESC
         LIMIT $4 OFFSET $5",
    )
    .bind(params.bill_id)
    .bind(params.tenant_id)
    .bind(params.status)
    .bind(params.limit)
    .bind(params.offset)
    .fetch_all(pool)
    .await
}

/// Sum of billable amounts in `usage_records` for a day window.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn sum_usage_billable_amount(
    pool: &PgPool,
    day_start: DateTime<Utc>,
    day_end: DateTime<Utc>,
) -> Result<Decimal, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(billable_amount), 0)::numeric
         FROM usage_records
         WHERE recorded_at >= $1 AND recorded_at <= $2",
    )
    .bind(day_start)
    .bind(day_end)
    .fetch_one(pool)
    .await
}

/// Sum of `bill_items.total_amount` for bills whose period covers the day.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn sum_bill_items_total_for_day(
    pool: &PgPool,
    day_start: DateTime<Utc>,
    day_end: DateTime<Utc>,
) -> Result<Decimal, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(bi.total), 0)::numeric
         FROM bill_items bi
         INNER JOIN bills b ON b.id = bi.bill_id
         WHERE b.billing_period_start <= $2
           AND b.billing_period_end >= $1",
    )
    .bind(day_start)
    .bind(day_end)
    .fetch_one(pool)
    .await
}

/// Open (flagged/acknowledged) discrepancies overlapping [from, to] dates.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn count_open_discrepancies_in_range(
    pool: &PgPool,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM reconciliation_discrepancies
         WHERE reconciliation_date >= $1
           AND reconciliation_date <= $2
           AND status IN ('flagged', 'acknowledged')",
    )
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ReconciliationDiscrepancyRow {
    pub id: uuid::Uuid,
    pub reconciliation_date: chrono::NaiveDate,
    pub scope_type: String,
    pub scope_id: String,
    pub oltp_call_count: i64,
    pub olap_preagg_call_count: i64,
    /// Historical/compat column; not an independent third source in current impl.
    #[serde(skip_serializing)]
    pub raw_log_call_count: i64,
    pub discrepancy_ratio: Decimal,
    pub status: String,
    pub details: Option<serde_json::Value>,
    pub resolution_note: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<uuid::Uuid>,
    pub created_at: DateTime<Utc>,
}

pub struct InsertReconciliationDiscrepancyParams<'a> {
    pub reconciliation_date: chrono::NaiveDate,
    pub scope_type: &'a str,
    pub scope_id: &'a str,
    pub oltp_call_count: i64,
    pub olap_preagg_call_count: i64,
    pub raw_log_call_count: i64,
    pub discrepancy_ratio: Decimal,
    pub status: &'a str,
    pub details: Option<serde_json::Value>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_reconciliation_discrepancy(
    pool: &PgPool,
    params: InsertReconciliationDiscrepancyParams<'_>,
) -> Result<uuid::Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO reconciliation_discrepancies (
            reconciliation_date, scope_type, scope_id,
            oltp_call_count, olap_preagg_call_count, raw_log_call_count,
            discrepancy_ratio, status, details
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id",
    )
    .bind(params.reconciliation_date)
    .bind(params.scope_type)
    .bind(params.scope_id)
    .bind(params.oltp_call_count)
    .bind(params.olap_preagg_call_count)
    .bind(params.raw_log_call_count)
    .bind(params.discrepancy_ratio)
    .bind(params.status)
    .bind(params.details)
    .fetch_one(pool)
    .await
}

pub struct ListReconciliationDiscrepanciesParams<'a> {
    pub from: Option<chrono::NaiveDate>,
    pub to: Option<chrono::NaiveDate>,
    pub status: Option<&'a str>,
    pub scope_type: Option<&'a str>,
    pub scope_id: Option<&'a str>,
    /// Free-text: id / date / status / scope / note (ILIKE).
    pub search: Option<&'a str>,
    pub filters: &'a [crate::list_query::ValidatedFilter],
    pub limit: i64,
    pub offset: i64,
}

fn push_recon_scope(
    builder: &mut QueryBuilder<'_, sqlx::Postgres>,
    params: &ListReconciliationDiscrepanciesParams<'_>,
) -> Result<(), crate::list_query::ListQueryError> {
    if let Some(from) = params.from {
        builder.push(" AND reconciliation_date >= ");
        builder.push_bind(from);
    }
    if let Some(to) = params.to {
        builder.push(" AND reconciliation_date <= ");
        builder.push_bind(to);
    }
    if let Some(status) = params.status {
        builder.push(" AND status = ");
        builder.push_bind(status.to_string());
    }
    if let Some(scope_type) = params.scope_type {
        builder.push(" AND scope_type = ");
        builder.push_bind(scope_type.to_string());
    }
    if let Some(scope_id) = params.scope_id {
        builder.push(" AND scope_id = ");
        builder.push_bind(scope_id.to_string());
    }
    if let Some(search) = params.search {
        let needle = search.to_string();
        let pat = format!("%{needle}%");
        builder.push(" AND (id::text ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR reconciliation_date::text ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR status ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR scope_type ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR COALESCE(scope_id, '') ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR COALESCE(resolution_note, '') ILIKE ");
        builder.push_bind(pat);
        builder.push(")");
    }
    crate::list_query::apply_pg_filters(builder, params.filters)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn count_reconciliation_discrepancies(
    pool: &PgPool,
    params: &ListReconciliationDiscrepanciesParams<'_>,
) -> Result<i64, sqlx::Error> {
    let mut builder: QueryBuilder<_> =
        QueryBuilder::new("SELECT COUNT(*)::BIGINT FROM reconciliation_discrepancies WHERE 1=1");
    push_recon_scope(&mut builder, params).map_err(|e| sqlx::Error::Protocol(e.message))?;
    Ok(builder.build().fetch_one(pool).await?.get(0))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_reconciliation_discrepancies(
    pool: &PgPool,
    params: ListReconciliationDiscrepanciesParams<'_>,
) -> Result<Vec<ReconciliationDiscrepancyRow>, sqlx::Error> {
    let mut builder: QueryBuilder<_> = QueryBuilder::new(
        "SELECT id, reconciliation_date, scope_type, scope_id, \
                oltp_call_count, olap_preagg_call_count, raw_log_call_count, \
                discrepancy_ratio, status, details, \
                resolution_note, resolved_at, resolved_by, created_at \
         FROM reconciliation_discrepancies WHERE 1=1",
    );
    push_recon_scope(&mut builder, &params).map_err(|e| sqlx::Error::Protocol(e.message))?;
    builder.push(" ORDER BY reconciliation_date DESC, created_at DESC LIMIT ");
    builder.push_bind(params.limit);
    builder.push(" OFFSET ");
    builder.push_bind(params.offset);
    builder.build_query_as().fetch_all(pool).await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_reconciliation_discrepancy(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<Option<ReconciliationDiscrepancyRow>, sqlx::Error> {
    sqlx::query_as::<_, ReconciliationDiscrepancyRow>(
        "SELECT id, reconciliation_date, scope_type, scope_id,
                oltp_call_count, olap_preagg_call_count, raw_log_call_count,
                discrepancy_ratio, status, details,
                resolution_note, resolved_at, resolved_by, created_at
         FROM reconciliation_discrepancies
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub struct UpdateReconciliationDiscrepancyParams<'a> {
    pub id: uuid::Uuid,
    pub status: &'a str,
    pub resolution_note: Option<&'a str>,
    pub resolved_by: Option<uuid::Uuid>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_reconciliation_discrepancy(
    pool: &PgPool,
    params: UpdateReconciliationDiscrepancyParams<'_>,
) -> Result<Option<ReconciliationDiscrepancyRow>, sqlx::Error> {
    sqlx::query_as::<_, ReconciliationDiscrepancyRow>(
        "UPDATE reconciliation_discrepancies
         SET status = $2,
             resolution_note = COALESCE($3, resolution_note),
             resolved_at = NOW(),
             resolved_by = $4
         WHERE id = $1
         RETURNING id, reconciliation_date, scope_type, scope_id,
                   oltp_call_count, olap_preagg_call_count, raw_log_call_count,
                   discrepancy_ratio, status, details,
                   resolution_note, resolved_at, resolved_by, created_at",
    )
    .bind(params.id)
    .bind(params.status)
    .bind(params.resolution_note)
    .bind(params.resolved_by)
    .fetch_optional(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_reconciliation_discrepancy(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, uuid::Uuid>(
        "DELETE FROM reconciliation_discrepancies WHERE id = $1 RETURNING id",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}
