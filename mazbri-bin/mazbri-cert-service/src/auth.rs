use crate::base_authorization_service::BaseAuthorizationState;
use crate::config::CertAuthConfig;
use crate::token::TokenManager;
use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::sync::Arc;
#[derive(Clone)]
pub struct AuthState {
    pub token_manager: Arc<TokenManager>,
}
impl AuthState {
    pub fn new(token_manager: Arc<TokenManager>, _auth_config: &CertAuthConfig) -> Self {
        Self { token_manager }
    }
    pub async fn update_config(&self, auth_config: &CertAuthConfig) {
        self.token_manager
            .update_config(Arc::new(auth_config.clone()))
            .await;
    }
}
fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let auth_header = headers.get("authorization")?.to_str().ok()?;
    auth_header
        .strip_prefix("Bearer ")
        .map(std::string::ToString::to_string)
}
pub async fn auth_middleware_inner(
    auth_state: Arc<AuthState>,
    request: Request,
    next: Next,
) -> Response {
    if let Some(token) = extract_bearer_token(request.headers()) {
        match auth_state.token_manager.validate(&token).await {
            Ok(true) => {
                crate::audit::note_authenticated();
                return next.run(request).await;
            }
            Ok(false) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!(
                        { "error" : "invalid_token", "message" :
                        "令牌无效或已过期" }
                    )),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error" : "auth_error", "message" : e.to_string() })),
                )
                    .into_response();
            }
        }
    }
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error" : "missing_token", "message" : "需要 Bearer Token 认证头" })),
    )
        .into_response()
}
pub async fn auth_middleware(
    State(auth_state): State<Arc<AuthState>>,
    request: Request,
    next: Next,
) -> Response {
    auth_middleware_inner(auth_state, request, next).await
}
/// 配置中心写接口的许可证闸，外层已经校验过 Bearer token。
///
/// 许可证判定为 active 或 grace 才放行，否则返回 403
/// （`base_authorization_not_active` / `base_authorization_expired`）。
/// 判定只看许可证原文，所以重启恢复、许可证续期、换钥和业务证书续期都不会让它失效。
pub async fn license_write_middleware(
    State(base_authorization): State<Arc<BaseAuthorizationState>>,
    request: Request,
    next: Next,
) -> Response {
    match base_authorization.require_management_writes().await {
        Ok(()) => next.run(request).await,
        Err(error) => {
            tracing::warn!(error = %error, "配置中心写入被许可证状态拒绝");
            error.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AuthState;
    use crate::config::CertAuthConfig;
    use crate::token::TokenManager;
    use std::sync::Arc;

    fn auth_config(tokens: &[&str]) -> CertAuthConfig {
        CertAuthConfig {
            static_tokens: tokens
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
        }
    }

    #[tokio::test]
    async fn token_validation_updates_correctly() {
        let startup = auth_config(&["old"]);
        let manager = Arc::new(TokenManager::new(Arc::new(startup.clone())));
        let state = AuthState::new(manager, &startup);
        state.update_config(&auth_config(&["new"])).await;
        assert!(matches!(
            state.token_manager.validate("new").await,
            Ok(true)
        ));
        assert!(matches!(
            state.token_manager.validate("old").await,
            Ok(false)
        ));
    }
}
