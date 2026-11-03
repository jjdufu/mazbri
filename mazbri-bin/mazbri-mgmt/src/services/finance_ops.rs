//! 经营看板金额 SSOT：日收入/成本物化视图、租户窗内消费切片、账户余额只读与 MV 刷新。
//!
//! - `revenue` / `tenant_spend`：同源自 `usage_records.billable_amount`（经 `mv_daily_revenue_cost`）
//! - `cost`：来自 `upstream_hourly_settlements`；`has_cost=false` 表示未接入，禁止按 0 成本算满毛利
//! - `cost_status`：`ok` | `unavailable` | `partial`；非 `ok` 时 `margin*` 为 null

use crate::services::billing::BillingError;
use chrono::{Duration, FixedOffset, NaiveDate, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use tracing::{error, info, warn};
use uuid::Uuid;

fn db_err(context: &'static str) -> impl FnOnce(sqlx::Error) -> BillingError {
    move |e| {
        error!(error = %e, context, "finance ops query failed");
        BillingError::Database
    }
}

pub const COST_STATUS_OK: &str = "ok";
pub const COST_STATUS_UNAVAILABLE: &str = "unavailable";
pub const COST_STATUS_PARTIAL: &str = "partial";
pub const MONEY_WINDOW_SEMANTICS: &str = "window";

const ZERO_TENANT: Uuid = Uuid::nil();

#[derive(Debug, Clone, Serialize)]
pub struct MoneyWindow {
    pub start: String,
    pub end: String,
    pub semantics: &'static str,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyFinancePoint {
    pub day: NaiveDate,
    pub revenue: String,
    pub cost: String,
    pub cost_status: &'static str,
    pub margin: Option<String>,
    pub margin_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FinanceOverview {
    pub window: MoneyWindow,
    pub days: Vec<DailyFinancePoint>,
    pub revenue_total: String,
    pub cost_total: String,
    pub cost_status: &'static str,
    pub margin_total: Option<String>,
    pub margin_rate: Option<f64>,
    pub inverted_days: Vec<NaiveDate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RankRow {
    /// 稳定主键（租户 UUID / `product_id` / `supplier_code`），仅次要「复制 ID」用。
    pub key: String,
    /// 面向 UI 的名称；与 `key` 分离，禁止把裸 UUID 当唯一展示列。
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_code: Option<String>,
    pub revenue: String,
    pub cost: String,
    pub cost_status: &'static str,
    pub margin: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FinanceRanks {
    pub window: MoneyWindow,
    pub by_tenant: Vec<RankRow>,
    pub by_product: Vec<RankRow>,
    pub by_supplier: Vec<RankRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BalanceRow {
    pub tenant_id: String,
    pub tenant_code: Option<String>,
    pub tenant_name: Option<String>,
    pub balance: String,
    pub credit_limit: String,
    pub available_balance: String,
    pub status: String,
    pub recent_recharge: String,
    pub avg_daily_spend: String,
    pub days_remaining: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TenantSpend {
    pub window: MoneyWindow,
    pub tenant_id: String,
    pub tenant_code: String,
    pub tenant_name: String,
    /// 租户视角窗内支出（与平台对该租户的 `revenue` 同切片）。
    pub tenant_spend: String,
    /// 平台视角该租户窗内应计收入（与 `tenant_spend` 同源）。
    pub revenue_platform: String,
    pub cost_total: String,
    pub cost_status: &'static str,
    pub margin_total: Option<String>,
    pub margin_rate: Option<f64>,
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

fn dec(v: Decimal) -> String {
    v.normalize().to_string()
}

fn dec_f64(v: Decimal) -> f64 {
    use std::str::FromStr;
    f64::from_str(&v.normalize().to_string()).unwrap_or(0.0)
}

/// 由日级 `has_cost` 推导窗级 `cost_status`。
#[must_use]
pub fn derive_cost_status(day_has_cost: &[bool]) -> &'static str {
    if day_has_cost.is_empty() || !day_has_cost.iter().any(|&h| h) {
        COST_STATUS_UNAVAILABLE
    } else if day_has_cost.iter().all(|&h| h) {
        COST_STATUS_OK
    } else {
        COST_STATUS_PARTIAL
    }
}

fn day_cost_status(has_cost: bool) -> &'static str {
    if has_cost {
        COST_STATUS_OK
    } else {
        COST_STATUS_UNAVAILABLE
    }
}

/// 仅 `cost_status=ok` 且收入非 0 时返回毛利率（百分比）。
#[must_use]
pub fn margin_rate_pct(cost_status: &str, revenue: Decimal, cost: Decimal) -> Option<f64> {
    if cost_status != COST_STATUS_OK || revenue.is_zero() {
        return None;
    }
    let pct = (revenue - cost) * Decimal::from(100) / revenue;
    pct.normalize().to_string().parse().ok()
}

fn margin_amount(cost_status: &str, revenue: Decimal, cost: Decimal) -> Option<String> {
    if cost_status != COST_STATUS_OK {
        return None;
    }
    Some(dec(revenue - cost))
}

fn shanghai_offset() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("+08:00 offset")
}

fn money_window(start: NaiveDate, end: NaiveDate, label: String) -> MoneyWindow {
    let tz = shanghai_offset();
    let start_dt = tz
        .from_local_datetime(&start.and_hms_opt(0, 0, 0).expect("midnight"))
        .single()
        .expect("unique +08 start");
    let end_dt = tz
        .from_local_datetime(&end.and_hms_opt(23, 59, 59).expect("end of day"))
        .single()
        .expect("unique +08 end");
    MoneyWindow {
        start: start_dt.to_rfc3339(),
        end: end_dt.to_rfc3339(),
        semantics: MONEY_WINDOW_SEMANTICS,
        label,
    }
}

fn recent_days_window(days: i64) -> (NaiveDate, NaiveDate, MoneyWindow) {
    let days = days.clamp(1, 90);
    let end = Utc::now().date_naive();
    let start = end - Duration::days(days - 1);
    let label = format!("近 {days} 日");
    (start, end, money_window(start, end, label))
}

async fn load_tenant_labels(
    platform: &PgPool,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, (String, String)>, BillingError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT id, tenant_code, COALESCE(name, '') AS name \
         FROM tenants WHERE id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(platform)
    .await
    .map_err(db_err("tenant labels for finance"))?;
    let mut map = HashMap::with_capacity(rows.len());
    for row in rows {
        let id: Uuid = row.get("id");
        let code: String = row.get("tenant_code");
        let name: String = row.get("name");
        map.insert(id, (code, name));
    }
    Ok(map)
}

async fn load_product_names(
    platform: &PgPool,
    product_ids: &[String],
) -> Result<HashMap<String, String>, BillingError> {
    if product_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT product_id, COALESCE(name, '') AS name FROM products WHERE product_id = ANY($1)",
    )
    .bind(product_ids)
    .fetch_all(platform)
    .await
    .map_err(db_err("product names for finance ranks"))?;
    let mut map = HashMap::with_capacity(rows.len());
    for row in rows {
        let id: String = row.get("product_id");
        let name: String = row.get("name");
        map.insert(id, name);
    }
    Ok(map)
}

async fn load_supplier_names(
    platform: &PgPool,
    codes: &[String],
) -> Result<HashMap<String, String>, BillingError> {
    if codes.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT supplier_code, COALESCE(name, '') AS name FROM suppliers WHERE supplier_code = ANY($1)",
    )
    .bind(codes)
    .fetch_all(platform)
    .await
    .map_err(db_err("supplier names for finance ranks"))?;
    let mut map = HashMap::with_capacity(rows.len());
    for row in rows {
        let code: String = row.get("supplier_code");
        let name: String = row.get("name");
        map.insert(code, name);
    }
    Ok(map)
}

/// 名称优先，其次编码；UUID 主键不得回落成裸 UUID 展示名。
fn pick_display_name(
    name: Option<&str>,
    code: Option<&str>,
    key: &str,
    key_is_uuid: bool,
) -> String {
    for candidate in [name, code].into_iter().flatten() {
        let trimmed = candidate.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if key_is_uuid {
        String::new()
    } else {
        key.to_string()
    }
}

fn round_days_remaining(avail: Decimal, avg_daily: Decimal) -> Option<f64> {
    let avg_f = dec_f64(avg_daily);
    if avg_f <= 0.0 {
        return None;
    }
    let days = dec_f64(avail) / avg_f;
    Some((days * 10.0).round() / 10.0)
}

fn round_money_dec(v: Decimal) -> String {
    format!("{:.2}", dec_f64(v))
}

/// 最近 `days` 天经营总览（默认 30）。
///
/// # Errors
///
/// 数据库查询失败时返回 [`BillingError::Database`]。
pub async fn finance_overview(pool: &PgPool, days: i64) -> Result<FinanceOverview, BillingError> {
    let (since, end, window) = recent_days_window(days);
    let rows = sqlx::query(
        "SELECT day, \
                COALESCE(SUM(revenue), 0)::numeric AS revenue, \
                COALESCE(SUM(cost), 0)::numeric AS cost, \
                COALESCE(BOOL_OR(has_cost), false) AS has_cost \
         FROM mv_daily_revenue_cost \
         WHERE day >= $1 AND day <= $2 \
         GROUP BY day ORDER BY day",
    )
    .bind(since)
    .bind(end)
    .fetch_all(pool)
    .await
    .map_err(db_err("finance overview"))?;

    let mut points = Vec::with_capacity(rows.len());
    let mut rev_total = Decimal::ZERO;
    let mut cost_total = Decimal::ZERO;
    let mut day_flags = Vec::with_capacity(rows.len());
    let mut inverted = Vec::new();
    for row in &rows {
        let day: NaiveDate = row.get("day");
        let revenue: Decimal = row.get("revenue");
        let cost: Decimal = row.get("cost");
        let has_cost: bool = row.get("has_cost");
        let status = day_cost_status(has_cost);
        day_flags.push(has_cost);
        rev_total += revenue;
        cost_total += cost;
        if has_cost && cost > revenue {
            inverted.push(day);
        }
        points.push(DailyFinancePoint {
            day,
            revenue: dec(revenue),
            cost: dec(cost),
            cost_status: status,
            margin: margin_amount(status, revenue, cost),
            margin_rate: margin_rate_pct(status, revenue, cost),
        });
    }
    let cost_status = derive_cost_status(&day_flags);
    Ok(FinanceOverview {
        window,
        days: points,
        revenue_total: dec(rev_total),
        cost_total: dec(cost_total),
        cost_status,
        margin_total: margin_amount(cost_status, rev_total, cost_total),
        margin_rate: margin_rate_pct(cost_status, rev_total, cost_total),
        inverted_days: inverted,
    })
}

async fn rank_query(
    pool: &PgPool,
    since: NaiveDate,
    end: NaiveDate,
    dim_sql: &str,
    filter_sql: &str,
    limit: i64,
) -> Result<Vec<(String, Decimal, Decimal, bool)>, BillingError> {
    let sql = format!(
        "SELECT {dim_sql} AS key, \
                COALESCE(SUM(revenue), 0)::numeric AS revenue, \
                COALESCE(SUM(cost), 0)::numeric AS cost, \
                COALESCE(BOOL_OR(has_cost), false) AS has_cost \
         FROM mv_daily_revenue_cost \
         WHERE day >= $1 AND day <= $2 {filter_sql} \
         GROUP BY 1 HAVING {dim_sql} IS NOT NULL AND {dim_sql}::text <> '' \
         ORDER BY SUM(revenue) DESC NULLS LAST LIMIT $3"
    );
    let rows = sqlx::query(&sql)
        .bind(since)
        .bind(end)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(db_err("finance ranks"))?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("key"),
                row.get::<Decimal, _>("revenue"),
                row.get::<Decimal, _>("cost"),
                row.get::<bool, _>("has_cost"),
            )
        })
        .collect())
}

