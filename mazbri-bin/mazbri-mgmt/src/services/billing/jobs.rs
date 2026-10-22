use super::{
    AnalyticsHttpClient, BillGenerationSummary, BillingAdjustmentListResponse, BillingError,
    DateTime, Decimal, PgPool, ReconciliationAuditResult, ReconciliationDiscrepancyListResponse,
    UpstreamBillScope, Utc, error, generate_upstream_bill, repo, validate_limit,
};
use mazbri_core::num::SaturatingFrom;

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

/// 网关侧 `request_logs` 当日请求数（主比对源）。
async fn fetch_gateway_request_count(
    analytics_client: &AnalyticsHttpClient,
    day_start: chrono::DateTime<chrono::Utc>,
    day_end: chrono::DateTime<chrono::Utc>,
) -> Result<i64, String> {
    let body = analytics_client
        .aggregate_requests(
            &day_start.to_rfc3339(),
            &day_end.to_rfc3339(),
            Some("mazbri-gateway"),
        )
        .await
        .map_err(|e| format!("gateway request_logs aggregate failed: {e}"))?;
    body.get("total_requests")
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(u64::saturating_from)))
        .map(i64::saturating_from)
        .ok_or_else(|| "gateway request_logs aggregate missing total_requests".to_string())
}

/// Ratio: billable `usage_records` vs gateway access count. Use max(sides, 1) as
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
) -> Result<serde_json::Value, BillingError> {
    let usage_amount = repo::sum_usage_billable_amount(billing_db, day_start, day_end)
        .await
        .map_err(|e| {
            error!("Database error summing usage billable_amount: {}", e);
            BillingError::Database
        })?;
    let bill_items_amount = repo::sum_bill_items_total_for_day(billing_db, day_start, day_end)
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

fn reconciliation_day_bounds(
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
    let day_end = reconciliation_date
        .and_hms_opt(23, 59, 59)
        .ok_or_else(|| {
            BillingError::Validation(format!(
                "invalid reconciliation day end for date {reconciliation_date}"
            ))
        })?
        .and_utc();

    Ok((day_start, day_end))
}

/// Upstream pre-aggregation (supplier-side): populated from bypass access logs only
/// after `mv_upstream_hourly_usage` filter — one hop per actual upstream call. 参考值。
async fn fetch_preagg_call_count(
    analytics_client: &AnalyticsHttpClient,
    day_start: chrono::DateTime<chrono::Utc>,
    day_end: chrono::DateTime<chrono::Utc>,
) -> Result<i64, String> {
    let summary_val = analytics_client
        .fetch_upstream_usage_summary(
            &day_start.to_rfc3339(),
            &day_end.to_rfc3339(),
            None,
            None,
            None,
            None,
        )
        .await
        .map_err(|e| format!("upstream preagg summary failed: {e}"))?;
    Ok(summary_val.as_array().map_or(0, |arr| {
        arr.iter()
            .filter_map(|row| row.get("total_calls").and_then(serde_json::Value::as_i64))
            .sum()
    }))
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
    let loss_ratio_threshold = options.loss_ratio_threshold;
    let (day_start, day_end) = reconciliation_day_bounds(reconciliation_date)?;

    let billable_call_count: i64 = sqlx::query_scalar(
        "SELECT COALESCE(COUNT(*), 0)::BIGINT FROM usage_records WHERE recorded_at >= $1 AND recorded_at <= $2",
    )
    .bind(day_start)
    .bind(day_end)
    .fetch_one(billing_db)
    .await
    .map_err(|e| {
        error!("Database error counting usage_records for reconciliation: {}", e);
        BillingError::Database
    })?;

    // usage_records is written once per billable gateway proxy call → compare to gateway
    // access logs (gateway rows only; bypass rows share request_id and would double count).
    let gateway = fetch_gateway_request_count(analytics_client, day_start, day_end).await;
    let upstream_preagg = fetch_preagg_call_count(analytics_client, day_start, day_end).await;
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
        "loss_ratio_threshold": loss_ratio_threshold.to_string(),
        "notes": "billable=usage_records; gateway=gateway request_logs (primary); upstream_preagg=upstream_hourly (reference)",
    });
    if let (Some(reason), Some(obj)) = (&error_reason, details.as_object_mut()) {
        obj.insert(
            "error".to_string(),
            serde_json::Value::String(reason.clone()),
        );
    }

    if options.include_amount_recon {
        let amount_recon = amount_recon_details(billing_db, day_start, day_end).await?;
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
        error!("Database error inserting reconciliation discrepancy: {}", e);
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
    if let Some(ref s) = status {
        if !ALLOWED_DISCREPANCY_STATUSES.contains(&s.as_str()) {
            return Err(BillingError::Validation(format!(
                "status must be one of: {}",
                ALLOWED_DISCREPANCY_STATUSES.join(", ")
            )));
        }
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
    // Ensure bill exists
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
    if let Some(ref s) = status {
        if s != "posted" && s != "void" {
            return Err(BillingError::Validation(
                "status must be posted or void".to_string(),
            ));
        }
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

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn open_discrepancy_warning_for_period(
    billing_db: &PgPool,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<Option<String>, BillingError> {
    let from = period_start.date_naive();
    let to = period_end.date_naive();
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

#[cfg(test)]
mod recon_classify_tests {
    use super::classify_reconciliation;
    use rust_decimal::Decimal;

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
}
