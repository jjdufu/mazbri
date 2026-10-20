use super::AppState;
use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::handlers::mfa_guard::require_mfa_code;
use crate::middleware::auth::AdminClaims;
use crate::response::ApiResponse;
use crate::services::billing as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use tracing::{debug, error};
use uuid::Uuid;
const fn default_limit() -> u32 {
    20
}
const fn default_page() -> u32 {
    1
}
#[derive(Debug, Deserialize)]
pub struct BillListQuery {
    pub tenant_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub q: Option<String>,
    pub status: Option<String>,
    /// JSON array of `{field,op,value}` list filters.
    #[serde(default)]
    pub filters: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}
#[derive(Debug, Deserialize)]
pub struct AccountListQuery {
    pub tenant_id: Option<uuid::Uuid>,
    pub status: Option<String>,
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u32,
}
#[derive(Debug, Deserialize)]
pub struct AdjustmentRequest {
    pub tenant_id: Uuid,
    pub amount: Decimal,
    pub direction: String,
    pub description: String,
}
#[derive(Debug, Deserialize)]
pub struct GenerateBillRequest {
    #[serde(default)]
    pub bill_kind: String,
    #[serde(default)]
    pub tenant_code: Option<String>,
    #[serde(default)]
    pub upstream_scope_type: Option<String>,
    #[serde(default)]
    pub upstream_scope_id: Option<String>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}
pub async fn list_bills(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Query(query): Query<BillListQuery>,
) -> Result<axum::response::Response, svc::BillingError> {
    debug!("Listing bills (by {})", claims.sub);
    let result = svc::list_bills(
        &state.db,
        svc::ListBillsParams {
            billing_db: &state.billing_db,
            tenant_id: query.tenant_id,
            search: query.q,
            status: query.status,
            filters_json: query.filters,
            page: query.page,
            limit: query.limit,
        },
    )
    .await?;
    Ok(ApiResponse::ok(result).into_response())
}
pub async fn get_bill(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<axum::response::Response, svc::BillingError> {
    debug!("Fetching bill {} (by {})", id, claims.sub);
    let result = svc::get_bill(&state.db, &state.billing_db, id).await?;
    Ok(ApiResponse::ok(result).into_response())
}
#[derive(Debug, Deserialize)]
pub struct BillStatusUpdateRequest {
    pub status: String,
}

pub async fn update_bill_status(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(payload): Json<BillStatusUpdateRequest>,
) -> Result<axum::response::Response, svc::BillingError> {
    if claims.role != "R_ADMIN" && claims.role != "R_FINANCE" {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin or Finance can update bill status",
        )
        .into_response());
    }
    let operator_id = match uuid::Uuid::parse_str(&claims.user_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Ok(ApiResponse::err(
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid operator ID format",
            )
            .into_response());
        }
    };
    let client_ip = extract_client_ip(&headers, Some(addr));
    let status = payload.status.clone();
    let result = svc::update_bill_status(&state.db, &state.billing_db, id, &payload.status).await?;
    state.audit.log(create_audit_log(
        operator_id,
        AuditAction::BillStatusUpdate,
        AuditResourceType::Account,
        id.to_string(),
        Some(serde_json::json!({ "bill_id": id, "status": status })),
        client_ip,
    ));
    Ok(ApiResponse::ok(result).into_response())
}

