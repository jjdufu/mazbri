use mazbri_core::metrics::{MetricsBuilder, REGISTRY, register_collector_once};
use prometheus::Error as PrometheusError;
use prometheus::{CounterVec, HistogramVec, IntGaugeVec};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::sync::LazyLock;
use tracing::warn;
pub static BILLING_BALANCE_QUERIES_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "billing_balance_queries_total",
        "Total number of balance query operations",
        &["result"],
    )
});
pub static BILLING_DEDUCTIONS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "billing_deductions_total",
        "Total number of deduction operations",
        &["result"],
    )
});
pub static BILLING_DEDUCTION_AMOUNTS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "billing_deduction_amounts",
        "Distribution of deduction amounts",
        vec![0.001, 0.01, 0.1, 1.0, 10.0, 100.0, 1000.0],
        &[],
    )
});
pub static BILLING_RECHARGES_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "billing_recharges_total",
        "Total number of recharge operations by payment method",
        &["method"],
    )
});
/// 计费链路积压：`stage`=outbox|queue，统计 pending/processing/failed（待重试）条数。
pub static BILLING_OUTBOX_PENDING: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    MetricsBuilder::new().gauge_vec(
        "billing_outbox_pending",
        "Unprocessed billing pipeline rows (pending, processing, failed awaiting retry) by stage",
        &["stage"],
    )
});
/// 最老未处理记录年龄（秒，`now() - created_at`），无积压时为 0。
pub static BILLING_OLDEST_PENDING_AGE_SECONDS: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    MetricsBuilder::new().gauge_vec(
        "billing_oldest_pending_age_seconds",
        "Age in seconds of the oldest unprocessed billing pipeline row by stage",
        &["stage"],
    )
});
/// 死信条数（`status = 'dead_letter'`）。
pub static BILLING_DEAD_LETTERS: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    MetricsBuilder::new().gauge_vec(
        "billing_dead_letters",
        "Dead-lettered billing pipeline rows by stage",
        &["stage"],
    )
});
/// 扣费失败（单条扣费路径），`reason` 取 [`ChargeFailureReason`]。
pub static BILLING_CHARGE_FAILURES_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "billing_charge_failures_total",
        "Charge queue item failures by bounded reason",
        &["reason"],
    )
});
/// 请求入队（`usage_charge_queue.created_at`）到入账完成的延迟。
pub static BILLING_CHARGE_LAG_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "billing_charge_lag_seconds",
        "Latency from charge enqueue to ledger commit",
        vec![
            0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0,
        ],
        &[],
    )
});
pub static BILLING_CHARGE_BATCH_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "billing_charge_batch_total",
        "Tenant-grouped charge batch outcomes",
        &["result"],
    )
});
pub static BILLING_CHARGE_BATCH_ITEMS: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "billing_charge_batch_items_total",
        "Charge queue items processed via tenant-grouped batches",
        &["result"],
    )
});
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_billing_metrics() -> Result<(), PrometheusError> {
    register_collector_once(&REGISTRY, Box::new(BILLING_BALANCE_QUERIES_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_DEDUCTIONS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_DEDUCTION_AMOUNTS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_RECHARGES_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_OUTBOX_PENDING.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(BILLING_OLDEST_PENDING_AGE_SECONDS.clone()),
    )?;
    register_collector_once(&REGISTRY, Box::new(BILLING_DEAD_LETTERS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_CHARGE_FAILURES_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_CHARGE_LAG_SECONDS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_CHARGE_BATCH_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_CHARGE_BATCH_ITEMS.clone()))?;
    Ok(())
}
/// 扣费失败原因（有限枚举，直接作为 Prom 标签）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeFailureReason {
    InsufficientBalance,
    AccountNotFound,
    InvalidAmount,
    Database,
    Other,
}

