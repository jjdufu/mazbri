use super::super::AppState;
use super::super::types::MfaVerifyRequest;
use crate::audit::extract_client_ip;
use crate::middleware::admin_token::TokenKind;
use crate::services::mfa as svc;
use axum::{
    Json,
    extract::{ConnectInfo, State},
};
/// MFA 登录验证：只接受 `mfa_pending` 临时令牌（HS256 + iss + aud + exp + 类型 + 未使用）。
pub async fn mfa_verify(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(req): Json<MfaVerifyRequest>,
) -> axum::response::Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let jwt_secret = &state.auth_state.jwt_secret;
    let (claims, user_id) = match svc::authenticate_stage_token(
        &state.redis,
        jwt_secret,
        &req.temp_token,
        TokenKind::MfaPending,
    )
    .await
    {
        Ok(authenticated) => authenticated,
        Err(response) => return *response,
    };
    svc::verify(svc::VerifyParams {
        pool: &state.admin_db,
        redis: &state.redis,
        mfa_cache: &state.mfa_cache,
        audit: &state.audit,
        jwt_secret,
        temp_claims: &claims,
        user_id,
        totp_code: req.totp_code.as_deref(),
        backup_code: req.backup_code.as_deref(),
        client_ip,
    })
    .await
}
