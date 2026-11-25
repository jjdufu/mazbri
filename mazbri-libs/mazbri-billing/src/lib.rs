mod calculator;
mod error;
mod metrics;
mod pipeline_health;
mod queue;
mod reconcile;
mod repository;
mod service;
mod types;
mod usage_events;
mod usage_outbox_worker;
mod worker;
pub use calculator::{BillingCalculator, CalculationResult, RateConfig};
pub use error::{BillingError, BillingResult as Result};
use mazbri_auth as _;
use mazbri_core::CoreError;
pub use metrics::{DeductionRecorder, register_billing_metrics};
pub use pipeline_health::{PipelineHealthConfig, PipelineHealthJob};
pub use queue::{ChargeEnqueue, ChargeQueue};
pub use reconcile::{ReconcileConfig, ReconcileJob, validate_clickhouse_table_ident};
pub use repository::{
    AccountBalanceRow, BillingRepository, DeductAndRecordResult, PgBillingRepository,
};
pub use service::{
    AccountBalance, BillInfo, BillLineItem, BillingMiddlewareState, BillingService,
    BillingServiceConfig, UsageBreakdownItem, UsageOutboxEvent, UsageRecord, UsageSummaryInfo,
};
pub use types::{
    AdjustmentDirection, ChargeQueueItem, ChargeStatus, FencedChargeItem, IntoAdjustmentDirection,
    QueueConfig, RecordUsageOutcome,
};
pub use usage_events::{
    UsageEventConsumer, UsageEventProducer, UsageEventProducerHandles, UsageEventsConfig,
};
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
