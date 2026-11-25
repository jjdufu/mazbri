use super::{
    AnalyticsHttpClient, BillGenerationSummary, BillingAdjustmentListResponse, BillingError,
    DateTime, Decimal, PgPool, ReconciliationAuditResult, ReconciliationDiscrepancyListResponse,
    UpstreamBillScope, Utc, error, floor_to_utc_hour, generate_upstream_bill, repo,
    sync_upstream_cost_settlements, validate_limit,
};
use mazbri_core::net::ssrf::SsrfChecker;
use mazbri_core::num::SaturatingFrom;
use std::sync::Arc;
use tracing::{info, warn};

const RECON_USAGE_COUNT_SQL: &str = "SELECT COALESCE(COUNT(*), 0)::BIGINT
     FROM usage_records
     WHERE recorded_at >= $1
       AND recorded_at < $2
       AND charge_status = 'charged'
       AND billable_amount > 0
       AND ($3::uuid IS NULL OR tenant_id = $3)
       AND ($4::text IS NULL OR product_id = $4)";

/// Structured scope pushed down identically to OLTP `usage_records` and OLAP `ClickHouse` queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconciliationScope {
    Global,
    Tenant {
        tenant_id: uuid::Uuid,
        tenant_code: Option<String>,
    },
    Product {
        product_id: String,
    },
}

/// Parse `(scope_type, scope_id)` into a validated [`ReconciliationScope`] so reconciliation
/// never silently ignores caller scope filters.
///
/// # Errors
///
/// Returns [`BillingError::Validation`] when `scope_type` is unknown or `scope_id` is invalid.
pub fn parse_reconciliation_scope(
    scope_type: &str,
    scope_id: &str,
) -> Result<ReconciliationScope, BillingError> {
    let kind = scope_type.trim().to_ascii_lowercase();
    let raw_id = scope_id.trim();
    match kind.as_str() {
        "system" | "global" | "all" => {
            if !raw_id.is_empty()
                && !matches!(
                    raw_id.to_ascii_lowercase().as_str(),
                    "system" | "global" | "all" | "*"
                )
            {
                return Err(BillingError::Validation(format!(
                    "scope_id '{raw_id}' must be empty for global/system reconciliation scope"
                )));
            }
            Ok(ReconciliationScope::Global)
        }
        "tenant" => {
            if raw_id.is_empty() {
                return Err(BillingError::Validation(
                    "scope_id is required when scope_type = 'tenant'".to_string(),
                ));
            }
            // Support either `<tenant_uuid>` or `<tenant_uuid>:<tenant_code>`
            let (uuid_part, code_part) = match raw_id.split_once(':') {
                Some((u, c)) => (
                    u.trim(),
                    Some(c.trim()).filter(|s| !s.is_empty()).map(str::to_string),
                ),
                None => (raw_id, None),
            };
            if let Some(ref tc) = code_part
                && tc.len() > 128
            {
                return Err(BillingError::Validation(
                    "tenant_code in scope_id exceeds 128 characters".to_string(),
                ));
            }
            let tenant_id = uuid::Uuid::parse_str(uuid_part).map_err(|_| {
                BillingError::Validation(format!(
                    "scope_id '{raw_id}' must be a valid tenant UUID when scope_type = 'tenant'"
                ))
            })?;
            Ok(ReconciliationScope::Tenant {
                tenant_id,
                tenant_code: code_part,
            })
        }
        "product" => {
            if raw_id.is_empty() || raw_id.len() > 100 {
                return Err(BillingError::Validation(
                    "scope_id must be 1..=100 chars when scope_type = 'product'".to_string(),
                ));
            }
            Ok(ReconciliationScope::Product {
                product_id: raw_id.to_string(),
            })
        }
        _ => Err(BillingError::Validation(format!(
            "scope_type '{scope_type}' must be one of: system, global, tenant, product"
        ))),
    }
}

/// Build the OLAP `billing_funnel_ch` JSON request body (`request_logs FINAL WHERE event_target = 'gateway'`
/// with `countIf(request_logs.billable = 1)` and `[start, end)` Unix-second bounds).
#[must_use]
pub fn build_gateway_billable_olap_query(
    day_start: DateTime<Utc>,
    day_end: DateTime<Utc>,
    scope: &ReconciliationScope,
) -> Option<serde_json::Value> {
    let mut params = serde_json::Map::new();
    params.insert(
        "start".to_string(),
        serde_json::Value::Number(day_start.timestamp().into()),
    );
    params.insert(
        "end".to_string(),
        serde_json::Value::Number(day_end.timestamp().into()),
    );
    match scope {
        ReconciliationScope::Global => {}
        ReconciliationScope::Product { product_id } => {
            params.insert(
                "product_id".to_string(),
                serde_json::Value::String(product_id.clone()),
            );
        }
        ReconciliationScope::Tenant {
            tenant_code: Some(code),
            ..
        } => {
            params.insert(
                "tenant_code".to_string(),
                serde_json::Value::String(code.clone()),
            );
        }
        ReconciliationScope::Tenant {
            tenant_code: None, ..
        } => return None,
    }
    Some(serde_json::json!({
        "template": "billing_funnel_ch",
        "params": params,
    }))
}

