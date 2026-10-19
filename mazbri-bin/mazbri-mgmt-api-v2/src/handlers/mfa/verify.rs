use super::super::AppState;
use super::super::types::MfaVerifyRequest;
use crate::audit::extract_client_ip;
use crate::middleware::auth::extract_user_id_response;
use crate::services::mfa as svc;
use axum::{
    Json,
    extract::{ConnectInfo, State},
    response::IntoResponse,
};
use tracing::warn;
pub async fn mfa_verify(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(req): Json<MfaVerifyRequest>,
) -> axum::response::Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let jwt_secret = &state.auth_state.jwt_secret;
    let decoding_key = jsonwebtoken::DecodingKey::from_secret(jwt_secret.as_ref());
    let validation = jsonwebtoken::Validation::default();
    let claims = match jsonwebtoken::decode::<super::super::types::AdminJwtClaims>(
        &req.temp_token,
        &decoding_key,
        &validation,
    ) {
        Ok(data) => data.claims,
        Err(e) => {
            warn!("Invalid temp token: {}", e);
            return crate::response::ApiResponse::err(
                axum::http::StatusCode::UNAUTHORIZED,
                mazbri_i18n::mfa::invalid_temp_token(),
            );
        }
    };
    let user_id = match extract_user_id_response(&crate::middleware::auth::AdminClaims {
        iss: claims.iss.clone(),
        sub: claims.sub.clone(),
        user_id: claims.user_id.clone(),
        role: claims.role.clone(),
        iat: claims.iat,
        exp: claims.exp,
        jti: claims.jti.clone(),
        token_type: claims.token_type.clone(),
    }) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };
    svc::verify(svc::VerifyParams {
        pool: &state.admin_db,
        mfa_cache: &state.mfa_cache,
        audit: &state.audit,
        jwt_secret,
        user_id,
        username: &claims.sub,
        user_id_str: &claims.user_id,
        role: &claims.role,
        totp_code: req.totp_code.as_deref(),
        backup_code: req.backup_code.as_deref(),
        client_ip,
    })
    .await
}
