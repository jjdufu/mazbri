//! 经营看板（6.3）：日收入/成本物化视图与账户余额只读查询；物化视图刷新。

use crate::services::billing::BillingError;
use chrono::{Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{PgPool, Row};
use tracing::{error, info, warn};

fn db_err(context: &'static str) -> impl FnOnce(sqlx::Error) -> BillingError {
    move |e| {
        error!(error = %e, context, "finance ops query failed");
        BillingError::Database
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyFinancePoint {
    pub day: NaiveDate,
    pub revenue: String,
    pub cost: String,
    pub margin: String,
    pub margin_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FinanceOverview {
    pub days: Vec<DailyFinancePoint>,
    pub revenue_total: String,
    pub cost_total: String,
    pub margin_total: String,
    pub margin_rate: Option<f64>,
    pub inverted_days: Vec<NaiveDate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RankRow {
    pub key: String,
    pub revenue: String,
    pub cost: String,
    pub margin: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FinanceRanks {
    pub by_tenant: Vec<RankRow>,
    pub by_product: Vec<RankRow>,
    pub by_supplier: Vec<RankRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BalanceRow {
    pub tenant_id: String,
    pub balance: String,
    pub credit_limit: String,
    pub available_balance: String,
    pub status: String,
    pub recent_recharge: String,
    pub avg_daily_spend: String,
    pub days_remaining: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconcileCalendarDay {
    pub day: NaiveDate,
    pub open_count: i64,
    pub resolved_count: i64,
    pub error_count: i64,
    pub status: String,
}

/// 刷新日收入/成本物化视图（CONCURRENTLY，需唯一索引）。
///
/// # Errors
///
/// 数据库执行失败时返回 [`BillingError::Database`]。
pub async fn refresh_daily_revenue_cost(pool: &PgPool) -> Result<(), BillingError> {
    sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY mv_daily_revenue_cost")
        .execute(pool)
        .await
        .map_err(db_err("refresh mv_daily_revenue_cost"))?;
    info!("refreshed mv_daily_revenue_cost");
    Ok(())
}

/// 后台每小时刷新一次。
pub fn spawn_mv_refresh(pool: std::sync::Arc<PgPool>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(3600));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        ticker.tick().await;
        loop {
            if let Err(e) = refresh_daily_revenue_cost(pool.as_ref()).await {
                warn!(error = %e, "mv_daily_revenue_cost refresh failed");
            }
            ticker.tick().await;
        }
    });
}

fn margin_rate(revenue: f64, cost: f64) -> Option<f64> {
    if revenue.abs() < f64::EPSILON {
        None
    } else {
        Some((revenue - cost) / revenue * 100.0)
    }
}

fn dec(v: Decimal) -> String {
    v.normalize().to_string()
}

fn dec_f64(v: Decimal) -> f64 {
    use std::str::FromStr;
    f64::from_str(&v.normalize().to_string()).unwrap_or(0.0)
}

/// 最近 `days` 天经营总览（默认 30）。
///
/// # Errors
///
/// 数据库查询失败时返回 [`BillingError::Database`]。
pub async fn finance_overview(pool: &PgPool, days: i64) -> Result<FinanceOverview, BillingError> {
    let days = days.clamp(1, 90);
    let since = Utc::now().date_naive() - Duration::days(days - 1);
    let rows = sqlx::query(
        "SELECT day, \
                COALESCE(SUM(revenue), 0)::numeric AS revenue, \
                COALESCE(SUM(cost), 0)::numeric AS cost, \
                COALESCE(SUM(margin), 0)::numeric AS margin \
         FROM mv_daily_revenue_cost \
         WHERE day >= $1 \
         GROUP BY day ORDER BY day",
    )
    .bind(since)
    .fetch_all(pool)
    .await
    .map_err(db_err("finance overview"))?;

    let mut points = Vec::with_capacity(rows.len());
    let mut rev_total = 0.0;
    let mut cost_total = 0.0;
    let mut inverted = Vec::new();
    for row in &rows {
        let day: NaiveDate = row.get("day");
        let revenue: Decimal = row.get("revenue");
        let cost: Decimal = row.get("cost");
        let margin: Decimal = row.get("margin");
        let r = dec_f64(revenue);
        let c = dec_f64(cost);
        rev_total += r;
        cost_total += c;
        if c > r && c > 0.0 {
            inverted.push(day);
        }
        points.push(DailyFinancePoint {
            day,
            revenue: dec(revenue),
            cost: dec(cost),
            margin: dec(margin),
            margin_rate: margin_rate(r, c),
        });
    }
    Ok(FinanceOverview {
        days: points,
        revenue_total: format!("{rev_total:.6}"),
        cost_total: format!("{cost_total:.6}"),
        margin_total: format!("{:.6}", rev_total - cost_total),
        margin_rate: margin_rate(rev_total, cost_total),
        inverted_days: inverted,
    })
}

async fn rank_query(
    pool: &PgPool,
    since: NaiveDate,
    dim_sql: &str,
    filter_sql: &str,
    limit: i64,
) -> Result<Vec<RankRow>, BillingError> {
    let sql = format!(
        "SELECT {dim_sql} AS key, \
                COALESCE(SUM(revenue), 0)::numeric AS revenue, \
                COALESCE(SUM(cost), 0)::numeric AS cost, \
                COALESCE(SUM(margin), 0)::numeric AS margin \
         FROM mv_daily_revenue_cost \
         WHERE day >= $1 {filter_sql} \
         GROUP BY 1 HAVING {dim_sql} IS NOT NULL AND {dim_sql}::text <> '' \
         ORDER BY SUM(revenue) DESC NULLS LAST LIMIT $2"
    );
    let rows = sqlx::query(&sql)
        .bind(since)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(db_err("finance ranks"))?;
    Ok(rows
        .into_iter()
        .map(|row| RankRow {
            key: row.get::<String, _>("key"),
            revenue: dec(row.get("revenue")),
            cost: dec(row.get("cost")),
            margin: dec(row.get("margin")),
        })
        .collect())
}

/// 按租户 / 产品 / 供应商排行。
///
/// # Errors
///
/// 数据库查询失败时返回 [`BillingError::Database`]。
pub async fn finance_ranks(
    pool: &PgPool,
    days: i64,
    limit: i64,
) -> Result<FinanceRanks, BillingError> {
    let days = days.clamp(1, 90);
    let limit = limit.clamp(1, 50);
    let since = Utc::now().date_naive() - Duration::days(days - 1);
    let by_tenant = rank_query(
        pool,
        since,
        "tenant_id::text",
        "AND tenant_id <> '00000000-0000-0000-0000-000000000000'::uuid AND revenue <> 0",
        limit,
    )
    .await?;
    let by_product = rank_query(
        pool,
        since,
        "product_id",
        "AND product_id <> '' AND revenue <> 0",
        limit,
    )
    .await?;
    let by_supplier = rank_query(
        pool,
        since,
        "supplier_code",
        "AND supplier_code <> '' AND cost <> 0",
        limit,
    )
    .await?;
    Ok(FinanceRanks {
        by_tenant,
        by_product,
        by_supplier,
    })
}

/// 账户余额、近窗充值与可用天数预测。
///
/// # Errors
///
/// 数据库查询失败时返回 [`BillingError::Database`]。
pub async fn finance_balances(pool: &PgPool, limit: i64) -> Result<Vec<BalanceRow>, BillingError> {
    let limit = limit.clamp(1, 200);
    let since = Utc::now().date_naive() - Duration::days(7);
    let rows = sqlx::query(
        "WITH spend AS ( \
            SELECT tenant_id, COALESCE(SUM(revenue), 0)::numeric / 7 AS avg_daily \
            FROM mv_daily_revenue_cost \
            WHERE day >= $1 AND tenant_id <> '00000000-0000-0000-0000-000000000000'::uuid \
            GROUP BY tenant_id \
         ), recharge AS ( \
            SELECT tenant_id, COALESCE(SUM(amount), 0)::numeric AS recent \
            FROM ledger_entries \
            WHERE entry_type = 'RECHARGE' AND created_at >= NOW() - INTERVAL '30 days' \
            GROUP BY tenant_id \
         ) \
         SELECT a.tenant_id::text AS tenant_id, a.balance, a.credit_limit, a.available_balance, a.status, \
                COALESCE(r.recent, 0)::numeric AS recent_recharge, \
                COALESCE(s.avg_daily, 0)::numeric AS avg_daily_spend \
         FROM tenant_accounts a \
         LEFT JOIN spend s ON s.tenant_id = a.tenant_id \
         LEFT JOIN recharge r ON r.tenant_id = a.tenant_id \
         ORDER BY a.available_balance ASC \
         LIMIT $2",
    )
    .bind(since)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(db_err("finance balances"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let avail: Decimal = row.get("available_balance");
            let avg: Decimal = row.get("avg_daily_spend");
            let avg_f = dec_f64(avg);
            let days_remaining = if avg_f > 0.0 {
                Some(dec_f64(avail) / avg_f)
            } else {
                None
            };
            BalanceRow {
                tenant_id: row.get("tenant_id"),
                balance: dec(row.get("balance")),
                credit_limit: dec(row.get("credit_limit")),
                available_balance: dec(avail),
                status: row.get("status"),
                recent_recharge: dec(row.get("recent_recharge")),
                avg_daily_spend: dec(avg),
                days_remaining,
            }
        })
        .collect())
}

/// 对账日历（按日汇总 discrepancies 状态）。
///
/// # Errors
///
/// 数据库查询失败时返回 [`BillingError::Database`]。
pub async fn reconcile_calendar(
    pool: &PgPool,
    days: i64,
) -> Result<Vec<ReconcileCalendarDay>, BillingError> {
    let days = days.clamp(1, 90);
    let since = Utc::now().date_naive() - Duration::days(days - 1);
    // status: ok | flagged | error | acknowledged | resolved
    let rows = sqlx::query(
        "SELECT reconciliation_date AS day, \
                COUNT(*) FILTER (WHERE status IN ('flagged', 'acknowledged'))::bigint AS open_count, \
                COUNT(*) FILTER (WHERE status IN ('resolved', 'ok'))::bigint AS resolved_count, \
                COUNT(*) FILTER (WHERE status = 'error')::bigint AS error_count \
         FROM reconciliation_discrepancies \
         WHERE reconciliation_date >= $1 \
         GROUP BY reconciliation_date ORDER BY reconciliation_date",
    )
    .bind(since)
    .fetch_all(pool)
    .await
    .map_err(db_err("reconcile calendar"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let open_count: i64 = row.get("open_count");
            let resolved_count: i64 = row.get("resolved_count");
            let error_count: i64 = row.get("error_count");
            let status = if error_count > 0 {
                "error"
            } else if open_count > 0 {
                "open"
            } else if resolved_count > 0 {
                "ok"
            } else {
                "empty"
            }
            .to_string();
            ReconcileCalendarDay {
                day: row.get("day"),
                open_count,
                resolved_count,
                error_count,
                status,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn margin_rate_handles_zero_revenue() {
        assert!(margin_rate(0.0, 1.0).is_none());
        assert!((margin_rate(100.0, 40.0).unwrap() - 60.0).abs() < 1e-9);
    }
}
