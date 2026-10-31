use super::super::alert_handlers as h;
use super::super::state::AppState;
use axum::{
    Router,
    routing::{get, post, put},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/alerts/rules",
            get(h::list_alert_rules).post(h::create_alert_rule),
        )
        .route(
            "/alerts/rules/{id}",
            put(h::update_alert_rule).delete(h::delete_alert_rule),
        )
        .route("/alerts/rules/{id}/enabled", put(h::set_alert_rule_enabled))
        .route("/alerts/events", get(h::list_alert_events))
        .route("/alerts/events/{id}/ack", post(h::ack_alert_event))
        .route("/alerts/events/{id}/assign", post(h::assign_alert_event))
        .route("/alerts/summary", get(h::alert_summary))
}
