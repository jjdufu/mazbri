use crate::audit::{self, AuditAction, AuditService, user_audit};
use crate::error::ApiError;
use crate::handlers::types::{AdminJwtClaims, AdminLoginRequest, AdminLogoutRequest, AdminUser};
use crate::middleware::auth::{AdminClaims, is_jti_revoked, revoke_jti};
use crate::repositories::admin as repo;
use crate::services::{MfaSettings, MfaSettingsCache, PasswordPolicy, PasswordPolicyCache};
use chrono::{Duration, Utc};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::redis::RedisClient;
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Resolve access-token TTL from `system_settings.session_timeout` (ms), clamped.
/// Falls back to service config when unset/invalid (H-09).
pub async fn resolve_access_token_expiration_secs(db: &PgPool, fallback_secs: u64) -> u64 {
    const MIN_SECS: u64 = 300; // 5 minutes
    const MAX_SECS: u64 = 86_400; // 24 hours
    match repo::get_system_setting_value(db, "session_timeout").await {
        Ok(Some(value)) => {
            let ms = value
                .as_i64()
                .or_else(|| value.as_u64().map(i64::saturating_from))
                .or_else(|| value.as_str().and_then(|s| s.parse().ok()));
            match ms {
                Some(ms) if ms > 0 => (u64::saturating_from(ms) / 1000).clamp(MIN_SECS, MAX_SECS),
                _ => fallback_secs.clamp(MIN_SECS, MAX_SECS),
            }
        }
        _ => fallback_secs.clamp(MIN_SECS, MAX_SECS),
    }
}

