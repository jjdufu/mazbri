use crate::handlers::certificate::CertificateState;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde_json::json;
pub async fn get_ca_certificate(State(state): State<CertificateState>) -> impl IntoResponse {
    match state.base_authorization.require_ca_manager().await {
        Ok(ca_manager) => {
            let ca_pem = ca_manager.get_ca_pem();
            (StatusCode::OK, Json(json!({ "ca_cert_pem" : ca_pem }))).into_response()
        }
        Err(error) => error.into_response(),
    }
}
