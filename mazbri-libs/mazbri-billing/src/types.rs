use crate::error::{BillingError, BillingResult};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "lowercase")]
pub enum ChargeStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    #[sqlx(rename = "dead_letter")]
    DeadLetter,
}

impl std::fmt::Display for ChargeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Processing => write!(f, "processing"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::DeadLetter => write!(f, "dead_letter"),
        }
    }
}

impl std::str::FromStr for ChargeStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "processing" => Ok(Self::Processing),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "dead_letter" => Ok(Self::DeadLetter),
            _ => Err(format!("Invalid charge status: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AdjustmentDirection {
    Credit,
    Debit,
}

impl AdjustmentDirection {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Credit => "CREDIT",
            Self::Debit => "DEBIT",
        }
    }
}

impl std::fmt::Display for AdjustmentDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for AdjustmentDirection {
    type Err = BillingError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "CREDIT" => Ok(Self::Credit),
            "DEBIT" => Ok(Self::Debit),
            other => Err(BillingError::InvalidConfig(format!(
                "Invalid adjustment direction '{other}': must be CREDIT or DEBIT"
            ))),
        }
    }
}

pub trait IntoAdjustmentDirection {
    /// # Errors
    ///
    /// 当方向字符串不是合法的 `CREDIT` 或 `DEBIT` 时返回校验错误。
    fn into_adjustment_direction(self) -> BillingResult<AdjustmentDirection>;
}

impl IntoAdjustmentDirection for AdjustmentDirection {
    fn into_adjustment_direction(self) -> BillingResult<AdjustmentDirection> {
        Ok(self)
    }
}

impl IntoAdjustmentDirection for &AdjustmentDirection {
    fn into_adjustment_direction(self) -> BillingResult<AdjustmentDirection> {
        Ok(*self)
    }
}

impl IntoAdjustmentDirection for &str {
    fn into_adjustment_direction(self) -> BillingResult<AdjustmentDirection> {
        self.parse()
    }
}

impl IntoAdjustmentDirection for &String {
    fn into_adjustment_direction(self) -> BillingResult<AdjustmentDirection> {
        self.as_str().parse()
    }
}

impl IntoAdjustmentDirection for String {
    fn into_adjustment_direction(self) -> BillingResult<AdjustmentDirection> {
        self.as_str().parse()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FencedChargeItem {
    pub queue_id: Uuid,
    pub lease_seq: i64,
    pub usage_record_id: i64,
    pub amount: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordUsageOutcome {
    Queued {
        usage_record_id: i64,
        queue_id: Uuid,
    },
    SyncCharged {
        usage_record_id: i64,
        charge_id: Option<Uuid>,
        charged: Decimal,
        new_balance: Decimal,
    },
    NoCharge {
        usage_record_id: i64,
    },
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct ChargeQueueItem {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub usage_record_id: i64,
    pub amount: Decimal,
    pub payload: Option<serde_json::Value>,
    pub status: ChargeStatus,
    #[serde(default)]
    pub lease_seq: i64,
    pub retry_count: i32,
    pub max_retries: i32,
    pub error_message: Option<String>,
    pub scheduled_at: DateTime<Utc>,
    pub transaction_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    /// 产生该扣费的请求侧 span（W3C traceparent），批量扣费 span 以 link 关联。
    pub trace_parent: Option<String>,
}

/// Shared worker queue settings for outbox + charge consumers.
///
/// Production semantics:
/// - When the queue is non-empty, consumers drain continuously (no forced sleep).
/// - `idle_poll_ms` only applies when a fetch returns zero rows.
/// - Multiple `worker_loops` per process + multi-replica are safe via
///   `FOR UPDATE SKIP LOCKED`.
#[derive(Debug, Clone)]
pub struct QueueConfig {
    pub max_retries: i32,
    pub retry_interval_secs: i64,
    pub batch_size: i32,
    /// Concurrent item processors within one batch.
    pub max_concurrency: usize,
    /// Parallel claim/process loops inside a single process.
    pub worker_loops: usize,
    /// Sleep only when queue is empty.
    pub idle_poll_ms: u64,
    /// Reclaim rows stuck in `processing` longer than this (crash recovery).
    pub stale_processing_secs: u64,
    pub worker_node_id: String,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_interval_secs: 60,
            batch_size: 500,
            max_concurrency: 8,
            worker_loops: 2,
            idle_poll_ms: 100,
            stale_processing_secs: 120,
            worker_node_id: hostname::get().map_or_else(
                |_| "unknown".to_string(),
                |h| h.to_string_lossy().to_string(),
            ),
        }
    }
}

impl QueueConfig {
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        if self.batch_size < 1 {
            self.batch_size = 1;
        }
        if self.max_concurrency < 1 {
            self.max_concurrency = 1;
        }
        if self.worker_loops < 1 {
            self.worker_loops = 1;
        }
        if self.idle_poll_ms < 10 {
            self.idle_poll_ms = 10;
        }
        if self.stale_processing_secs < 30 {
            self.stale_processing_secs = 30;
        }
        if self.max_retries < 1 {
            self.max_retries = 1;
        }
        if self.retry_interval_secs < 1 {
            self.retry_interval_secs = 1;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{AdjustmentDirection, IntoAdjustmentDirection};

    #[test]
    fn adjustment_direction_parses_strictly() {
        assert_eq!(
            "CREDIT".into_adjustment_direction().expect("CREDIT"),
            AdjustmentDirection::Credit
        );
        assert_eq!(
            "credit".into_adjustment_direction().expect("credit"),
            AdjustmentDirection::Credit
        );
        assert_eq!(
            "DEBIT".into_adjustment_direction().expect("DEBIT"),
            AdjustmentDirection::Debit
        );
        assert_eq!(
            "debit".into_adjustment_direction().expect("debit"),
            AdjustmentDirection::Debit
        );
        assert!("UNKNOWN".into_adjustment_direction().is_err());
        assert!("".into_adjustment_direction().is_err());
    }
}
