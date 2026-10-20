use mazbri_core::num::SaturatingFrom;
use crate::audit::{AuditAction, AuditResourceType, AuditService, create_audit_log, tenant_audit};
use crate::handlers::product::{
    RechargeRequest, RefundRequest, RefundResponse, Transaction, TransactionListQuery,
    TransactionListResponse,
};
use crate::handlers::types::ListParams;
use crate::repositories::product as repo;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use mazbri_product::model::{CreateProductRequest, UpdateProductRequest};
use mazbri_product::service::{
    GrantAccessRequest, ProductDeleteError, ProductWriteError, TenantProductConfig,
    UpdateTenantProductConfigResult,
};
use rust_decimal::Decimal;
use sqlx::PgPool;
use tracing::{error, info};
pub async fn list_products(
    product_service: &mazbri_product::ProductService,
    params: &ListParams,
) -> axum::response::Response {
    let search = params.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let page_size = params.capped_page_size().max(1);
    let offset = (params.page.saturating_sub(1)) * page_size;
    let filters = match crate::list_query::parse_and_validate_filters(
        params.filters.as_deref(),
        &crate::list_query::products_catalog(),
    ) {
        Ok(f) => f,
        Err(e) => {
            return crate::response::ApiResponse::err(StatusCode::BAD_REQUEST, e.message)
                .into_response();
        }
    };
    match product_service
        .list_products_page(search, i64::from(page_size), i64::from(offset), |qb| {
            crate::list_query::apply_pg_filters(qb, &filters).map_err(|e| e.message)
        })
        .await
    {
        Ok((page, total)) => {
            let total = u64::saturating_from(total);
            let total_pages = mazbri_core::num::f64_to_u64((mazbri_core::num::u64_to_f64(total) / f64::from(page_size)).ceil());
            crate::response::ApiResponse::ok(serde_json::json!(
                { "products" : page, "pagination" : { "page" : params.page,
                "page_size" : page_size, "total" : total, "total_pages" :
                total_pages, } }
            ))
            .into_response()
        }
        Err(e) => crate::response::ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list products: {e}"),
        ),
    }
}
pub async fn list_authorizations(
    product_service: &mazbri_product::ProductService,
    params: &ListParams,
) -> axum::response::Response {
    let search = params.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let page_size = params.capped_page_size().max(1);
    let offset = (params.page.saturating_sub(1)) * page_size;
    let filters = match crate::list_query::parse_and_validate_filters(
        params.filters.as_deref(),
        &crate::list_query::authorizations_catalog(),
    ) {
        Ok(f) => f,
        Err(e) => {
            return crate::response::ApiResponse::err(StatusCode::BAD_REQUEST, e.message)
                .into_response();
        }
    };
    match product_service
        .list_authorizations_page(search, i64::from(page_size), i64::from(offset), |qb| {
            crate::list_query::apply_pg_filters(qb, &filters).map_err(|e| e.message)
        })
        .await
    {
        Ok((page, total)) => {
            let total = u64::saturating_from(total);
            let total_pages = mazbri_core::num::f64_to_u64((mazbri_core::num::u64_to_f64(total) / f64::from(page_size)).ceil());
            crate::response::ApiResponse::ok(serde_json::json!(
                { "authorizations" : page, "pagination" : { "page" : params.page,
                "page_size" : page_size, "total" : total, "total_pages" :
                total_pages, } }
            ))
            .into_response()
        }
        Err(e) => crate::response::ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list authorizations: {e}"),
        ),
    }
}
pub async fn create_product(
    product_service: &mazbri_product::ProductService,
    req: CreateProductRequest,
    operator: &str,
    audit: &AuditService,
    operator_id: uuid::Uuid,
    client_ip: Option<String>,
) -> axum::response::Response {
    info!(
        "Creating product from reservation {} (by {})",
        req.reservation_id, operator
    );
    let reservation_id = req.reservation_id;
    match product_service.create_product(req, operator).await {
        Ok(product) => {
            let product_id = product.product_id.clone();
            let product_value = serde_json::json!(&product);
            let audit_product_value = product_value.clone();
            audit.log(create_audit_log(
                operator_id,
                AuditAction::ProductCreate,
                AuditResourceType::Product,
                product_id,
                Some(serde_json::json!(
                    { "reservation_id" : reservation_id, "product" : audit_product_value }
                )),
                client_ip,
            ));
            crate::response::created(product_value)
        }
        Err(error) => product_write_error_response("create product", &error),
    }
}
pub async fn create_product_reservation(
    product_service: &mazbri_product::ProductService,
    operator: &str,
    audit: &AuditService,
    operator_id: uuid::Uuid,
    client_ip: Option<String>,
) -> axum::response::Response {
    match product_service.reserve_product_id(operator).await {
        Ok(reservation) => {
            let reservation_value = serde_json::json!(&reservation);
            let audit_reservation_value = reservation_value.clone();
            audit.log(create_audit_log(
                operator_id,
                AuditAction::ProductReservationCreate,
                AuditResourceType::Product,
                reservation.id.to_string(),
                Some(serde_json::json!({ "reservation" : audit_reservation_value })),
                client_ip,
            ));
            crate::response::created(reservation_value)
        }
        Err(error) => product_write_error_response("reserve product_id", &error),
    }
}
pub async fn release_product_reservation(
    product_service: &mazbri_product::ProductService,
    reservation_id: uuid::Uuid,
    operator: &str,
    audit: &AuditService,
    operator_id: uuid::Uuid,
    client_ip: Option<String>,
) -> axum::response::Response {
    match product_service
        .release_product_id_reservation(reservation_id, operator)
        .await
    {
        Ok(released) => {
            audit.log(create_audit_log(
                operator_id,
                AuditAction::ProductReservationRelease,
                AuditResourceType::Product,
                reservation_id.to_string(),
                Some(serde_json::json!(
                    { "reservation_id" : reservation_id, "released" : released }
                )),
                client_ip,
            ));
            crate::response::ApiResponse::ok(serde_json::json!({ "released" : released }))
                .into_response()
        }
        Err(error) => product_write_error_response("release product_id reservation", &error),
    }
}
pub async fn get_product(
    product_service: &mazbri_product::ProductService,
    product_id: &str,
) -> axum::response::Response {
    match product_service.get_product_by_id(product_id).await {
        Ok(Some(product)) => {
            crate::response::ApiResponse::ok(serde_json::json!(product)).into_response()
        }
        Ok(None) => crate::response::ApiResponse::err(
            StatusCode::NOT_FOUND,
            format!("Product not found: {product_id}"),
        ),
        Err(e) => crate::response::ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get product: {e}"),
        ),
    }
}
pub async fn update_product(
    product_service: &mazbri_product::ProductService,
    product_id: &str,
    req: UpdateProductRequest,
    operator: &str,
    audit: &AuditService,
    operator_id: uuid::Uuid,
    client_ip: Option<String>,
) -> axum::response::Response {
    info!("Updating product: {} (by {})", product_id, operator);
    let request_details = serde_json::json!(
        { "name" : &req.name, "description" : &req.description, "enabled" : &req.enabled }
    );
    match product_service.update_product(product_id, req).await {
        Ok(Some(product)) => {
            let product_value = serde_json::json!(&product);
            let audit_product_value = product_value.clone();
            audit.log(create_audit_log(
                operator_id,
                AuditAction::ProductUpdate,
                AuditResourceType::Product,
                product.product_id.clone(),
                Some(serde_json::json!(
                    { "request" : request_details, "product" : audit_product_value }
                )),
                client_ip,
            ));
            crate::response::ApiResponse::ok(product_value).into_response()
        }
        Ok(None) => crate::response::ApiResponse::err(
            StatusCode::NOT_FOUND,
            format!("Product not found: {product_id}"),
        ),
        Err(error) => product_write_error_response("update product", &error),
    }
}
pub async fn delete_product(
    product_service: &mazbri_product::ProductService,
    product_id: &str,
    operator: &str,
    audit: &AuditService,
    operator_id: uuid::Uuid,
    client_ip: Option<String>,
) -> axum::response::Response {
    info!("Deleting product: {} (by {})", product_id, operator);
    match product_service.delete_product(product_id).await {
        Ok(true) => {
            audit.log(create_audit_log(
                operator_id,
                AuditAction::ProductDelete,
                AuditResourceType::Product,
                product_id.to_string(),
                Some(serde_json::json!({ "product_id" : product_id })),
                client_ip,
            ));
            crate::response::ApiResponse::ok(serde_json::json!({ "message" : "Product deleted" }))
                .into_response()
        }
        Ok(false) => crate::response::ApiResponse::err(
            StatusCode::NOT_FOUND,
            format!("Product not found: {product_id}"),
        ),
        Err(error) => product_delete_error_response("delete product", &error),
    }
}
pub struct GrantTenantAccessArgs<'a> {
    pub product_service: &'a mazbri_product::ProductService,
    pub product_id: &'a str,
    pub req: GrantAccessRequest,
    pub audit: &'a AuditService,
    pub operator_id: uuid::Uuid,
    pub operator_name: &'a str,
    pub tenant_code: &'a str,
    pub client_ip: Option<String>,
}

