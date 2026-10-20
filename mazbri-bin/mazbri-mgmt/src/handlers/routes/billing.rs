use super::super::billing_handlers;
use super::super::state::AppState;
use axum::{
    Router,
    routing::{get, patch, post},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/billing/accounts", get(billing_handlers::list_accounts))
        .route("/billing/bills", get(billing_handlers::list_bills))
        .route(
            "/billing/bills/generate",
            post(billing_handlers::generate_bill),
        )
        .route(
            "/billing/bills/generate-async",
            post(billing_handlers::generate_bill_async),
        )
        .route(
            "/billing/reconciliation/audit",
            post(billing_handlers::run_reconciliation_audit),
        )
        .route(
            "/billing/reconciliation/settings",
            get(billing_handlers::get_reconciliation_settings)
                .put(billing_handlers::update_reconciliation_settings),
        )
        .route(
            "/billing/reconciliation/discrepancies",
            get(billing_handlers::list_reconciliation_discrepancies),
        )
        .route(
            "/billing/reconciliation/discrepancies/{id}",
            patch(billing_handlers::update_reconciliation_discrepancy)
                .delete(billing_handlers::delete_reconciliation_discrepancy),
        )
        .route(
            "/billing/bills/{id}",
            get(billing_handlers::get_bill).delete(billing_handlers::delete_bill),
        )
        .route(
            "/billing/bills/{id}/status",
            post(billing_handlers::update_bill_status),
        )
        .route(
            "/billing/bills/{id}/adjustments",
            get(billing_handlers::list_bill_adjustments)
                .post(billing_handlers::create_bill_adjustment),
        )
        .route(
            "/billing/adjustments",
            get(billing_handlers::list_global_bill_adjustments),
        )
        .route(
            "/billing/adjustments/{id}/void",
            post(billing_handlers::void_bill_adjustment),
        )
        .route(
            "/billing/adjustment",
            post(billing_handlers::create_adjustment),
        )
}
