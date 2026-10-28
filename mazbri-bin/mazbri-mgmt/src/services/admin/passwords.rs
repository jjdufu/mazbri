use super::auth_flow::generate_admin_jwt;
use crate::error::ApiError;
use crate::handlers::types::{AdminUser, ChangePasswordRequest, FirstLoginChangePasswordRequest};
use crate::middleware::admin_token::{self, SessionBinding};
use crate::middleware::auth::AdminClaims;
use crate::repositories::admin as repo;
use crate::services::{MfaSettingsCache, PasswordPolicyCache};
use chrono::Utc;
use mazbri_core::num::SaturatingFrom;
use mazbri_core::redis::RedisClient;
use sqlx::PgPool;
use tracing::{error, info, warn};
/// 校验首次登录时可选的新用户名；返回需写入的用户名（去除首尾空白）。
async fn validate_new_username<'a>(
    db: &PgPool,
    current_user_id: uuid::Uuid,
    role: &str,
    new_username: Option<&'a str>,
) -> Result<Option<&'a str>, ApiError> {
    let Some(new_uname) = new_username else {
        return Ok(None);
    };
    let trimmed = new_uname.trim();
    if trimmed.is_empty() {
        return Err(ApiError::validation("新用户名不能为空".to_string()));
    }
    if role != "R_ADMIN" {
        return Err(ApiError::validation("只有管理员能修改用户名".to_string()));
    }
    if trimmed.len() < 3 || trimmed.len() > 50 {
        return Err(ApiError::validation(
            "用户名长度必须在 3 到 50 个字符之间".to_string(),
        ));
    }
    let existing_user = repo::get_admin_user_by_username(db, trimmed)
        .await
        .map_err(|e| {
            error!("Failed to query user by username: {}", e);
            ApiError::internal("查询用户失败".to_string())
        })?;
    if existing_user.is_some_and(|existing| existing.id != current_user_id) {
        return Err(ApiError::validation("用户名已被占用".to_string()));
    }
    Ok(Some(trimmed))
}

/// 强制改密（首次登录）参数。
pub struct FirstLoginChangePasswordParams<'a> {
    pub db: &'a PgPool,
    pub redis: &'a RedisClient,
    pub jwt_secret: &'a str,
    pub password_policy_cache: &'a PasswordPolicyCache,
    pub mfa_settings_cache: &'a MfaSettingsCache,
    pub jwt_expiration_secs: u64,
    pub refresh_token_expiration_secs: u64,
    /// 中间件已按 `password_reset` 类型校验过的临时令牌声明。
    pub claims: &'a AdminClaims,
    pub user_id: uuid::Uuid,
    pub req: &'a FirstLoginChangePasswordRequest,
}