pub async fn grant_tenant_access(args: GrantTenantAccessArgs<'_>) -> axum::response::Response {
    let product_service = args.product_service;
    let product_id = args.product_id;
    let req = args.req;
    let audit = args.audit;
    let operator_id = args.operator_id;
    let operator_name = args.operator_name;
    let tenant_code = args.tenant_code;
    let client_ip = args.client_ip;

    info!(
        "Granting tenant {} access to product {} with config {:?} (by {})",
        req.tenant_id, product_id, req.config, operator_name
    );
    match product_service
        .grant_tenant_access(req.tenant_id, product_id, req.config.as_ref())
        .await
    {
        Ok(tenant_product) => {
            audit.log(create_audit_log(
                operator_id,
                AuditAction::ProductGrant,
                AuditResourceType::Product,
                product_id.to_string(),
                Some(serde_json::json!(
                    { "tenant_code" : tenant_code, "tenant_id" : req.tenant_id.to_string(),
                    "config" : req.config, "enabled" : tenant_product.enabled }
                )),
                client_ip,
            ));
            crate::response::created(serde_json::json!(
                { "message" : "Access granted", "tenant_product" : { "product_id" :
                tenant_product.product_id, "enabled" : tenant_product.enabled,
                "custom_config" : tenant_product.custom_config } }
            ))
        }
        Err(e) => crate::response::ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to grant access: {e}"),
        ),
    }
}