/// Send a reconciliation alert webhook via an SSRF-checked HTTP client (10s timeout,
/// disabled redirects, literal-IP + connect-time DNS validation).
///
/// # Errors
///
/// Returns an error description if SSRF validation rejects the URL, transport fails, or status is non-2xx.
pub async fn send_reconciliation_alert_webhook(
    ssrf: &SsrfChecker,
    client: &reqwest::Client,
    webhook_url: &str,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let req = ssrf
        .checked_post(client, webhook_url.trim())
        .map_err(|e| format!("SSRF blocked reconciliation webhook: {e}"))?;
    let resp = req
        .json(payload)
        .send()
        .await
        .map_err(|e| format!("reconciliation webhook transport error: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "reconciliation webhook returned HTTP {}",
            resp.status()
        ));
    }
    Ok(())
}

/// 启动后台上游通道小时成本自动结算同步任务：
/// - 启动时立即回填近 90 日整点窗口（覆盖经营分析最长 90 日周期）并刷新 `mv_daily_revenue_cost`；
/// - 随后每 5 分钟增量幂等同步近 2 小时整点窗口并刷新物化视图。
pub fn spawn_upstream_cost_settlement_sync(
    billing_db: Arc<PgPool>,
    platform_db: Arc<PgPool>,
    analytics_client: Arc<AnalyticsHttpClient>,
) {
    tokio::spawn(async move {
        let mut startup_ok = false;
        for attempt in 1..=5_u8 {
            let now = Utc::now();
            let window_start = floor_to_utc_hour(now - chrono::Duration::days(90));
            let window_end = floor_to_utc_hour(now) + chrono::Duration::hours(1);
            match sync_upstream_cost_settlements(
                billing_db.as_ref(),
                platform_db.as_ref(),
                analytics_client.as_ref(),
                window_start,
                window_end,
                true,
            )
            .await
            {
                Ok(buckets) => {
                    info!(
                        buckets,
                        "initial upstream cost settlement sync & mv_daily_revenue_cost refresh completed"
                    );
                    startup_ok = true;
                    break;
                }
                Err(e) => {
                    warn!(
                        attempt,
                        error = %e,
                        "initial upstream cost settlement sync failed; retrying shortly"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
            }
        }
        if !startup_ok {
            warn!(
                "initial upstream cost settlement sync exhausted retries; continuing periodic sync"
            );
        }

        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(300));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let now = Utc::now();
            let window_start = floor_to_utc_hour(now) - chrono::Duration::hours(2);
            let window_end = floor_to_utc_hour(now) + chrono::Duration::hours(1);
            match sync_upstream_cost_settlements(
                billing_db.as_ref(),
                platform_db.as_ref(),
                analytics_client.as_ref(),
                window_start,
                window_end,
                false,
            )
            .await
            {
                Ok(buckets) => {
                    info!(buckets, "periodic upstream cost settlement sync completed");
                }
                Err(e) => {
                    warn!(error = %e, "periodic upstream cost settlement sync failed");
                }
            }
        }
    });
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn execute_upstream_billing_job(
    billing_db: &PgPool,
    platform_db: &PgPool,
    analytics_client: &AnalyticsHttpClient,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    scope: Option<UpstreamBillScope>,
) -> Result<BillGenerationSummary, BillingError> {
    let bill_id = generate_upstream_bill(
        billing_db,
        platform_db,
        analytics_client,
        period_start,
        period_end,
        scope,
    )
    .await?;

    let bill = repo::get_bill(billing_db, bill_id)
        .await
        .map_err(|e| {
            error!("Database error fetching generated bill: {}", e);
            BillingError::Database
        })?
        .ok_or(BillingError::BillNotFound(bill_id))?;

    let items = repo::list_bill_items(billing_db, &bill)
        .await
        .map_err(|e| {
            error!("Database error listing items for generated bill: {}", e);
            BillingError::Database
        })?;

    Ok(BillGenerationSummary {
        bill_id,
        total_channels: items.len(),
        total_calls: bill.total_calls,
        total_amount: bill.total,
    })
}

#[derive(Debug, Clone, Copy)]
pub struct ReconciliationAuditOptions {
    pub loss_ratio_threshold: Decimal,
    pub include_amount_recon: bool,
}

/// 网关侧 `request_logs` 在左闭右开 `[day_start, day_end)` 内的可计费请求数（`billable = 1`）。
async fn fetch_gateway_billable_count(
    analytics_client: &AnalyticsHttpClient,
    day_start: chrono::DateTime<chrono::Utc>,
    day_end: chrono::DateTime<chrono::Utc>,
    scope: &ReconciliationScope,
) -> Result<i64, String> {
    if let Some(olap_body) = build_gateway_billable_olap_query(day_start, day_end, scope) {
        let resp = analytics_client
            .olap_query(&olap_body)
            .await
            .map_err(|e| format!("gateway billing_funnel_ch olap query failed: {e}"))?;
        let count = resp
            .get("rows")
            .and_then(serde_json::Value::as_array)
            .and_then(|rows| rows.first())
            .and_then(|row| row.get("billable"))
            .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(u64::saturating_from)))
            .map(i64::saturating_from)
            .ok_or_else(|| "billing_funnel_ch response missing rows[0].billable".to_string())?;
        return Ok(count);
    }

    // Bare tenant UUID fallback: resolve tenant_code from gateway request_logs in the window,
    // then run exact `billing_funnel_ch` (`billable = 1`) for that tenant_code.
    if let ReconciliationScope::Tenant {
        tenant_id,
        tenant_code: None,
    } = scope
    {
        let last_inclusive = day_end - chrono::Duration::nanoseconds(1);
        let sample = analytics_client
            .query_request_logs(&[
                ("start_time", day_start.to_rfc3339()),
                ("end_time", last_inclusive.to_rfc3339()),
                ("event_target", "gateway".to_string()),
                ("tenant_id", tenant_id.to_string()),
                ("page", "1".to_string()),
                ("page_size", "1".to_string()),
            ])
            .await
            .map_err(|e| format!("gateway tenant lookup failed: {e}"))?;
        let resolved_code = sample
            .get("items")
            .or_else(|| sample.get("logs"))
            .and_then(serde_json::Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("tenant_code"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let Some(code) = resolved_code else {
            // Zero gateway logs for this tenant_id in the window -> 0 billable calls.
            return Ok(0);
        };
        let with_code = ReconciliationScope::Tenant {
            tenant_id: *tenant_id,
            tenant_code: Some(code),
        };
        if let Some(olap_body) = build_gateway_billable_olap_query(day_start, day_end, &with_code) {
            let resp = analytics_client
                .olap_query(&olap_body)
                .await
                .map_err(|e| format!("gateway billing_funnel_ch olap query failed: {e}"))?;
            let count = resp
                .get("rows")
                .and_then(serde_json::Value::as_array)
                .and_then(|rows| rows.first())
                .and_then(|row| row.get("billable"))
                .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(u64::saturating_from)))
                .map(i64::saturating_from)
                .ok_or_else(|| "billing_funnel_ch response missing rows[0].billable".to_string())?;
            return Ok(count);
        }
    }

    Ok(0)
}