pub async fn delete_bill(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<axum::response::Response, svc::BillingError> {
    if claims.role != "R_ADMIN" {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin can delete bills",
        )
        .into_response());
    }
    let operator_id = match uuid::Uuid::parse_str(&claims.user_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Ok(ApiResponse::err(
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid operator ID format",
            )
            .into_response());
        }
    };
    let client_ip = extract_client_ip(&headers, Some(addr));
    debug!("Deleting bill {} (by {})", id, claims.sub);
    let deleted_id = svc::delete_bill(&state.billing_db, id).await?;
    state.audit.log(create_audit_log(
        operator_id,
        AuditAction::BillDelete,
        AuditResourceType::Account,
        deleted_id.to_string(),
        Some(serde_json::json!({ "bill_id" : deleted_id })),
        client_ip,
    ));
    Ok(ApiResponse::ok(serde_json::json!(
        { "bill_id" : deleted_id, "message" : "Bill deleted successfully" }
    ))
    .into_response())
}
pub async fn list_accounts(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Query(query): Query<AccountListQuery>,
) -> Result<axum::response::Response, svc::BillingError> {
    debug!("Listing accounts (by {})", claims.sub);
    let result = svc::list_accounts(
        &state.billing_db,
        query.tenant_id,
        query.status,
        query.cursor,
        query.limit,
    )
    .await?;
    Ok(ApiResponse::ok(result).into_response())
}
pub async fn create_adjustment(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<AdjustmentRequest>,
) -> Result<axum::response::Response, svc::BillingError> {
    if claims.role != "R_ADMIN" && claims.role != "R_FINANCE" {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin or Finance can create adjustments",
        )
        .into_response());
    }
    if let Err(response) = require_mfa_code(&state, &claims, &headers).await {
        return Ok(*response);
    }
    let amount_text = payload.amount.to_string();
    let direction = payload.direction.clone();
    let description = payload.description.clone();
    let operator_id = match uuid::Uuid::parse_str(&claims.user_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Ok(ApiResponse::err(
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid operator ID format",
            )
            .into_response());
        }
    };
    let client_ip = extract_client_ip(&headers, Some(addr));
    let billing_service = mazbri_billing::BillingService::with_pgpool(
        state.billing_db.clone(),
        mazbri_billing::BillingServiceConfig::default(),
    );
    let transaction_id = billing_service
        .create_adjustment(
            payload.tenant_id,
            payload.amount,
            &payload.direction,
            &payload.description,
        )
        .await
        .map_err(|e| {
            error!("Adjustment failed: {}", e);
            svc::BillingError::Database
        })?;
    state.audit.log(create_audit_log(
        operator_id,
        AuditAction::AccountAdjustment,
        AuditResourceType::Account,
        payload.tenant_id.to_string(),
        Some(serde_json::json!(
            { "tenant_id" : payload.tenant_id, "amount" : amount_text,
            "direction" : direction, "description" : description,
            "transaction_id" : transaction_id }
        )),
        client_ip,
    ));
    Ok(ApiResponse::ok(serde_json::json!(
        { "transaction_id" : transaction_id, "message" :
        "Adjustment successful" }
    ))
    .into_response())
}
pub async fn generate_bill(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<GenerateBillRequest>,
) -> Result<axum::response::Response, svc::BillingError> {
    if claims.role != "R_ADMIN" && claims.role != "R_FINANCE" {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin or Finance can generate bills",
        )
        .into_response());
    }
    if let Err(response) = require_mfa_code(&state, &claims, &headers).await {
        return Ok(*response);
    }
    let operator_id = match uuid::Uuid::parse_str(&claims.user_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Ok(ApiResponse::err(
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid operator ID format",
            )
            .into_response());
        }
    };
    let client_ip = extract_client_ip(&headers, Some(addr));
    let bill_kind = payload.bill_kind.trim().to_lowercase();
    let period_start = payload.period_start;
    let period_end = payload.period_end;
    let period_start_audit = period_start.to_rfc3339();
    let period_end_audit = period_end.to_rfc3339();
    let (bill_id, audit_details) = if bill_kind.is_empty() || bill_kind == "customer" {
        let tenant_code = match payload
            .tenant_code
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            Some(value) => value,
            None => {
                return Ok(ApiResponse::err(
                    axum::http::StatusCode::BAD_REQUEST,
                    "Customer bill requires tenant_code",
                )
                .into_response());
            }
        };
        let tenant_id =
            match crate::services::tenant::resolve_tenant_id(&state.db, tenant_code).await {
                Ok(id) => id,
                Err(error) => {
                    return Ok(error.into_response());
                }
            };
        let bill_id =
            svc::generate_bill(&state.billing_db, tenant_id, period_start, period_end).await?;
        svc::snapshot_customer_bill_items(
            &state.billing_db,
            bill_id,
            tenant_id,
            period_start,
            period_end,
        )
        .await?;
        (
            bill_id,
            serde_json::json!(
                { "bill_kind" : "customer", "tenant_code" : tenant_code, "tenant_id" :
                tenant_id, "period_start" : period_start_audit, "period_end" :
                period_end_audit }
            ),
        )
    } else if bill_kind == "upstream" {
        let scope_type = match payload
            .upstream_scope_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(value) => value.to_lowercase(),
            None => {
                return Ok(ApiResponse::err(
                    axum::http::StatusCode::BAD_REQUEST,
                    "Upstream bill requires upstream_scope_type",
                )
                .into_response());
            }
        };
        let scope_id = match payload
            .upstream_scope_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(value) => value,
            None => {
                return Ok(ApiResponse::err(
                    axum::http::StatusCode::BAD_REQUEST,
                    "Upstream bill requires upstream_scope_id",
                )
                .into_response());
            }
        };
        let scope_id = match Uuid::parse_str(scope_id) {
            Ok(value) => value,
            Err(_) => {
                return Ok(ApiResponse::err(
                    axum::http::StatusCode::BAD_REQUEST,
                    "upstream_scope_id must be a valid UUID",
                )
                .into_response());
            }
        };
        let scope = match scope_type.as_str() {
            "channel" => svc::UpstreamBillScope::channel(scope_id),
            "supplier" => svc::UpstreamBillScope::supplier(scope_id),
            _ => {
                return Ok(ApiResponse::err(
                    axum::http::StatusCode::BAD_REQUEST,
                    "upstream_scope_type must be channel or supplier",
                )
                .into_response());
            }
        };
        let bill_id = svc::generate_upstream_bill(
            &state.billing_db,
            &state.db,
            &state.analytics_client,
            period_start,
            period_end,
            Some(scope),
        )
        .await?;
        (
            bill_id,
            serde_json::json!(
                { "bill_kind" : "upstream", "upstream_scope_type" : scope_type,
                "upstream_scope_id" : scope_id, "period_start" :
                period_start_audit, "period_end" : period_end_audit }
            ),
        )
    } else {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::BAD_REQUEST,
            "bill_kind must be customer or upstream",
        )
        .into_response());
    };
    let mut warnings: Vec<String> = Vec::new();
    let recon_runtime = state.billing_reconciliation_settings.get().await;
    if recon_runtime.warn_on_open_discrepancies_before_bill {
        if let Ok(Some(msg)) =
            svc::open_discrepancy_warning_for_period(&state.billing_db, period_start, period_end)
                .await
        {
            warnings.push(msg);
        }
    }

    state.audit.log(create_audit_log(
        operator_id,
        AuditAction::BillGenerate,
        AuditResourceType::Account,
        bill_id.to_string(),
        Some(audit_details),
        client_ip,
    ));
    Ok(ApiResponse::ok(serde_json::json!({
        "bill_id": bill_id,
        "message": "Bill generated successfully",
        "warnings": warnings,
    }))
    .into_response())
}

