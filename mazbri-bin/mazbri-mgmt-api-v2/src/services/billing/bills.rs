use super::*;

pub async fn list_bills(
    db: &PgPool,
    billing_db: &PgPool,
    tenant_id: Option<uuid::Uuid>,
    search: Option<String>,
    status: Option<String>,
    filters_json: Option<String>,
    page: u32,
    limit: u32,
) -> Result<BillListResponse, BillingError> {
    validate_limit(limit)?;
    let page = page.max(1);
    let status = normalize_bill_status(&status)?;
    let search = search
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let search_pattern = search.as_ref().map(|value| format!("%{}%", value));
    let search_ref = search_pattern.as_deref();
    let filters = crate::list_query::parse_and_validate_filters(
        filters_json.as_deref(),
        &crate::list_query::bills_catalog(),
    )
    .map_err(|e| BillingError::Validation(e.message))?;
    // Customer bills store empty party_*; UI shows platform tenant name/code.
    // Resolve those identities on the platform DB, then filter billing rows by tenant_id.
    let matched_tenant_ids = if let Some(ref needle) = search {
        tenant_repo::search_tenant_ids(db, needle)
            .await
            .map_err(|e| {
                error!("Database error resolving tenants for bill search: {}", e);
                BillingError::Database
            })?
    } else {
        Vec::new()
    };
    let offset = page.saturating_sub(1).saturating_mul(limit) as i64;
    let total = repo::count_bills(
        billing_db,
        tenant_id,
        search_ref,
        &matched_tenant_ids,
        status.as_deref(),
        &filters,
    )
    .await
    .map_err(|e| {
        error!("Database error counting bills: {}", e);
        BillingError::Database
    })?;
    let bills = repo::list_bills(
        billing_db,
        tenant_id,
        search_ref,
        &matched_tenant_ids,
        status.as_deref(),
        &filters,
        limit as i64,
        offset,
    )
    .await
    .map_err(|e| {
        error!("Database error listing bills: {}", e);
        BillingError::Database
    })?;
    let bills = enrich_bill_rows(db, bills).await?;
    let total_pages = (total as f64 / limit as f64).ceil() as u64;
    Ok(BillListResponse {
        bills,
        total,
        page,
        limit,
        total_pages,
    })
}
pub async fn get_bill(
    db: &PgPool,
    billing_db: &PgPool,
    bill_id: uuid::Uuid,
) -> Result<BillDetailResponse, BillingError> {
    let bill = repo::get_bill(billing_db, bill_id)
        .await
        .map_err(|e| {
            error!("Database error fetching bill {}: {}", bill_id, e);
            BillingError::Database
        })?
        .ok_or(BillingError::BillNotFound(bill_id))?;
    let bill = enrich_bill_rows(db, vec![bill])
        .await?
        .into_iter()
        .next()
        .ok_or(BillingError::Database)?;
    let items = repo::list_bill_items(billing_db, &bill)
        .await
        .map_err(|e| {
            error!("Database error listing bill items {}: {}", bill_id, e);
            BillingError::Database
        })?;
    Ok(BillDetailResponse { bill, items })
}
pub async fn delete_bill(pool: &PgPool, bill_id: uuid::Uuid) -> Result<uuid::Uuid, BillingError> {
    let deleted_id = repo::delete_bill(pool, bill_id)
        .await
        .map_err(|e| {
            error!("Database error deleting bill {}: {}", bill_id, e);
            BillingError::Database
        })?
        .ok_or(BillingError::BillNotFound(bill_id))?;
    Ok(deleted_id)
}

/// Allowed transitions for operator status flow (H-17).
pub(super) fn can_transition_bill_status(from: &BillStatus, to: &BillStatus) -> bool {
    use BillStatus::*;
    matches!(
        (from, to),
        (Pending, Processing)
            | (Pending, Paid)
            | (Pending, Overdue)
            | (Pending, Cancelled)
            | (Processing, Paid)
            | (Processing, Overdue)
            | (Processing, Cancelled)
            | (Overdue, Paid)
            | (Overdue, Cancelled)
    )
}

pub async fn update_bill_status(
    db: &PgPool,
    billing_db: &PgPool,
    bill_id: uuid::Uuid,
    new_status: &str,
) -> Result<BillDetailResponse, BillingError> {
    let target = BillStatus::from_str(&new_status.to_lowercase()).map_err(|e| {
        BillingError::InvalidBillStatus(format!(
            "Invalid status '{}': {}. Valid: pending, processing, paid, overdue, cancelled",
            new_status, e
        ))
    })?;
    let current = repo::get_bill(billing_db, bill_id)
        .await
        .map_err(|e| {
            error!("Database error fetching bill {}: {}", bill_id, e);
            BillingError::Database
        })?
        .ok_or(BillingError::BillNotFound(bill_id))?;
    let from = BillStatus::from_str(&current.status)
        .map_err(|e| BillingError::InvalidBillStatus(format!("Stored status invalid: {}", e)))?;
    if from == target {
        return get_bill(db, billing_db, bill_id).await;
    }
    if !can_transition_bill_status(&from, &target) {
        return Err(BillingError::Validation(format!(
            "Cannot transition bill status from '{}' to '{}'",
            from, target
        )));
    }
    let paid_at = if target == BillStatus::Paid {
        Some(Utc::now())
    } else {
        None
    };
    let updated = repo::update_bill_status(billing_db, bill_id, &target.to_string(), paid_at)
        .await
        .map_err(|e| {
            error!("Database error updating bill status {}: {}", bill_id, e);
            BillingError::Database
        })?;
    if !updated {
        return Err(BillingError::BillNotFound(bill_id));
    }
    get_bill(db, billing_db, bill_id).await
}