/// Ratio: billable `usage_records` vs gateway billable access count. Use max(sides, 1) as
/// denominator so a near-empty side does not explode the ratio.
fn compute_loss_ratio(billable_call_count: i64, gateway_call_count: i64) -> Decimal {
    let billable = Decimal::from(billable_call_count);
    let gateway = Decimal::from(gateway_call_count);
    let diff = (billable - gateway).abs();
    let denominator = if billable_call_count == 0 && gateway_call_count == 0 {
        Decimal::ONE
    } else {
        Decimal::from(billable_call_count.max(gateway_call_count).max(1))
    };
    diff / denominator
}

/// 对账判定：analytics 任一来源失败 → `error`；否则按阈值 `ok` / `flagged`。
fn classify_reconciliation(
    billable_call_count: i64,
    gateway: &Result<i64, String>,
    upstream_preagg: &Result<i64, String>,
    threshold: Decimal,
) -> (&'static str, Decimal, Option<String>) {
    let errors: Vec<&str> = [gateway, upstream_preagg]
        .into_iter()
        .filter_map(|r| r.as_ref().err().map(String::as_str))
        .collect();
    if !errors.is_empty() {
        return ("error", Decimal::ZERO, Some(errors.join("; ")));
    }
    let gateway_call_count = gateway.as_ref().map_or(0, |v| *v);
    let ratio = compute_loss_ratio(billable_call_count, gateway_call_count);
    let status = if ratio > threshold { "flagged" } else { "ok" };
    (status, ratio, None)
}

async fn amount_recon_details(
    billing_db: &PgPool,
    day_start: chrono::DateTime<chrono::Utc>,
    day_end: chrono::DateTime<chrono::Utc>,
    scope: &ReconciliationScope,
) -> Result<serde_json::Value, BillingError> {
    let (tenant_filter, product_filter) = match scope {
        ReconciliationScope::Global => (None, None),
        ReconciliationScope::Tenant { tenant_id, .. } => (Some(*tenant_id), None),
        ReconciliationScope::Product { product_id } => (None, Some(product_id.as_str())),
    };
    let usage_amount = repo::sum_usage_billable_amount_scoped(
        billing_db,
        day_start,
        day_end,
        tenant_filter,
        product_filter,
    )
    .await
    .map_err(|e| {
        error!("Database error summing usage billable_amount: {}", e);
        BillingError::Database
    })?;
    let bill_items_amount =
        repo::sum_bill_items_total_for_day_scoped(billing_db, day_start, day_end, tenant_filter)
            .await
            .map_err(|e| {
                error!("Database error summing bill_items for day: {}", e);
                BillingError::Database
            })?;
    let amount_diff = if usage_amount >= bill_items_amount {
        usage_amount - bill_items_amount
    } else {
        bill_items_amount - usage_amount
    };
    Ok(serde_json::json!({
        "usage_billable_amount": usage_amount.to_string(),
        "bill_items_total": bill_items_amount.to_string(),
        "amount_diff": amount_diff.to_string(),
    }))
}

/// Half-open UTC day bounds `[00:00:00.000000Z, next_day 00:00:00.000000Z)`.
///
/// # Errors
///
/// Returns `BillingError::Validation` if `reconciliation_date` overflows `NaiveDate::succ_opt`
/// or cannot construct midnight timestamps.
pub fn reconciliation_day_bounds(
    reconciliation_date: chrono::NaiveDate,
) -> Result<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>), BillingError> {
    let day_start = reconciliation_date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| {
            BillingError::Validation(format!(
                "invalid reconciliation day start for date {reconciliation_date}"
            ))
        })?
        .and_utc();
    let next_date = reconciliation_date.succ_opt().ok_or_else(|| {
        BillingError::Validation(format!(
            "invalid reconciliation next day for date {reconciliation_date}"
        ))
    })?;
    let day_end = next_date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| {
            BillingError::Validation(format!(
                "invalid reconciliation day end for date {reconciliation_date}"
            ))
        })?
        .and_utc();

    Ok((day_start, day_end))
}