pub struct RevokeTenantAccessArgs<'a> {
    pub product_service: &'a mazbri_product::ProductService,
    pub product_id: &'a str,
    pub tenant_id: uuid::Uuid,
    pub audit: &'a AuditService,
    pub operator_id: uuid::Uuid,
    pub operator_name: &'a str,
    pub tenant_code: &'a str,
    pub client_ip: Option<String>,
}

pub async fn revoke_tenant_access(args: RevokeTenantAccessArgs<'_>) -> axum::response::Response {
    let product_service = args.product_service;
    let product_id = args.product_id;
    let tenant_id = args.tenant_id;
    let audit = args.audit;
    let operator_id = args.operator_id;
    let operator_name = args.operator_name;
    let tenant_code = args.tenant_code;
    let client_ip = args.client_ip;

    info!(
        "Revoking tenant {} access to product {} (by {})",
        tenant_id, product_id, operator_name
    );
    match product_service
        .revoke_tenant_access(tenant_id, product_id)
        .await
    {
        Ok(true) => {
            audit.log(create_audit_log(
                operator_id,
                AuditAction::ProductRevoke,
                AuditResourceType::Product,
                product_id.to_string(),
                Some(serde_json::json!(
                    { "tenant_code" : tenant_code, "tenant_id" : tenant_id.to_string(),
                    "product_id" : product_id }
                )),
                client_ip,
            ));
            crate::response::ApiResponse::ok(serde_json::json!({ "message" : "Access revoked" }))
                .into_response()
        }
        Ok(false) => {
            crate::response::ApiResponse::err(StatusCode::NOT_FOUND, "Tenant access not found")
        }
        Err(e) => crate::response::ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to revoke access: {e}"),
        ),
    }
}