fn to_rank_row(
    key: String,
    display_name: String,
    revenue: Decimal,
    cost: Decimal,
    has_cost: bool,
    tenant_code: Option<String>,
) -> RankRow {
    let status = day_cost_status(has_cost);
    RankRow {
        key,
        display_name,
        tenant_code,
        revenue: dec(revenue),
        cost: dec(cost),
        cost_status: status,
        margin: margin_amount(status, revenue, cost),
    }
}

/// 按租户 / 产品 / 供应商排行；每行必带 `display_name`，`key` 仅作复制 ID。
///
/// # Errors
///
/// 数据库查询失败时返回 [`BillingError::Database`]。
pub async fn finance_ranks(
    billing: &PgPool,
    platform: &PgPool,
    days: i64,
    limit: i64,
) -> Result<FinanceRanks, BillingError> {
    let limit = limit.clamp(1, 50);
    let (since, end, window) = recent_days_window(days);
    let by_tenant_raw = rank_query(
        billing,
        since,
        end,
        "tenant_id::text",
        "AND tenant_id <> '00000000-0000-0000-0000-000000000000'::uuid AND revenue <> 0",
        limit,
    )
    .await?;
    let tenant_ids: Vec<Uuid> = by_tenant_raw
        .iter()
        .filter_map(|(key, _, _, _)| Uuid::parse_str(key).ok())
        .collect();
    let labels = load_tenant_labels(platform, &tenant_ids).await?;
    let by_tenant = by_tenant_raw
        .into_iter()
        .map(|(key, revenue, cost, has_cost)| {
            let (code, name) = Uuid::parse_str(&key)
                .ok()
                .and_then(|id| labels.get(&id).cloned())
                .map_or((None, None), |(c, n)| (Some(c), Some(n)));
            let display_name = pick_display_name(
                name.as_deref(),
                code.as_deref(),
                &key,
                true,
            );
            to_rank_row(key, display_name, revenue, cost, has_cost, code)
        })
        .collect();

    let by_product_raw = rank_query(
        billing,
        since,
        end,
        "product_id",
        "AND product_id <> '' AND revenue <> 0",
        limit,
    )
    .await?;
    let product_ids: Vec<String> = by_product_raw.iter().map(|(k, _, _, _)| k.clone()).collect();
    let product_names = load_product_names(platform, &product_ids).await?;
    let by_product = by_product_raw
        .into_iter()
        .map(|(key, revenue, cost, has_cost)| {
            let name = product_names.get(&key).map(String::as_str);
            let display_name = pick_display_name(name, Some(&key), &key, false);
            to_rank_row(key, display_name, revenue, cost, has_cost, None)
        })
        .collect();

    let by_supplier_raw = rank_query(
        billing,
        since,
        end,
        "supplier_code",
        "AND supplier_code <> '' AND (cost <> 0 OR has_cost)",
        limit,
    )
    .await?;
    let supplier_codes: Vec<String> = by_supplier_raw
        .iter()
        .map(|(k, _, _, _)| k.clone())
        .collect();
    let supplier_names = load_supplier_names(platform, &supplier_codes).await?;
    let by_supplier = by_supplier_raw
        .into_iter()
        .map(|(key, revenue, cost, has_cost)| {
            let name = supplier_names.get(&key).map(String::as_str);
            let display_name = pick_display_name(name, Some(&key), &key, false);
            to_rank_row(key, display_name, revenue, cost, has_cost, None)
        })
        .collect();

    Ok(FinanceRanks {
        window,
        by_tenant,
        by_product,
        by_supplier,
    })
}

