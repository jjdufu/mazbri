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
pub static BILLING_QUEUE_DEPTH: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    MetricsBuilder::new().gauge_vec(
        "billing_queue_depth",
        "Number of items in the charge queue by status",
        &["status"],
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
pub fn register_billing_metrics() -> Result<(), PrometheusError> {
    register_collector_once(&REGISTRY, Box::new(BILLING_BALANCE_QUERIES_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_DEDUCTIONS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_DEDUCTION_AMOUNTS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_RECHARGES_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_QUEUE_DEPTH.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_CHARGE_BATCH_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BILLING_CHARGE_BATCH_ITEMS.clone()))?;
    Ok(())
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
