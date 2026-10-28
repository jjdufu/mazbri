use super::admin_token::{self, TokenKind, TokenRejection};
use crate::audit::AuditErrorInfo;
use axum::{
    Extension,
    extract::{MatchedPath, Request},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
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
    /// 令牌用途对应的受众（见 [`super::admin_token::TokenKind::audience`]）；旧令牌缺失。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    pub sub: String,
    pub user_id: String,
    pub role: String,
    pub iat: i64,
    pub exp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
    /// 密码验证成功时生成的会话号。缺失即解码失败。
    pub sid: String,
    /// 会话绝对到期（刷新令牌 `exp`）。只有临时令牌可以省略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sxp: Option<i64>,
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
/// 无需请求头令牌的接口（完整路径精确匹配）。
///
/// 其中 MFA 相关接口凭请求体中的专用临时令牌调用，由处理器按类型严格校验。
/// `/auth/mfa/setup` 不在此列：它属于已登录用户的 MFA 设置，需要访问令牌。
pub const PUBLIC_ROUTES: [&str; 4] = [
    "/api/v1/mgmt/auth/login",
    "/api/v1/mgmt/auth/mfa/verify",
    "/api/v1/mgmt/auth/mfa/required-setup",
    "/api/v1/mgmt/auth/mfa/required-enable",
];
/// 只接受刷新令牌的接口。
pub const REFRESH_ROUTE: &str = "/api/v1/mgmt/auth/refresh";
/// 只接受强制改密临时令牌（`Authorization: Bearer`）的接口。
pub const PASSWORD_RESET_ROUTE: &str = "/api/v1/mgmt/auth/first-login-change-password";

/// 路由对令牌的要求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteAuth {
    Public,
    Requires(TokenKind),
}

