use super::super::types::AdminUser;
use crate::response::ApiResponse;
use axum::{Extension, response::IntoResponse};
use chrono::Utc;
pub async fn get_current_admin(
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
) -> impl IntoResponse {
    let user = AdminUser {
        id: claims.user_id.clone(),
        username: claims.sub.clone(),
        role: claims.role.clone(),
        created_at: Utc::now().to_rfc3339(),
    };
    ApiResponse::ok(serde_json::json!(user))
}