#[derive(Debug, Deserialize)]
pub struct ReconciliationAuditRequest {
    pub reconciliation_date: chrono::NaiveDate,
    #[serde(default)]
    pub scope_type: Option<String>,
    #[serde(default)]
    pub scope_id: Option<String>,
}

fn loss_ratio_threshold_decimal(threshold: f64) -> Decimal {
    // conf stores f64; convert via string for stable Decimal scale.
    Decimal::from_str(&format!("{threshold:.8}")).unwrap_or_else(|_| Decimal::new(5, 2))
}

pub async fn run_reconciliation_audit(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<ReconciliationAuditRequest>,
) -> Result<axum::response::Response, svc::BillingError> {
    if claims.role != "R_ADMIN" && claims.role != "R_FINANCE" {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin or Finance can run reconciliation audit",
        )
        .into_response());
    }

    let recon_cfg = state.billing_reconciliation_settings.get().await;
    let scope_type = payload
        .scope_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(recon_cfg.default_scope_type.as_str())
        .to_string();
    let scope_id = payload
        .scope_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(recon_cfg.default_scope_id.as_str())
        .to_string();
    let threshold = loss_ratio_threshold_decimal(recon_cfg.loss_ratio_threshold);
    let options = svc::ReconciliationAuditOptions {
        loss_ratio_threshold: threshold,
        include_amount_recon: recon_cfg.include_amount_recon,
        include_raw_log_source: recon_cfg.include_raw_log_source,
    };

    let result = svc::execute_reconciliation_audit(
        &state.billing_db,
        &state.analytics_client,
        payload.reconciliation_date,
        &scope_type,
        &scope_id,
        options,
    )
    .await?;

    if let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) {
        let client_ip = extract_client_ip(&headers, Some(addr));
        state.audit.log(create_audit_log(
            operator_id,
            AuditAction::ReconciliationAudit,
            AuditResourceType::Account,
            result.discrepancy_id.to_string(),
            Some(serde_json::json!({
                "reconciliation_date": result.reconciliation_date.to_string(),
                "scope_type": result.scope_type,
                "scope_id": result.scope_id,
                "oltp_call_count": result.oltp_call_count,
                "olap_preagg_call_count": result.olap_preagg_call_count,
                "raw_log_call_count": result.raw_log_call_count,
                "raw_log_source": result.raw_log_source,
                "loss_ratio": result.loss_ratio.to_string(),
                "loss_ratio_threshold": result.loss_ratio_threshold.to_string(),
                "status": result.status,
            })),
            client_ip,
        ));
    }

    Ok(ApiResponse::ok(result).into_response())
}

