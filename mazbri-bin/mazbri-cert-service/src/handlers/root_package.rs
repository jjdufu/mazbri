use crate::error::CertError;
use axum::{Json, extract::State, response::IntoResponse};
use chrono::Utc;
use std::sync::Arc;

pub async fn get_current_root_materials(
    State(state): State<Arc<crate::server::AppState>>,
) -> Result<impl IntoResponse, CertError> {
    let decision =
        crate::license_service::current_decision(state.repo.get_pool(), Utc::now()).await?;
    if !decision.decryption_allowed {
        return Err(CertError::NotFound(
            "commercial root protection material".to_string(),
        ));
    }
    let Some(bundle) = state.current_root_material_bundle_for_decryption().await? else {
        return Err(CertError::NotFound(
            "commercial root protection material".to_string(),
        ));
    };
    Ok(Json(bundle))
}
