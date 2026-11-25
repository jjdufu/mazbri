use super::{BillingError, DateTime, Decimal, PgPool, Row, Serialize, Utc, error, repo};

const CUSTOMER_BILL_SNAPSHOT_USAGE_SQL: &str = "SELECT
    api_endpoint,
    COUNT(*) as quantity,
    COALESCE(SUM(billable_units), 0)::BIGINT as total_units,
    COALESCE(MIN(unit_price), 0) as unit_price,
    COALESCE(SUM(billable_amount), 0) as total
 FROM usage_records
 WHERE tenant_id = $1
   AND recorded_at >= $2
   AND recorded_at < $3
   AND charge_status = 'charged'
   AND billable_amount > 0
 GROUP BY api_endpoint
 ORDER BY total DESC, api_endpoint ASC";

/// Atomically generates (or updates active non-cancelled) customer bill header AND snapshots
/// `bill_items` in a single PostgreSQL transaction using `charge_status = 'charged' AND billable_amount > 0`
/// over the half-open window `[period_start, period_end)`.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn generate_bill(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<uuid::Uuid, BillingError> {
    let billing_service = mazbri_billing::BillingService::with_pgpool(
        pool.clone().into(),
        mazbri_billing::BillingServiceConfig::default(),
    );
    billing_service
        .generate_bill(tenant_id, period_start, period_end)
        .await
        .map_err(|error| match error {
            mazbri_billing::BillingError::Core(mazbri_core::CoreError::Validation(msg)) => {
                BillingError::Validation(msg)
            }
            other => {
                error!(
                    "Database error generating bill for {}: {}",
                    tenant_id, other
                );
                BillingError::Database
            }
        })
}

/// Idempotent single-transaction snapshot helper for customer `bill_items`.
///
/// [`generate_bill`] already writes `bills` and `bill_items` atomically in one transaction;
/// when `bill_items` are already present (or `total_calls == 0`), this helper preserves that
/// exact atomic snapshot. If invoked standalone on a bill row without items, it snapshots
/// `usage_records` inside a single transaction using `charge_status = 'charged' AND billable_amount > 0`
/// and half-open `[period_start, period_end)`.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn snapshot_customer_bill_items(
    pool: &PgPool,
    bill_id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<(), BillingError> {
    if period_start >= period_end {
        return Err(BillingError::Validation(
            "Billing period start must be before end".to_string(),
        ));
    }
    let mut tx = pool.begin().await.map_err(|e| {
        error!(
            "Database error starting customer bill snapshot transaction: {}",
            e
        );
        BillingError::Database
    })?;

    let bill_meta = sqlx::query(
        "SELECT total_calls FROM bills WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(bill_id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        error!("Database error locking bill for snapshot {}: {}", bill_id, e);
        BillingError::Database
    })?;
    let Some(bill_meta) = bill_meta else {
        return Err(BillingError::BillNotFound(bill_id));
    };
    let total_calls: i64 = bill_meta.get("total_calls");
    let has_items: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM bill_items WHERE bill_id = $1)")
            .bind(bill_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                error!("Database error checking existing bill items: {}", e);
                BillingError::Database
            })?;
    if has_items || total_calls == 0 {
        tx.commit().await.map_err(|e| {
            error!("Database error committing snapshot check: {}", e);
            BillingError::Database
        })?;
        return Ok(());
    }

    let rows = sqlx::query(CUSTOMER_BILL_SNAPSHOT_USAGE_SQL)
        .bind(tenant_id)
        .bind(period_start)
        .bind(period_end)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| {
            error!("Database error listing customer bill items: {}", e);
            BillingError::Database
        })?;
    for (index, row) in rows.into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO bill_items (
                bill_id, item_key, description,
                quantity, total_units, unit_price, total, metadata
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(bill_id)
        .bind(format!("item-{}", index + 1))
        .bind(row.get::<String, _>("api_endpoint"))
        .bind(row.get::<i64, _>("quantity"))
        .bind(row.get::<i64, _>("total_units"))
        .bind(row.get::<Decimal, _>("unit_price"))
        .bind(row.get::<Decimal, _>("total"))
        .bind(serde_json::json!({ "source": "customer_usage" }))
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            error!("Database error inserting customer bill item: {}", e);
            BillingError::Database
        })?;
    }
    tx.commit().await.map_err(|e| {
        error!("Database error committing customer bill snapshot: {}", e);
        BillingError::Database
    })?;
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub(super) struct UpstreamBillItemDraft {
    pub(crate) supplier_code: String,
    pub(crate) supplier_name: String,
    pub(crate) channel_key: String,
    pub(crate) channel_code: String,
    pub(crate) channel_name: String,
    pub(crate) unit_price: Decimal,
    pub(crate) quantity: i64,
}
#[derive(Debug, Serialize)]
pub struct BillGenerationSummary {
    pub bill_id: uuid::Uuid,
    pub total_channels: usize,
    pub total_calls: i64,
    pub total_amount: Decimal,
}

#[derive(Debug, Serialize)]
pub struct ReconciliationAuditResult {
    pub discrepancy_id: uuid::Uuid,
    pub reconciliation_date: chrono::NaiveDate,
    pub scope_type: String,
    pub scope_id: String,
    pub billable_call_count: i64,
    pub upstream_preagg_call_count: i64,
    pub gateway_call_count: i64,
    /// analytics 拉取失败原因（`status = "error"` 时）。
    pub error: Option<String>,
    pub loss_ratio: Decimal,
    pub loss_ratio_threshold: Decimal,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct ReconciliationDiscrepancyListResponse {
    pub items: Vec<repo::ReconciliationDiscrepancyRow>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u64,
}

#[derive(Debug, Serialize)]
pub struct BillingAdjustmentListResponse {
    pub items: Vec<repo::BillingAdjustmentRow>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u64,
}

#[cfg(test)]
mod tests {
    use super::CUSTOMER_BILL_SNAPSHOT_USAGE_SQL;

    #[test]
    fn snapshot_sql_uses_charged_predicate_and_half_open_window() {
        assert!(CUSTOMER_BILL_SNAPSHOT_USAGE_SQL.contains("charge_status = 'charged'"));
        assert!(CUSTOMER_BILL_SNAPSHOT_USAGE_SQL.contains("billable_amount > 0"));
        assert!(CUSTOMER_BILL_SNAPSHOT_USAGE_SQL.contains("recorded_at >= $2"));
        assert!(CUSTOMER_BILL_SNAPSHOT_USAGE_SQL.contains("recorded_at < $3"));
        assert!(!CUSTOMER_BILL_SNAPSHOT_USAGE_SQL.contains("recorded_at <="));
    }
}
