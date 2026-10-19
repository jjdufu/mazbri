use super::*;

pub fn create_service_name_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_service_names_proxy))
        .route("/", axum::routing::post(create_service_name_proxy))
        .route("/{name}", get(get_service_name_proxy))
        .route("/{name}", axum::routing::put(update_service_name_proxy))
        .route("/{name}", axum::routing::delete(delete_service_name_proxy))
        .route("/{name}/validate", get(validate_service_name_proxy))
}
pub fn create_cert_proxy_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_certificates_proxy))
        .route("/{id}", get(get_certificate_proxy))
        .route("/{id}/full", get(get_full_certificate_proxy))
        .route("/{id}/auto-renewal", get(get_auto_renewal_config_proxy))
        .route(
            "/{id}/auto-renewal",
            axum::routing::put(configure_auto_renewal_proxy),
        )
        .route("/issue", axum::routing::post(issue_certificate_proxy))
        .route("/renew/{id}", axum::routing::post(renew_certificate_proxy))
        .route(
            "/revoke/{id}",
            axum::routing::post(revoke_certificate_proxy),
        )
        .route("/ca", get(get_ca_certificate_proxy))
        .route("/presets", get(list_presets_proxy))
        .route("/presets/{id}", get(get_preset_proxy))
        .route("/presets", axum::routing::post(create_preset_proxy))
        .route("/presets/{id}", axum::routing::put(update_preset_proxy))
        .route("/presets/{id}", axum::routing::delete(delete_preset_proxy))
        .route("/services", get(list_service_names_proxy))
        .route("/services", axum::routing::post(create_service_name_proxy))
        .route("/services/{name}", get(get_service_name_proxy))
        .route(
            "/services/{name}",
            axum::routing::put(update_service_name_proxy),
        )
        .route(
            "/services/{name}",
            axum::routing::delete(delete_service_name_proxy),
        )
        .route(
            "/services/{name}/validate",
            get(validate_service_name_proxy),
        )
        .route(
            "/authorizations/license/request",
            axum::routing::post(export_license_request_proxy),
        )
        .route(
            "/authorizations/license/import",
            axum::routing::post(import_license_proxy),
        )
        .route(
            "/authorizations/license/status",
            get(get_license_status_proxy),
        )
}