pub(crate) fn generate_admin_jwt(
    username: &str,
    user_id: &str,
    role: &str,
    jwt_secret: &str,
    expiration_secs: i64,
) -> Result<String, String> {
    let now = Utc::now().timestamp();
    let expiration = now + expiration_secs;
    let claims = AdminJwtClaims {
        iss: "mazbri-mgmt-api".to_string(),
        sub: username.to_string(),
        user_id: user_id.to_string(),
        role: role.to_string(),
        iat: now,
        exp: expiration,
        jti: Some(Uuid::new_v4().to_string()),
        token_type: None,
    };
    let encoding_key = jsonwebtoken::EncodingKey::from_secret(jwt_secret.as_ref());
    jsonwebtoken::encode(&jsonwebtoken::Header::default(), &claims, &encoding_key)
        .map_err(|e| format!("Failed to encode JWT: {e}"))
}
fn refresh_admin_jwt(
    username: &str,
    user_id: &str,
    role: &str,
    jwt_secret: &str,
    expiration_secs: i64,
) -> Result<String, String> {
    let now = Utc::now().timestamp();
    let expiration = now + expiration_secs;
    let claims = AdminJwtClaims {
        iss: "mazbri-mgmt-api".to_string(),
        sub: username.to_string(),
        user_id: user_id.to_string(),
        role: role.to_string(),
        iat: now,
        exp: expiration,
        jti: Some(Uuid::new_v4().to_string()),
        token_type: Some("refresh".to_string()),
    };
    let encoding_key = jsonwebtoken::EncodingKey::from_secret(jwt_secret.as_ref());
    jsonwebtoken::encode(&jsonwebtoken::Header::default(), &claims, &encoding_key)
        .map_err(|e| format!("Failed to encode refresh JWT: {e}"))
}
fn resolve_password_hash(user_row: &repo::AdminUserFullRow) -> Result<String, ApiError> {
    match user_row.password_hash.as_deref() {
        Some(hash) if !hash.is_empty() => Ok(hash.to_string()),
        _ => {
            error!(
                "No password_hash in database for user {}. Password verification failed. \
                 / 用户 {} 在数据库中没有 password_hash，密码验证失败。",
                user_row.username, user_row.username
            );
            audit::set_failure_reason("password_hash_missing");
            Err(ApiError::validation("密码验证失败".to_string()))
        }
    }
}
async fn handle_failed_login(
    db: &PgPool,
    user_id: uuid::Uuid,
    user_row: &repo::AdminUserFullRow,
    policy: &PasswordPolicy,
    username: &str,
) -> Result<serde_json::Value, ApiError> {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let new_attempts = user_row.login_attempts + 1;
    let should_lock = new_attempts >= policy.max_login_attempts;
    if should_lock {
        let lockout_duration = Duration::minutes(policy.lockout_duration_minutes.into());
        let locked_until_time = Utc::now() + lockout_duration;
        if let Err(e) =
            repo::update_login_failure(db, user_id, new_attempts, Some(locked_until_time)).await
        {
            error!(
                "Failed to update login failure count for user {}: {}. Lockout bypass possible!",
                username, e
            );
            audit::set_failure_reason("db_error");
            return Err(ApiError::internal("登录验证失败，请稍后重试".to_string()));
        }
        warn!(
            "User {} locked after {} failed login attempts",
            username, new_attempts
        );
        audit::set_failure_reason("lock_triggered");
        audit::mark_security_event();
        return Err(ApiError::validation(format!(
            "登录失败次数过多。账户已被锁定 {} 分钟。",
            policy.lockout_duration_minutes
        )));
    }
    if let Err(e) = repo::update_login_failure(db, user_id, new_attempts, None).await {
        error!(
            "Failed to update login failure count for user {}: {}",
            username, e
        );
    }
    let remaining_attempts = policy.max_login_attempts.saturating_sub(new_attempts);
    audit::set_failure_reason("invalid_password");
    Err(ApiError::validation(format!(
        "用户名或密码无效。剩余 {remaining_attempts} 次尝试。"
    )))
}
fn determine_mfa(
    user_row: &repo::AdminUserFullRow,
    username: &str,
    role: &str,
    user_id: uuid::Uuid,
    mfa_settings: &MfaSettings,
    jwt_secret: &str,
    jwt_expiration_secs: i64,
) -> Result<Option<serde_json::Value>, ApiError> {
    let role_string = role.to_string();
    let (mfa_required_by_global_settings, totp_digits) = if mfa_settings.mfa_enabled_globally {
        if mfa_settings.mfa_required_for_all {
            (true, mfa_settings.totp_digits)
        } else if !mfa_settings.mfa_required_for_roles.is_empty() {
            (
                mfa_settings.mfa_required_for_roles.contains(&role_string),
                mfa_settings.totp_digits,
            )
        } else {
            (false, mfa_settings.totp_digits)
        }
    } else {
        (false, mfa_settings.totp_digits)
    };
    if user_row.mfa.mfa_enabled || mfa_required_by_global_settings {
        if user_row.mfa.mfa_enabled {
            if let Some(mfa_lock_until) = user_row.mfa_locked_until
                && mfa_lock_until > Utc::now()
            {
                warn!("Login blocked: MFA locked for user: {}", username);
                audit::set_failure_reason("mfa_locked");
                return Err(ApiError::validation(
                    "由于失败次数过多，MFA 已锁定。请稍后再试。".to_string(),
                ));
            }
            let temp_token = generate_admin_jwt(
                username,
                &user_id.to_string(),
                &role_string,
                jwt_secret,
                jwt_expiration_secs,
            )
            .map_err(|e| {
                error!("Failed to generate temp MFA JWT: {}", e);
                ApiError::internal("生成认证令牌失败".to_string())
            })?;
            info!("Password verified, MFA required for user: {}", username);
            audit::add_detail("login_stage", serde_json::json!("mfa_required"));
            return Ok(Some(serde_json::json!(
                { "mfa_required" : true, "temp_token" : temp_token, "totp_digits"
                : totp_digits, "message" : "需要 MFA 验证" }
            )));
        }
        if mfa_required_by_global_settings && !user_row.mfa.mfa_enabled {
            let temp_token = generate_admin_jwt(
                username,
                &user_id.to_string(),
                &role_string,
                jwt_secret,
                jwt_expiration_secs,
            )
            .map_err(|e| {
                error!("Failed to generate temp MFA setup JWT: {}", e);
                ApiError::internal("生成认证令牌失败".to_string())
            })?;
            info!(
                "Password verified, MFA setup required for user: {} (role: {})",
                username, role
            );
            audit::add_detail("login_stage", serde_json::json!("mfa_setup_required"));
            return Ok(Some(serde_json::json!(
                { "mfa_setup_required" : true, "temp_token" : temp_token,
                "message" : "安全策略要求设置 MFA" }
            )));
        }
    }
    if user_row.mfa.mfa_required && !user_row.mfa.mfa_enabled {
        let temp_token = generate_admin_jwt(
            username,
            &user_id.to_string(),
            &role_string,
            jwt_secret,
            jwt_expiration_secs,
        )
        .map_err(|e| {
            error!("Failed to generate temp MFA setup JWT: {}", e);
            ApiError::internal("生成认证令牌失败".to_string())
        })?;
        info!(
            "Password verified, MFA setup required for user: {}",
            username
        );
        audit::add_detail("login_stage", serde_json::json!("mfa_setup_required"));
        return Ok(Some(serde_json::json!(
            { "mfa_setup_required" : true, "temp_token" : temp_token, "message" :
            "安全策略要求设置 MFA" }
        )));
    }
    Ok(None)
}
struct GenerateLoginResponseParams<'a> {
    db: &'a PgPool,
    username: &'a str,
    user_id: uuid::Uuid,
    role: &'a str,
    created_at: &'a str,
    jwt_secret: &'a str,
    jwt_expiration_secs: u64,
    refresh_token_expiration_secs: u64,
    audit: &'a AuditService,
    client_ip: Option<String>,
}

