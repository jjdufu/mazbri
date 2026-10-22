use super::super::admin_management;
use super::super::state::AppState;
use axum::{
    Router,
    routing::{get, post, put},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/admins",
            get(admin_management::list_admins).post(admin_management::create_admin),
        )
        .route(
            "/admins/{id}",
            get(admin_management::get_admin)
                .put(admin_management::update_admin)
                .delete(admin_management::delete_admin),
        )
        .route(
            "/admins/{id}/password",
            put(admin_management::reset_admin_password),
        )
        .route(
            "/admins/{id}/status",
            put(admin_management::set_admin_status),
        )
        .route(
            "/admins/{id}/force-password-reset",
            put(admin_management::force_password_reset),
        )
        .route(
            "/admins/{id}/reset-mfa",
            post(admin_management::reset_admin_mfa),
        )
}
