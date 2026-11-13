use super::super::state::AppState;
use super::super::{
    admin, appearance_handlers, audit_handlers, mfa_settings_handlers, password_policy_handlers,
};
use axum::{
    Router,
    routing::{get, post},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/settings/system",
            get(admin::get_system_settings).put(admin::update_system_settings),
        )
        .route(
            "/settings/user",
            get(admin::get_user_settings).put(admin::update_user_settings),
        )
        .route(
            "/settings/appearance",
            get(appearance_handlers::get_appearance_settings)
                .put(appearance_handlers::update_appearance_settings),
        )
        .route(
            "/settings/appearance/reset",
            post(appearance_handlers::reset_appearance_settings),
        )
        .route(
            "/settings/password-policy",
            get(password_policy_handlers::get_password_policy)
                .put(password_policy_handlers::update_password_policy),
        )
        .route(
            "/settings/mfa",
            get(mfa_settings_handlers::get_mfa_settings)
                .put(mfa_settings_handlers::update_mfa_settings),
        )
        .route("/audit-logs", get(audit_handlers::list_audit_logs))
        .route("/audit-logs/{id}", get(audit_handlers::get_audit_log))
        .route("/audit-logs/export", get(audit_handlers::export_audit_logs))
}
