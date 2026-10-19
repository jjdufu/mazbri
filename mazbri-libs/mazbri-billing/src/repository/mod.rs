use crate::service::{
    AddFundsResult, BillInfo, BillLineItem, UsageBreakdownItem, UsageOutboxEvent, UsageRecord,
    UsageSummaryInfo,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mazbri_core::CoreError;
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;
pub mod mapping;
pub mod queries;
pub mod transactions;
pub use mapping::AccountBalanceRow;
#[async_trait]
pub trait BillingRepository: Send + Sync + std::fmt::Debug {
    async fn get_account_id(&self, tenant_id: Uuid) -> Result<Option<Uuid>, CoreError>;
    async fn record_usage(&self, record: &UsageRecord) -> Result<i64, CoreError>;
    async fn insert_usage_outbox(&self, event: &UsageOutboxEvent) -> Result<(), CoreError>;
    async fn check_balance(&self, tenant_id: Uuid) -> Result<Option<AccountBalanceRow>, CoreError>;
    async fn deduct_balance(&self, tenant_id: Uuid, amount: Decimal) -> Result<Decimal, CoreError>;
    async fn get_balance(&self, tenant_id: Uuid) -> Result<Option<AccountBalanceRow>, CoreError>;
    async fn add_funds(
        &self,
        tenant_id: Uuid,
        amount: Decimal,
        payment_method: &str,
    ) -> Result<AddFundsResult, CoreError>;
    async fn get_bill(&self, tenant_id: Uuid, bill_id: Uuid)
    -> Result<Option<BillInfo>, CoreError>;
    async fn list_bills(
        &self,
        tenant_id: Uuid,
        page_size: i32,
        start_time: Option<i64>,
        end_time: Option<i64>,
        status: Option<&str>,
    ) -> Result<Vec<BillInfo>, CoreError>;
    async fn get_usage_summary(
        &self,
        tenant_id: Uuid,
        start_time: i64,
        end_time: i64,
    ) -> Result<UsageSummaryInfo, CoreError>;
    async fn get_bill_items(
        &self,
        tenant_id: Uuid,
        bill_id: Uuid,
    ) -> Result<Vec<BillLineItem>, CoreError>;
    async fn get_usage_breakdown(
        &self,
        tenant_id: Uuid,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<UsageBreakdownItem>, CoreError>;
    async fn generate_bill(
        &self,
        tenant_id: Uuid,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Result<Uuid, CoreError>;
    async fn account_exists(&self, tenant_id: Uuid) -> Result<bool, CoreError>;
    async fn deduct_and_record(
        &self,
        tenant_id: Uuid,
        amount: Decimal,
        account_id: Uuid,
        usage_record_id: i64,
        queue_id: Uuid,
    ) -> Result<DeductAndRecordResult, CoreError>;
    /// Batch charge for one tenant: single balance UPDATE for SUM(amount), then per-item ledger.
    /// `items` is (queue_id, usage_record_id, amount).
    async fn deduct_and_record_batch(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        items: &[(Uuid, i64, Decimal)],
    ) -> Result<DeductAndRecordResult, CoreError>;
    async fn create_adjustment(
        &self,
        tenant_id: Uuid,
        amount: Decimal,
        direction: &str,
        description: &str,
    ) -> Result<Uuid, CoreError>;
}
#[derive(Debug, Clone)]
pub struct DeductAndRecordResult {
    pub transaction_id: Uuid,
    pub balance_before: Decimal,
    pub balance_after: Decimal,
}
impl DeductAndRecordResult {
    pub fn new_balance(&self) -> Decimal {
        self.balance_after
    }
}
#[derive(Clone)]
pub struct PgBillingRepository {
    /// Write pool: charge workers, outbox, mutations.
    pool: Arc<PgPool>,
    /// Read pool: balance precheck / reports. May alias write pool when dual-pool is disabled.
    read_pool: Arc<PgPool>,
}
impl PgBillingRepository {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self {
            pool: Arc::clone(&pool),
            read_pool: pool,
        }
    }
    /// Separate read/write pools so balance precheck cannot starve behind charge UPDATEs.
    pub fn with_pools(write_pool: Arc<PgPool>, read_pool: Arc<PgPool>) -> Self {
        Self {
            pool: write_pool,
            read_pool,
        }
    }
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
    pub fn read_pool(&self) -> &PgPool {
        &self.read_pool
    }
    fn map_error(e: sqlx::Error) -> CoreError {
        CoreError::Database(e)
    }
}
impl std::fmt::Debug for PgBillingRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgBillingRepository")
            .field("pool", &"Arc<PgPool>")
            .finish()
    }
}