pub struct UpdateTenantProductConfigArgs<'a> {
    pub product_service: &'a mazbri_product::ProductService,
    pub audit: &'a AuditService,
    pub tenant_id: uuid::Uuid,
    pub product_id: &'a str,
    pub req: TenantProductConfig,
    pub expected_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub operator_id: uuid::Uuid,
    pub operator_name: &'a str,
    pub client_ip: Option<String>,
}

pub async fn update_tenant_product_config(
    args: UpdateTenantProductConfigArgs<'_>,
) -> axum::response::Response {
    let product_service = args.product_service;
    let audit = args.audit;
    let tenant_id = args.tenant_id;
    let product_id = args.product_id;
    let req = args.req;
    let expected_updated_at = args.expected_updated_at;
    let operator_id = args.operator_id;
    let operator_name = args.operator_name;
    let client_ip = args.client_ip;

    info!(
        "Updating tenant {} config for product {} with payload {:?} (by {})",
        tenant_id, product_id, req, operator_name
    );

    match product_service
        .update_tenant_product_config(tenant_id, product_id, &req, expected_updated_at)
        .await
    {
        Ok(UpdateTenantProductConfigResult::Updated(tenant_product)) => {
            audit.log(tenant_audit(
                operator_id,
                AuditAction::QuotaUpdate,
                tenant_id,
                serde_json::json!({ "product_id" : product_id, "config" : req }),
                client_ip,
            ));
            crate::response::ApiResponse::ok(serde_json::json!(
                { "message" : "Tenant product config updated", "tenant_product" :
                { "product_id" : tenant_product.product_id, "enabled" :
                tenant_product.enabled, "custom_config" : tenant_product
                .custom_config } }
            ))
            .into_response()
        }
        Ok(UpdateTenantProductConfigResult::NotFound) => crate::response::ApiResponse::err(
            StatusCode::NOT_FOUND,
            "Tenant product access not found",
        ),
        Ok(UpdateTenantProductConfigResult::Conflict(latest_updated_at)) => {
            crate::response::ApiResponse::err(
                StatusCode::CONFLICT,
                format!(
                    "Tenant product config was modified by another operator at {}. Reload and retry.",
                    latest_updated_at.to_rfc3339()
                ),
            )
        }
        Err(e) => crate::response::ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to update tenant product config: {e}"),
        ),
    }
}
pub async fn get_tenant_product_config(
    product_service: &mazbri_product::ProductService,
    tenant_id: uuid::Uuid,
    product_id: &str,
) -> axum::response::Response {
    match product_service
        .get_tenant_product_config(tenant_id, product_id)
        .await
    {
        Ok(Some(product)) => crate::response::ApiResponse::ok(serde_json::json!(
            { "product_id" : product.product_id, "product_code" : product
            .product_code, "product_name" : product.product_name,
            "description" : product.description, "enabled" : product.enabled,
            "cb_pro_enabled" : product.cb_pro_enabled, "config" : product.config,
            "created_at" : product.created_at, "updated_at" : product.updated_at, }
        ))
        .into_response(),
        Ok(None) => crate::response::ApiResponse::err(
            StatusCode::NOT_FOUND,
            "Tenant product access not found",
        ),
        Err(e) => crate::response::ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get tenant product config: {e}"),
        ),
    }
}
pub async fn list_product_tenants(
    db: &PgPool,
    product_service: &mazbri_product::ProductService,
    product_id: &str,
    params: &ListParams,
) -> axum::response::Response {
    match product_service.list_product_tenants(product_id).await {
        Ok(all_tenants) => {
            let total = all_tenants.len() as u64;
            let page_size = params.capped_page_size() as usize;
            let offset = params.offset() as usize;
            let page_records = all_tenants
                .into_iter()
                .skip(offset)
                .take(page_size)
                .collect::<Vec<_>>();
            let tenant_ids: Vec<uuid::Uuid> = page_records.iter().map(|p| p.tenant_id).collect();
            let identities =
                match crate::repositories::tenant::get_tenant_identities(db, &tenant_ids).await {
                    Ok(map) => map,
                    Err(e) => {
                        return crate::response::ApiResponse::err(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Failed to fetch tenant identities: {e}"),
                        );
                    }
                };
            let page = page_records
                .into_iter()
                .map(|p| {
                    let (tenant_code, tenant_id_str) = identities
                        .get(&p.tenant_id)
                        .cloned()
                        .unwrap_or_else(|| ("UNKNOWN".to_string(), "UNKNOWN".to_string()));
                    serde_json::json!(
                        { "tenant_code" : tenant_code, "tenant_id" : tenant_id_str,
                        "enabled" : p.enabled, "created_at" : p.created_at, }
                    )
                })
                .collect::<Vec<_>>();
            let total_pages = mazbri_core::num::f64_to_u64((mazbri_core::num::u64_to_f64(total) / mazbri_core::num::usize_to_f64(page_size)).ceil());
            crate::response::ApiResponse::ok(serde_json::json!(
                { "tenants" : page, "pagination" : { "page" : params.page,
                "page_size" : page_size, "total" : total, "total_pages" :
                total_pages, } }
            ))
            .into_response()
        }
        Err(e) => crate::response::ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list product tenants: {e}"),
        ),
    }
}
pub async fn list_tenant_products(
    product_service: &mazbri_product::ProductService,
    tenant_id: uuid::Uuid,
    params: &ListParams,
) -> axum::response::Response {
    match product_service.list_tenant_products(tenant_id).await {
        Ok(products) => {
            let all_json: Vec<serde_json::Value> = products
                .iter()
                .map(|p| {
                    serde_json::json!(
                        { "product_id" : p.product_id, "product_code" : p.product_code,
                        "product_name" : p.product_name, "description" : p.description,
                        "enabled" : p.enabled, "cb_pro_enabled" : p.cb_pro_enabled,
                        "config" : p.config, "created_at" : p.created_at, "updated_at" : p.updated_at, }
                    )
                })
                .collect();
            let total = all_json.len() as u64;
            let page_size = params.capped_page_size() as usize;
            let offset = params.offset() as usize;
            let page: Vec<_> = all_json.into_iter().skip(offset).take(page_size).collect();
            let total_pages = mazbri_core::num::f64_to_u64((mazbri_core::num::u64_to_f64(total) / mazbri_core::num::usize_to_f64(page_size)).ceil());
            crate::response::ApiResponse::ok(serde_json::json!(
                { "products" : page, "pagination" : { "page" : params.page,
                "page_size" : page_size, "total" : total, "total_pages" :
                total_pages, }, "actions" : { "grant" : true, "revoke" : true } }
            ))
            .into_response()
        }
        Err(e) => crate::response::ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list tenant products: {e}"),
        ),
    }
}
pub async fn get_tenant_account(
    db: &PgPool,
    billing_db: &PgPool,
    tenant_id: uuid::Uuid,
    id_or_code: &str,
) -> axum::response::Response {
    if !repo::tenant_exists(db, tenant_id).await {
        return crate::response::ApiResponse::err(
            StatusCode::NOT_FOUND,
            format!("Tenant not found: {id_or_code}"),
        );
    }
    let identities =
        match crate::repositories::tenant::get_tenant_identities(db, &[tenant_id]).await {
            Ok(map) => map,
            Err(e) => {
                return crate::response::ApiResponse::err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to fetch tenant identity: {e}"),
                );
            }
        };
    let (tenant_code, tenant_id_str) = identities
        .get(&tenant_id)
        .cloned()
        .unwrap_or_else(|| ("UNKNOWN".to_string(), "UNKNOWN".to_string()));
    match repo::get_tenant_account(billing_db, tenant_id).await {
        Ok(Some(account)) => {
            let can_refund = account.balance > Decimal::ZERO;
            account_response(&account, &tenant_code, &tenant_id_str, can_refund)
        }
        Ok(None) => match repo::create_tenant_account(billing_db, tenant_id).await {
            Ok(account) => {
                let can_refund = account.balance > Decimal::ZERO;
                account_response(&account, &tenant_code, &tenant_id_str, can_refund)
            }
            Err(e) => crate::response::ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create account: {e}"),
            ),
        },
        Err(e) => crate::response::ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get account: {e}"),
        ),
    }
}
pub struct RechargeTenantAccountArgs<'a> {
    pub db: &'a PgPool,
    pub billing_db: &'a PgPool,
    pub audit: &'a AuditService,
    pub tenant_id: uuid::Uuid,
    pub tenant_id_str: &'a str,
    pub req: &'a RechargeRequest,
    pub operator_id: uuid::Uuid,
    pub operator_name: &'a str,
    pub client_ip: Option<String>,
}