async fn generate_login_response(
    params: GenerateLoginResponseParams<'_>,
) -> Result<serde_json::Value, ApiError> {
    let token = generate_admin_jwt(
        params.username,
        &params.user_id.to_string(),
        params.role,
        params.jwt_secret,
        i64::saturating_from(params.jwt_expiration_secs),
    )
    .map_err(|e| {
        error!("Failed to generate admin JWT: {}", e);
        ApiError::internal("生成认证令牌失败".to_string())
    })?;
    let refresh_token = refresh_admin_jwt(
        params.username,
        &params.user_id.to_string(),
        params.role,
        params.jwt_secret,
        i64::saturating_from(params.refresh_token_expiration_secs),
    )
    .map_err(|e| {
        error!("Failed to generate refresh JWT: {}", e);
        ApiError::internal("生成刷新令牌失败".to_string())
    })?;
    let user = AdminUser {
        id: params.user_id.to_string(),
        username: params.username.to_string(),
        role: params.role.to_string(),
        created_at: params.created_at.to_string(),
    };
    if let Err(e) = repo::update_last_login(params.db, params.user_id).await {
        warn!("Failed to update last_login_at: {}", e);
    }
    info!(
        "Admin login successful: {} (role: {})",
        params.username, user.role
    );
    if let Err(e) = params
        .audit
        .log_critical(user_audit(
            params.user_id,
            AuditAction::UserLogin,
            Some(serde_json::json!({ "username" : params.username, "role" : user.role, })),
            params.client_ip,
        ))
        .await
    {
        error!(
            "Failed to write critical audit log for login success: {}",
            e
        );
    }
    Ok(serde_json::json!(
        { "user" : user, "token" : token, "expires_in" : params.jwt_expiration_secs,
        "refresh_token" : refresh_token }
    ))
}

