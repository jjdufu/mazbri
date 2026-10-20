use super::super::config_center as cc;
use super::super::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/config-center/configs",
            get(cc::list_configs).post(cc::create_config),
        )
        .route(
            "/config-center/configs/{service_name}",
            get(cc::get_config)
                .put(cc::update_config)
                .delete(cc::delete_config),
        )
        .route(
            "/config-center/configs/{service_name}/history",
            get(cc::get_history),
        )
        .route(
            "/config-center/configs/{service_name}/rollback/{version}",
            post(cc::rollback),
        )
        .route("/config-center/services", get(cc::list_services))
        .route(
            "/config-center/service-names",
            get(cc::list_canonical_service_names),
        )
        .route("/config-center/export", get(cc::export_configs))
        .route("/config-center/import", post(cc::import_configs))
        .route("/config-center/validate", post(cc::validate_toml_endpoint))
        .route("/config-center/nodes", get(cc::list_nodes))
}
