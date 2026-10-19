use super::*;

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
pub async fn snapshot_customer_bill_items(
    pool: &PgPool,
    bill_id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<(), BillingError> {
    let rows = sqlx::query(
        "SELECT
            api_endpoint,
            billing_type,
            COUNT(*) as quantity,
            COALESCE(SUM(billable_units), 0)::BIGINT as total_units,
            COALESCE(MIN(unit_price), 0) as unit_price,
            COALESCE(SUM(billable_amount), 0) as total
         FROM usage_records
         WHERE tenant_id = $1
           AND recorded_at >= $2
           AND recorded_at <= $3
         GROUP BY api_endpoint, billing_type
         ORDER BY total DESC, api_endpoint ASC",
    )
    .bind(tenant_id)
    .bind(period_start)
    .bind(period_end)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!("Database error listing customer bill items: {}", e);
        BillingError::Database
    })?;
    let mut tx = pool.begin().await.map_err(|e| {
        error!(
            "Database error starting customer bill snapshot transaction: {}",
            e
        );
        BillingError::Database
    })?;
    sqlx::query("DELETE FROM bill_items WHERE bill_id = $1")
        .bind(bill_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            error!("Database error clearing customer bill items: {}", e);
            BillingError::Database
        })?;
    for (index, row) in rows.into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO bill_items (
                bill_id, item_key, description, billing_type,
                quantity, total_units, unit_price, total, metadata
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(bill_id)
        .bind(format!("item-{}", index + 1))
        .bind(row.get::<String, _>("api_endpoint"))
        .bind(row.get::<String, _>("billing_type"))
        .bind(row.get::<i64, _>("quantity"))
        .bind(row.get::<i64, _>("total_units"))
        .bind(row.get::<Decimal, _>("unit_price"))
        .bind(row.get::<Decimal, _>("total"))
        .bind(serde_json::json!({ "source" : "customer_usage" }))
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
    pub oltp_call_count: i64,
    pub olap_preagg_call_count: i64,
    pub raw_log_call_count: i64,
    pub raw_log_source: String,
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