/// 按路由模板（`MatchedPath`，完整路径）精确判定所需令牌类型；
/// 未知/缺失路径一律要求访问令牌（默认拒绝其它类型）。
#[must_use]
pub fn route_auth(matched_path: Option<&str>) -> RouteAuth {
    match matched_path {
        Some(path) if PUBLIC_ROUTES.contains(&path) => RouteAuth::Public,
        Some(REFRESH_ROUTE) => RouteAuth::Requires(TokenKind::Refresh),
        Some(PASSWORD_RESET_ROUTE) => RouteAuth::Requires(TokenKind::PasswordReset),
        _ => RouteAuth::Requires(TokenKind::Access),
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let token = headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")?
        .trim();
    (!token.is_empty()).then(|| token.to_string())
}

fn request_token(headers: &HeaderMap, kind: TokenKind) -> Option<String> {
    match kind {
        TokenKind::Refresh => extract_bearer_or_session_cookie(headers, true),
        TokenKind::Access => extract_bearer_or_session_cookie(headers, false),
        // 临时令牌只通过 Bearer 头传递，不读取会话 Cookie。
        TokenKind::MfaPending | TokenKind::MfaSetup | TokenKind::PasswordReset => {
            bearer_token(headers)
        }
    }
}

/// 校验令牌类型、签名、iss/aud/exp 与黑名单。
async fn authenticate(
    auth_state: &AdminAuthState,
    token: &str,
    kind: TokenKind,
) -> Result<AdminClaims, TokenRejection> {
    let claims = if kind == TokenKind::Refresh {
        admin_token::decode_refresh(token, &auth_state.jwt_secret)?
    } else {
        admin_token::decode_kind(token, &auth_state.jwt_secret, kind)?
    };
    // 签名已校验，身份可信：先记录操作人，后续拒绝也能归属到具体账号。
    audit_actor_from_claims(&claims);
    // 所有类型统一 fail-closed：黑名单查询失败时拒绝（503），不放行。
    admin_token::ensure_not_revoked(&auth_state.redis, &claims).await?;
    Ok(claims)
}

fn rejection_response(rejection: TokenRejection, token: &str, secret: &str) -> Response {
    admin_token::audit_rejection(rejection, token, secret);
    let status = if rejection == TokenRejection::CheckUnavailable {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::UNAUTHORIZED
    };
    let mut response = (
        status,
        axum::Json(serde_json::json!({
            "error": rejection.reason(),
            "message": rejection.message(),
        })),
    )
        .into_response();
    AuditErrorInfo::attach(&mut response, rejection.reason(), rejection.message());
    response
}

/// 管理端认证中间件：按完整路由模板正向放行。
///
/// - 公开接口（[`PUBLIC_ROUTES`]）直接放行；
/// - `/auth/refresh` 只接受刷新令牌；
/// - `/auth/first-login-change-password` 只接受 `password_reset` 临时令牌；
/// - 其余接口只接受 `access` 令牌，refresh、临时令牌、缺少类型的旧令牌一律 401。
pub async fn admin_auth_middleware(
    Extension(auth_state): Extension<Arc<AdminAuthState>>,
    mut request: Request,
    next: axum::middleware::Next,
) -> Response {
    let matched = request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str);
    let kind = match route_auth(matched) {
        RouteAuth::Public => return next.run(request).await,
        RouteAuth::Requires(kind) => kind,
    };
    let Some(token) = request_token(request.headers(), kind) else {
        crate::audit::set_failure_reason("missing_token");
        return unauthorized("missing_token", "缺少认证令牌");
    };
    match authenticate(&auth_state, &token, kind).await {
        Ok(claims) => {
            request.extensions_mut().insert(claims);
            next.run(request).await
        }
        Err(rejection) => rejection_response(rejection, &token, &auth_state.jwt_secret),
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

/// 把已签发的 access / refresh 写进响应的 `Set-Cookie`。调用方自己决定 HTTP 状态。
pub fn set_session_cookies(
    response: &mut Response,
    access_token: &str,
    refresh_token: &str,
    access_max_age: u64,
    refresh_max_age: u64,
) {
    let headers = response.headers_mut();
    append_set_cookie(
        headers,
        &format_set_cookie(ACCESS_COOKIE, access_token, access_max_age, false),
    );
    append_set_cookie(
        headers,
        &format_set_cookie(REFRESH_COOKIE, refresh_token, refresh_max_age, false),
    );
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
/// 超级管理员角色代码（显示名“超级管理员”，全系统唯一，不可新建）。
pub const ROLE_SUPER_ADMIN: &str = "R_ADMIN";
/// 管理员角色代码（显示名“管理员”，拥有全部业务权限；代码沿用历史值 `R_FINANCE`）。
pub const ROLE_ADMIN: &str = "R_FINANCE";
/// 只读用户角色代码。
pub const ROLE_VIEWER: &str = "R_VIEWER";

#[must_use]
pub fn can_manage_certificates(claims: &AdminClaims) -> bool {
    is_admin(claims)
}
#[must_use]
pub fn can_manage_tenants(claims: &AdminClaims) -> bool {
    is_business_admin(claims)
}
#[must_use]
pub fn can_access_settings(claims: &AdminClaims) -> bool {
    is_admin(claims)
}
#[must_use]
pub fn can_manage_admins(claims: &AdminClaims) -> bool {
    is_admin(claims)
}
/// 是否为超级管理员（`R_ADMIN`）。
#[must_use]
pub fn is_admin(claims: &AdminClaims) -> bool {
    claims.role == ROLE_SUPER_ADMIN
}
/// 是否拥有业务读写权限：超级管理员或管理员（`R_ADMIN` / `R_FINANCE`）。
#[must_use]
pub fn is_business_admin(claims: &AdminClaims) -> bool {
    matches!(claims.role.as_str(), ROLE_SUPER_ADMIN | ROLE_ADMIN)
}
#[must_use]
pub fn can_write_business(claims: &AdminClaims) -> bool {
    is_business_admin(claims)
}
#[must_use]
pub fn can_access_control_panel(claims: &AdminClaims) -> bool {
    is_admin(claims)
}
#[must_use]
pub fn can_assign_role(claims: &AdminClaims, target_role: &str) -> bool {
    get_role_level(&claims.role) >= get_role_level(target_role)
}
#[must_use]
pub fn get_role_level(role: &str) -> u8 {
    match role {
        ROLE_SUPER_ADMIN => 3,
        ROLE_ADMIN => 2,
        ROLE_VIEWER => 1,
        _ => 0,
    }
}
#[must_use]
pub fn is_read_only(claims: &AdminClaims) -> bool {
    claims.role == ROLE_VIEWER
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

#[cfg(test)]
mod route_auth_tests {
    use super::*;
    use crate::audit::route_map::{API_PREFIX, ROUTES};

    #[test]
    fn public_routes_are_exact_full_paths() {
        for path in PUBLIC_ROUTES {
            assert_eq!(route_auth(Some(path)), RouteAuth::Public);
        }
        // 不再接受后缀匹配或相对路径。
        for path in [
            "/auth/login",
            "/api/v1/mgmt/admin-users/auth/login",
            "/api/v1/mgmt/hypothetical/{a}/{b}",
            "/api/v1/mgmt/auth/login/",
            "/api/v1/mgmt//auth/login",
            "/other/api/v1/mgmt/auth/login",
            "/api/v1/mgmt/auth/mfa/verify/extra",
        ] {
            assert_eq!(
                route_auth(Some(path)),
                RouteAuth::Requires(TokenKind::Access),
                "{path}"
            );
        }
    }

    #[test]
    fn mfa_setup_requires_access_token() {
        assert_eq!(
            route_auth(Some("/api/v1/mgmt/auth/mfa/setup")),
            RouteAuth::Requires(TokenKind::Access)
        );
    }

    #[test]
    fn special_routes_require_their_own_token_type() {
        assert_eq!(
            route_auth(Some(REFRESH_ROUTE)),
            RouteAuth::Requires(TokenKind::Refresh)
        );
        assert_eq!(
            route_auth(Some(PASSWORD_RESET_ROUTE)),
            RouteAuth::Requires(TokenKind::PasswordReset)
        );
    }

    #[test]
    fn missing_matched_path_defaults_to_access() {
        assert_eq!(route_auth(None), RouteAuth::Requires(TokenKind::Access));
    }

    #[test]
    fn every_known_route_except_auth_entrypoints_requires_access() {
        let special = [
            "/auth/login",
            "/auth/mfa/verify",
            "/auth/mfa/required-setup",
            "/auth/mfa/required-enable",
            "/auth/refresh",
            "/auth/first-login-change-password",
        ];
        for route in ROUTES {
            let full = format!("{API_PREFIX}{}", route.path);
            if special.contains(&route.path) {
                assert_ne!(
                    route_auth(Some(&full)),
                    RouteAuth::Requires(TokenKind::Access),
                    "{full}"
                );
            } else {
                assert_eq!(
                    route_auth(Some(&full)),
                    RouteAuth::Requires(TokenKind::Access),
                    "{full}"
                );
            }
        }
    }

    #[test]
    fn temp_tokens_are_only_read_from_bearer_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("mazbri_at=cookie-value"),
        );
        assert_eq!(request_token(&headers, TokenKind::PasswordReset), None);
        assert_eq!(
            request_token(&headers, TokenKind::Access).as_deref(),
            Some("cookie-value")
        );
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer header-value"),
        );
        assert_eq!(
            request_token(&headers, TokenKind::PasswordReset).as_deref(),
            Some("header-value")
        );
    }
}

#[cfg(test)]
mod role_helper_tests {
    use super::*;

    fn claims(role: &str) -> AdminClaims {
        AdminClaims {
            iss: "mazbri-mgmt".to_string(),
            aud: None,
            sub: "tester".to_string(),
            user_id: uuid::Uuid::nil().to_string(),
            role: role.to_string(),
            iat: 0,
            exp: 0,
            jti: None,
            token_type: None,
            sid: uuid::Uuid::nil().to_string(),
            sxp: None,
        }
    }

    #[test]
    fn super_admin_only_helpers() {
        assert!(is_admin(&claims(ROLE_SUPER_ADMIN)));
        for role in [ROLE_ADMIN, ROLE_VIEWER, "R_OPERATOR", ""] {
            let c = claims(role);
            assert!(!is_admin(&c), "{role}");
            assert!(!can_manage_admins(&c), "{role}");
            assert!(!can_manage_certificates(&c), "{role}");
            assert!(!can_access_settings(&c), "{role}");
            assert!(!can_access_control_panel(&c), "{role}");
        }
    }

    #[test]
    fn business_helpers_cover_super_admin_and_admin_only() {
        for role in [ROLE_SUPER_ADMIN, ROLE_ADMIN] {
            let c = claims(role);
            assert!(is_business_admin(&c), "{role}");
            assert!(can_manage_tenants(&c), "{role}");
            assert!(can_write_business(&c), "{role}");
        }
        // 历史角色 R_OPERATOR 已移除，不再拥有任何业务权限。
        for role in [ROLE_VIEWER, "R_OPERATOR", ""] {
            let c = claims(role);
            assert!(!is_business_admin(&c), "{role}");
            assert!(!can_manage_tenants(&c), "{role}");
            assert!(!can_write_business(&c), "{role}");
        }
        assert!(is_read_only(&claims(ROLE_VIEWER)));
    }

    #[test]
    fn role_levels_ignore_legacy_operator() {
        assert_eq!(get_role_level(ROLE_SUPER_ADMIN), 3);
        assert_eq!(get_role_level(ROLE_ADMIN), 2);
        assert_eq!(get_role_level(ROLE_VIEWER), 1);
        assert_eq!(get_role_level("R_OPERATOR"), 0);
        assert!(can_assign_role(&claims(ROLE_SUPER_ADMIN), ROLE_ADMIN));
        assert!(!can_assign_role(&claims(ROLE_ADMIN), ROLE_SUPER_ADMIN));
        assert!(!can_assign_role(&claims(ROLE_VIEWER), ROLE_ADMIN));
    }
}
