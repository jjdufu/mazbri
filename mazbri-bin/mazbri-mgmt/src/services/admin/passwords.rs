use super::auth_flow::generate_admin_jwt;
use crate::error::ApiError;
use crate::handlers::types::{AdminUser, ChangePasswordRequest, FirstLoginChangePasswordRequest};
use crate::repositories::admin as repo;
use crate::services::PasswordPolicyCache;
use mazbri_core::num::SaturatingFrom;
use sqlx::PgPool;
use tracing::{error, info, warn};
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn first_login_change_password(
    db: &PgPool,
    jwt_secret: &str,
    password_policy_cache: &PasswordPolicyCache,
    jwt_expiration_secs: u64,
    user_id: uuid::Uuid,
    req: &FirstLoginChangePasswordRequest,
) -> Result<serde_json::Value, ApiError> {
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
    let mut final_username = user_row.username.clone();
    let mut passed_username = None;
    if let Some(ref new_uname) = req.new_username {
        let trimmed = new_uname.trim();
        if trimmed.is_empty() {
            return Err(ApiError::validation("新用户名不能为空".to_string()));
        }
        if user_row.role != "R_ADMIN" {
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
        if let Some(existing) = existing_user {
            if existing.id != user_row.id {
                return Err(ApiError::validation("用户名已被占用".to_string()));
            }
        }
        final_username = trimmed.to_string();
        passed_username = Some(trimmed);
    }

    let new_password_hash = hash_new_password(&req.new_password)?;
    if let Some(reuse_error) = policy
        .check_password_reuse(db, user_id, &new_password_hash)
        .await
    {
        return Err(ApiError::validation(reuse_error));
    }
    repo::update_password(db, user_id, passed_username, &new_password_hash)
        .await
        .map_err(|e| {
            error!("Failed to update password: {}", e);
            ApiError::internal("更新密码失败".to_string())
        })?;
    let token = generate_admin_jwt(
        &final_username,
        &user_id.to_string(),
        &user_row.role,
        jwt_secret,
        i64::saturating_from(jwt_expiration_secs),
    )
    .map_err(|e| {
        error!("Failed to generate admin JWT: {}", e);
        ApiError::internal("生成认证令牌失败".to_string())
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
    Ok(serde_json::json!(
        { "user" : user, "token" : token, "expires_in" : jwt_expiration_secs,
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