/// 强制改密成功后：若账号需要 MFA，返回 MFA 阶段响应（新的一次性临时令牌），
/// 否则签发正式访问令牌。改密本身不能跳过 MFA。
async fn post_password_change_stage(
    params: &FirstLoginChangePasswordParams<'_>,
    final_username: &str,
) -> Result<Option<serde_json::Value>, ApiError> {
    let user_row = repo::get_admin_user_by_username(params.db, final_username)
        .await
        .map_err(|e| {
            error!("Failed to reload user after password change: {}", e);
            ApiError::internal("内部服务器错误".to_string())
        })?
        .ok_or_else(|| ApiError::internal("内部服务器错误".to_string()))?;
    let mfa_settings = params.mfa_settings_cache.get().await;
    let binding = SessionBinding::stage(params.claims.sid.clone());
    let stage =
        super::auth_flow::determine_mfa(&user_row, &mfa_settings, &binding, params.jwt_secret)?;
    Ok(stage.map(|mut value| {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("password_changed".to_string(), serde_json::json!(true));
        }
        value
    }))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn first_login_change_password(
    params: FirstLoginChangePasswordParams<'_>,
) -> Result<serde_json::Value, ApiError> {
    let db = params.db;
    let user_id = params.user_id;
    let req = params.req;
    let password_policy_cache = params.password_policy_cache;
    let policy = password_policy_cache.get().await;
    let validation = policy.validate(&req.new_password);
    if !validation.is_valid {
        let error_message = validation
            .errors
            .first()
            .cloned()
            .unwrap_or_else(|| "密码不符合要求".to_string());
        return Err(ApiError::validation(error_message));
    }
    let user_row = match repo::get_admin_user_by_id(db, user_id).await {
        Ok(row) => row,
        Err(e) => {
            error!("Database error during password change: {}", e);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            return Err(ApiError::internal("内部服务器错误".to_string()));
        }
    };
    let Some(user_row) = user_row else {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        return Err(ApiError::validation("用户名或密码无效".to_string()));
    };
    if !user_row.is_active {
        return Err(ApiError::Forbidden);
    }
    if !user_row.force_password_reset {
        return Err(ApiError::validation(
            "不需要修改密码。请使用正常登录".to_string(),
        ));
    }
    let Some(admin_password_hash) = user_row.password_hash else {
        error!(
            "首次登录修改密码时用户 {} 在数据库中没有 password_hash",
            user_row.username
        );
        return Err(ApiError::internal("服务器配置错误".to_string()));
    };
    let password_valid = mazbri_auth::constant_time_argon2_verify(
        req.current_password.as_bytes(),
        &admin_password_hash,
    )
    .is_ok();
    if !password_valid {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        return Err(ApiError::validation("用户名或密码无效".to_string()));
    }
    let passed_username =
        validate_new_username(db, user_row.id, &user_row.role, req.new_username.as_deref()).await?;
    let final_username = passed_username.map_or_else(|| user_row.username.clone(), str::to_string);

    let new_password_hash = hash_new_password(&req.new_password)?;
    if let Some(reuse_error) = policy
        .check_password_reuse(db, user_id, &new_password_hash)
        .await
    {
        return Err(ApiError::validation(reuse_error));
    }
    // 一次性：在修改密码前原子消费临时令牌，重放/并发请求在此被拒绝。
    if let Err(rejection) = admin_token::consume_temporary(params.redis, params.claims).await {
        crate::audit::set_failure_reason(rejection.reason());
        crate::audit::mark_security_event();
        return Err(ApiError::Unauthorized);
    }
    repo::update_password(db, user_id, passed_username, &new_password_hash)
        .await
        .map_err(|e| {
            error!("Failed to update password: {}", e);
            ApiError::internal("更新密码失败".to_string())
        })?;
    let user = AdminUser {
        id: user_id.to_string(),
        username: final_username,
        role: user_row.role.clone(),
        created_at: user_row.created_at.to_rfc3339(),
    };
    if let Err(e) = policy
        .record_password_history(db, user_id, &new_password_hash)
        .await
    {
        warn!("Failed to record password history: {}", e);
    }
    info!("首次登录修改密码成功: {}", user_row.username);
    finish_first_login(&params, user).await
}