/// 账户余额、近窗充值与可用天数预测；附带租户展示名。
///
/// # Errors
///
/// 数据库查询失败时返回 [`BillingError::Database`]。
pub async fn finance_balances(
    billing: &PgPool,
    platform: &PgPool,
    limit: i64,
) -> Result<Vec<BalanceRow>, BillingError> {
    let limit = limit.clamp(1, 200);
    let since = Utc::now().date_naive() - Duration::days(7);
    let rows = sqlx::query(
        "WITH spend AS ( \
            SELECT tenant_id, COALESCE(SUM(revenue), 0)::numeric / 7 AS avg_daily \
            FROM mv_daily_revenue_cost \
            WHERE day >= $1 AND tenant_id <> $3::uuid \
            GROUP BY tenant_id \
         ), recharge AS ( \
            SELECT tenant_id, COALESCE(SUM(amount), 0)::numeric AS recent \
            FROM ledger_entries \
            WHERE entry_type = 'RECHARGE' AND created_at >= NOW() - INTERVAL '30 days' \
            GROUP BY tenant_id \
         ) \
         SELECT a.tenant_id, a.balance, a.credit_limit, a.available_balance, a.status, \
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
    .bind(ZERO_TENANT)
    .fetch_all(billing)
    .await
    .map_err(db_err("finance balances"))?;

    let ids: Vec<Uuid> = rows.iter().map(|row| row.get("tenant_id")).collect();
    let labels = load_tenant_labels(platform, &ids).await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let tenant_id: Uuid = row.get("tenant_id");
            let avail: Decimal = row.get("available_balance");
            let avg: Decimal = row.get("avg_daily_spend");
            let days_remaining = round_days_remaining(avail, avg);
            let (tenant_code, tenant_name) = labels
                .get(&tenant_id)
                .cloned()
                .map_or((None, None), |(c, n)| (Some(c), Some(n)));
            BalanceRow {
                tenant_id: tenant_id.to_string(),
                tenant_code,
                tenant_name,
                balance: round_money_dec(row.get("balance")),
                credit_limit: round_money_dec(row.get("credit_limit")),
                available_balance: round_money_dec(avail),
                status: row.get("status"),
                recent_recharge: round_money_dec(row.get("recent_recharge")),
                avg_daily_spend: round_money_dec(avg),
                days_remaining,
            }
        })
        .collect())
}

