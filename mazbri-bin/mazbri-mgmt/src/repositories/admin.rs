use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
/// 管理员 MFA 开关：是否已绑定、是否被要求绑定。
#[derive(Debug, Clone, Copy, serde::Serialize, sqlx::FromRow)]
pub struct AdminMfaFlags {
    pub mfa_enabled: bool,
    pub mfa_required: bool,
}
#[derive(Debug, sqlx::FromRow)]
pub struct AdminUserFullRow {
    pub id: uuid::Uuid,
    pub username: String,
    pub password_hash: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub password_changed_at: DateTime<Utc>,
    pub force_password_reset: bool,
    #[sqlx(flatten)]
    pub mfa: AdminMfaFlags,
    pub mfa_locked_until: Option<DateTime<Utc>>,
    pub login_attempts: i32,
    pub locked_until: Option<DateTime<Utc>>,
}
#[derive(Debug, sqlx::FromRow)]
pub struct AdminUserPasswordRow {
    pub id: uuid::Uuid,
    pub username: String,
    pub password_hash: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub force_password_reset: bool,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, sqlx::FromRow)]
pub struct AdminUserBasicRow {
    pub username: String,
    pub password_hash: Option<String>,
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_admin_user_by_username(
    pool: &PgPool,
    username: &str,
) -> Result<Option<AdminUserFullRow>, sqlx::Error> {
    sqlx::query_as::<_, AdminUserFullRow>(
        r"
        SELECT id, username, password_hash, role, is_active, created_at, password_changed_at,
               force_password_reset, mfa_enabled, mfa_locked_until, mfa_required,
               login_attempts, locked_until
        FROM admin_users
        WHERE username = $1
    ",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_admin_user_by_id(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<AdminUserPasswordRow>, sqlx::Error> {
    sqlx::query_as::<_, AdminUserPasswordRow>(
        r"
        SELECT id, username, password_hash, role, is_active, force_password_reset, created_at
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
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_admin_user_basic_by_id(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<AdminUserBasicRow>, sqlx::Error> {
    sqlx::query_as::<_, AdminUserBasicRow>(
        r"
        SELECT id, username, password_hash
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
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_login_failure(
    pool: &PgPool,
    user_id: uuid::Uuid,
    new_attempts: i32,
    locked_until: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    if let Some(lock_time) = locked_until {
        let query = r"
            UPDATE admin_users
            SET login_attempts = $1, locked_until = $2
            WHERE id = $3
        ";
        sqlx::query(query)
            .bind(new_attempts)
            .bind(lock_time)
            .bind(user_id)
            .execute(pool)
            .await?;
    } else {
        let query = r"
            UPDATE admin_users
            SET login_attempts = $1
            WHERE id = $2
        ";
        sqlx::query(query)
            .bind(new_attempts)
            .bind(user_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn reset_login_attempts(pool: &PgPool, user_id: uuid::Uuid) -> Result<(), sqlx::Error> {
    let query = r"
        UPDATE admin_users
        SET login_attempts = 0, locked_until = NULL
        WHERE id = $1
    ";
    sqlx::query(query).bind(user_id).execute(pool).await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_last_login(pool: &PgPool, user_id: uuid::Uuid) -> Result<(), sqlx::Error> {
    let query = r"
        UPDATE admin_users
        SET last_login_at = NOW(), updated_at = NOW()
        WHERE id = $1
    ";
    sqlx::query(query).bind(user_id).execute(pool).await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_password(
    pool: &PgPool,
    user_id: uuid::Uuid,
    new_username: Option<&str>,
    new_password_hash: &str,
) -> Result<(), sqlx::Error> {
    let query = r"
        UPDATE admin_users
        SET username = COALESCE($1, username),
            password_hash = $2,
            password_changed_at = NOW(),
            force_password_reset = false,
            updated_at = NOW()
        WHERE id = $3
    ";
    sqlx::query(query)
        .bind(new_username)
        .bind(new_password_hash)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_system_settings(
    pool: &PgPool,
) -> Result<std::collections::HashMap<String, serde_json::Value>, sqlx::Error> {
    let query = r"
        SELECT key, value, description, category, updated_at
        FROM system_settings
        ORDER BY category, key
    ";
    let rows = sqlx::query(query).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let key: String = row.get("key");
            let value: serde_json::Value = row.get("value");
            (key, value)
        })
        .collect())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_system_setting_value(
    pool: &PgPool,
    key: &str,
) -> Result<Option<serde_json::Value>, sqlx::Error> {
    sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT value FROM system_settings WHERE key = $1 LIMIT 1",
    )
    .bind(key)
    .fetch_optional(pool)
    .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn upsert_system_setting(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    key: &str,
    value: &serde_json::Value,
    category: &str,
) -> Result<(), sqlx::Error> {
    let query = r"
        INSERT INTO system_settings (key, value, description, category)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (key) DO UPDATE
        SET value = $2, updated_at = NOW()
    ";
    sqlx::query(query)
        .bind(key)
        .bind(value)
        .bind("")
        .bind(category)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_admin_user_id_by_username(pool: &PgPool) -> Result<uuid::Uuid, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM admin_users WHERE username = 'admin' LIMIT 1")
        .fetch_one(pool)
        .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_user_settings(
    pool: &PgPool,
) -> Result<std::collections::HashMap<String, serde_json::Value>, sqlx::Error> {
    let query = r"
        SELECT key, value
        FROM user_settings
        WHERE user_id = (SELECT id FROM admin_users WHERE username = 'admin' LIMIT 1)
    ";
    let rows = sqlx::query(query).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let key: String = row.get("key");
            let value: serde_json::Value = row.get("value");
            (key, value)
        })
        .collect())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn upsert_user_setting(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: uuid::Uuid,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let query = r"
        INSERT INTO user_settings (user_id, key, value)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, key) DO UPDATE
        SET value = $3, updated_at = NOW()
    ";
    sqlx::query(query)
        .bind(user_id)
        .bind(key)
        .bind(value)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
