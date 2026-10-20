use super::{Router, AppState, get, list_certificates_proxy, get_certificate_proxy, get_full_certificate_proxy, get_auto_renewal_config_proxy, configure_auto_renewal_proxy, issue_certificate_proxy, renew_certificate_proxy, revoke_certificate_proxy, get_ca_certificate_proxy, export_license_request_proxy, import_license_proxy, get_license_status_proxy};

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