pub async fn recharge_tenant_account(
    args: RechargeTenantAccountArgs<'_>,
) -> axum::response::Response {
    let db = args.db;
    let billing_db = args.billing_db;
    let audit = args.audit;
    let tenant_id = args.tenant_id;
    let tenant_id_str = args.tenant_id_str;
    let req = args.req;
    let operator_id = args.operator_id;
    let operator_name = args.operator_name;
    let client_ip = args.client_ip;

    let amount = Decimal::from(req.amount);
    if amount <= Decimal::ZERO {
        return crate::response::ApiResponse::err(
            StatusCode::BAD_REQUEST,
            "Amount must be a positive number",
        );
    }
    if !repo::tenant_exists(db, tenant_id).await {
        return crate::response::ApiResponse::err(
            StatusCode::NOT_FOUND,
            format!("Tenant not found: {tenant_id_str}"),
        );
    }
    let payment_method = req.payment_method.as_deref().unwrap_or("manual");
    let description = req.description.as_deref().unwrap_or("Account recharge");
    if let Err(e) = repo::ensure_tenant_account(billing_db, tenant_id).await {
        return crate::response::ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to ensure account exists: {e}"),
        );
    }
    match repo::process_recharge(
        billing_db,
        repo::ProcessRechargeParams {
            tenant_id,
            amount,
            payment_method,
            payment_reference: &req.payment_reference,
            description,
            operator_id,
            operator_name,
        },
    )
    .await
    {
        Ok(result) => {
            info!(
                "Recharged tenant {} with amount {} by {}",
                tenant_id, amount, operator_name
            );
            audit.log(tenant_audit(
                operator_id,
                AuditAction::AccountRecharge,
                tenant_id,
                serde_json::json!(
                    { "amount" : amount.to_string(), "description" : description,
                    "balance_before" : result.balance_before.to_string(),
                    "balance_after" : result.balance_after.to_string(),
                    "transaction_id" : result.transaction_id.to_string(),
                    "payment_method" : payment_method }
                ),
                client_ip,
            ));
            crate::response::ApiResponse::ok(serde_json::json!(
                { "transaction_id" : result.transaction_id, "account_id" : result
                .account_id, "balance_before" : result.balance_before
                .to_string(), "balance_after" : result.balance_after.to_string(),
                "status" : result.status, }
            ))
            .into_response()
        }
        Err(e) => crate::response::ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to recharge: {e}"),
        ),
    }
}

