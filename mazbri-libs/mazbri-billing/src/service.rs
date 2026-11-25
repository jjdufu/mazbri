use crate::{
    calculator::{BillingCalculator, CalculationResult, RateConfig},
    error::{BillingError, BillingResult},
    metrics::{BILLING_BALANCE_QUERIES_TOTAL, DeductionRecorder},
    queue::ChargeQueue,
    repository::{BillingRepository, DeductAndRecordResult, PgBillingRepository},
    types::{
        AdjustmentDirection, FencedChargeItem, IntoAdjustmentDirection, RecordUsageOutcome,
    },
};
use chrono::{DateTime, Utc};
use moka::sync::Cache;
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct BillingMiddlewareState {
    pub billing_service: Arc<BillingService>,
    pub charge_queue: Option<Arc<ChargeQueue>>,
}

impl BillingMiddlewareState {
    #[must_use]
    pub fn new(
        billing_service: Arc<BillingService>,
        charge_queue: Option<Arc<ChargeQueue>>,
    ) -> Self {
        Self {
            billing_service,
            charge_queue,
        }
    }

    #[must_use]
    pub fn without_queue(billing_service: Arc<BillingService>) -> Self {
        Self {
            billing_service,
            charge_queue: None,
        }
    }
}

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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    /// 网关 billing.enqueue span 的 W3C traceparent，计费 worker 据此挂 span link。
    pub trace_parent: Option<String>,
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
    rate_config_cache: Cache<String, RateConfig>,
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
    /// Lookup errors (e.g. pool acquire timeout) are NOT cached, so a transient DB
    /// failure no longer poisons the cache as "account not found" for the TTL.
    pub async fn get_account_id(&self, tenant_id: Uuid) -> Option<Uuid> {
        self.try_get_account_id(tenant_id).await.ok().flatten()
    }
    /// Like [`Self::get_account_id`] but distinguishes "not found" (`Ok(None)`) from
    /// a lookup failure (`Err`).
    ///
    /// # Errors
    ///
    /// 数据库访问失败（如连接池获取超时）时返回错误，且不写入缓存。
    pub async fn try_get_account_id(&self, tenant_id: Uuid) -> BillingResult<Option<Uuid>> {
        if let Some(account_id) = self.account_id_cache.get(&tenant_id) {
            return Ok(account_id);
        }
        let account_id = self
            .repo
            .get_account_id(tenant_id)
            .await
            .map_err(BillingError::from)?;
        self.account_id_cache.insert(tenant_id, account_id);
        Ok(account_id)
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
    /// Atomically persists `usage_records` (idempotent on mandatory `event_id`) and either
    /// enqueues into `async_queue` (`worker.enabled == true`) or synchronously executes
    /// fenced `deduct_and_record` (`worker.enabled == false`) in the same transaction.
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn record_usage_and_charge(
        &self,
        record: UsageRecord,
        async_queue: Option<&ChargeQueue>,
    ) -> BillingResult<RecordUsageOutcome> {
        if record.billing_result.amount < Decimal::ZERO {
            return Err(BillingError::InvalidAmount(format!(
                "Usage billable_amount must be non-negative, got {}",
                record.billing_result.amount
            )));
        }
        let amount = record.billing_result.amount;
        let recorder = (async_queue.is_none() && amount > Decimal::ZERO)
            .then(|| DeductionRecorder::new(amount));
        match self
            .repo
            .record_usage_and_charge(&record, async_queue)
            .await
            .map_err(|e| match e {
                mazbri_core::CoreError::Validation(msg) if msg.contains("Insufficient balance") => {
                    BillingError::InsufficientBalance {
                        required: amount,
                        available: Decimal::ZERO,
                    }
                }
                mazbri_core::CoreError::Validation(msg) => BillingError::InvalidAmount(msg),
                mazbri_core::CoreError::NotFound { .. } => {
                    BillingError::AccountNotFound(record.tenant_id)
                }
                other => BillingError::from(other),
            }) {
            Ok(outcome) => {
                if let Some(rec) = recorder {
                    rec.record_success();
                }
                Ok(outcome)
            }
            Err(error @ BillingError::InsufficientBalance { .. }) => {
                if let Some(rec) = recorder {
                    rec.record_insufficient();
                }
                Err(error)
            }
            Err(error) => {
                if let Some(rec) = recorder {
                    rec.record_failed();
                }
                Err(error)
            }
        }
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
        if estimated_cost < Decimal::ZERO {
            return Err(BillingError::InvalidAmount(format!(
                "estimated_cost must be non-negative, got {estimated_cost}"
            )));
        }
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
        if amount < Decimal::ZERO {
            return Err(BillingError::InvalidAmount(format!(
                "Deduction amount must be non-negative, got {amount}"
            )));
        }
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
                mazbri_core::CoreError::Validation(msg) => BillingError::InvalidAmount(msg),
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
        lease_seq: i64,
    ) -> BillingResult<DeductAndRecordResult> {
        if amount < Decimal::ZERO {
            return Err(BillingError::InvalidAmount(format!(
                "Charge amount must be non-negative, got {amount}"
            )));
        }
        let recorder = DeductionRecorder::new(amount);
        match self
            .repo
            .deduct_and_record(
                tenant_id,
                amount,
                account_id,
                usage_record_id,
                queue_id,
                lease_seq,
            )
            .await
            .map_err(|e| match e {
                mazbri_core::CoreError::Validation(msg) if msg.contains("Insufficient balance") => {
                    BillingError::InsufficientBalance {
                        required: amount,
                        available: Decimal::ZERO,
                    }
                }
                mazbri_core::CoreError::Validation(msg) => BillingError::InvalidAmount(msg),
                other => BillingError::from(other),
            }) {
            Ok(result) => {
                if result.charged_items > 0 {
                    recorder.record_success();
                }
                Ok(result)
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
    /// Batch charge for a single tenant (one balance UPDATE for the fenced sum).
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn deduct_and_record_batch(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        items: &[FencedChargeItem],
    ) -> BillingResult<DeductAndRecordResult> {
        for item in items {
            if item.amount < Decimal::ZERO {
                return Err(BillingError::InvalidAmount(format!(
                    "Batch charge item amount must be non-negative, got {}",
                    item.amount
                )));
            }
        }
        let total: Decimal = items.iter().map(|i| i.amount).sum();
        let recorder = DeductionRecorder::new(total);
        match self
            .repo
            .deduct_and_record_batch(tenant_id, account_id, items)
            .await
            .map_err(|e| match e {
                mazbri_core::CoreError::Validation(msg) if msg.contains("Insufficient balance") => {
                    BillingError::InsufficientBalance {
                        required: total,
                        available: Decimal::ZERO,
                    }
                }
                mazbri_core::CoreError::Validation(msg) => BillingError::InvalidAmount(msg),
                other => BillingError::from(other),
            }) {
            Ok(result) => {
                for _ in 0..result.charged_items {
                    recorder.record_success();
                }
                crate::metrics::BILLING_CHARGE_BATCH_TOTAL
                    .with_label_values(&["success"])
                    .inc();
                crate::metrics::BILLING_CHARGE_BATCH_ITEMS
                    .with_label_values(&["success"])
                    .inc_by(mazbri_core::num::usize_to_f64(result.charged_items));
                Ok(result)
            }
            Err(error @ BillingError::InsufficientBalance { .. }) => {
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
    /// Standard account adjustment (`allow_overdraft = false`: debit adjustments must keep
    /// `balance_after >= -credit_limit`).
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn create_adjustment(
        &self,
        tenant_id: Uuid,
        amount: Decimal,
        direction: impl IntoAdjustmentDirection,
        description: &str,
    ) -> BillingResult<Uuid> {
        let direction = direction.into_adjustment_direction()?;
        self.create_adjustment_with_options(tenant_id, amount, direction, false, description)
            .await
    }
    /// Account adjustment with explicit super-admin reversal overdraft control.
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn create_adjustment_with_options(
        &self,
        tenant_id: Uuid,
        amount: Decimal,
        direction: AdjustmentDirection,
        allow_overdraft: bool,
        description: &str,
    ) -> BillingResult<Uuid> {
        if amount <= Decimal::ZERO {
            return Err(BillingError::InvalidAmount(format!(
                "Adjustment amount must be greater than zero, got {amount}"
            )));
        }
        self.repo
            .create_adjustment(tenant_id, amount, direction, allow_overdraft, description)
            .await
            .map_err(|e| match e {
                mazbri_core::CoreError::Validation(msg) if msg.contains("Insufficient balance") => {
                    BillingError::InsufficientBalance {
                        required: amount,
                        available: Decimal::ZERO,
                    }
                }
                mazbri_core::CoreError::Validation(msg) => BillingError::InvalidAmount(msg),
                mazbri_core::CoreError::NotFound { .. } => BillingError::AccountNotFound(tenant_id),
                other => BillingError::from(other),
            })
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
        let rate_config = if let Some(pid) = product_id {
            self.load_tenant_rate_config(tenant_id, pid).await?
        } else {
            RateConfig::default()
        };
        Ok(BillingCalculator::new(rate_config))
    }
    async fn load_tenant_rate_config(
        &self,
        tenant_id: Uuid,
        product_id: &str,
    ) -> BillingResult<RateConfig> {
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
            r"SELECT COALESCE(tp.custom_config->>'unit_price', '0.001') as unit_price
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
            let per_call_rate = Decimal::from_str_exact(unit_price.trim()).map_err(|e| {
                BillingError::InvalidConfig(format!(
                    "tenant product unit_price '{unit_price}' is not a decimal: {e}"
                ))
            })?;
            if per_call_rate < Decimal::ZERO || per_call_rate.scale() > 6 {
                return Err(BillingError::InvalidConfig(format!(
                    "tenant product unit_price '{unit_price}' must be non-negative with scale <= 6"
                )));
            }
            let cached = RateConfig {
                per_call_rate,
                tier_name: "custom".to_string(),
            };
            self.rate_config_cache.insert(cache_key, cached.clone());
            Ok(cached)
        } else {
            let cached = RateConfig::default();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::AccountBalanceRow;
    use async_trait::async_trait;
    use mazbri_core::CoreError;
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::Mutex;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).expect("valid decimal")
    }

    #[derive(Debug, Default)]
    struct MockState {
        account_id: Uuid,
        balance: Decimal,
        credit_limit: Decimal,
        next_usage_id: i64,
        usage_by_event: HashMap<Uuid, (i64, Decimal, String)>,
        queue_rows: HashMap<Uuid, (i64, String, i64, Decimal)>, // queue_id -> (lease_seq, status, usage_id, amount)
        ledger_events: Vec<(Option<Uuid>, Decimal, String)>,
    }

    #[derive(Debug)]
    struct MockBillingRepo {
        tenant_id: Uuid,
        state: Mutex<MockState>,
    }

    impl MockBillingRepo {
        fn new(tenant_id: Uuid, balance: Decimal, credit_limit: Decimal) -> Self {
            Self {
                tenant_id,
                state: Mutex::new(MockState {
                    account_id: Uuid::now_v7(),
                    balance,
                    credit_limit,
                    next_usage_id: 1,
                    usage_by_event: HashMap::new(),
                    queue_rows: HashMap::new(),
                    ledger_events: Vec::new(),
                }),
            }
        }

        fn enqueue_and_claim(&self, event_id: Uuid, amount: Decimal) -> (Uuid, i64, i64) {
            let mut st = self.state.lock().expect("lock");
            let usage_id = st.next_usage_id;
            st.next_usage_id += 1;
            st.usage_by_event
                .insert(event_id, (usage_id, amount, "queued".to_string()));
            let qid = Uuid::now_v7();
            st.queue_rows
                .insert(qid, (1, "processing".to_string(), usage_id, amount));
            (qid, 1, usage_id)
        }

        fn reclaim_and_reclaim_by_worker_b(&self, queue_id: Uuid) -> i64 {
            let mut st = self.state.lock().expect("lock");
            let entry = st.queue_rows.get_mut(&queue_id).expect("queue row");
            entry.0 += 1;
            entry.1 = "processing".to_string();
            entry.0
        }

        fn try_mark_failed(&self, queue_id: Uuid, lease_seq: i64) -> bool {
            let mut st = self.state.lock().expect("lock");
            let Some(entry) = st.queue_rows.get_mut(&queue_id) else {
                return false;
            };
            if entry.1 != "processing" || entry.0 != lease_seq {
                return false;
            }
            entry.1 = "failed".to_string();
            true
        }
    }

    #[async_trait]
    impl BillingRepository for MockBillingRepo {
        async fn get_account_id(&self, tenant_id: Uuid) -> Result<Option<Uuid>, CoreError> {
            let st = self.state.lock().expect("lock");
            Ok((tenant_id == self.tenant_id).then_some(st.account_id))
        }

        async fn record_usage(&self, _record: &UsageRecord) -> Result<i64, CoreError> {
            let mut st = self.state.lock().expect("lock");
            let id = st.next_usage_id;
            st.next_usage_id += 1;
            Ok(id)
        }

        async fn record_usage_and_charge(
            &self,
            record: &UsageRecord,
            async_queue: Option<&ChargeQueue>,
        ) -> Result<RecordUsageOutcome, CoreError> {
            let ev = record
                .event_id
                .filter(|id| !id.is_nil())
                .ok_or_else(|| CoreError::Validation("missing event_id".into()))?;
            let mut st = self.state.lock().expect("lock");
            if let Some(&(uid, amt, ref status)) = st.usage_by_event.get(&ev)
                && status == "charged"
            {
                return Ok(RecordUsageOutcome::SyncCharged {
                    usage_record_id: uid,
                    charge_id: None,
                    charged: amt,
                    new_balance: st.balance,
                });
            }
            let uid = st.next_usage_id;
            st.next_usage_id += 1;
            let amt = record.billing_result.amount;
            if amt.is_zero() {
                st.usage_by_event
                    .insert(ev, (uid, amt, "pending".to_string()));
                return Ok(RecordUsageOutcome::NoCharge {
                    usage_record_id: uid,
                });
            }
            assert!(async_queue.is_none());
            if st.balance + st.credit_limit < amt {
                return Err(CoreError::Validation(format!(
                    "Insufficient balance: required {amt}"
                )));
            }
            st.balance -= amt;
            let new_balance = st.balance;
            st.usage_by_event
                .insert(ev, (uid, amt, "charged".to_string()));
            st.ledger_events.push((Some(ev), amt, "CHARGE".to_string()));
            Ok(RecordUsageOutcome::SyncCharged {
                usage_record_id: uid,
                charge_id: Some(Uuid::now_v7()),
                charged: amt,
                new_balance,
            })
        }

        async fn insert_usage_outbox(&self, _event: &UsageOutboxEvent) -> Result<(), CoreError> {
            Ok(())
        }

        async fn check_balance(
            &self,
            tenant_id: Uuid,
        ) -> Result<Option<AccountBalanceRow>, CoreError> {
            self.get_balance(tenant_id).await
        }

        async fn deduct_balance(
            &self,
            _tenant_id: Uuid,
            amount: Decimal,
        ) -> Result<Decimal, CoreError> {
            if amount < Decimal::ZERO {
                return Err(CoreError::Validation("negative debit".into()));
            }
            let mut st = self.state.lock().expect("lock");
            if st.balance + st.credit_limit < amount {
                return Err(CoreError::Validation("Insufficient balance".into()));
            }
            st.balance -= amount;
            Ok(st.balance)
        }

        async fn get_balance(
            &self,
            tenant_id: Uuid,
        ) -> Result<Option<AccountBalanceRow>, CoreError> {
            if tenant_id != self.tenant_id {
                return Ok(None);
            }
            let st = self.state.lock().expect("lock");
            Ok(Some(AccountBalanceRow {
                balance: st.balance,
                credit_limit: st.credit_limit,
                available_balance: st.balance + st.credit_limit,
                billing_cycle: Some("monthly".to_string()),
            }))
        }

        async fn get_bill(
            &self,
            _tenant_id: Uuid,
            _bill_id: Uuid,
        ) -> Result<Option<BillInfo>, CoreError> {
            Ok(None)
        }

        async fn list_bills(
            &self,
            _tenant_id: Uuid,
            _page_size: i32,
            _start_time: Option<i64>,
            _end_time: Option<i64>,
            _status: Option<&str>,
        ) -> Result<Vec<BillInfo>, CoreError> {
            Ok(Vec::new())
        }

        async fn get_usage_summary(
            &self,
            _tenant_id: Uuid,
            _start_time: i64,
            _end_time: i64,
        ) -> Result<UsageSummaryInfo, CoreError> {
            Ok(UsageSummaryInfo {
                total_requests: 0,
                total_bytes_in: 0,
                total_bytes_out: 0,
                total_billing_service_duration_ms: 0,
                total_cost: Decimal::ZERO,
            })
        }

        async fn get_bill_items(
            &self,
            _tenant_id: Uuid,
            _bill_id: Uuid,
        ) -> Result<Vec<BillLineItem>, CoreError> {
            Ok(Vec::new())
        }

        async fn get_usage_breakdown(
            &self,
            _tenant_id: Uuid,
            _start_time: i64,
            _end_time: i64,
        ) -> Result<Vec<UsageBreakdownItem>, CoreError> {
            Ok(Vec::new())
        }

        async fn generate_bill(
            &self,
            _tenant_id: Uuid,
            _period_start: DateTime<Utc>,
            _period_end: DateTime<Utc>,
        ) -> Result<Uuid, CoreError> {
            Ok(Uuid::now_v7())
        }

        async fn account_exists(&self, tenant_id: Uuid) -> Result<bool, CoreError> {
            Ok(tenant_id == self.tenant_id)
        }

        async fn deduct_and_record(
            &self,
            _tenant_id: Uuid,
            amount: Decimal,
            _account_id: Uuid,
            usage_record_id: i64,
            queue_id: Uuid,
            lease_seq: i64,
        ) -> Result<DeductAndRecordResult, CoreError> {
            let mut st = self.state.lock().expect("lock");
            let Some(q) = st.queue_rows.get_mut(&queue_id) else {
                return Ok(DeductAndRecordResult {
                    transaction_id: Uuid::nil(),
                    balance_before: st.balance,
                    balance_after: st.balance,
                    charged_items: 0,
                });
            };
            if q.1 != "processing" || q.0 != lease_seq {
                return Ok(DeductAndRecordResult {
                    transaction_id: Uuid::nil(),
                    balance_before: st.balance,
                    balance_after: st.balance,
                    charged_items: 0,
                });
            }
            q.1 = "completed".to_string();
            let before = st.balance;
            st.balance -= amount;
            let after = st.balance;
            let ev = Uuid::from_u64_pair(0x4d41_5a42_5249_4348, usage_record_id.cast_unsigned());
            st.ledger_events
                .push((Some(ev), amount, "CHARGE".to_string()));
            Ok(DeductAndRecordResult {
                transaction_id: Uuid::now_v7(),
                balance_before: before,
                balance_after: after,
                charged_items: 1,
            })
        }

        async fn deduct_and_record_batch(
            &self,
            tenant_id: Uuid,
            account_id: Uuid,
            items: &[FencedChargeItem],
        ) -> Result<DeductAndRecordResult, CoreError> {
            let mut last = DeductAndRecordResult {
                transaction_id: Uuid::nil(),
                balance_before: Decimal::ZERO,
                balance_after: Decimal::ZERO,
                charged_items: 0,
            };
            let mut total_charged = 0usize;
            for item in items {
                let res = self
                    .deduct_and_record(
                        tenant_id,
                        item.amount,
                        account_id,
                        item.usage_record_id,
                        item.queue_id,
                        item.lease_seq,
                    )
                    .await?;
                total_charged += res.charged_items;
                last = res;
            }
            last.charged_items = total_charged;
            Ok(last)
        }

        async fn create_adjustment(
            &self,
            _tenant_id: Uuid,
            amount: Decimal,
            direction: AdjustmentDirection,
            allow_overdraft: bool,
            _description: &str,
        ) -> Result<Uuid, CoreError> {
            if amount <= Decimal::ZERO {
                return Err(CoreError::Validation("non-positive adjustment".into()));
            }
            let mut st = self.state.lock().expect("lock");
            match direction {
                AdjustmentDirection::Credit => st.balance += amount,
                AdjustmentDirection::Debit => {
                    let after = st.balance - amount;
                    if !allow_overdraft && after < -st.credit_limit {
                        return Err(CoreError::Validation("Insufficient balance".into()));
                    }
                    st.balance = after;
                }
            }
            st.ledger_events
                .push((None, amount, direction.as_str().to_string()));
            Ok(Uuid::now_v7())
        }
    }

    #[tokio::test]
    async fn concurrent_fencing_simulation_charges_once_and_preserves_completed_state() {
        let tenant_id = Uuid::now_v7();
        let repo = Arc::new(MockBillingRepo::new(
            tenant_id,
            dec("100.000123"),
            dec("5.000000"),
        ));
        let svc = BillingService::new(repo.clone(), BillingServiceConfig::default());
        let account_id = svc.get_account_id(tenant_id).await.expect("account_id");

        // Worker A claims lease_seq = 1 and stalls
        let event_id = Uuid::now_v7();
        let (queue_id, lease_a, usage_id) = repo.enqueue_and_claim(event_id, dec("0.000040"));
        assert_eq!(lease_a, 1);

        // Reclaim happens; Worker B claims lease_seq = 2 and commits charge
        let lease_b = repo.reclaim_and_reclaim_by_worker_b(queue_id);
        assert_eq!(lease_b, 2);
        let res_b = svc
            .deduct_and_record(
                tenant_id,
                dec("0.000040"),
                account_id,
                usage_id,
                queue_id,
                lease_b,
            )
            .await
            .expect("worker B succeeds");
        assert_eq!(res_b.charged_items, 1);
        assert_eq!(res_b.balance_after, dec("100.000083"));

        // Worker A wakes up and attempts deduct_and_record(lease_seq = 1) + mark_failed(lease_seq = 1)
        let res_a = svc
            .deduct_and_record(
                tenant_id,
                dec("0.000040"),
                account_id,
                usage_id,
                queue_id,
                lease_a,
            )
            .await
            .expect("worker A fenced no-op");
        assert_eq!(res_a.charged_items, 0);
        assert!(!repo.try_mark_failed(queue_id, lease_a));

        // Balance deducted exactly once, 1 ledger entry, terminal state remains 'completed'
        let bal = svc.get_balance(tenant_id).await.expect("balance");
        assert_eq!(bal.balance, dec("100.000083"));
        let st = repo.state.lock().expect("lock");
        assert_eq!(st.ledger_events.len(), 1);
        assert_eq!(st.queue_rows[&queue_id].1, "completed");
    }

    #[tokio::test]
    async fn adjustments_and_sync_record_usage_enforce_invariants() {
        let tenant_id = Uuid::now_v7();
        let repo = Arc::new(MockBillingRepo::new(
            tenant_id,
            dec("1.000000"),
            dec("2.000000"),
        ));
        let svc = BillingService::new(repo.clone(), BillingServiceConfig::default());
        let account_id = svc.get_account_id(tenant_id).await.expect("account");

        // Negative deduction rejected
        assert!(matches!(
            svc.deduct_balance(tenant_id, dec("-0.01")).await,
            Err(BillingError::InvalidAmount(_))
        ));

        // Zero or negative adjustment rejected; invalid direction string rejected
        assert!(matches!(
            svc.create_adjustment(tenant_id, Decimal::ZERO, AdjustmentDirection::Credit, "x")
                .await,
            Err(BillingError::InvalidAmount(_))
        ));
        assert!(matches!(
            svc.create_adjustment(tenant_id, dec("1"), "INVALID", "x")
                .await,
            Err(BillingError::InvalidConfig(_))
        ));

        // Debit beyond -credit_limit (-2.000000) rejected without super-admin override
        assert!(matches!(
            svc.create_adjustment(tenant_id, dec("3.000001"), AdjustmentDirection::Debit, "x")
                .await,
            Err(BillingError::InsufficientBalance { .. })
        ));

        // Super-admin reversal override succeeds
        svc.create_adjustment_with_options(
            tenant_id,
            dec("3.000001"),
            AdjustmentDirection::Debit,
            true,
            "super-admin reversal",
        )
        .await
        .expect("reversal succeeds");
        assert_eq!(
            svc.get_balance(tenant_id).await.expect("bal").balance,
            dec("-2.000001")
        );

        // Credit back to positive and test idempotent sync record_usage_and_charge
        svc.create_adjustment(tenant_id, dec("5.000001"), "CREDIT", "topup")
            .await
            .expect("credit");
        let ev = Uuid::now_v7();
        let rec = UsageRecord {
            event_id: Some(ev),
            tenant_id,
            account_id,
            api_endpoint: "/v1/check".to_string(),
            http_method: "POST".to_string(),
            response_status: 200,
            request_size: 10,
            response_size: 20,
            billing_service_duration_ms: 3,
            billing_result: CalculationResult {
                amount: dec("0.001000"),
                units: 1,
                unit_price: dec("0.001000"),
                description: "1 call".to_string(),
            },
            product_id: Some("8000000001".to_string()),
            billing_code: Some(0),
        };
        let first = svc
            .record_usage_and_charge(rec.clone(), None)
            .await
            .expect("first sync charge");
        let second = svc
            .record_usage_and_charge(rec, None)
            .await
            .expect("idempotent retry");
        assert_eq!(
            svc.get_balance(tenant_id).await.expect("bal").balance,
            dec("2.999000")
        );
        match (first, second) {
            (
                RecordUsageOutcome::SyncCharged {
                    new_balance: b1, ..
                },
                RecordUsageOutcome::SyncCharged {
                    new_balance: b2, ..
                },
            ) => assert_eq!(b1, b2),
            other => panic!("unexpected outcomes: {other:?}"),
        }
    }
}