impl ChargeFailureReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InsufficientBalance => "insufficient_balance",
            Self::AccountNotFound => "account_not_found",
            Self::InvalidAmount => "invalid_amount",
            Self::Database => "database",
            Self::Other => "other",
        }
    }

    #[must_use]
    pub fn classify(error: &crate::BillingError) -> Self {
        use crate::BillingError as E;
        match error {
            E::InsufficientBalance { .. } => Self::InsufficientBalance,
            E::AccountNotFound(_) => Self::AccountNotFound,
            E::InvalidAmount(_) => Self::InvalidAmount,
            E::Core(mazbri_core::CoreError::Database(_)) => Self::Database,
            _ => Self::Other,
        }
    }
}

pub fn record_charge_failure(reason: ChargeFailureReason) {
    BILLING_CHARGE_FAILURES_TOTAL
        .with_label_values(&[reason.as_str()])
        .inc();
}

/// 入账完成时记录 `now - enqueued_at`；时钟回拨得到的负值按 0 记。
pub fn record_charge_lag(enqueued_at: chrono::DateTime<chrono::Utc>) {
    let lag_ms = (chrono::Utc::now() - enqueued_at).num_milliseconds().max(0);
    BILLING_CHARGE_LAG_SECONDS
        .with_label_values(&["" as &str; 0])
        .observe(mazbri_core::num::i64_to_f64(lag_ms) / 1000.0);
}

pub struct DeductionRecorder {
    /// Histogram sample only. Ledger math must stay on [`Decimal`]; Prometheus needs f64.
    amount: f64,
}

impl DeductionRecorder {
    /// Record a deduction amount for metrics.
    ///
    /// Accepts [`Decimal`] so callers never string-parse money or `unwrap_or(0.0)`.
    /// Out-of-range values saturate (with a warning) instead of silently becoming zero.
    #[must_use]
    pub fn new(amount: Decimal) -> Self {
        Self {
            amount: decimal_to_histogram_f64(amount),
        }
    }

    pub fn record_success(&self) {
        BILLING_DEDUCTIONS_TOTAL
            .with_label_values(&["success"])
            .inc();
        BILLING_DEDUCTION_AMOUNTS
            .with_label_values(&["" as &str; 0])
            .observe(self.amount);
    }

    pub fn record_insufficient(&self) {
        BILLING_DEDUCTIONS_TOTAL
            .with_label_values(&["insufficient"])
            .inc();
    }

    pub fn record_failed(&self) {
        BILLING_DEDUCTIONS_TOTAL
            .with_label_values(&["failed"])
            .inc();
    }
}

/// Convert money to f64 for Prometheus histograms only (never for balance/ledger updates).
///
/// - Prefer exact [`ToPrimitive::to_f64`].
/// - On overflow/underflow: saturate to `f64::{MAX,MIN}` and warn — never map a non-zero
///   amount to `0.0` (that hid real charges in metrics).
pub fn decimal_to_histogram_f64(amount: Decimal) -> f64 {
    if let Some(value) = amount.to_f64() {
        return value;
    }
    warn!(
        %amount,
        "billing Decimal amount does not fit f64 for metrics; saturating histogram sample"
    );
    if amount.is_sign_negative() {
        f64::MIN
    } else {
        f64::MAX
    }
}

/// Display / report percentage from Decimal math; never silent-zero a non-zero ratio.
pub fn decimal_ratio_to_percentage_f64(part: Decimal, whole: Decimal) -> f64 {
    if whole <= Decimal::ZERO {
        return 0.0;
    }
    let pct = part / whole * Decimal::from(100);
    if let Some(value) = pct.to_f64() {
        return value;
    }
    // Ratio should almost always fit; fall back via separate conversions without zeroing.
    match (part.to_f64(), whole.to_f64()) {
        (Some(p), Some(w)) if w != 0.0 => p / w * 100.0,
        _ => {
            warn!(%part, %whole, "usage breakdown percentage cannot convert to f64");
            if part >= whole {
                100.0
            } else {
                // Last resort for display-only; ledger amounts remain Decimal elsewhere.
                f64::MAX
            }
        }
    }
}
