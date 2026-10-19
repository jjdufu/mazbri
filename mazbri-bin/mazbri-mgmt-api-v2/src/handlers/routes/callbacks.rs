use super::super::callback_handlers;
use super::super::state::AppState;
use axum::{
    Router,
    routing::{delete, get, post},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/callbacks/subscriptions",
            get(callback_handlers::list_subscriptions).post(callback_handlers::create_subscription),
        )
        .route(
            "/callbacks/subscriptions/{id}",
            delete(callback_handlers::delete_subscription),
        )
        .route(
            "/callbacks/deliveries",
            get(callback_handlers::list_delivery_logs),
        )
        .route(
            "/callbacks/deliveries/{id}/retry",
            post(callback_handlers::retry_delivery),
        )
}