#[derive(Debug, Deserialize)]
pub struct ReconciliationDiscrepancyListQuery {
    pub from: Option<chrono::NaiveDate>,
    pub to: Option<chrono::NaiveDate>,
    pub status: Option<String>,
    pub scope_type: Option<String>,
    pub scope_id: Option<String>,
    /// Free-text keyword (id / date / status / scope / note).
    #[serde(default)]
    pub q: Option<String>,
    /// JSON array of `{field,op,value}` list filters.
    #[serde(default)]
    pub filters: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit", alias = "limit")]
    pub page_size: u32,
}

pub async fn list_reconciliation_discrepancies(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Query(query): Query<ReconciliationDiscrepancyListQuery>,
) -> Result<axum::response::Response, svc::BillingError> {
    // Admin/Finance full access; Viewer read-only list.
    if !matches!(
        claims.role.as_str(),
        "R_ADMIN" | "R_FINANCE" | "R_VIEWER" | "R_OPERATOR"
    ) {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin, Finance, Operator or Viewer can list reconciliation discrepancies",
        )
        .into_response());
    }

    let result = svc::list_reconciliation_discrepancies(
        &state.billing_db,
        svc::ListDiscrepanciesParams {
            from: query.from,
            to: query.to,
            status: query.status,
            scope_type: query.scope_type,
            scope_id: query.scope_id,
            search: query.q,
            filters_json: query.filters,
            page: query.page,
            page_size: query.page_size,
        },
    )
    .await?;
    Ok(ApiResponse::ok(result).into_response())
}

#[derive(Debug, Deserialize)]
pub struct UpdateReconciliationDiscrepancyRequest {
    pub status: String,
    #[serde(default)]
    pub resolution_note: Option<String>,
}

pub async fn update_reconciliation_discrepancy(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateReconciliationDiscrepancyRequest>,
) -> Result<axum::response::Response, svc::BillingError> {
    if claims.role != "R_ADMIN" && claims.role != "R_FINANCE" {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin or Finance can update reconciliation discrepancies",
        )
        .into_response());
    }

    let operator_id = uuid::Uuid::parse_str(&claims.user_id).ok();

    let result = svc::update_reconciliation_discrepancy(
        &state.billing_db,
        id,
        &payload.status,
        payload.resolution_note.as_deref(),
        operator_id,
    )
    .await?;

    if let Some(operator_id) = operator_id {
        let client_ip = extract_client_ip(&headers, Some(addr));
        state.audit.log(create_audit_log(
            operator_id,
            AuditAction::ReconciliationDiscrepancyUpdate,
            AuditResourceType::Account,
            result.id.to_string(),
            Some(serde_json::json!({
                "status": result.status,
                "resolution_note": result.resolution_note,
                "reconciliation_date": result.reconciliation_date.to_string(),
            })),
            client_ip,
        ));
    }

    Ok(ApiResponse::ok(result).into_response())
}

