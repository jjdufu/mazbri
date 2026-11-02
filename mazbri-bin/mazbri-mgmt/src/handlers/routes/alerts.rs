use super::super::alert_handlers as h;
use super::super::alert_ops_handlers as ops;
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
        .route(
            "/alerts/silences",
            get(ops::list_silences).post(ops::create_silence),
        )
        .route("/alerts/silences/{id}", axum::routing::delete(ops::delete_silence))
        .route(
            "/alerts/inhibits",
            get(ops::list_inhibits).post(ops::create_inhibit),
        )
        .route(
            "/alerts/inhibits/{id}",
            put(ops::update_inhibit).delete(ops::delete_inhibit),
        )
}
