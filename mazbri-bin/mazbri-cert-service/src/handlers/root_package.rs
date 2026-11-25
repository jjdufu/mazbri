use crate::error::CertError;
use axum::{Json, extract::State, response::IntoResponse};
use chrono::Utc;
use std::sync::Arc;

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
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

/// 返回多版本根保护材料密钥环（`GET /authorizations/root/materials/keyring`）。
///
/// # Errors
///
/// 许可证不允许解密、未装载密码或根材料缺失时返回错误。
pub async fn get_root_material_keyring(
    State(state): State<Arc<crate::server::AppState>>,
) -> Result<impl IntoResponse, CertError> {
    let decision =
        crate::license_service::current_decision(state.repo.get_pool(), Utc::now()).await?;
    if !decision.decryption_allowed {
        return Err(CertError::NotFound(
            "commercial root protection material".to_string(),
        ));
    }
    let keyring = state
        .current_root_material_keyring(decision.issuance_allowed())
        .await?;
    Ok(Json(keyring))
}
