use super::super::state::AppState;
use super::super::{admin, mfa};
use axum::{
    Router,
    routing::{get, post, put},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(admin::admin_login))
        .route(
            "/auth/first-login-change-password",
            post(admin::first_login_change_password),
        )
        .route("/auth/me", get(admin::get_current_admin))
        .route("/auth/change-password", put(admin::change_password))
        .route("/auth/refresh", post(admin::admin_refresh_token))
        .route("/auth/logout", post(admin::admin_logout))
        .route("/auth/mfa/setup", post(mfa::mfa_setup))
        .route("/auth/mfa/enable", post(mfa::mfa_enable))
        .route("/auth/mfa/disable", post(mfa::mfa_disable))
        .route("/auth/mfa/verify", post(mfa::mfa_verify))
        .route("/auth/mfa/status", get(mfa::mfa_status))
        .route(
            "/auth/mfa/regenerate-codes",
            post(mfa::regenerate_backup_codes),
        )
        .route("/auth/mfa/required-setup", post(mfa::mfa_required_setup))
        .route("/auth/mfa/required-enable", post(mfa::mfa_required_enable))
}