pub async fn delete_reconciliation_discrepancy(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<axum::response::Response, svc::BillingError> {
    if claims.role != "R_ADMIN" {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin can delete reconciliation discrepancies",
        )
        .into_response());
    }
    let operator_id = match uuid::Uuid::parse_str(&claims.user_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Ok(ApiResponse::err(
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid operator ID format",
            )
            .into_response());
        }
    };
    let client_ip = extract_client_ip(&headers, Some(addr));
    debug!(
        "Deleting reconciliation discrepancy {} (by {})",
        id, claims.sub
    );
    let deleted_id = svc::delete_reconciliation_discrepancy(&state.billing_db, id).await?;
    state.audit.log(create_audit_log(
        operator_id,
        AuditAction::ReconciliationDiscrepancyDelete,
        AuditResourceType::Account,
        deleted_id.to_string(),
        Some(serde_json::json!({ "discrepancy_id": deleted_id })),
        client_ip,
    ));
    Ok(ApiResponse::ok(serde_json::json!({
        "id": deleted_id,
        "message": "Reconciliation discrepancy deleted successfully"
    }))
    .into_response())
}

pub async fn get_reconciliation_settings(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
) -> Result<axum::response::Response, svc::BillingError> {
    if !matches!(
        claims.role.as_str(),
        "R_ADMIN" | "R_FINANCE" | "R_OPERATOR" | "R_VIEWER"
    ) {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin, Finance, Operator or Viewer can view reconciliation settings",
        )
        .into_response());
    }
    let settings = state.billing_reconciliation_settings.get().await;
    Ok(ApiResponse::ok(settings).into_response())
}

pub async fn update_reconciliation_settings(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<crate::services::BillingReconciliationRuntimeSettings>,
) -> Result<axum::response::Response, svc::BillingError> {
    if claims.role != "R_ADMIN" && claims.role != "R_FINANCE" {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin or Finance can update reconciliation settings",
        )
        .into_response());
    }

    let settings = state
        .billing_reconciliation_settings
        .save(payload)
        .await
        .map_err(svc::BillingError::Validation)?;

    if let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) {
        let client_ip = extract_client_ip(&headers, Some(addr));
        state.audit.log(create_audit_log(
            operator_id,
            AuditAction::ReconciliationSettingsUpdate,
            AuditResourceType::System,
            "billing_reconciliation".to_string(),
            Some(serde_json::to_value(&settings).unwrap_or_else(|_| serde_json::json!({}))),
            client_ip,
        ));
    }

    Ok(ApiResponse::ok(settings).into_response())
}

#[derive(Debug, Deserialize)]
pub struct CreateBillAdjustmentRequest {
    pub adjustment_type: String,
    pub amount: Decimal,
    pub reason: String,
}

pub async fn create_bill_adjustment(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(payload): Json<CreateBillAdjustmentRequest>,
) -> Result<axum::response::Response, svc::BillingError> {
    if claims.role != "R_ADMIN" && claims.role != "R_FINANCE" {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin or Finance can create bill adjustments",
        )
        .into_response());
    }
    if let Err(response) = require_mfa_code(&state, &claims, &headers).await {
        return Ok(*response);
    }
    let operator_id = uuid::Uuid::parse_str(&claims.user_id).ok();
    let result = svc::create_bill_adjustment(
        &state.billing_db,
        id,
        &payload.adjustment_type,
        payload.amount,
        &payload.reason,
        operator_id,
    )
    .await?;

    if let Some(operator_id) = operator_id {
        let client_ip = extract_client_ip(&headers, Some(addr));
        state.audit.log(create_audit_log(
            operator_id,
            AuditAction::BillAdjustmentCreate,
            AuditResourceType::Account,
            result.id.to_string(),
            Some(serde_json::json!({
                "bill_id": result.bill_id,
                "adjustment_type": result.adjustment_type,
                "amount": result.amount.to_string(),
                "reason": result.reason,
                "status": result.status,
                "ledger_only": true,
            })),
            client_ip,
        ));
    }

    Ok(ApiResponse::ok(result).into_response())
}