/// Upstream (supplier-side) call count over `[day_start, day_end)` (reference value).
async fn fetch_preagg_call_count(
    analytics_client: &AnalyticsHttpClient,
    day_start: chrono::DateTime<chrono::Utc>,
    day_end: chrono::DateTime<chrono::Utc>,
    scope: &ReconciliationScope,
) -> Result<i64, String> {
    match scope {
        ReconciliationScope::Global => {
            let hourly_val = analytics_client
                .fetch_upstream_hourly_usage(&day_start.to_rfc3339(), &day_end.to_rfc3339(), None, None)
                .await
                .map_err(|e| format!("upstream hourly summary failed: {e}"))?;
            Ok(hourly_val.as_array().map_or(0, |arr| {
                arr.iter()
                    .filter_map(|row| row.get("total_calls").and_then(serde_json::Value::as_i64))
                    .sum()
            }))
        }
        ReconciliationScope::Tenant { .. } | ReconciliationScope::Product { .. } => {
            // Bypass upstream logs can be filtered by tenant_id / product_id via request histogram
            // over the exact half-open [day_start, day_end) window.
            let mut params = vec![
                ("start_time", day_start.to_rfc3339()),
                ("end_time", day_end.to_rfc3339()),
                ("event_target", "bypass".to_string()),
            ];
            match scope {
                ReconciliationScope::Tenant { tenant_id, .. } => {
                    params.push(("tenant_id", tenant_id.to_string()));
                }
                ReconciliationScope::Product { product_id } => {
                    params.push(("product_id", product_id.clone()));
                }
                ReconciliationScope::Global => {}
            }
            let hist = analytics_client
                .request_histogram(&params)
                .await
                .map_err(|e| format!("scoped upstream histogram failed: {e}"))?;
            Ok(hist
                .get("total")
                .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(u64::saturating_from)))
                .map_or(0, i64::saturating_from))
        }
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn execute_reconciliation_audit(
    billing_db: &PgPool,
    analytics_client: &AnalyticsHttpClient,
    reconciliation_date: chrono::NaiveDate,
    scope_type: &str,
    scope_id: &str,
    options: ReconciliationAuditOptions,
) -> Result<ReconciliationAuditResult, BillingError> {
    let scope = parse_reconciliation_scope(scope_type, scope_id)?;
    let loss_ratio_threshold = options.loss_ratio_threshold;
    let (day_start, day_end) = reconciliation_day_bounds(reconciliation_date)?;

    let (tenant_filter, product_filter) = match &scope {
        ReconciliationScope::Global => (None, None),
        ReconciliationScope::Tenant { tenant_id, .. } => (Some(*tenant_id), None),
        ReconciliationScope::Product { product_id } => (None, Some(product_id.as_str())),
    };

    let billable_call_count: i64 = sqlx::query_scalar(RECON_USAGE_COUNT_SQL)
        .bind(day_start)
        .bind(day_end)
        .bind(tenant_filter)
        .bind(product_filter)
        .fetch_one(billing_db)
        .await
        .map_err(|e| {
            error!(
                "Database error counting usage_records for reconciliation: {}",
                e
            );
            BillingError::Database
        })?;

    let gateway =
        fetch_gateway_billable_count(analytics_client, day_start, day_end, &scope).await;
    let upstream_preagg =
        fetch_preagg_call_count(analytics_client, day_start, day_end, &scope).await;
    let (status, loss_ratio, error_reason) = classify_reconciliation(
        billable_call_count,
        &gateway,
        &upstream_preagg,
        loss_ratio_threshold,
    );
    if let Some(reason) = &error_reason {
        error!("reconciliation audit {reconciliation_date} analytics error: {reason}");
    }
    let gateway_call_count = gateway.unwrap_or(0);
    let upstream_preagg_call_count = upstream_preagg.unwrap_or(0);

    let mut details = serde_json::json!({
        "billable_calls": billable_call_count,
        "gateway_calls": gateway_call_count,
        "upstream_preagg_calls": upstream_preagg_call_count,
        "audit_date": reconciliation_date.to_string(),
        "window_start": day_start.to_rfc3339(),
        "window_end": day_end.to_rfc3339(),
        "loss_ratio_threshold": loss_ratio_threshold.to_string(),
        "notes": "billable=usage_records(charged & billable_amount>0); gateway=gateway request_logs FINAL (billable=1); upstream_preagg=bypass request_logs (reference)",
    });
    if let (Some(reason), Some(obj)) = (&error_reason, details.as_object_mut()) {
        obj.insert(
            "error".to_string(),
            serde_json::Value::String(reason.clone()),
        );
    }

    if options.include_amount_recon {
        let amount_recon = amount_recon_details(billing_db, day_start, day_end, &scope).await?;
        if let Some(obj) = details.as_object_mut() {
            obj.insert("amount_recon".to_string(), amount_recon);
        }
    }

    let discrepancy_id = repo::insert_reconciliation_discrepancy(
        billing_db,
        repo::InsertReconciliationDiscrepancyParams {
            reconciliation_date,
            scope_type,
            scope_id,
            billable_call_count,
            upstream_preagg_call_count,
            gateway_call_count,
            discrepancy_ratio: loss_ratio,
            status,
            details: Some(details),
        },
    )
    .await
    .map_err(|e| {
        error!(
            "Database error inserting reconciliation discrepancy: {}",
            e
        );
        BillingError::Database
    })?;

    Ok(ReconciliationAuditResult {
        discrepancy_id,
        reconciliation_date,
        scope_type: scope_type.to_string(),
        scope_id: scope_id.to_string(),
        billable_call_count,
        upstream_preagg_call_count,
        gateway_call_count,
        error: error_reason,
        loss_ratio,
        loss_ratio_threshold,
        status: status.to_string(),
    })
}