/// 租户窗内消费 / 平台对该租户收入（同一 MV 切片）。
///
/// # Errors
///
/// - `from > to` 时返回 [`BillingError::Validation`]
/// - 数据库失败返回 [`BillingError::Database`]
pub async fn tenant_spend(
    billing: &PgPool,
    platform: &PgPool,
    tenant_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<TenantSpend, BillingError> {
    if from > to {
        return Err(BillingError::Validation(
            "from 必须小于等于 to".to_string(),
        ));
    }
    let span_days = (to - from).num_days() + 1;
    if span_days > 90 {
        return Err(BillingError::Validation(
            "时间窗最长 90 日".to_string(),
        ));
    }
    let label = format!("{from} ~ {to}");
    let window = money_window(from, to, label);

    let labels = load_tenant_labels(platform, &[tenant_id]).await?;
    let (tenant_code, tenant_name) = labels.get(&tenant_id).cloned().unwrap_or_else(|| {
        (tenant_id.to_string(), String::new())
    });

    let row = sqlx::query(
        "SELECT COALESCE(SUM(revenue), 0)::numeric AS revenue, \
                COALESCE(SUM(cost), 0)::numeric AS cost, \
                COALESCE(BOOL_OR(has_cost), false) AS has_cost \
         FROM mv_daily_revenue_cost \
         WHERE tenant_id = $1 AND day >= $2 AND day <= $3",
    )
    .bind(tenant_id)
    .bind(from)
    .bind(to)
    .fetch_one(billing)
    .await
    .map_err(db_err("tenant spend"))?;

    let revenue: Decimal = row.get("revenue");
    let cost: Decimal = row.get("cost");
    let has_cost: bool = row.get("has_cost");
    // 上游成本挂在全零 UUID，租户切片通常无成本行 → unavailable（禁止假 100% 毛利）。
    let cost_status = day_cost_status(has_cost);
    let amount = dec(revenue);
    Ok(TenantSpend {
        window,
        tenant_id: tenant_id.to_string(),
        tenant_code,
        tenant_name,
        tenant_spend: amount.clone(),
        revenue_platform: amount,
        cost_total: dec(cost),
        cost_status,
        margin_total: margin_amount(cost_status, revenue, cost),
        margin_rate: margin_rate_pct(cost_status, revenue, cost),
    })
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
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn derive_cost_status_unavailable_when_no_cost_days() {
        assert_eq!(derive_cost_status(&[]), COST_STATUS_UNAVAILABLE);
        assert_eq!(derive_cost_status(&[false, false]), COST_STATUS_UNAVAILABLE);
    }

    #[test]
    fn derive_cost_status_ok_and_partial() {
        assert_eq!(derive_cost_status(&[true, true]), COST_STATUS_OK);
        assert_eq!(derive_cost_status(&[true, false]), COST_STATUS_PARTIAL);
    }

    #[test]
    fn margin_rate_never_fakes_100_when_cost_unavailable() {
        // 旧逻辑：cost=0 仍算 100%；现要求 unavailable → None
        assert!(margin_rate_pct(COST_STATUS_UNAVAILABLE, d("100"), d("0")).is_none());
        assert!(margin_amount(COST_STATUS_UNAVAILABLE, d("100"), d("0")).is_none());
        assert!(margin_rate_pct(COST_STATUS_PARTIAL, d("100"), d("10")).is_none());
    }

    #[test]
    fn margin_rate_ok_when_cost_present() {
        let rate = margin_rate_pct(COST_STATUS_OK, d("100"), d("40")).unwrap();
        assert!((rate - 60.0).abs() < 1e-9);
        assert_eq!(
            margin_amount(COST_STATUS_OK, d("100"), d("40")).as_deref(),
            Some("60")
        );
        assert!(margin_rate_pct(COST_STATUS_OK, d("0"), d("1")).is_none());
    }

    #[test]
    fn pick_display_name_never_exposes_bare_uuid() {
        let uuid = "28467749-3701-4a13-9abc-1234567890ab";
        assert_eq!(
            pick_display_name(Some("演示租户"), Some("demo"), uuid, true),
            "演示租户"
        );
        assert_eq!(
            pick_display_name(None, Some("demo"), uuid, true),
            "demo"
        );
        assert_eq!(pick_display_name(None, None, uuid, true), "");
        assert_eq!(
            pick_display_name(None, None, "80000001", false),
            "80000001"
        );
        assert_eq!(
            pick_display_name(Some("身份证核验"), None, "80000001", false),
            "身份证核验"
        );
    }

    #[test]
    fn round_days_remaining_one_decimal() {
        assert_eq!(
            round_days_remaining(d("38.428811455"), d("1")),
            Some(38.4)
        );
        assert!(round_days_remaining(d("100"), d("0")).is_none());
    }
}