pub async fn list_bill_adjustments(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<axum::response::Response, svc::BillingError> {
    if !matches!(
        claims.role.as_str(),
        "R_ADMIN" | "R_FINANCE" | "R_VIEWER" | "R_OPERATOR"
    ) {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin, Finance, Operator or Viewer can list bill adjustments",
        )
        .into_response());
    }
    let items = svc::list_bill_adjustments_for_bill(&state.billing_db, id).await?;
    Ok(ApiResponse::ok(serde_json::json!({ "items": items })).into_response())
}

#[derive(Debug, Deserialize)]
pub struct GlobalBillAdjustmentListQuery {
    pub bill_id: Option<uuid::Uuid>,
    pub tenant_id: Option<uuid::Uuid>,
    pub status: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit", alias = "limit")]
    pub page_size: u32,
}

pub async fn list_global_bill_adjustments(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Query(query): Query<GlobalBillAdjustmentListQuery>,
) -> Result<axum::response::Response, svc::BillingError> {
    if !matches!(
        claims.role.as_str(),
        "R_ADMIN" | "R_FINANCE" | "R_VIEWER" | "R_OPERATOR"
    ) {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin, Finance, Operator or Viewer can list bill adjustments",
        )
        .into_response());
    }
    let result = svc::list_billing_adjustments(
        &state.billing_db,
        query.bill_id,
        query.tenant_id,
        query.status,
        query.page,
        query.page_size,
    )
    .await?;
    Ok(ApiResponse::ok(result).into_response())
}

pub async fn void_bill_adjustment(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<axum::response::Response, svc::BillingError> {
    if claims.role != "R_ADMIN" && claims.role != "R_FINANCE" {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin or Finance can void bill adjustments",
        )
        .into_response());
    }
    if let Err(response) = require_mfa_code(&state, &claims, &headers).await {
        return Ok(*response);
    }
    let result = svc::void_bill_adjustment(&state.billing_db, id).await?;
    if let Ok(operator_id) = uuid::Uuid::parse_str(&claims.user_id) {
        let client_ip = extract_client_ip(&headers, Some(addr));
        state.audit.log(create_audit_log(
            operator_id,
            AuditAction::BillAdjustmentVoid,
            AuditResourceType::Account,
            result.id.to_string(),
            Some(serde_json::json!({
                "bill_id": result.bill_id,
                "status": result.status,
            })),
            client_ip,
        ));
    }
    Ok(ApiResponse::ok(result).into_response())
}

pub async fn generate_bill_async(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<GenerateBillRequest>,
) -> Result<axum::response::Response, svc::BillingError> {
    if claims.role != "R_ADMIN" && claims.role != "R_FINANCE" {
        return Ok(ApiResponse::err(
            axum::http::StatusCode::FORBIDDEN,
            "Only Admin or Finance can generate bills",
        )
        .into_response());
    }

    let operator_id = match uuid::Uuid::parse_str(&claims.user_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Ok(ApiResponse::err(
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid operator ID format",
            )
            .into_response());
        }
    };
    let client_ip = extract_client_ip(&headers, Some(addr));
    let bill_kind = payload.bill_kind.trim().to_lowercase();
    let period_start = payload.period_start;
    let period_end = payload.period_end;

    if bill_kind == "supplier" || bill_kind == "upstream" {
        let scope_type = payload
            .upstream_scope_type
            .as_deref()
            .map(str::trim)
            .unwrap_or("supplier");
        let scope_id = payload
            .upstream_scope_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .and_then(|v| Uuid::parse_str(v).ok());

        let scope = scope_id.map(|id| match scope_type {
            "channel" => svc::UpstreamBillScope::channel(id),
            _ => svc::UpstreamBillScope::supplier(id),
        });

        let summary = svc::execute_upstream_billing_job(
            &state.billing_db,
            &state.db,
            &state.analytics_client,
            period_start,
            period_end,
            scope,
        )
        .await?;

        state.audit.log(create_audit_log(
            operator_id,
            AuditAction::BillGenerate,
            AuditResourceType::Account,
            summary.bill_id.to_string(),
            Some(serde_json::json!({
                "bill_kind": "upstream",
                "summary": summary,
            })),
            client_ip,
        ));

        Ok(ApiResponse::ok(summary).into_response())
    } else {
        generate_bill(
            State(state),
            ConnectInfo(addr),
            Extension(claims),
            headers,
            Json(payload),
        )
        .await
    }
}