const ALLOWED_DISCREPANCY_STATUSES: &[&str] =
    &["ok", "flagged", "error", "acknowledged", "resolved"];

fn is_allowed_status_transition(from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }
    matches!(
        (from, to),
        ("ok" | "flagged" | "error", "acknowledged" | "resolved") | ("acknowledged", "resolved")
    )
}

/// `list_reconciliation_discrepancies` 的入参。
pub struct ListDiscrepanciesParams {
    pub from: Option<chrono::NaiveDate>,
    pub to: Option<chrono::NaiveDate>,
    pub status: Option<String>,
    pub scope_type: Option<String>,
    pub scope_id: Option<String>,
    pub search: Option<String>,
    pub filters_json: Option<String>,
    pub page: u32,
    pub page_size: u32,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_reconciliation_discrepancies(
    billing_db: &PgPool,
    params: ListDiscrepanciesParams,
) -> Result<ReconciliationDiscrepancyListResponse, BillingError> {
    let ListDiscrepanciesParams {
        from,
        to,
        status,
        scope_type,
        scope_id,
        search,
        filters_json,
        page,
        page_size,
    } = params;
    validate_limit(page_size)?;
    let page = page.max(1);
    let status = status
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    if let Some(ref s) = status
        && !ALLOWED_DISCREPANCY_STATUSES.contains(&s.as_str())
    {
        return Err(BillingError::Validation(format!(
            "status must be one of: {}",
            ALLOWED_DISCREPANCY_STATUSES.join(", ")
        )));
    }
    let scope_type = scope_type
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let scope_id = scope_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let search = search
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let filters = crate::list_query::parse_and_validate_filters(
        filters_json.as_deref(),
        &crate::list_query::reconciliation_catalog(),
    )
    .map_err(|e| BillingError::Validation(e.message))?;
    let offset = i64::from(page.saturating_sub(1).saturating_mul(page_size));
    let list_params = repo::ListReconciliationDiscrepanciesParams {
        from,
        to,
        status: status.as_deref(),
        scope_type: scope_type.as_deref(),
        scope_id: scope_id.as_deref(),
        search: search.as_deref(),
        filters: &filters,
        limit: i64::from(page_size),
        offset,
    };
    let total = repo::count_reconciliation_discrepancies(billing_db, &list_params)
        .await
        .map_err(|e| {
            error!(
                "Database error counting reconciliation discrepancies: {}",
                e
            );
            BillingError::Database
        })?;
    let items = repo::list_reconciliation_discrepancies(billing_db, list_params)
        .await
        .map_err(|e| {
            error!("Database error listing reconciliation discrepancies: {}", e);
            BillingError::Database
        })?;
    let total_pages = if total == 0 {
        0
    } else {
        mazbri_core::num::f64_to_u64(
            (mazbri_core::num::i64_to_f64(total) / f64::from(page_size)).ceil(),
        )
    };
    Ok(ReconciliationDiscrepancyListResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
    })
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_reconciliation_discrepancy(
    billing_db: &PgPool,
    id: uuid::Uuid,
    new_status: &str,
    resolution_note: Option<&str>,
    resolved_by: Option<uuid::Uuid>,
) -> Result<repo::ReconciliationDiscrepancyRow, BillingError> {
    let new_status = new_status.trim().to_lowercase();
    if !ALLOWED_DISCREPANCY_STATUSES.contains(&new_status.as_str()) {
        return Err(BillingError::Validation(format!(
            "status must be one of: {}",
            ALLOWED_DISCREPANCY_STATUSES.join(", ")
        )));
    }
    if new_status != "acknowledged" && new_status != "resolved" {
        return Err(BillingError::Validation(
            "PATCH only allows status=acknowledged or status=resolved".to_string(),
        ));
    }

    let existing = repo::get_reconciliation_discrepancy(billing_db, id)
        .await
        .map_err(|e| {
            error!(
                "Database error fetching reconciliation discrepancy {}: {}",
                id, e
            );
            BillingError::Database
        })?
        .ok_or(BillingError::DiscrepancyNotFound(id))?;

    if existing.status == "resolved" && new_status != "resolved" {
        return Err(BillingError::Validation(
            "resolved discrepancies cannot transition to another status".to_string(),
        ));
    }
    if !is_allowed_status_transition(&existing.status, &new_status) {
        return Err(BillingError::Validation(format!(
            "invalid status transition from '{}' to '{}'",
            existing.status, new_status
        )));
    }

    let note = resolution_note.map(str::trim).filter(|s| !s.is_empty());

    repo::update_reconciliation_discrepancy(
        billing_db,
        repo::UpdateReconciliationDiscrepancyParams {
            id,
            status: &new_status,
            resolution_note: note,
            resolved_by,
        },
    )
    .await
    .map_err(|e| {
        error!(
            "Database error updating reconciliation discrepancy {}: {}",
            id, e
        );
        BillingError::Database
    })?
    .ok_or(BillingError::DiscrepancyNotFound(id))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_reconciliation_discrepancy(
    billing_db: &PgPool,
    id: uuid::Uuid,
) -> Result<uuid::Uuid, BillingError> {
    let deleted = repo::delete_reconciliation_discrepancy(billing_db, id)
        .await
        .map_err(|e| {
            error!(
                "Database error deleting reconciliation discrepancy {}: {}",
                id, e
            );
            BillingError::Database
        })?;
    deleted.ok_or(BillingError::DiscrepancyNotFound(id))
}

const ALLOWED_BILL_ADJUSTMENT_TYPES: &[&str] = &["credit", "debit", "correction", "other"];

/// Option A: ledger-only bill adjustment (does NOT change `tenant_accounts.balance`).
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_bill_adjustment(
    billing_db: &PgPool,
    bill_id: uuid::Uuid,
    adjustment_type: &str,
    amount: Decimal,
    reason: &str,
    operator_id: Option<uuid::Uuid>,
) -> Result<repo::BillingAdjustmentRow, BillingError> {
    let adjustment_type = adjustment_type.trim().to_lowercase();
    if !ALLOWED_BILL_ADJUSTMENT_TYPES.contains(&adjustment_type.as_str()) {
        return Err(BillingError::Validation(format!(
            "adjustment_type must be one of: {}",
            ALLOWED_BILL_ADJUSTMENT_TYPES.join(", ")
        )));
    }
    if amount <= Decimal::ZERO {
        return Err(BillingError::Validation(
            "amount must be greater than zero".to_string(),
        ));
    }
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(BillingError::Validation("reason is required".to_string()));
    }

    let bill = repo::get_bill(billing_db, bill_id)
        .await
        .map_err(|e| {
            error!("Database error fetching bill {}: {}", bill_id, e);
            BillingError::Database
        })?
        .ok_or(BillingError::BillNotFound(bill_id))?;

    let id = repo::insert_billing_adjustment(
        billing_db,
        repo::InsertBillingAdjustmentParams {
            bill_id: Some(bill_id),
            tenant_id: bill.tenant_id,
            adjustment_type: &adjustment_type,
            amount,
            reason,
            reference_period_start: bill.period_start,
            reference_period_end: bill.period_end,
            status: "posted",
            operator_id,
        },
    )
    .await
    .map_err(|e| {
        error!("Database error inserting billing adjustment: {}", e);
        BillingError::Database
    })?;

    repo::get_billing_adjustment(billing_db, id)
        .await
        .map_err(|e| {
            error!("Database error loading billing adjustment {}: {}", id, e);
            BillingError::Database
        })?
        .ok_or(BillingError::AdjustmentNotFound(id))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_bill_adjustments_for_bill(
    billing_db: &PgPool,
    bill_id: uuid::Uuid,
) -> Result<Vec<repo::BillingAdjustmentRow>, BillingError> {
    let _ = repo::get_bill(billing_db, bill_id)
        .await
        .map_err(|e| {
            error!("Database error fetching bill {}: {}", bill_id, e);
            BillingError::Database
        })?
        .ok_or(BillingError::BillNotFound(bill_id))?;

    repo::list_billing_adjustments_for_bill(billing_db, bill_id)
        .await
        .map_err(|e| {
            error!(
                "Database error listing adjustments for bill {}: {}",
                bill_id, e
            );
            BillingError::Database
        })
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_billing_adjustments(
    billing_db: &PgPool,
    bill_id: Option<uuid::Uuid>,
    tenant_id: Option<uuid::Uuid>,
    status: Option<String>,
    page: u32,
    page_size: u32,
) -> Result<BillingAdjustmentListResponse, BillingError> {
    validate_limit(page_size)?;
    let page = page.max(1);
    let status = status
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    if let Some(ref s) = status
        && s != "posted"
        && s != "void"
    {
        return Err(BillingError::Validation(
            "status must be posted or void".to_string(),
        ));
    }
    let offset = i64::from(page.saturating_sub(1).saturating_mul(page_size));
    let params = repo::ListBillingAdjustmentsParams {
        bill_id,
        tenant_id,
        status: status.as_deref(),
        limit: i64::from(page_size),
        offset,
    };
    let total = repo::count_billing_adjustments(billing_db, &params)
        .await
        .map_err(|e| {
            error!("Database error counting billing adjustments: {}", e);
            BillingError::Database
        })?;
    let items = repo::list_billing_adjustments(billing_db, params)
        .await
        .map_err(|e| {
            error!("Database error listing billing adjustments: {}", e);
            BillingError::Database
        })?;
    let total_pages = if total == 0 {
        0
    } else {
        mazbri_core::num::f64_to_u64(
            (mazbri_core::num::i64_to_f64(total) / f64::from(page_size)).ceil(),
        )
    };
    Ok(BillingAdjustmentListResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
    })
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn void_bill_adjustment(
    billing_db: &PgPool,
    adjustment_id: uuid::Uuid,
) -> Result<repo::BillingAdjustmentRow, BillingError> {
    let existing = repo::get_billing_adjustment(billing_db, adjustment_id)
        .await
        .map_err(|e| {
            error!(
                "Database error fetching billing adjustment {}: {}",
                adjustment_id, e
            );
            BillingError::Database
        })?
        .ok_or(BillingError::AdjustmentNotFound(adjustment_id))?;

    if existing.status == "void" {
        return Ok(existing);
    }

    repo::void_billing_adjustment(billing_db, adjustment_id)
        .await
        .map_err(|e| {
            error!(
                "Database error voiding billing adjustment {}: {}",
                adjustment_id, e
            );
            BillingError::Database
        })?
        .ok_or(BillingError::AdjustmentNotFound(adjustment_id))
}

/// Inclusive calendar-date span of a half-open `[period_start, period_end)` billing period.
#[must_use]
pub fn inclusive_date_span_for_half_open_period(
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> (chrono::NaiveDate, chrono::NaiveDate) {
    let from = period_start.date_naive();
    let effective_end = if period_end > period_start {
        period_end - chrono::Duration::nanoseconds(1)
    } else {
        period_end
    };
    (from, effective_end.date_naive())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn open_discrepancy_warning_for_period(
    billing_db: &PgPool,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<Option<String>, BillingError> {
    let (from, to) = inclusive_date_span_for_half_open_period(period_start, period_end);
    let count = repo::count_open_discrepancies_in_range(billing_db, from, to)
        .await
        .map_err(|e| {
            error!("Database error counting open discrepancies: {}", e);
            BillingError::Database
        })?;
    if count > 0 {
        Ok(Some(format!(
            "{count} open usage-reconciliation discrepancies (flagged/acknowledged) overlap period {from}..{to}"
        )))
    } else {
        Ok(None)
    }
}

/// 定时对账（后台 supervisor）的系统审计事件：`actor_type = system`，无操作人。
///
/// `outcome` 为对账结果或错误描述；`alert_sent` 表示 flagged 告警 webhook 是否发送成功。
#[must_use]
pub fn scheduled_reconciliation_audit(
    outcome: Result<&ReconciliationAuditResult, &str>,
    alert_sent: Option<bool>,
) -> crate::audit::AuditLogRequest {
    use crate::audit::{ActorType, AuditAction, AuditLogRequest, AuditResourceType, AuditStatus};
    let base = AuditLogRequest {
        actor_type: ActorType::System,
        action: AuditAction::ReconciliationRun,
        resource_type: AuditResourceType::Billing,
        ..AuditLogRequest::default()
    };
    match outcome {
        Ok(result) => AuditLogRequest {
            resource_id: Some(result.discrepancy_id.to_string()),
            details: Some(serde_json::json!({
                "trigger": "schedule",
                "reconciliation_date": result.reconciliation_date.to_string(),
                "scope_type": result.scope_type,
                "scope_id": result.scope_id,
                "result_status": result.status,
                "billable_call_count": result.billable_call_count,
                "upstream_preagg_call_count": result.upstream_preagg_call_count,
                "gateway_call_count": result.gateway_call_count,
                "loss_ratio": result.loss_ratio.to_string(),
                "loss_ratio_threshold": result.loss_ratio_threshold.to_string(),
                "alert_sent": alert_sent,
            })),
            ..base
        },
        Err(error) => AuditLogRequest {
            details: Some(serde_json::json!({ "trigger": "schedule" })),
            status: AuditStatus::Failure,
            error_message: Some(error.chars().take(500).collect()),
            failure_reason: Some("server_error".to_string()),
            ..base
        },
    }
}

#[cfg(test)]
mod recon_classify_tests {
    use super::{
        RECON_USAGE_COUNT_SQL, ReconciliationScope, build_gateway_billable_olap_query,
        classify_reconciliation, inclusive_date_span_for_half_open_period,
        parse_reconciliation_scope, reconciliation_day_bounds, send_reconciliation_alert_webhook,
    };
    use chrono::{DateTime, NaiveDate, Utc};
    use mazbri_core::net::ssrf::SsrfChecker;
    use rust_decimal::Decimal;
    use std::time::Duration;

    fn threshold() -> Decimal {
        Decimal::new(5, 2)
    }

    #[test]
    fn normal_is_ok() {
        let (status, _, err) = classify_reconciliation(100, &Ok(100), &Ok(100), threshold());
        assert_eq!(status, "ok");
        assert!(err.is_none());
    }

    #[test]
    fn analytics_failure_is_error() {
        let (status, ratio, err) =
            classify_reconciliation(100, &Err("down".into()), &Ok(100), threshold());
        assert_eq!(status, "error");
        assert_eq!(ratio, Decimal::ZERO);
        assert_eq!(err.as_deref(), Some("down"));
        let (status, _, _) =
            classify_reconciliation(100, &Ok(100), &Err("preagg down".into()), threshold());
        assert_eq!(status, "error");
    }

    #[test]
    fn over_threshold_is_flagged() {
        let (status, _, _) = classify_reconciliation(100, &Ok(80), &Ok(80), threshold());
        assert_eq!(status, "flagged");
    }

    #[test]
    fn reconciliation_day_bounds_use_next_day_midnight_exclusive() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 10).expect("date");
        let (start, end) = reconciliation_day_bounds(d).expect("bounds");
        assert_eq!(start.to_rfc3339(), "2026-11-10T00:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-11-11T00:00:00+00:00");

        // Sub-second event at 23:59:59.999999Z must fall strictly inside [start, end)
        let last_micro = DateTime::parse_from_rfc3339("2026-11-10T23:59:59.999999Z")
            .expect("dt")
            .with_timezone(&Utc);
        assert!(last_micro >= start && last_micro < end);
        assert!(!(end >= start && end < end));

        // Half-open period [2026-11-10T00:00:00Z, 2026-11-11T00:00:00Z) maps to single date 2026-11-10
        let (from_d, to_d) = inclusive_date_span_for_half_open_period(start, end);
        assert_eq!(from_d, d);
        assert_eq!(to_d, d);

        assert!(RECON_USAGE_COUNT_SQL.contains("recorded_at >= $1"));
        assert!(RECON_USAGE_COUNT_SQL.contains("recorded_at < $2"));
        assert!(RECON_USAGE_COUNT_SQL.contains("charge_status = 'charged'"));
        assert!(RECON_USAGE_COUNT_SQL.contains("billable_amount > 0"));
    }

    #[test]
    fn scope_parsing_and_olap_query_pushdown() {
        assert_eq!(
            parse_reconciliation_scope("system", "").expect("system"),
            ReconciliationScope::Global
        );
        assert!(parse_reconciliation_scope("system", "unexpected").is_err());
        assert!(parse_reconciliation_scope("unknown", "x").is_err());

        let prod = parse_reconciliation_scope("product", "8000000001").expect("product");
        assert_eq!(
            prod,
            ReconciliationScope::Product {
                product_id: "8000000001".to_string()
            }
        );

        let d = NaiveDate::from_ymd_opt(2026, 11, 10).expect("date");
        let (start, end) = reconciliation_day_bounds(d).expect("bounds");
        let olap = build_gateway_billable_olap_query(start, end, &prod).expect("olap body");
        assert_eq!(olap["template"], "billing_funnel_ch");
        assert_eq!(olap["params"]["product_id"], "8000000001");
        assert_eq!(olap["params"]["start"], start.timestamp());
        assert_eq!(olap["params"]["end"], end.timestamp());

        let tid = uuid::Uuid::new_v4();
        let tenant_with_code =
            parse_reconciliation_scope("tenant", &format!("{tid}:TENANT_A")).expect("tenant");
        let tenant_olap =
            build_gateway_billable_olap_query(start, end, &tenant_with_code).expect("tenant olap");
        assert_eq!(tenant_olap["params"]["tenant_code"], "TENANT_A");
    }

    #[tokio::test]
    async fn reconciliation_webhook_enforces_ssrf_and_blocks_redirects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let strict_ssrf = SsrfChecker::new();
        let strict_client = strict_ssrf
            .build_safe_http_client(Duration::from_secs(5), Duration::from_secs(10))
            .expect("safe client");
        let payload = serde_json::json!({"event": "billing.reconciliation.flagged"});

        for blocked in [
            "http://127.0.0.1:8080/hook",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::ffff:127.0.0.1]:8080/hook",
        ] {
            let err = send_reconciliation_alert_webhook(&strict_ssrf, &strict_client, blocked, &payload)
                .await
                .expect_err("SSRF target must be rejected");
            assert!(err.contains("SSRF blocked"));
        }

        // Redirect response (302) must NOT be followed and must fail as non-2xx:
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest\r\nContent-Length: 0\r\n\r\n",
                    )
                    .await;
            }
        });
        let allow_local = SsrfChecker::with_allow_private(true);
        let local_client = allow_local
            .build_safe_http_client(Duration::from_secs(5), Duration::from_secs(10))
            .expect("local safe client");
        let err = send_reconciliation_alert_webhook(
            &allow_local,
            &local_client,
            &format!("http://{addr}/hook"),
            &payload,
        )
        .await
        .expect_err("302 redirect must not be followed and must report non-2xx");
        assert!(err.contains("302"));
    }
}

