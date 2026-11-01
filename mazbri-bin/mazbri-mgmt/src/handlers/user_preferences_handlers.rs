//! 步骤 27：用户偏好读写。

use super::AppState;
use crate::middleware::auth::{AdminClaims, extract_user_id};
use crate::error::ApiError;
use crate::response::ApiResponse;
use crate::services::user_preferences as svc;
use axum::{
    extract::State,
    response::IntoResponse,
    Extension, Json,
};

/// `GET /admin/preferences`
///
/// # Errors
///
/// 鉴权失败或数据库错误。
pub async fn get_preferences(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = extract_user_id(&claims)?;
    let data = svc::get(&state.admin_db, user_id)
        .await
        .map_err(|e| ApiError::internal(format!("读取偏好失败: {e}")))?;
    Ok(ApiResponse::ok(data))
}

/// `PUT /admin/preferences`
///
/// # Errors
///
/// 鉴权失败或数据库错误。
pub async fn put_preferences(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Json(req): Json<svc::UpdatePreferences>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = extract_user_id(&claims)?;
    let data = svc::upsert(&state.admin_db, user_id, &req)
        .await
        .map_err(|e| ApiError::internal(format!("保存偏好失败: {e}")))?;
    Ok(ApiResponse::ok(data))
}
