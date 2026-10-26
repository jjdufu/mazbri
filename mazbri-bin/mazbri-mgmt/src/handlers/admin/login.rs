use super::super::AppState;
use super::super::types::{AdminLoginRequest, AdminLogoutRequest};
use crate::error::ApiError;
use crate::middleware::auth::{
    REFRESH_COOKIE, clear_session_cookies, cookie_value, json_with_session_cookies,
};
use crate::services::admin;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, State},
    response::Response,
};
use tracing::debug;
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn admin_login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(req): Json<AdminLoginRequest>,
) -> Result<Response, ApiError> {
    debug!("Login attempt for user '{}'", req.username);
    let client_ip = crate::audit::extract_client_ip(&headers, Some(addr));
    let jwt_secret = &state.auth_state.jwt_secret;
    let data = admin::admin_login(admin::AdminLoginParams {
        db: &state.admin_db,
        jwt_secret,
        jwt_expiration_secs: state.config.auth.jwt_expiration_secs,
        refresh_token_expiration_secs: state.config.auth.refresh_token_expiration_secs,
        password_policy_cache: &state.password_policy_cache,
        mfa_settings_cache: &state.mfa_settings_cache,
        audit: &state.audit,
        req: &req,
        client_ip,
    })
    .await?;
    Ok(json_with_session_cookies(
        data,
        Some(state.config.auth.refresh_token_expiration_secs),
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn admin_refresh_token(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
) -> Result<Response, ApiError> {
    let data = admin::admin_refresh_token(admin::AdminRefreshParams {
        db: &state.admin_db,
        redis: &state.redis,
        claims: &claims,
        jwt_secret: &state.auth_state.jwt_secret,
        jwt_expiration_secs_fallback: state.config.auth.jwt_expiration_secs,
        password_policy_cache: &state.password_policy_cache,
        mfa_settings_cache: &state.mfa_settings_cache,
    })
    .await?;
    Ok(json_with_session_cookies(
        data,
        Some(state.config.auth.refresh_token_expiration_secs),
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn admin_logout(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
    headers: axum::http::HeaderMap,
    Json(mut req): Json<AdminLogoutRequest>,
) -> Result<Response, ApiError> {
    let client_ip = crate::audit::extract_client_ip(&headers, Some(addr));
    if req.refresh_token.is_none() {
        req.refresh_token = cookie_value(&headers, REFRESH_COOKIE);
    }
    let data = admin::admin_logout(
        &claims,
        &state.auth_state.jwt_secret,
        &state.redis,
        &state.audit,
        &req,
        client_ip,
    )
    .await?;
    Ok(clear_session_cookies(json_with_session_cookies(data, None)))
}
