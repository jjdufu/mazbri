use super::super::product;
use super::super::state::AppState;
use axum::{
    Router,
    routing::{delete, get, post},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/products",
            get(product::list_products).post(product::create_product),
        )
        .route(
            "/products/reservations",
            post(product::create_product_reservation),
        )
        .route(
            "/products/reservations/{reservation_id}",
            delete(product::release_product_reservation),
        )
        .route(
            "/products/{product_id}",
            get(product::get_product)
                .put(product::update_product)
                .delete(product::delete_product),
        )
        .route(
            "/products/{product_id}/grant",
            post(product::grant_tenant_access),
        )
        .route(
            "/products/{product_id}/revoke",
            post(product::revoke_tenant_access),
        )
        .route(
            "/products/{product_id}/tenants",
            get(product::list_product_tenants),
        )
        .route("/authorizations", get(product::list_authorizations))
}
