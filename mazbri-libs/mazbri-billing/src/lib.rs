mod calculator;
mod error;
mod metrics;
mod middleware;
mod queue;
mod repository;
mod service;
mod types;
mod usage_outbox_worker;
mod worker;
pub use calculator::{BillingCalculator, CalculationResult, RateConfig};
pub use error::{BillingError, BillingResult as Result};
use mazbri_core::CoreError;
pub use metrics::{DeductionRecorder, register_billing_metrics};
pub use middleware::{BillingMiddlewareState, balance_check_middleware, usage_middleware};
pub use queue::{ChargeQueue, QueueStats};
pub use repository::{
    AccountBalanceRow, BillingRepository, DeductAndRecordResult, PgBillingRepository,
};
pub use service::{
    AccountBalance, AddFundsResult, BillInfo, BillLineItem, BillingService, BillingServiceConfig,
    UsageBreakdownItem, UsageOutboxEvent, UsageRecord, UsageSummaryInfo,
};
pub use types::{ChargeQueueItem, ChargeStatus, QueueConfig};
pub use usage_outbox_worker::UsageOutboxWorker;
pub use worker::ChargeWorker;
impl From<BillingError> for CoreError {
    fn from(err: BillingError) -> Self {
        match err {
            BillingError::InsufficientBalance { .. } => CoreError::InsufficientBalance {
                required: err.to_string(),
                available: String::new(),
            },
            BillingError::Core(e) => e,
            _ => CoreError::Internal(err.to_string()),
        }
    }
}