/// 按用户名加载可登录账户（不存在/禁用/锁定时返回错误）。
async fn load_login_user(db: &PgPool, username: &str) -> Result<repo::AdminUserFullRow, ApiError> {
    let user_row = match repo::get_admin_user_by_username(db, username).await {
        Ok(row) => row,
        Err(e) => {
            error!("Database error during login: {}", e);
            audit::set_failure_reason("db_error");
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            return Err(ApiError::internal(
                "Database error during login".to_string(),
            ));
        }
    };
    let Some(user_row) = user_row else {
        // 不存在的账号名可能是误输入的密码，落库前脱敏。
        audit::mask_attempted_username();
        audit::set_failure_reason("user_not_found");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        return Err(ApiError::Unauthorized);
    };
    let username = &user_row.username;
    if !user_row.is_active {
        warn!("Login attempt for disabled user: {}", username);
        audit::set_failure_reason("account_disabled");
        return Err(ApiError::validation("账户已禁用".to_string()));
    }
    if let Some(lock_until) = user_row.locked_until
        && lock_until > Utc::now()
    {
        let remaining_seconds = (lock_until - Utc::now()).num_seconds();
        warn!(
            "Login attempt for locked user: {}, locked until: {}",
            username, lock_until
        );
        audit::set_failure_reason("account_locked");
        return Err(ApiError::validation(format!(
            "账户已被临时锁定。请在 {remaining_seconds} 秒后重试。"
        )));
    }
    Ok(user_row)
}

