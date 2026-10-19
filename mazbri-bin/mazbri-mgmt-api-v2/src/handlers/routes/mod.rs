//! HTTP route table for mgmt-api-v2, split by domain for easier review/diff.
use super::state::AppState;
use axum::Router;

mod admins;
mod analytics;
mod auth;
mod billing;
mod callbacks;
mod config_center;
mod products;
mod settings;
mod suppliers_routes;
mod tenants;
mod upstream;

/// Compose the full admin API router from domain sub-routers.
pub fn create_routes() -> Router<AppState> {
    Router::new()
        .merge(auth::routes())
        .merge(products::routes())
        .merge(suppliers_routes::routes())
        .merge(upstream::routes())
        .merge(tenants::routes())
        .merge(analytics::routes())
        .merge(settings::routes())
        .merge(billing::routes())
        .merge(admins::routes())
        .merge(config_center::routes())
        .merge(callbacks::routes())
}