/// 改密完成后的下一阶段：需要 MFA 时进入 MFA 阶段，否则签发正式访问令牌。
async fn finish_first_login(
    params: &FirstLoginChangePasswordParams<'_>,
    user: AdminUser,
) -> Result<serde_json::Value, ApiError> {
    if let Some(stage) = post_password_change_stage(params, &user.username).await? {
        return Ok(stage);
    }
    let issued_at = Utc::now().timestamp();
    let binding = SessionBinding {
        sid: params.claims.sid.clone(),
        sxp: Some(
            issued_at
                .saturating_add(i64::saturating_from(params.refresh_token_expiration_secs).max(1)),
        ),
    };
    let access_ttl = super::auth_flow::resolve_access_token_expiration_secs(
        params.db,
        params.jwt_expiration_secs,
    )
    .await;
    let token = generate_admin_jwt(
        &user.username,
        &user.id,
        &user.role,
        &binding,
        params.jwt_secret,
        i64::saturating_from(access_ttl),
        issued_at,
    )
    .map_err(|e| {
        error!("Failed to generate admin JWT: {}", e);
        ApiError::internal("生成认证令牌失败".to_string())
    })?;
    let refresh = super::auth_flow::issue_refresh_token(
        &user.username,
        &user.id,
        &user.role,
        &binding,
        params.jwt_secret,
        issued_at,
    )
    .map_err(|e| {
        error!("Failed to generate refresh JWT: {}", e);
        ApiError::internal("生成刷新令牌失败".to_string())
    })?;
    Ok(serde_json::json!(
        { "user" : user, "token" : token.token, "refresh_token" : refresh.token,
        "expires_in" : token.exp.saturating_sub(token.iat).max(1),
        "refresh_max_age" : refresh.exp.saturating_sub(refresh.iat).max(1),
        "message" : "密码修改成功" }
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn change_password(
    db: &PgPool,
    password_policy_cache: &PasswordPolicyCache,
    user_id: uuid::Uuid,
    req: &ChangePasswordRequest,
) -> Result<serde_json::Value, ApiError> {
    if req.new_password != req.confirm_password {
        return Err(ApiError::validation("新密码与确认密码不匹配".to_string()));
    }
    let policy = password_policy_cache.get().await;
    let validation = policy.validate(&req.new_password);
    if !validation.is_valid {
        let error_message = validation
            .errors
            .first()
            .cloned()
            .unwrap_or_else(|| "密码不符合要求".to_string());
        return Err(ApiError::validation(error_message));
    }
    let new_password_hash = hash_new_password(&req.new_password)?;
    if let Some(reuse_error) = policy
        .check_password_reuse(db, user_id, &new_password_hash)
        .await
    {
        return Err(ApiError::validation(reuse_error));
    }
    let user_row = match repo::get_admin_user_basic_by_id(db, user_id).await {
        Ok(Some(row)) => row,
        Ok(None) => {
            return Err(ApiError::not_found("user".to_string(), String::new()));
        }
        Err(e) => {
            error!("Database error during password change: {}", e);
            return Err(ApiError::internal("查询用户失败".to_string()));
        }
    };
    let Some(admin_password_hash) = user_row.password_hash else {
        error!(
            "No password_hash in database for user: {}",
            user_row.username
        );
        return Err(ApiError::internal("服务器配置错误".to_string()));
    };
    let password_valid = mazbri_auth::constant_time_argon2_verify(
        req.current_password.as_bytes(),
        &admin_password_hash,
    )
    .is_ok();
    if !password_valid {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        return Err(ApiError::validation("当前密码不正确".to_string()));
    }
    repo::update_password(db, user_id, None, &new_password_hash)
        .await
        .map_err(|e| {
            error!("Failed to update password: {}", e);
            ApiError::internal("更新密码失败".to_string())
        })?;
    if let Err(e) = policy
        .record_password_history(db, user_id, &new_password_hash)
        .await
    {
        warn!("Failed to record password history: {}", e);
    }
    info!(
        "Password changed successfully for user: {}",
        user_row.username
    );
    Ok(serde_json::json!({ "message" : "密码修改成功" }))
}
fn hash_new_password(password: &str) -> Result<String, ApiError> {
    use argon2::{
        Argon2, PasswordHasher,
        password_hash::{SaltString, rand_core::OsRng},
    };
    let salt = SaltString::generate(&mut OsRng);
    let params = mazbri_auth::argon2_params().map_err(|e| {
        error!("Failed to create Argon2 params: {}", e);
        ApiError::internal("Argon2 参数初始化失败".to_string())
    })?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| {
            error!("Failed to hash password: {}", e);
            ApiError::internal("处理新密码失败".to_string())
        })
        .map(|hash| hash.to_string())
}
