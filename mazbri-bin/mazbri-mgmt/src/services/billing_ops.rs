//! 计费链路健康（6.1）与供应商成本（3.1）的只读聚合。

use crate::repositories::billing_ops as repo;
use crate::services::billing::BillingError;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::error;

/// 查询时间窗（相对当前时刻），与 OLAP 模板的 `range` 取值一致。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum OpsRange {
    #[serde(rename = "15m")]
    M15,
    #[default]
    #[serde(rename = "1h")]
    H1,
    #[serde(rename = "6h")]
    H6,
    #[serde(rename = "24h")]
    H24,
    #[serde(rename = "7d")]
    D7,
    #[serde(rename = "30d")]
    D30,
}

impl OpsRange {
    #[must_use]
    pub const fn seconds(self) -> i64 {
        match self {
            Self::M15 => 900,
            Self::H1 => 3_600,
            Self::H6 => 21_600,
            Self::H24 => 86_400,
            Self::D7 => 604_800,
            Self::D30 => 2_592_000,
        }
    }

    #[must_use]
    pub fn since(self, now: DateTime<Utc>) -> DateTime<Utc> {
        now - Duration::seconds(self.seconds())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FunnelStage {
    pub stage: &'static str,
    pub count: i64,
    /// 与上一段的差值（上一段 − 本段），首段为 0。
    pub diff: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BillingFunnelPg {
    pub since: DateTime<Utc>,
    pub stages: Vec<FunnelStage>,
    pub outbox_by_status: Vec<repo::StatusCount>,
    pub queue_by_status: Vec<repo::StatusCount>,
    pub charge_failures: i64,
    pub charge_lag: repo::ChargeLag,
}

#[derive(Debug, Clone, Serialize)]
pub struct BillingFailures {
    pub failures: Vec<repo::ChargeFailureRow>,
    pub dead_letters: Vec<repo::DeadLetterRow>,
}

fn db_err(context: &'static str) -> impl FnOnce(sqlx::Error) -> BillingError {
    move |e| {
        error!(error = %e, context, "billing ops query failed");
        BillingError::Database
    }
}

fn total(rows: &[repo::StatusCount]) -> i64 {
    rows.iter().map(|r| r.count).sum()
}

/// PG 段漏斗：`usage_outbox` → `usage_charge_queue` → `ledger_entries`(CHARGE)。
#[must_use]
pub fn funnel_stages(outbox: i64, queue: i64, ledger: i64) -> Vec<FunnelStage> {
    vec![
        FunnelStage {
            stage: "usage_outbox",
            count: outbox,
            diff: 0,
        },
        FunnelStage {
            stage: "usage_charge_queue",
            count: queue,
            diff: outbox - queue,
        },
        FunnelStage {
            stage: "ledger_entries",
            count: ledger,
            diff: queue - ledger,
        },
    ]
}

/// # Errors
///
/// 数据库查询失败时返回 [`BillingError::Database`]。
pub async fn billing_funnel_pg(
    pool: &PgPool,
    range: OpsRange,
) -> Result<BillingFunnelPg, BillingError> {
    let since = range.since(Utc::now());
    let (outbox, queue, ledger, failures, lag) = tokio::try_join!(
        async {
            repo::outbox_by_status(pool, since)
                .await
                .map_err(db_err("usage_outbox"))
        },
        async {
            repo::queue_by_status(pool, since)
                .await
                .map_err(db_err("usage_charge_queue"))
        },
        async {
            repo::ledger_charge_count(pool, since)
                .await
                .map_err(db_err("ledger_entries"))
        },
        async {
            repo::charge_failure_count(pool, since)
                .await
                .map_err(db_err("charge_failures"))
        },
        async {
            repo::charge_lag(pool, since)
                .await
                .map_err(db_err("charge_lag"))
        },
    )?;
    Ok(BillingFunnelPg {
        since,
        stages: funnel_stages(total(&outbox), total(&queue), ledger),
        outbox_by_status: outbox,
        queue_by_status: queue,
        charge_failures: failures,
        charge_lag: lag,
    })
}

/// # Errors
///
/// 数据库查询失败时返回 [`BillingError::Database`]。
pub async fn billing_failures(
    pool: &PgPool,
    range: OpsRange,
    limit: u32,
) -> Result<BillingFailures, BillingError> {
    let since = range.since(Utc::now());
    let limit = i64::from(limit.clamp(1, 200));
    let (failures, dead_letters) = tokio::try_join!(
        async {
            repo::recent_charge_failures(pool, since, limit)
                .await
                .map_err(db_err("charge_failures"))
        },
        async {
            repo::recent_dead_letters(pool, since, limit)
                .await
                .map_err(db_err("dead_letters"))
        },
    )?;
    Ok(BillingFailures {
        failures,
        dead_letters,
    })
}

/// # Errors
///
/// 数据库查询失败时返回 [`BillingError::Database`]。
pub async fn supplier_costs(
    pool: &PgPool,
    range: OpsRange,
) -> Result<Vec<repo::SupplierCostRow>, BillingError> {
    repo::supplier_costs(pool, range.since(Utc::now()))
        .await
        .map_err(db_err("upstream_hourly_settlements"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn funnel_diffs_are_previous_minus_current() {
        let s = funnel_stages(10, 9, 9);
        assert_eq!(s.iter().map(|x| x.diff).collect::<Vec<_>>(), vec![0, 1, 0]);
        assert_eq!(s[2].stage, "ledger_entries");
    }

    #[test]
    fn range_parses_and_rejects_unknown() {
        let r: OpsRange = serde_json::from_str("\"30d\"").unwrap_or_default();
        assert_eq!(r.seconds(), 2_592_000);
        assert!(serde_json::from_str::<OpsRange>("\"2d\"").is_err());
    }
}
