use crate::audit::AuditErrorInfo;
use axum::{
    Extension,
    extract::Request,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use jsonwebtoken::{DecodingKey, Validation, decode};
use mazbri_auth::JwtKeyManager;
use mazbri_core::num::SaturatingFrom;
use mazbri_core::redis::RedisClient;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::info;

/// `HttpOnly` access-token cookie (browser session; not readable by JS).
pub const ACCESS_COOKIE: &str = "mazbri_at";
/// `HttpOnly` refresh-token cookie.
pub const REFRESH_COOKIE: &str = "mazbri_rt";
const DEFAULT_REFRESH_MAX_AGE_SECS: u64 = 604_800;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminClaims {
    pub iss: String,
    pub sub: String,
    pub user_id: String,
    pub role: String,
    pub iat: i64,
    pub exp: i64,
    /// Present on tokens issued after H-11; older tokens may omit it.
    #[serde(default)]
    pub jti: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
}
#[derive(Clone)]
pub struct AdminAuthState {
    pub jwt_secret: String,
    pub redis: Arc<RedisClient>,
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_auth_state_from_db(
    admin_db: Arc<PgPool>,
    redis: Arc<RedisClient>,
) -> Result<AdminAuthState, String> {
    let manager = JwtKeyManager::new(admin_db, mazbri_core::ServiceName::Mgmt.as_str());
    match manager.get_jwt_secret().await {
        Ok(secret) => {
            info!("JWT 密钥已从数据库加载（HKDF 派生）");
            Ok(AdminAuthState {
                jwt_secret: secret,
                redis,
            })
        }
        Err(e) => Err(format!("Failed to load JWT secret from database: {e}")),
    }
}

/// Redis key for revoked admin JWT jti (logout denylist).
#[must_use]
pub fn revoked_jti_key(jti: &str) -> String {
    format!("admin:revoked_jti:{jti}")
}

pub async fn is_jti_revoked(redis: &RedisClient, jti: &str) -> bool {
    redis.exists(&revoked_jti_key(jti)).await.unwrap_or(false)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn revoke_jti(redis: &RedisClient, jti: &str, ttl_secs: i64) -> Result<(), String> {
    let ttl = usize::saturating_from(ttl_secs.max(1));
    redis
        .set_ex_bytes(&revoked_jti_key(jti), b"1", ttl)
        .await
        .map_err(|e| format!("Failed to revoke token: {e}"))
}
fn normalize_path(path: &str) -> String {
    let mut result = String::with_capacity(path.len());
    let mut prev_was_slash = false;
    for ch in path.chars() {
        if ch == '/' {
            if !prev_was_slash {
                result.push(ch);
            }
            prev_was_slash = true;
        } else {
            result.push(ch);
            prev_was_slash = false;
        }
    }
    result
}
pub async fn admin_auth_middleware(
    Extension(auth_state): Extension<Arc<AdminAuthState>>,
    mut request: Request,
    next: axum::middleware::Next,
) -> Response {
    let path = normalize_path(request.uri().path());
    if path.ends_with("/auth/login")
        || path.ends_with("/auth/mfa/verify")
        || path.ends_with("/auth/mfa/setup")
        || path.ends_with("/auth/mfa/required-setup")
        || path.ends_with("/auth/mfa/required-enable")
        || path == "/health"
    {
        return next.run(request).await;
    }
    let is_refresh = path.ends_with("/auth/refresh");
    let Some(token) = extract_bearer_or_session_cookie(request.headers(), is_refresh) else {
        return unauthorized("missing_token", "缺少认证令牌");
    };
    let decoding_key = DecodingKey::from_secret(auth_state.jwt_secret.as_ref());
    let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.set_issuer(&["mazbri-mgmt-api"]);
    match decode::<AdminClaims>(&token, &decoding_key, &validation) {
        Ok(token_data) => {
            // 签名已校验，身份可信：先记录操作人，后续拒绝也能归属到具体账号。
            audit_actor_from_claims(&token_data.claims);
            let token_type = token_data.claims.token_type.as_deref();
            if is_refresh {
                if token_type != Some("refresh") {
                    return unauthorized(
                        "invalid_token_type",
                        "无效的令牌类型，需要 refresh token",
                    );
                }
            } else if token_type == Some("refresh") {
                return unauthorized("invalid_token_type", "请使用访问令牌，而非 refresh token");
            }
            if let Some(ref jti) = token_data.claims.jti
                && is_jti_revoked(&auth_state.redis, jti).await
            {
                return unauthorized("token_revoked", "令牌已注销");
            }
            request.extensions_mut().insert(token_data.claims);
            next.run(request).await
        }
        Err(e) => {
            let (reason, error_msg) = match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                    audit_actor_from_expired(&token, &decoding_key);
                    ("token_expired", "令牌已过期")
                }
                jsonwebtoken::errors::ErrorKind::InvalidToken => ("invalid_token", "无效的令牌"),
                _ => ("invalid_token", "令牌验证失败"),
            };
            let mut response = unauthorized("invalid_token", error_msg);
            crate::audit::set_failure_reason(reason);
            AuditErrorInfo::attach(&mut response, reason, error_msg);
            response
        }
    }
}

/// 把已验签令牌中的身份写入审计作用域（不记录令牌本身）。
fn audit_actor_from_claims(claims: &AdminClaims) {
    crate::audit::set_actor(
        uuid::Uuid::parse_str(&claims.user_id).ok(),
        Some(&claims.sub),
        Some(&claims.role),
    );
}

/// 令牌过期但签名有效时，仍可用于审计归属。
fn audit_actor_from_expired(token: &str, key: &DecodingKey) {
    let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.set_issuer(&["mazbri-mgmt-api"]);
    validation.validate_exp = false;
    if let Ok(data) = decode::<AdminClaims>(token, key, &validation) {
        audit_actor_from_claims(&data.claims);
    }
}

/// Prefer `Authorization: Bearer`, then session cookies (`mazbri_at` / `mazbri_rt`).
pub fn extract_bearer_or_session_cookie(
    headers: &HeaderMap,
    prefer_refresh: bool,
) -> Option<String> {
    if let Some(auth_header) = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        && let Some(token) = auth_header.strip_prefix("Bearer ")
    {
        let token = token.trim();
        if !token.is_empty() {
            return Some(token.to_string());
        }
    }
    let name = if prefer_refresh {
        REFRESH_COOKIE
    } else {
        ACCESS_COOKIE
    };
    cookie_value(headers, name)
}

pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once('=')
            && k.trim() == name
        {
            let v = v.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn cookie_secure_flag() -> bool {
    !mazbri_core::is_dev_flag()
}

fn format_set_cookie(name: &str, value: &str, max_age_secs: u64, clear: bool) -> String {
    let max_age = if clear { 0 } else { max_age_secs };
    let mut cookie = format!("{name}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax");
    if cookie_secure_flag() {
        cookie.push_str("; Secure");
    }
    cookie
}

fn append_set_cookie(headers: &mut HeaderMap, cookie: &str) {
    if let Ok(value) = HeaderValue::from_str(cookie) {
        headers.append(header::SET_COOKIE, value);
    }
}

/// Build JSON success response; if `token`/`refresh_token` present, set `HttpOnly` cookies
/// and **omit** those fields from the body (XSS cannot read them from JS storage).
pub fn json_with_session_cookies(
    mut data: serde_json::Value,
    refresh_max_age_secs: Option<u64>,
) -> Response {
    let access = data
        .get("token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let refresh = data
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let expires_in = data
        .get("expires_in")
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(u64::saturating_from)))
        .unwrap_or(3600);

    if access.is_none() && refresh.is_none() {
        return crate::response::ApiResponse::ok(data).into_response();
    }

    if let Some(obj) = data.as_object_mut() {
        obj.remove("token");
        obj.remove("refresh_token");
        obj.insert("cookie_auth".to_string(), serde_json::json!(true));
    }

    let mut response = crate::response::ApiResponse::ok(data).into_response();
    let headers = response.headers_mut();
    if let Some(at) = access {
        append_set_cookie(
            headers,
            &format_set_cookie(ACCESS_COOKIE, &at, expires_in, false),
        );
    }
    if let Some(rt) = refresh {
        let max_age = refresh_max_age_secs.unwrap_or(DEFAULT_REFRESH_MAX_AGE_SECS);
        append_set_cookie(
            headers,
            &format_set_cookie(REFRESH_COOKIE, &rt, max_age, false),
        );
    }
    response
}

/// Expire session cookies (logout).
#[must_use]
pub fn clear_session_cookies(mut response: Response) -> Response {
    let headers = response.headers_mut();
    append_set_cookie(headers, &format_set_cookie(ACCESS_COOKIE, "", 0, true));
    append_set_cookie(headers, &format_set_cookie(REFRESH_COOKIE, "", 0, true));
    response
}
pub fn get_claims(request: &Request) -> Option<&AdminClaims> {
    request.extensions().get::<AdminClaims>()
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn require_role(claims: &AdminClaims, required_roles: &[&str]) -> Result<(), Box<Response>> {
    if required_roles.contains(&claims.role.as_str()) {
        Ok(())
    } else {
        Err(Box::new(forbidden(
            "insufficient_permissions",
            &format!("此操作需要以下角色之一: {}", required_roles.join(", ")),
        )))
    }
}
#[must_use]
pub fn can_manage_certificates(claims: &AdminClaims) -> bool {
    claims.role == "R_ADMIN"
}
#[must_use]
pub fn can_manage_tenants(claims: &AdminClaims) -> bool {
    matches!(claims.role.as_str(), "R_ADMIN" | "R_OPERATOR")
}
#[must_use]
pub fn can_access_settings(claims: &AdminClaims) -> bool {
    claims.role == "R_ADMIN"
}
#[must_use]
pub fn can_manage_admins(claims: &AdminClaims) -> bool {
    claims.role == "R_ADMIN"
}
#[must_use]
pub fn is_admin(claims: &AdminClaims) -> bool {
    claims.role == "R_ADMIN"
}
#[must_use]
pub fn is_operator_or_above(claims: &AdminClaims) -> bool {
    matches!(claims.role.as_str(), "R_ADMIN" | "R_OPERATOR")
}
#[must_use]
pub fn can_write_business(claims: &AdminClaims) -> bool {
    matches!(claims.role.as_str(), "R_ADMIN" | "R_OPERATOR")
}
#[must_use]
pub fn can_access_control_panel(claims: &AdminClaims) -> bool {
    claims.role == "R_ADMIN"
}
#[must_use]
pub fn can_assign_role(claims: &AdminClaims, target_role: &str) -> bool {
    let role_level = |role: &str| -> u8 {
        match role {
            "R_ADMIN" => 3,
            "R_OPERATOR" | "R_FINANCE" => 2,
            "R_VIEWER" => 1,
            _ => 0,
        }
    };
    role_level(&claims.role) >= role_level(target_role)
}
#[must_use]
pub fn get_role_level(role: &str) -> u8 {
    match role {
        "R_ADMIN" => 3,
        "R_OPERATOR" | "R_FINANCE" => 2,
        "R_VIEWER" => 1,
        _ => 0,
    }
}
#[must_use]
pub fn is_read_only(claims: &AdminClaims) -> bool {
    claims.role == "R_VIEWER"
}
fn unauthorized(error: &str, message: &str) -> Response {
    let mut response = (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({ "error" : error, "message" : message })),
    )
        .into_response();
    AuditErrorInfo::attach(&mut response, error, message);
    response
}
fn forbidden(error: &str, message: &str) -> Response {
    let mut response = (
        StatusCode::FORBIDDEN,
        axum::Json(serde_json::json!({ "error" : error, "message" : message })),
    )
        .into_response();
    AuditErrorInfo::attach(&mut response, error, message);
    response
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn extract_user_id(claims: &AdminClaims) -> Result<uuid::Uuid, crate::error::ApiError> {
    let uid = uuid::Uuid::parse_str(&claims.user_id).map_err(|e| {
        tracing::warn!(
            action = "extract_user_id", user_id_raw = % claims.user_id, error = % e,
            "Invalid user_id in JWT claims / JWT claims 中的 user_id 无效"
        );
        crate::error::ApiError::Unauthorized
    })?;
    if uid.is_nil() {
        tracing::warn!(
            action = "extract_user_id", user_id_raw = % claims.user_id,
            "Nil UUID in JWT claims / JWT claims 中的 user_id 为空 UUID"
        );
        return Err(crate::error::ApiError::Unauthorized);
    }
    Ok(uid)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn extract_user_id_response(
    claims: &AdminClaims,
) -> Result<uuid::Uuid, (StatusCode, axum::Json<serde_json::Value>)> {
    let uid = uuid::Uuid::parse_str(&claims.user_id).map_err(|e| {
        tracing::warn!(
            action = "extract_user_id", user_id_raw = % claims.user_id, error = % e,
            "Invalid user_id in JWT claims / JWT claims 中的 user_id 无效"
        );
        (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!(
                { "error" : "invalid_token", "message" :
                "令牌中的用户标识无效", "messageKey" :
                "api.auth.invalid_user_id" }
            )),
        )
    })?;
    if uid.is_nil() {
        tracing::warn!(
            action = "extract_user_id", user_id_raw = % claims.user_id,
            "Nil UUID in JWT claims / JWT claims 中的 user_id 为空 UUID"
        );
        return Err((
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!(
                { "error" : "invalid_token", "message" :
                "令牌中的用户标识无效", "messageKey" :
                "api.auth.invalid_user_id" }
            )),
        ));
    }
    Ok(uid)
}