pub struct AdminLoginParams<'a> {
    pub db: &'a PgPool,
    pub jwt_secret: &'a str,
    pub jwt_expiration_secs: u64,
    pub refresh_token_expiration_secs: u64,
    pub password_policy_cache: &'a PasswordPolicyCache,
    pub mfa_settings_cache: &'a MfaSettingsCache,
    pub audit: &'a AuditService,
    pub req: &'a AdminLoginRequest,
    pub client_ip: Option<String>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn admin_login(params: AdminLoginParams<'_>) -> Result<serde_json::Value, ApiError> {
    let db = params.db;
    let jwt_secret = params.jwt_secret;
    // H-09: prefer system_settings.session_timeout over static config for access tokens.
    let jwt_expiration_secs =
        resolve_access_token_expiration_secs(db, params.jwt_expiration_secs).await;
    let refresh_token_expiration_secs = params.refresh_token_expiration_secs;
    let password_policy_cache = params.password_policy_cache;
    let mfa_settings_cache = params.mfa_settings_cache;
    let audit = params.audit;
    let req = params.req;
    let client_ip = params.client_ip;
    audit::set_attempted_username(&req.username);
    let user_row = load_login_user(db, &req.username).await?;
    let user_id = user_row.id;
    let username = &user_row.username;
    let role = &user_row.role;
    let admin_password_hash = resolve_password_hash(&user_row)?;
    let password_valid =
        mazbri_auth::constant_time_argon2_verify(req.password.as_bytes(), &admin_password_hash)
            .is_ok();
    if !password_valid {
        let policy = password_policy_cache.get().await;
        return handle_failed_login(db, user_id, &user_row, &policy, username).await;
    }
    // 密码已验证：身份确认，后续阶段（重置密码/MFA/成功）均归属该账号。
    audit::set_actor(Some(user_id), Some(username), Some(role));
    if user_row.force_password_reset {
        audit::add_detail("login_stage", serde_json::json!("password_reset_required"));
        let temp_token = generate_admin_jwt(
            username,
            &user_id.to_string(),
            role,
            jwt_secret,
            i64::saturating_from(jwt_expiration_secs),
        )
        .map_err(|e| {
            error!("Failed to generate temp password reset JWT: {}", e);
            ApiError::internal("生成认证令牌失败".to_string())
        })?;
        info!(
            "Password verified, password reset required for user: {}",
            username
        );
        return Ok(serde_json::json!(
            { "password_reset_required" : true, "username" : username, "role" : role, "temp_token" :
            temp_token, "message" : "需要重置密码。请修改您的密码。" }
        ));
    }
    let policy = password_policy_cache.get().await;
    let (is_expired, days_until_expiry, _should_warn) =
        policy.check_password_expiry(Some(user_row.password_changed_at));
    if is_expired {
        warn!(
            "Login blocked: password expired for user: {} (days until expiry: {})",
            username, days_until_expiry
        );
        audit::set_failure_reason("password_expired");
        return Err(ApiError::validation(
            "密码已过期。请联系管理员重置密码。".to_string(),
        ));
    }
    if (user_row.login_attempts > 0 || user_row.locked_until.is_some())
        && let Err(e) = repo::reset_login_attempts(db, user_id).await
    {
        error!(
            "Failed to reset login failure count for user {}: {}",
            username, e
        );
    }
    let mfa_settings = mfa_settings_cache.get().await;
    if let Some(mfa_response) = determine_mfa(
        &user_row,
        username,
        role,
        user_id,
        &mfa_settings,
        jwt_secret,
        i64::saturating_from(jwt_expiration_secs),
    )? {
        return Ok(mfa_response);
    }
    let created_at = user_row.created_at.to_rfc3339();
    generate_login_response(GenerateLoginResponseParams {
        db,
        username,
        user_id,
        role,
        created_at: &created_at,
        jwt_secret,
        jwt_expiration_secs,
        refresh_token_expiration_secs,
        audit,
        client_ip,
    })
    .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn admin_refresh_token(
    db: &PgPool,
    claims: &AdminClaims,
    jwt_secret: &str,
    jwt_expiration_secs_fallback: u64,
    redis: &RedisClient,
) -> Result<serde_json::Value, ApiError> {
    match &claims.token_type {
        Some(tt) if tt == "refresh" => {}
        _ => {
            return Err(ApiError::validation(
                "无效的令牌类型，需要 refresh token".to_string(),
            ));
        }
    }
    if let Some(ref jti) = claims.jti
        && is_jti_revoked(redis, jti).await
    {
        return Err(ApiError::Unauthorized);
    }
    let jwt_expiration_secs =
        resolve_access_token_expiration_secs(db, jwt_expiration_secs_fallback).await;
    let new_token = generate_admin_jwt(
        &claims.sub,
        &claims.user_id,
        &claims.role,
        jwt_secret,
        i64::saturating_from(jwt_expiration_secs),
    )
    .map_err(|e| {
        error!("Failed to generate access JWT during refresh: {}", e);
        ApiError::internal("生成认证令牌失败".to_string())
    })?;
    info!(
        "Token refreshed for user: {} (role: {})",
        claims.sub, claims.role
    );
    Ok(serde_json::json!({ "token" : new_token, "expires_in" : jwt_expiration_secs }))
}

/// Logout: revoke access jti (+ optional refresh) and audit (H-11).
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn admin_logout(
    claims: &AdminClaims,
    jwt_secret: &str,
    redis: &RedisClient,
    audit: &AuditService,
    req: &AdminLogoutRequest,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    let now = Utc::now().timestamp();
    if let Some(ref jti) = claims.jti {
        let ttl = (claims.exp - now).max(1);
        if let Err(e) = revoke_jti(redis, jti, ttl).await {
            error!("Failed to revoke access token jti: {}", e);
        }
    }
    if let Some(ref refresh_token) = req.refresh_token {
        let decoding_key = jsonwebtoken::DecodingKey::from_secret(jwt_secret.as_ref());
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.set_issuer(&["mazbri-mgmt-api"]);
        if let Ok(token_data) =
            jsonwebtoken::decode::<AdminClaims>(refresh_token, &decoding_key, &validation)
            && token_data.claims.token_type.as_deref() == Some("refresh")
            && let Some(ref jti) = token_data.claims.jti
        {
            let ttl = (token_data.claims.exp - now).max(1);
            if let Err(e) = revoke_jti(redis, jti, ttl).await {
                error!("Failed to revoke refresh token jti: {}", e);
            }
        }
    }
    let user_id = Uuid::parse_str(&claims.user_id).unwrap_or(Uuid::nil());
    if let Err(e) = audit
        .log_critical(user_audit(
            user_id,
            AuditAction::UserLogout,
            Some(serde_json::json!({ "username": claims.sub, "role": claims.role })),
            client_ip,
        ))
        .await
    {
        error!("Failed to write audit log for logout: {}", e);
    }
    info!("Admin logout: {} (role: {})", claims.sub, claims.role);
    Ok(serde_json::json!({ "message": "已退出登录" }))
}
