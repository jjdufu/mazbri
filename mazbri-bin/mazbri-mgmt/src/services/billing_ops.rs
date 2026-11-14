//! 计费链路对账与明细（6.1）的有界只读查询。
//! 积压、年龄、失败与入账延迟趋势由 billing 自身指标提供，不在此计算。
//!
//! 漏斗契约（step07）：每段 `{ key, name, count, ratio_from_prev?, failed }`，
//! 顶层带 `mode`（`sync_pass_through` | `async_funnel`）与 `window`；
//! 禁止无 `mode` 的误导 100% 平线「漏斗转化」。

pub use crate::handlers::types::{FailureQueueKind, OpsRange};
use crate::repositories::billing_ops as repo;
use crate::services::billing::BillingError;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::PgPool;
use tracing::error;

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

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::M15 => "近 15 分钟",
            Self::H1 => "近 1 小时",
            Self::H6 => "近 6 小时",
            Self::H24 => "近 24 小时",
            Self::D7 => "近 7 日",
            Self::D30 => "近 30 日",
        }
    }
}

/// 漏斗/管道健康时间窗（绝对起止 + 语义）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FunnelWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub semantics: &'static str,
    pub label: String,
}

pub const FUNNEL_WINDOW_SEMANTICS: &str = "window";
pub const FUNNEL_MODE_SYNC_PASS_THROUGH: &str = "sync_pass_through";
pub const FUNNEL_MODE_ASYNC_FUNNEL: &str = "async_funnel";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FunnelStage {
    pub key: &'static str,
    pub name: &'static str,
    pub count: i64,
    /// 相对上一段的转化率（本段 / 上一段）；首段为 `null`；上一段为 0 时亦为 `null`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio_from_prev: Option<f64>,
    pub failed: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BillingFunnelPg {
    pub window: FunnelWindow,
    pub stages: Vec<FunnelStage>,
    /// `sync_pass_through`：各段计数无跌落且无失败 → UI 展示管道健康/无堆积，非假漏斗。
    /// `async_funnel`：存在跌落或失败 → 适用经典漏斗转化语义。
    pub mode: &'static str,
    pub outbox_by_status: Vec<repo::StatusCount>,
    pub queue_by_status: Vec<repo::StatusCount>,
    pub charge_failures: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct BillingFailuresParams {
    pub range: OpsRange,
    pub limit: u32,
    pub failed_offset: u32,
    pub dlq_offset: u32,
    pub queue: Option<FailureQueueKind>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BillingFailures {
    pub limit: i64,
    pub offset: i64,
    pub failed_offset: i64,
    pub dlq_offset: i64,
    /// 时间窗内失败记录总数（分页用）。
    pub failures_total: i64,
    /// 时间窗内死信总数（分页用）。
    pub dead_letters_total: i64,
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

fn failed_statuses(rows: &[repo::StatusCount]) -> i64 {
    rows.iter()
        .filter(|r| r.status == "failed" || r.status == "dead_letter")
        .map(|r| r.count)
        .sum()
}

fn ratio_from_prev(prev: i64, curr: i64) -> Option<f64> {
    if prev <= 0 {
        None
    } else {
        Some(mazbri_core::num::i64_to_f64(curr) / mazbri_core::num::i64_to_f64(prev))
    }
}

/// 由各段计数与失败判定展示模式：无跌落且无失败 → 管道健康（同步直通观感）。
#[must_use]
pub fn detect_funnel_mode(stages: &[FunnelStage], charge_failures: i64) -> &'static str {
    let has_drop = stages.windows(2).any(|w| w[1].count < w[0].count);
    let stage_failed = stages.iter().any(|s| s.failed > 0);
    if has_drop || stage_failed || charge_failures > 0 {
        FUNNEL_MODE_ASYNC_FUNNEL
    } else {
        FUNNEL_MODE_SYNC_PASS_THROUGH
    }
}

/// PG 段漏斗：`outbox` → `queue` → `ledger`。
///
/// `failed`：outbox/queue 取状态合计中的 `failed` + `dead_letter`；ledger 段挂
/// `charge_failures`（入账失败未进入 ledger）。
#[must_use]
pub fn funnel_stages(
    outbox: i64,
    queue: i64,
    ledger: i64,
    outbox_failed: i64,
    queue_failed: i64,
    charge_failures: i64,
) -> Vec<FunnelStage> {
    vec![
        FunnelStage {
            key: "outbox",
            name: "usage_outbox",
            count: outbox,
            ratio_from_prev: None,
            failed: outbox_failed,
        },
        FunnelStage {
            key: "queue",
            name: "usage_charge_queue",
            count: queue,
            ratio_from_prev: ratio_from_prev(outbox, queue),
            failed: queue_failed,
        },
        FunnelStage {
            key: "ledger",
            name: "ledger_entries",
            count: ledger,
            ratio_from_prev: ratio_from_prev(queue, ledger),
            failed: charge_failures,
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
    let end = Utc::now();
    let start = range.since(end);
    let (outbox, queue, ledger, failures) = tokio::try_join!(
        async {
            repo::outbox_by_status(pool, start)
                .await
                .map_err(db_err("usage_outbox"))
        },
        async {
            repo::queue_by_status(pool, start)
                .await
                .map_err(db_err("usage_charge_queue"))
        },
        async {
            repo::ledger_charge_count(pool, start)
                .await
                .map_err(db_err("ledger_entries"))
        },
        async {
            repo::charge_failure_count(pool, start)
                .await
                .map_err(db_err("charge_failures"))
        },
    )?;
    let stages = funnel_stages(
        total(&outbox),
        total(&queue),
        ledger,
        failed_statuses(&outbox),
        failed_statuses(&queue),
        failures,
    );
    let mode = detect_funnel_mode(&stages, failures);
    Ok(BillingFunnelPg {
        window: FunnelWindow {
            start,
            end,
            semantics: FUNNEL_WINDOW_SEMANTICS,
            label: range.label().to_string(),
        },
        stages,
        mode,
        outbox_by_status: outbox,
        queue_by_status: queue,
        charge_failures: failures,
    })
}

/// # Errors
///
/// 数据库查询失败时返回 [`BillingError::Database`]。
pub async fn billing_failures(
    pool: &PgPool,
    params: BillingFailuresParams,
) -> Result<BillingFailures, BillingError> {
    let since = params.range.since(Utc::now());
    let limit = i64::from(params.limit.clamp(1, 200));
    let failed_page = repo::Page {
        limit,
        offset: i64::from(params.failed_offset),
    };
    let dlq_page = repo::Page {
        limit,
        offset: i64::from(params.dlq_offset),
    };
    match params.queue {
        Some(FailureQueueKind::Failures) => {
            let (failures_total, failures) = tokio::try_join!(
                async {
                    repo::charge_failure_count(pool, since)
                        .await
                        .map_err(db_err("charge_failures"))
                },
                async {
                    repo::recent_charge_failures(pool, since, failed_page)
                        .await
                        .map_err(db_err("charge_failures"))
                },
            )?;
            Ok(BillingFailures {
                limit,
                offset: failed_page.offset,
                failed_offset: failed_page.offset,
                dlq_offset: dlq_page.offset,
                failures_total,
                dead_letters_total: 0,
                failures,
                dead_letters: Vec::new(),
            })
        }
        Some(FailureQueueKind::DeadLetters) => {
            let (dead_letters_total, dead_letters) = tokio::try_join!(
                async {
                    repo::dead_letter_count(pool, since)
                        .await
                        .map_err(db_err("dead_letters"))
                },
                async {
                    repo::recent_dead_letters(pool, since, dlq_page)
                        .await
                        .map_err(db_err("dead_letters"))
                },
            )?;
            Ok(BillingFailures {
                limit,
                offset: dlq_page.offset,
                failed_offset: failed_page.offset,
                dlq_offset: dlq_page.offset,
                failures_total: 0,
                dead_letters_total,
                failures: Vec::new(),
                dead_letters,
            })
        }
        None => {
            let (failures_total, dead_letters_total, failures, dead_letters) = tokio::try_join!(
                async {
                    repo::charge_failure_count(pool, since)
                        .await
                        .map_err(db_err("charge_failures"))
                },
                async {
                    repo::dead_letter_count(pool, since)
                        .await
                        .map_err(db_err("dead_letters"))
                },
                async {
                    repo::recent_charge_failures(pool, since, failed_page)
                        .await
                        .map_err(db_err("charge_failures"))
                },
                async {
                    repo::recent_dead_letters(pool, since, dlq_page)
                        .await
                        .map_err(db_err("dead_letters"))
                },
            )?;
            Ok(BillingFailures {
                limit,
                offset: failed_page.offset,
                failed_offset: failed_page.offset,
                dlq_offset: dlq_page.offset,
                failures_total,
                dead_letters_total,
                failures,
                dead_letters,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn funnel_ratios_and_ratios_are_stable() {
        let s = funnel_stages(10, 9, 8, 1, 2, 3);
        assert_eq!(
            s.iter().map(|x| x.key).collect::<Vec<_>>(),
            vec!["outbox", "queue", "ledger"]
        );
        assert_eq!(s[0].failed, 1);
        assert_eq!(s[1].failed, 2);
        assert_eq!(s[2].failed, 3);
        assert!(s[0].ratio_from_prev.is_none());
        assert!((s[1].ratio_from_prev.unwrap() - 0.9).abs() < f64::EPSILON);
        assert!((s[2].ratio_from_prev.unwrap() - 8.0 / 9.0).abs() < f64::EPSILON);
    }

    #[test]
    fn flat_healthy_pipeline_is_sync_pass_through() {
        let s = funnel_stages(100, 100, 100, 0, 0, 0);
        assert_eq!(detect_funnel_mode(&s, 0), FUNNEL_MODE_SYNC_PASS_THROUGH);
    }

    #[test]
    fn drop_or_failure_is_async_funnel() {
        let drop = funnel_stages(100, 80, 80, 0, 0, 0);
        assert_eq!(detect_funnel_mode(&drop, 0), FUNNEL_MODE_ASYNC_FUNNEL);
        let fail = funnel_stages(100, 100, 100, 0, 0, 5);
        assert_eq!(detect_funnel_mode(&fail, 5), FUNNEL_MODE_ASYNC_FUNNEL);
    }

    #[test]
    fn ratio_none_when_prev_zero() {
        let s = funnel_stages(0, 0, 0, 0, 0, 0);
        assert!(s[1].ratio_from_prev.is_none());
        assert!(s[2].ratio_from_prev.is_none());
    }

    #[test]
    fn range_parses_and_rejects_unknown() {
        let r: OpsRange = serde_json::from_str("\"30d\"").unwrap_or_default();
        assert_eq!(r.seconds(), 2_592_000);
        assert_eq!(r.label(), "近 30 日");
        assert!(serde_json::from_str::<OpsRange>("\"2d\"").is_err());
    }

    #[test]
    fn failed_statuses_sum_failed_and_dead_letter() {
        let rows = vec![
            repo::StatusCount {
                status: "processed".into(),
                count: 10,
            },
            repo::StatusCount {
                status: "failed".into(),
                count: 2,
            },
            repo::StatusCount {
                status: "dead_letter".into(),
                count: 1,
            },
        ];
        assert_eq!(failed_statuses(&rows), 3);
        assert_eq!(total(&rows), 13);
    }

    #[test]
    fn failure_queue_kind_parses_snake_case() {
        assert_eq!(
            serde_json::from_str::<FailureQueueKind>("\"failures\"").ok(),
            Some(FailureQueueKind::Failures)
        );
        assert_eq!(
            serde_json::from_str::<FailureQueueKind>("\"dead_letters\"").ok(),
            Some(FailureQueueKind::DeadLetters)
        );
        assert!(serde_json::from_str::<FailureQueueKind>("\"unknown\"").is_err());
    }
}