pub struct RefundTenantAccountArgs<'a> {
    pub db: &'a PgPool,
    pub billing_db: &'a PgPool,
    pub audit: &'a AuditService,
    pub tenant_id: uuid::Uuid,
    pub tenant_id_str: &'a str,
    pub req: &'a RefundRequest,
    pub operator_id: uuid::Uuid,
    pub operator_name: &'a str,
    pub client_ip: Option<String>,
}

pub async fn refund_tenant_account(args: RefundTenantAccountArgs<'_>) -> axum::response::Response {
    let db = args.db;
    let billing_db = args.billing_db;
    let audit = args.audit;
    let tenant_id = args.tenant_id;
    let tenant_id_str = args.tenant_id_str;
    let req = args.req;
    let operator_id = args.operator_id;
    let operator_name = args.operator_name;
    let client_ip = args.client_ip;

    let amount = Decimal::from(req.amount);
    if amount <= Decimal::ZERO {
        return crate::response::ApiResponse::err(
            StatusCode::BAD_REQUEST,
            "Amount must be a positive number",
        );
    }
    if let Some(ref reason) = req.reason {
        if reason.len() > 500 {
            return crate::response::ApiResponse::err(
                StatusCode::BAD_REQUEST,
                "Reason must not exceed 500 characters",
            );
        }
    }
    if !repo::tenant_exists(db, tenant_id).await {
        return crate::response::ApiResponse::err(
            StatusCode::NOT_FOUND,
            format!("Tenant not found: {tenant_id_str}"),
        );
    }
    match repo::process_refund(
        billing_db,
        tenant_id,
        amount,
        &req.reason,
        &req.bill_id,
        operator_id,
        operator_name,
    )
    .await
    {
        Ok(result) => {
            info!(
                "Refunded tenant {} with amount {} by {} (transaction: {})",
                tenant_id, amount, operator_name, result.transaction_id
            );
            audit.log(tenant_audit(
                operator_id,
                AuditAction::AccountRefund,
                tenant_id,
                serde_json::json!(
                    { "amount" : amount.to_string(), "reason" : req.reason,
                    "bill_id" : req.bill_id, "balance_before" : result
                    .balance_before.to_string(), "balance_after" : result
                    .balance_after.to_string(), "transaction_id" : result
                    .transaction_id.to_string() }
                ),
                client_ip,
            ));
            crate::response::ApiResponse::ok(serde_json::json!(RefundResponse {
                success: true,
                transaction_id: result.transaction_id,
                balance_before: result.balance_before.to_string(),
                balance_after: result.balance_after.to_string(),
            }))
            .into_response()
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("余额不足") || error_msg.contains("非活动状态") {
                crate::response::ApiResponse::err(StatusCode::BAD_REQUEST, error_msg)
            } else if error_msg.contains("未找到租户账户") {
                crate::response::ApiResponse::err(StatusCode::NOT_FOUND, error_msg)
            } else {
                crate::response::ApiResponse::err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to process refund: {error_msg}"),
                )
            }
        }
    }
}
pub async fn list_tenant_transactions(
    db: &PgPool,
    billing_db: &PgPool,
    tenant_id: uuid::Uuid,
    tenant_id_str: &str,
    query: &TransactionListQuery,
) -> axum::response::Response {
    if !repo::tenant_exists(db, tenant_id).await {
        return crate::response::ApiResponse::err(
            StatusCode::NOT_FOUND,
            format!("Tenant not found: {tenant_id_str}"),
        );
    }
    if let Some(ref tt) = query.transaction_type {
        if !["credit", "refund", "charge", "adjustment", "payment"].contains(&tt.as_str()) {
            return crate::response::ApiResponse::err(
                StatusCode::BAD_REQUEST,
                "Transaction type must be one of: credit, refund, charge, adjustment, payment",
            );
        }
    }
    let limit = query.limit.clamp(1, 100);
    match repo::get_tenant_transactions(
        billing_db,
        tenant_id,
        i32::saturating_from(query.page),
        i32::saturating_from(limit),
        &query.transaction_type,
    )
    .await
    {
        Ok(rows) => {
            let mut transactions = Vec::new();
            let mut total = 0i64;
            for row in &rows {
                if total == 0 {
                    total = row.total_count;
                }
                transactions.push(Transaction {
                    id: row.id,
                    transaction_type: row.transaction_type.clone(),
                    amount: row.amount.to_string(),
                    balance_before: row.balance_before.to_string(),
                    balance_after: row.balance_after.to_string(),
                    description: row.description.clone(),
                    payment_method: row.payment_method.clone(),
                    operator_id: row.operator_id,
                    operator_name: row.operator_name.clone(),
                    created_at: row.created_at,
                });
            }
            let response_json = match serde_json::to_value(TransactionListResponse {
                transactions,
                total,
                page: query.page,
                limit,
            }) {
                Ok(v) => v,
                Err(e) => {
                    error!("Failed to serialize transaction list response: {}", e);
                    return crate::response::ApiResponse::err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to serialize response",
                    );
                }
            };
            crate::response::ApiResponse::ok(response_json).into_response()
        }
        Err(e) => crate::response::ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list transactions: {e}"),
        ),
    }
}
fn account_response(
    account: &repo::TenantAccountRow,
    tenant_code: &str,
    tenant_id_str: &str,
    can_refund: bool,
) -> axum::response::Response {
    crate::response::ApiResponse::ok(serde_json::json!(
        { "tenant_code" : tenant_code, "tenant_id" : tenant_id_str, "balance" :
        account.balance.to_string(), "credit_limit" : account.credit_limit
        .to_string(), "available_balance" : account.available_balance
        .to_string(), "status" : account.status, "credit_tier" : account
        .credit_tier, "billing_cycle" : account.billing_cycle, "created_at" :
        account.created_at, "updated_at" : account.updated_at, "actions" : {
        "recharge" : true, "refund" : can_refund } }
    ))
    .into_response()
}
fn product_write_error_response(
    action: &str,
    error: &ProductWriteError,
) -> axum::response::Response {
    let (status, message) = product_write_error_details(action, error);
    crate::response::ApiResponse::err(status, message)
}
fn product_write_error_details(action: &str, error: &ProductWriteError) -> (StatusCode, String) {
    match error {
        ProductWriteError::InvalidProductId(validation_error) => {
            (StatusCode::BAD_REQUEST, validation_error.to_string())
        }
        ProductWriteError::DuplicateProductId(product_id) => (
            StatusCode::CONFLICT,
            format!("product_id '{product_id}' already exists"),
        ),
        ProductWriteError::ExhaustedProductIdSpace => (
            StatusCode::CONFLICT,
            "product_id space exhausted".to_string(),
        ),
        ProductWriteError::ProductIdReservationNotFound(reservation_id) => (
            StatusCode::NOT_FOUND,
            format!("product_id reservation '{reservation_id}' not found"),
        ),
        ProductWriteError::ProductIdReservationExpired(reservation_id) => (
            StatusCode::CONFLICT,
            format!("product_id reservation '{reservation_id}' expired"),
        ),
        ProductWriteError::ProductIdReservationUnavailable(reservation_id) => (
            StatusCode::CONFLICT,
            format!(
                "product_id reservation '{reservation_id}' is no longer available"
            ),
        ),
        ProductWriteError::ProductIdReservationOwnedByAnotherOperator(reservation_id) => (
            StatusCode::FORBIDDEN,
            format!(
                "product_id reservation '{reservation_id}' belongs to another operator"
            ),
        ),
        ProductWriteError::Database(database_error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to {action}: {database_error}"),
        ),
    }
}
fn product_delete_error_response(
    action: &str,
    error: &ProductDeleteError,
) -> axum::response::Response {
    let (status, message) = product_delete_error_details(action, error);
    crate::response::ApiResponse::err(status, message)
}
fn product_delete_error_details(action: &str, error: &ProductDeleteError) -> (StatusCode, String) {
    match error {
        ProductDeleteError::NotFound => (StatusCode::NOT_FOUND, "Product not found".to_string()),
        ProductDeleteError::HasActiveTenants(_) => (StatusCode::BAD_REQUEST, error.to_string()),
        ProductDeleteError::Database(database_error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to {action}: {database_error}"),
        ),
    }
}
