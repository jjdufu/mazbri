use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamBillScopeKind {
    Channel,
    Supplier,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpstreamBillScope {
    pub kind: UpstreamBillScopeKind,
    pub scope_id: uuid::Uuid,
}
impl UpstreamBillScope {
    #[must_use]
    pub fn channel(scope_id: uuid::Uuid) -> Self {
        Self {
            kind: UpstreamBillScopeKind::Channel,
            scope_id,
        }
    }
    #[must_use]
    pub fn supplier(scope_id: uuid::Uuid) -> Self {
        Self {
            kind: UpstreamBillScopeKind::Supplier,
            scope_id,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BillStatus {
    Pending,
    Processing,
    Paid,
    Overdue,
    Cancelled,
}
impl FromStr for BillStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "processing" => Ok(Self::Processing),
            "paid" => Ok(Self::Paid),
            "overdue" => Ok(Self::Overdue),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("Invalid bill status: {s}")),
        }
    }
}
impl std::fmt::Display for BillStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Processing => write!(f, "processing"),
            Self::Paid => write!(f, "paid"),
            Self::Overdue => write!(f, "overdue"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountStatus {
    Active,
    Suspended,
    Closed,
}
impl FromStr for AccountStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "suspended" => Ok(Self::Suspended),
            "closed" => Ok(Self::Closed),
            _ => Err(format!("Invalid account status: {s}")),
        }
    }
}
impl std::fmt::Display for AccountStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Suspended => write!(f, "suspended"),
            Self::Closed => write!(f, "closed"),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub created_at: DateTime<Utc>,
    pub id: uuid::Uuid,
}
#[derive(Debug, Error)]
pub enum BillingError {
    #[error("Invalid bill status: {0}")]
    InvalidBillStatus(String),
    #[error("Invalid account status: {0}")]
    InvalidAccountStatus(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Limit must be at most 100")]
    LimitTooLarge,
    #[error("Limit must be at least 1")]
    LimitTooSmall,
    #[error("Invalid cursor: {0}")]
    InvalidCursor(String),
    #[error("Bill not found: {0}")]
    BillNotFound(uuid::Uuid),
    #[error("Reconciliation discrepancy not found: {0}")]
    DiscrepancyNotFound(uuid::Uuid),
    #[error("Billing adjustment not found: {0}")]
    AdjustmentNotFound(uuid::Uuid),
    #[error("Database error")]
    Database,
}
impl IntoResponse for BillingError {
    fn into_response(self) -> axum::response::Response {
        let message = self.to_string();
        let status = match &self {
            Self::Database => StatusCode::INTERNAL_SERVER_ERROR,
            Self::BillNotFound(_) | Self::DiscrepancyNotFound(_) | Self::AdjustmentNotFound(_) => {
                StatusCode::NOT_FOUND
            }
            _ => StatusCode::BAD_REQUEST,
        };
        crate::response::ApiResponse::err(status, message)
    }
}
#[derive(Debug, Serialize)]
pub struct BillListResponse {
    pub bills: Vec<repo::BillRow>,
    pub total: i64,
    pub page: u32,
    pub limit: u32,
    pub total_pages: u64,
}
#[derive(Debug, Serialize)]
pub struct BillDetailResponse {
    pub bill: repo::BillRow,
    pub items: Vec<repo::BillLineItemRow>,
}
#[derive(Debug, Serialize)]
pub struct AccountListResponse {
    pub accounts: Vec<repo::AccountRow>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
pub(super) fn enrich_bill_row(
    mut bill: repo::BillRow,
    identities: &HashMap<uuid::Uuid, tenant_repo::TenantDisplayIdentity>,
) -> repo::BillRow {
    if bill.bill_kind == "customer"
        && let Some(tenant_id) = bill.tenant_id
        && let Some(identity) = identities.get(&tenant_id)
    {
        bill.tenant_name.clone_from(&identity.name);
        bill.tenant_code.clone_from(&identity.tenant_code);
    }
    bill
}
pub(super) async fn enrich_bill_rows(
    db: &PgPool,
    bills: Vec<repo::BillRow>,
) -> Result<Vec<repo::BillRow>, BillingError> {
    let tenant_ids = bills
        .iter()
        .filter(|bill| bill.bill_kind == "customer")
        .filter_map(|bill| bill.tenant_id)
        .collect::<Vec<_>>();
    let identities = tenant_repo::get_tenant_display_identities(db, &tenant_ids)
        .await
        .map_err(|e| {
            error!("Database error fetching tenant identities for bills: {}", e);
            BillingError::Database
        })?;
    Ok(bills
        .into_iter()
        .map(|bill| enrich_bill_row(bill, &identities))
        .collect())
}
pub(super) fn decode_cursor(encoded: &str) -> Option<Cursor> {
    BASE64
        .decode(encoded)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}
pub(super) fn encode_cursor(created_at: DateTime<Utc>, id: uuid::Uuid) -> String {
    BASE64.encode(serde_json::to_string(&Cursor { created_at, id }).unwrap_or_default())
}
pub(super) fn validate_limit(limit: u32) -> Result<(), BillingError> {
    if limit > 100 {
        return Err(BillingError::LimitTooLarge);
    }
    if limit < 1 {
        return Err(BillingError::LimitTooSmall);
    }
    Ok(())
}
pub(super) fn normalize_bill_status(raw: Option<&String>) -> Result<Option<String>, BillingError> {
    match raw {
        Some(s) if !s.is_empty() => {
            let normalized = s.to_lowercase();
            BillStatus::from_str(&normalized)
                .map_err(|e| {
                    BillingError::InvalidBillStatus(
                        format!(
                            "Invalid status '{s}': {e}. Valid values: pending, processing, paid, overdue, cancelled"
                        ),
                    )
                })?;
            Ok(Some(normalized))
        }
        _ => Ok(None),
    }
}
pub(super) fn normalize_account_status(
    raw: Option<&String>,
) -> Result<Option<String>, BillingError> {
    match raw {
        Some(s) if !s.is_empty() => {
            let normalized = s.to_lowercase();
            AccountStatus::from_str(&normalized).map_err(|e| {
                BillingError::InvalidAccountStatus(format!(
                    "Invalid status '{s}': {e}. Valid values: active, suspended, closed"
                ))
            })?;
            Ok(Some(normalized))
        }
        _ => Ok(None),
    }
}