#[cfg(test)]
mod scheduled_reconciliation_audit_tests {
    use super::{Decimal, ReconciliationAuditResult, scheduled_reconciliation_audit};
    use crate::audit::{ActorType, AuditAction, AuditResourceType, AuditStatus};

    fn result() -> ReconciliationAuditResult {
        ReconciliationAuditResult {
            discrepancy_id: uuid::Uuid::nil(),
            reconciliation_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 25).expect("date"),
            scope_type: "global".to_string(),
            scope_id: String::new(),
            billable_call_count: 10,
            upstream_preagg_call_count: 9,
            gateway_call_count: 10,
            error: None,
            loss_ratio: Decimal::new(1, 1),
            loss_ratio_threshold: Decimal::new(5, 2),
            status: "flagged".to_string(),
        }
    }

    #[test]
    fn success_is_system_event_with_summary() {
        let r = result();
        let req = scheduled_reconciliation_audit(Ok(&r), Some(true));
        assert_eq!(req.actor_type, ActorType::System);
        assert!(req.user_id.is_none());
        assert_eq!(req.action, AuditAction::ReconciliationRun);
        assert_eq!(req.resource_type, AuditResourceType::Billing);
        assert_eq!(req.status, AuditStatus::Success);
        let details = req.details.expect("details");
        assert_eq!(details["result_status"], "flagged");
        assert_eq!(details["reconciliation_date"], "2026-10-25");
        assert_eq!(details["alert_sent"], true);
    }

    #[test]
    fn failure_records_truncated_error() {
        let long = "x".repeat(800);
        let req = scheduled_reconciliation_audit(Err(&long), None);
        assert_eq!(req.status, AuditStatus::Failure);
        assert_eq!(req.failure_reason.as_deref(), Some("server_error"));
        assert_eq!(req.error_message.map(|m| m.len()), Some(500));
        assert!(req.resource_id.is_none());
    }
}
