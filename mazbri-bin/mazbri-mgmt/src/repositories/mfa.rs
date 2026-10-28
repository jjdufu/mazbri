use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MfaSetupState {
    pub mfa_enabled: bool,
    pub mfa_secret: Option<String>,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MfaDisableState {
    pub mfa_enabled: bool,
    pub mfa_required: bool,
    pub mfa_secret: Option<String>,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MfaRequiredSetupState {
    pub mfa_enabled: bool,
    pub mfa_required: bool,
    pub mfa_secret: Option<String>,
    pub role: String,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MfaRequiredEnableState {
    pub mfa_enabled: bool,
    pub mfa_required: bool,
    pub mfa_secret: Option<String>,
    pub role: String,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MfaVerifyState {
    pub mfa_enabled: bool,
    pub mfa_secret: Option<String>,
    pub mfa_backup_codes: Option<serde_json::Value>,
    pub mfa_backup_codes_version: i64,
    pub mfa_attempts: i32,
    pub mfa_locked_until: Option<DateTime<Utc>>,
    pub auth_epoch: i64,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MfaStatusRow {
    pub mfa_enabled: bool,
    pub mfa_required: bool,
    pub mfa_changed_at: Option<DateTime<Utc>>,
    pub mfa_backup_codes: Option<serde_json::Value>,
}
#[derive(Debug, sqlx::FromRow)]
pub struct MfaFailureUpdate {
    pub mfa_attempts: i32,
    pub mfa_locked_until: Option<DateTime<Utc>>,
    pub auth_epoch: i64,
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_mfa_setup_state(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<MfaSetupState>, sqlx::Error> {
    sqlx::query_as::<_, MfaSetupState>(
        r"
        SELECT mfa_enabled, mfa_secret
        FROM admin_users
        WHERE id = $1
    ",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// # Errors
///
/// 数据库更新失败时返回错误。
pub async fn store_mfa_secret(
    pool: &PgPool,
    user_id: uuid::Uuid,
    encrypted_secret: &str,
    hashed_codes: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        UPDATE admin_users
        SET mfa_secret = $1,
            mfa_backup_codes = $2,
            mfa_backup_codes_version = mfa_backup_codes_version + 1,
            updated_at = NOW()
        WHERE id = $3
    ",
    )
    .bind(encrypted_secret)
    .bind(serde_json::json!(hashed_codes))
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 步长已经推进之后写启用。不再比较步长。已启用或行不存在时返回 `Ok(None)`。
///
/// # Errors
///
/// 数据库更新失败时返回错误。
pub async fn mark_mfa_enabled(
    pool: &PgPool,
    user_id: uuid::Uuid,
    changed_at: DateTime<Utc>,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        r"
        UPDATE admin_users
        SET mfa_enabled = true,
            mfa_changed_at = $2,
            mfa_attempts = 0,
            mfa_locked_until = NULL,
            auth_epoch = auth_epoch + 1,
            updated_at = NOW()
        WHERE id = $1
          AND mfa_enabled = false
        RETURNING auth_epoch
        ",
    )
    .bind(user_id)
    .bind(changed_at)
    .fetch_optional(pool)
    .await
}

/// 启用 MFA。步长不晚于上次时返回 `Ok(None)`；成功返回新的 `auth_epoch`。
///
/// # Errors
///
/// 数据库更新失败时返回错误。
pub async fn enable_mfa(
    pool: &PgPool,
    user_id: uuid::Uuid,
    step_at: i64,
    changed_at: DateTime<Utc>,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        r"
        UPDATE admin_users
        SET mfa_enabled = true,
            mfa_changed_at = $2,
            mfa_totp_last_step_at = $3,
            mfa_attempts = 0,
            mfa_locked_until = NULL,
            auth_epoch = auth_epoch + 1,
            updated_at = NOW()
        WHERE id = $1
          AND (mfa_totp_last_step_at IS NULL OR mfa_totp_last_step_at < $3)
        RETURNING auth_epoch
        ",
    )
    .bind(user_id)
    .bind(changed_at)
    .bind(step_at)
    .fetch_optional(pool)
    .await
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_mfa_disable_state(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<MfaDisableState>, sqlx::Error> {
    sqlx::query_as::<_, MfaDisableState>(
        r"
        SELECT mfa_enabled, mfa_required, mfa_secret
        FROM admin_users
        WHERE id = $1
    ",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// 关闭 MFA。`changed_at` 必须等于本次新令牌的 `iat`。
///
/// # Errors
///
/// 数据库更新失败时返回错误。
pub async fn disable_mfa(
    pool: &PgPool,
    user_id: uuid::Uuid,
    changed_at: DateTime<Utc>,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        r"
        UPDATE admin_users
        SET mfa_enabled = false,
            mfa_secret = NULL,
            mfa_backup_codes = NULL,
            mfa_totp_last_step_at = NULL,
            mfa_changed_at = $2,
            mfa_backup_codes_version = mfa_backup_codes_version + 1,
            auth_epoch = auth_epoch + 1,
            mfa_attempts = 0,
            mfa_locked_until = NULL,
            updated_at = NOW()
        WHERE id = $1
        RETURNING auth_epoch
        ",
    )
    .bind(user_id)
    .bind(changed_at)
    .fetch_optional(pool)
    .await
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_mfa_verify_state(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<MfaVerifyState>, sqlx::Error> {
    sqlx::query_as::<_, MfaVerifyState>(
        r"
        SELECT mfa_enabled, mfa_secret, mfa_backup_codes, mfa_backup_codes_version,
               mfa_attempts, mfa_locked_until, auth_epoch, created_at
        FROM admin_users
        WHERE id = $1
    ",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// 原子增加失败次数。锁已过期时从 0 起算；触发锁定时同一条语句递增 `auth_epoch`。
///
/// 调用方必须先确认当前未锁定。
///
/// # Errors
///
/// 数据库更新失败时返回错误。
pub async fn record_mfa_failure(
    pool: &PgPool,
    user_id: uuid::Uuid,
    max_attempts: i32,
    lockout_secs: i64,
) -> Result<Option<MfaFailureUpdate>, sqlx::Error> {
    sqlx::query_as(
        r"
        WITH base AS (
            SELECT id,
                   CASE
                       WHEN mfa_locked_until IS NOT NULL AND mfa_locked_until <= NOW() THEN 0
                       ELSE mfa_attempts
                   END AS n
            FROM admin_users
            WHERE id = $1
        )
        UPDATE admin_users AS u
        SET mfa_attempts = base.n + 1,
            mfa_locked_until = CASE
                WHEN base.n + 1 >= $2 THEN NOW() + ($3 * interval '1 second')
                WHEN u.mfa_locked_until IS NOT NULL AND u.mfa_locked_until <= NOW() THEN NULL
                ELSE u.mfa_locked_until
            END,
            auth_epoch = CASE
                WHEN base.n + 1 >= $2 THEN u.auth_epoch + 1
                ELSE u.auth_epoch
            END,
            updated_at = NOW()
        FROM base
        WHERE u.id = base.id
        RETURNING u.mfa_attempts, u.mfa_locked_until, u.auth_epoch
        ",
    )
    .bind(user_id)
    .bind(max_attempts)
    .bind(lockout_secs)
    .fetch_optional(pool)
    .await
}

/// 条件推进已接受的 TOTP 步长。返回 false 表示重放。
///
/// # Errors
///
/// 数据库更新失败时返回错误。
pub async fn advance_totp_step(
    pool: &PgPool,
    user_id: uuid::Uuid,
    step_at: i64,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        r"
        UPDATE admin_users
        SET mfa_totp_last_step_at = $2,
            updated_at = NOW()
        WHERE id = $1
          AND (mfa_totp_last_step_at IS NULL OR mfa_totp_last_step_at < $2)
        ",
    )
    .bind(user_id)
    .bind(step_at)
    .execute(pool)
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// 按版本列替换备用码数组。`Ok(None)` 表示版本已被并发请求推进。
///
/// # Errors
///
/// 数据库更新失败时返回错误。
pub async fn cas_replace_backup_codes(
    pool: &PgPool,
    user_id: uuid::Uuid,
    seen_version: i64,
    new_codes: &serde_json::Value,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        r"
        UPDATE admin_users
        SET mfa_backup_codes = $3,
            mfa_backup_codes_version = mfa_backup_codes_version + 1,
            updated_at = NOW()
        WHERE id = $1 AND mfa_backup_codes_version = $2
        RETURNING mfa_backup_codes_version
        ",
    )
    .bind(user_id)
    .bind(seen_version)
    .bind(new_codes)
    .fetch_optional(pool)
    .await
}

/// 只清零失败计数与锁定，不写 `mfa_changed_at`。
///
/// # Errors
///
/// 数据库更新失败时返回错误。
pub async fn reset_mfa_verify_state(pool: &PgPool, user_id: uuid::Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        UPDATE admin_users
        SET mfa_attempts = 0,
            mfa_locked_until = NULL,
            updated_at = NOW()
        WHERE id = $1
        ",
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_mfa_status(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<MfaStatusRow>, sqlx::Error> {
    sqlx::query_as::<_, MfaStatusRow>(
        r"
        SELECT mfa_enabled, mfa_required, mfa_changed_at, mfa_backup_codes
        FROM admin_users
        WHERE id = $1
    ",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn is_mfa_enabled(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar::<_, bool>("SELECT mfa_enabled FROM admin_users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

/// # Errors
///
/// 数据库更新失败时返回错误。
pub async fn update_backup_codes(
    pool: &PgPool,
    user_id: uuid::Uuid,
    hashed_codes: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        UPDATE admin_users
        SET mfa_backup_codes = $1,
            mfa_backup_codes_version = mfa_backup_codes_version + 1,
            updated_at = NOW()
        WHERE id = $2
        ",
    )
    .bind(serde_json::json!(hashed_codes))
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_mfa_required_setup_state(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<MfaRequiredSetupState>, sqlx::Error> {
    sqlx::query_as::<_, MfaRequiredSetupState>(
        r"
        SELECT mfa_enabled, mfa_required, mfa_secret, role
        FROM admin_users
        WHERE id = $1
    ",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_mfa_required_enable_state(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<MfaRequiredEnableState>, sqlx::Error> {
    sqlx::query_as::<_, MfaRequiredEnableState>(
        r"
        SELECT mfa_enabled, mfa_required, mfa_secret, role, created_at
        FROM admin_users
        WHERE id = $1
    ",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}
