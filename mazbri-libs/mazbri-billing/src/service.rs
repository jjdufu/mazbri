use crate::{
    calculator::{BillingCalculator, BillingModel, CalculationResult, RateConfig},
    error::{BillingError, BillingResult},
    metrics::{BILLING_BALANCE_QUERIES_TOTAL, BILLING_RECHARGES_TOTAL, DeductionRecorder},
    repository::{BillingRepository, DeductAndRecordResult, PgBillingRepository},
};
use chrono::{DateTime, Utc};
use moka::sync::Cache;
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;
#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub event_id: Option<Uuid>,
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub api_endpoint: String,
    pub http_method: String,
    pub response_status: u16,
    pub request_size: i64,
    pub response_size: i64,
    pub billing_service_duration_ms: i32,
    pub billing_result: CalculationResult,
    pub product_id: Option<String>,
    pub billing_code: Option<i16>,
}
#[derive(Debug, Clone)]
pub struct UsageOutboxEvent {
    pub event_id: Uuid,
    pub request_id: Uuid,
    pub tenant_id: Uuid,
    pub account_id: Option<Uuid>,
    pub product_id: Option<String>,
    pub api_endpoint: String,
    pub http_method: String,
    pub response_status: u16,
    pub request_size: i64,
    pub response_size: i64,
    pub billing_service_duration_ms: i32,
    pub billing_result: CalculationResult,
    pub billing_code: Option<i16>,
}
#[derive(Debug, Clone)]
pub struct BillingServiceConfig {
    pub auto_balance_check: bool,
    pub min_balance_ratio: Decimal,
}
impl Default for BillingServiceConfig {
    fn default() -> Self {
        Self {
            auto_balance_check: true,
            min_balance_ratio: Decimal::from_str_exact("0.1")
                .expect("默认最小余额比例是硬编码的有效值"),
        }
    }
}
pub struct BillingService {
    repo: Arc<dyn BillingRepository>,
    pool: Option<Arc<PgPool>>,
    config: BillingServiceConfig,
    account_id_cache: Cache<Uuid, Option<Uuid>>,
    rate_config_cache: Cache<String, (RateConfig, BillingModel)>,
}
impl std::fmt::Debug for BillingService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BillingService")
            .field("repo", &"Arc<dyn BillingRepository>")
            .field("pool", &self.pool.as_ref().map(|_| "Arc<PgPool>"))
            .field("config", &self.config)
            .field(
                "account_id_cache_entries",
                &self.account_id_cache.entry_count(),
            )
            .field(
                "rate_config_cache_entries",
                &self.rate_config_cache.entry_count(),
            )
            .finish()
    }
}
impl Clone for BillingService {
    fn clone(&self) -> Self {
        Self {
            repo: self.repo.clone(),
            pool: self.pool.clone(),
            config: self.config.clone(),
            account_id_cache: self.account_id_cache.clone(),
            rate_config_cache: self.rate_config_cache.clone(),
        }
    }
}
impl BillingService {
    pub fn new(repo: Arc<dyn BillingRepository>, config: BillingServiceConfig) -> Self {
        Self {
            repo,
            pool: None,
            config,
            account_id_cache: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(5))
                .build(),
            rate_config_cache: Cache::builder()
                .max_capacity(100_000)
                .time_to_live(Duration::from_secs(5))
                .build(),
        }
    }
    #[must_use]
    pub fn with_pgpool(pool: Arc<PgPool>, config: BillingServiceConfig) -> Self {
        Self::with_pgpools(Arc::clone(&pool), pool, config)
    }
    /// Dual-pool constructor: `read_pool` serves balance precheck; `write_pool` serves charges.
    #[must_use]
    pub fn with_pgpools(
        write_pool: Arc<PgPool>,
        read_pool: Arc<PgPool>,
        config: BillingServiceConfig,
    ) -> Self {
        Self {
            repo: Arc::new(PgBillingRepository::with_pools(
                Arc::clone(&write_pool),
                read_pool,
            )),
            pool: Some(write_pool),
            config,
            account_id_cache: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(5))
                .build(),
            rate_config_cache: Cache::builder()
                .max_capacity(100_000)
                .time_to_live(Duration::from_secs(5))
                .build(),
        }
    }
    #[must_use]
    pub fn db(&self) -> Option<&PgPool> {
        self.pool.as_deref()
    }
    pub async fn get_account_id(&self, tenant_id: Uuid) -> Option<Uuid> {
        if let Some(account_id) = self.account_id_cache.get(&tenant_id) {
            return account_id;
        }
        let account_id = self.repo.get_account_id(tenant_id).await.ok().flatten();
        self.account_id_cache.insert(tenant_id, account_id);
        account_id
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn record_usage(&self, record: UsageRecord) -> BillingResult<i64> {
        self.repo
            .record_usage(&record)
            .await
            .map_err(BillingError::from)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn enqueue_usage_outbox(&self, event: UsageOutboxEvent) -> BillingResult<()> {
        self.repo
            .insert_usage_outbox(&event)
            .await
            .map_err(BillingError::from)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn check_balance(
        &self,
        tenant_id: Uuid,
        estimated_cost: Decimal,
    ) -> BillingResult<bool> {
        let row = match self
            .repo
            .check_balance(tenant_id)
            .await
            .map_err(BillingError::from)
        {
            Ok(Some(row)) => row,
            Ok(None) => {
                BILLING_BALANCE_QUERIES_TOTAL
                    .with_label_values(&["not_found"])
                    .inc();
                return Err(BillingError::AccountNotFound(tenant_id));
            }
            Err(error) => {
                BILLING_BALANCE_QUERIES_TOTAL
                    .with_label_values(&["error"])
                    .inc();
                return Err(error);
            }
        };
        let min_required = estimated_cost * self.config.min_balance_ratio;
        let sufficient = row.available_balance >= estimated_cost + min_required;
        BILLING_BALANCE_QUERIES_TOTAL
            .with_label_values(&[if sufficient {
                "sufficient"
            } else {
                "insufficient"
            }])
            .inc();
        Ok(sufficient)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn deduct_balance(&self, tenant_id: Uuid, amount: Decimal) -> BillingResult<Decimal> {
        let recorder = DeductionRecorder::new(amount);
        match self
            .repo
            .deduct_balance(tenant_id, amount)
            .await
            .map_err(|e| match e {
                mazbri_core::CoreError::Validation(msg) if msg.contains("Insufficient balance") => {
                    BillingError::InsufficientBalance {
                        required: amount,
                        available: Decimal::ZERO,
                    }
                }
                mazbri_core::CoreError::NotFound { .. } => BillingError::AccountNotFound(tenant_id),
                other => BillingError::from(other),
            }) {
            Ok(balance) => {
                recorder.record_success();
                Ok(balance)
            }
            Err(error @ BillingError::InsufficientBalance { .. }) => {
                recorder.record_insufficient();
                Err(error)
            }
            Err(error) => {
                recorder.record_failed();
                Err(error)
            }
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_balance(&self, tenant_id: Uuid) -> BillingResult<AccountBalance> {
        let row = match self
            .repo
            .get_balance(tenant_id)
            .await
            .map_err(BillingError::from)
        {
            Ok(Some(row)) => {
                BILLING_BALANCE_QUERIES_TOTAL
                    .with_label_values(&["success"])
                    .inc();
                row
            }
            Ok(None) => {
                BILLING_BALANCE_QUERIES_TOTAL
                    .with_label_values(&["not_found"])
                    .inc();
                return Err(BillingError::AccountNotFound(tenant_id));
            }
            Err(error) => {
                BILLING_BALANCE_QUERIES_TOTAL
                    .with_label_values(&["error"])
                    .inc();
                return Err(error);
            }
        };
        Ok(row.into())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn add_funds(
        &self,
        tenant_id: Uuid,
        amount: Decimal,
        payment_method: &str,
    ) -> BillingResult<AddFundsResult> {
        match self
            .repo
            .add_funds(tenant_id, amount, payment_method)
            .await
            .map_err(|e| match e {
                mazbri_core::CoreError::Validation(msg) => BillingError::InvalidAmount(msg),
                mazbri_core::CoreError::NotFound { .. } => BillingError::AccountNotFound(tenant_id),
                other => BillingError::from(other),
            }) {
            Ok(result) => {
                BILLING_RECHARGES_TOTAL
                    .with_label_values(&[payment_method])
                    .inc();
                Ok(result)
            }
            Err(error) => Err(error),
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_bill(
        &self,
        tenant_id: Uuid,
        bill_id: Uuid,
    ) -> BillingResult<Option<BillInfo>> {
        self.repo
            .get_bill(tenant_id, bill_id)
            .await
            .map_err(BillingError::from)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn list_bills(
        &self,
        tenant_id: Uuid,
        page_size: i32,
        start_time: Option<i64>,
        end_time: Option<i64>,
        status: Option<&str>,
    ) -> BillingResult<Vec<BillInfo>> {
        self.repo
            .list_bills(tenant_id, page_size, start_time, end_time, status)
            .await
            .map_err(BillingError::from)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_usage_summary(
        &self,
        tenant_id: Uuid,
        start_time: i64,
        end_time: i64,
    ) -> BillingResult<UsageSummaryInfo> {
        self.repo
            .get_usage_summary(tenant_id, start_time, end_time)
            .await
            .map_err(BillingError::from)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_bill_items(
        &self,
        tenant_id: Uuid,
        bill_id: Uuid,
    ) -> BillingResult<Vec<BillLineItem>> {
        self.repo
            .get_bill_items(tenant_id, bill_id)
            .await
            .map_err(|e| match e {
                mazbri_core::CoreError::NotFound { id, .. } => {
                    BillingError::BillNotFound(Uuid::parse_str(&id).unwrap_or(bill_id))
                }
                other => BillingError::from(other),
            })
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_usage_breakdown(
        &self,
        tenant_id: Uuid,
        start_time: i64,
        end_time: i64,
    ) -> BillingResult<Vec<UsageBreakdownItem>> {
        self.repo
            .get_usage_breakdown(tenant_id, start_time, end_time)
            .await
            .map_err(BillingError::from)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn generate_bill(
        &self,
        tenant_id: Uuid,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> BillingResult<Uuid> {
        self.repo
            .generate_bill(tenant_id, period_start, period_end)
            .await
            .map_err(BillingError::from)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn deduct_and_record(
        &self,
        tenant_id: Uuid,
        amount: Decimal,
        account_id: Uuid,
        usage_record_id: i64,
        queue_id: Uuid,
    ) -> BillingResult<DeductAndRecordResult> {
        let recorder = DeductionRecorder::new(amount);
        match self
            .repo
            .deduct_and_record(tenant_id, amount, account_id, usage_record_id, queue_id)
            .await
            .map_err(BillingError::from)
        {
            Ok(result) => {
                recorder.record_success();
                Ok(result)
            }
            Err(error) if error.to_string().contains("Insufficient balance") => {
                recorder.record_insufficient();
                Err(error)
            }
            Err(error) => {
                recorder.record_failed();
                Err(error)
            }
        }
    }
    /// Batch charge for a single tenant (one balance UPDATE for the sum).
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn deduct_and_record_batch(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        items: &[(Uuid, i64, Decimal)],
    ) -> BillingResult<DeductAndRecordResult> {
        let total: Decimal = items.iter().map(|(_, _, a)| *a).sum();
        let recorder = DeductionRecorder::new(total);
        match self
            .repo
            .deduct_and_record_batch(tenant_id, account_id, items)
            .await
            .map_err(BillingError::from)
        {
            Ok(result) => {
                // Count each item as a successful deduction for metrics continuity.
                for _ in items {
                    recorder.record_success();
                }
                crate::metrics::BILLING_CHARGE_BATCH_TOTAL
                    .with_label_values(&["success"])
                    .inc();
                crate::metrics::BILLING_CHARGE_BATCH_ITEMS
                    .with_label_values(&["success"])
                    .inc_by(mazbri_core::num::usize_to_f64(items.len()));
                Ok(result)
            }
            Err(error) if error.to_string().contains("Insufficient balance") => {
                recorder.record_insufficient();
                crate::metrics::BILLING_CHARGE_BATCH_TOTAL
                    .with_label_values(&["insufficient"])
                    .inc();
                Err(error)
            }
            Err(error) => {
                recorder.record_failed();
                crate::metrics::BILLING_CHARGE_BATCH_TOTAL
                    .with_label_values(&["failed"])
                    .inc();
                Err(error)
            }
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn create_adjustment(
        &self,
        tenant_id: Uuid,
        amount: Decimal,
        direction: &str,
        description: &str,
    ) -> BillingResult<Uuid> {
        self.repo
            .create_adjustment(tenant_id, amount, direction, description)
            .await
            .map_err(BillingError::from)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_calculator(
        &self,
        tenant_id: Uuid,
        product_id: Option<&str>,
    ) -> BillingResult<BillingCalculator> {
        let Some(_) = self.get_account_id(tenant_id).await else {
            return Err(BillingError::AccountNotFound(tenant_id));
        };
        let (rate_config, billing_model) = if let Some(pid) = product_id {
            self.load_tenant_rate_config(tenant_id, pid).await?
        } else {
            (RateConfig::default(), BillingModel::default())
        };
        Ok(BillingCalculator::new(rate_config, billing_model))
    }
    async fn load_tenant_rate_config(
        &self,
        tenant_id: Uuid,
        product_id: &str,
    ) -> BillingResult<(RateConfig, BillingModel)> {
        let cache_key = format!("{tenant_id}:{product_id}");
        if let Some(cached) = self.rate_config_cache.get(&cache_key) {
            return Ok(cached);
        }
        let pool = self.db().ok_or_else(|| {
            BillingError::Core(mazbri_core::CoreError::Validation(
                "No database pool available".to_string(),
            ))
        })?;
        let row = sqlx::query(
            r"SELECT COALESCE(tp.custom_config->>'unit_price', '0.001') as unit_price,
                      COALESCE(tp.custom_config->>'billing_type', 'call') as billing_type
               FROM tenant_products tp
               JOIN products p ON tp.product_id = p.id
               WHERE tp.tenant_id = $1 AND p.product_id = $2 AND tp.enabled = true",
        )
        .bind(tenant_id)
        .bind(product_id)
        .fetch_optional(pool)
        .await
        .map_err(BillingError::from)?;
        if let Some(row) = row {
            let unit_price: String = row.get("unit_price");
            let billing_type: String = row.get("billing_type");
            let rate_config = RateConfig {
                per_call_rate: unit_price.parse::<Decimal>().unwrap_or_else(|_| {
                    Decimal::from_str_exact(RateConfig::DEFAULT_PER_CALL_RATE)
                        .expect("default per-call rate constant is valid")
                }),
                tier_name: "custom".to_string(),
            };
            // Non-call legacy values are coerced to per-call inside from_billing_type.
            let billing_model = BillingModel::from_billing_type(&billing_type);
            let cached = (rate_config, billing_model);
            self.rate_config_cache.insert(cache_key, cached.clone());
            Ok(cached)
        } else {
            let cached = (RateConfig::default(), BillingModel::default());
            self.rate_config_cache.insert(cache_key, cached.clone());
            Ok(cached)
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct AccountBalance {
    pub balance: Decimal,
    pub credit_limit: Decimal,
    pub available: Decimal,
    pub billing_cycle: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct BillInfo {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub status: String,
    pub total_amount: Decimal,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
}
#[derive(Debug, Clone, Serialize)]
pub struct UsageSummaryInfo {
    pub total_requests: i64,
    pub total_bytes_in: i64,
    pub total_bytes_out: i64,
    pub total_billing_service_duration_ms: i64,
    pub total_cost: Decimal,
}
#[derive(Debug, Clone, Serialize)]
pub struct BillLineItem {
    pub api_endpoint: String,
    pub billing_type: String,
    pub call_count: i64,
    pub total_units: i64,
    pub unit_price: Decimal,
    pub total_amount: Decimal,
}
#[derive(Debug, Clone, Serialize)]
pub struct UsageBreakdownItem {
    pub category: String,
    pub request_count: i64,
    pub cost: Decimal,
    pub percentage: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct AddFundsResult {
    pub transaction_id: Uuid,
    pub new_balance: Decimal,
    pub processed_at: i64,
}
