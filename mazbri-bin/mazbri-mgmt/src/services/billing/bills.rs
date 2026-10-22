use super::{
    BillDetailResponse, BillListResponse, BillStatus, BillingError, FromStr, PgPool, Utc,
    enrich_bill_rows, error, normalize_bill_status, repo, tenant_repo, validate_limit,
};

/// `list_bills` 的入参。
pub struct ListBillsParams<'a> {
    pub billing_db: &'a PgPool,
    pub tenant_id: Option<uuid::Uuid>,
    pub search: Option<String>,
    pub status: Option<String>,
    pub filters_json: Option<String>,
    pub page: u32,
    pub limit: u32,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_bills(
    db: &PgPool,
    params: ListBillsParams<'_>,
) -> Result<BillListResponse, BillingError> {
    let ListBillsParams {
        billing_db,
        tenant_id,
        search,
        status,
        filters_json,
        page,
        limit,
    } = params;
    validate_limit(limit)?;
    let page = page.max(1);
    let status = normalize_bill_status(status.as_ref())?;
    let search = search
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let search_pattern = search.as_ref().map(|value| format!("%{value}%"));
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
    let offset = i64::from(page.saturating_sub(1).saturating_mul(limit));
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
        repo::ListBillsQuery {
            tenant_id,
            search: search_ref,
            matched_tenant_ids: &matched_tenant_ids,
            status: status.as_deref(),
            filters: &filters,
            limit: i64::from(limit),
            offset,
        },
    )
    .await
    .map_err(|e| {
        error!("Database error listing bills: {}", e);
        BillingError::Database
    })?;
    let bills = enrich_bill_rows(db, bills).await?;
    let total_pages = mazbri_core::num::f64_to_u64(
        (mazbri_core::num::i64_to_f64(total) / f64::from(limit)).ceil(),
    );
    Ok(BillListResponse {
        bills,
        total,
        page,
        limit,
        total_pages,
    })
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
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
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
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
pub(super) fn can_transition_bill_status(from: BillStatus, to: BillStatus) -> bool {
    use BillStatus::{Cancelled, Overdue, Paid, Pending, Processing};
    matches!(
        (from, to),
        (Pending, Processing | Paid | Overdue | Cancelled)
            | (Processing | Overdue, Paid)
            | (Processing, Overdue | Cancelled)
            | (Overdue, Cancelled)
    )
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_bill_status(
    db: &PgPool,
    billing_db: &PgPool,
    bill_id: uuid::Uuid,
    new_status: &str,
) -> Result<BillDetailResponse, BillingError> {
    let target = BillStatus::from_str(&new_status.to_lowercase()).map_err(|e| {
        BillingError::InvalidBillStatus(format!(
            "Invalid status '{new_status}': {e}. Valid: pending, processing, paid, overdue, cancelled"
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
        .map_err(|e| BillingError::InvalidBillStatus(format!("Stored status invalid: {e}")))?;
    if from == target {
        return get_bill(db, billing_db, bill_id).await;
    }
    if !can_transition_bill_status(from, target) {
        return Err(BillingError::Validation(format!(
            "Cannot transition bill status from '{from}' to '{target}'"
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
