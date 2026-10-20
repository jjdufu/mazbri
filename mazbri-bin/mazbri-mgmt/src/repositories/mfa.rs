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
    pub password_hash: Option<String>,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MfaVerifyState {
    pub mfa_enabled: bool,
    pub mfa_secret: Option<String>,
    pub mfa_backup_codes: Option<serde_json::Value>,
    pub mfa_attempts: i32,
    pub mfa_locked_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MfaStatusRow {
    pub mfa_enabled: bool,
    pub mfa_verified_at: Option<DateTime<Utc>>,
    pub mfa_backup_codes: Option<serde_json::Value>,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MfaRequiredSetupState {
    pub mfa_enabled: bool,
    pub mfa_required: bool,
    pub mfa_secret: Option<String>,
    pub role: String,
    pub mfa_enabled_globally: bool,
    pub mfa_required_for_roles: Option<Vec<String>>,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MfaRequiredEnableState {
    pub mfa_enabled: bool,
    pub mfa_required: bool,
    pub mfa_secret: Option<String>,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub mfa_enabled_globally: bool,
    pub mfa_required_for_roles: Option<Vec<String>>,
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
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
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn store_mfa_secret(
    pool: &PgPool,
    user_id: uuid::Uuid,
    encrypted_secret: &str,
    hashed_codes: &[String],
) -> Result<(), sqlx::Error> {
    let query = r"
        UPDATE admin_users
        SET mfa_secret = $1,
            mfa_backup_codes = $2,
            updated_at = NOW()
        WHERE id = $3
    ";
    sqlx::query(query)
        .bind(encrypted_secret)
        .bind(serde_json::json!(hashed_codes))
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn enable_mfa(pool: &PgPool, user_id: uuid::Uuid) -> Result<(), sqlx::Error> {
    let query = r"
        UPDATE admin_users
        SET mfa_enabled = true,
            mfa_verified_at = NOW(),
            mfa_attempts = 0,
            updated_at = NOW()
        WHERE id = $1
    ";
    sqlx::query(query).bind(user_id).execute(pool).await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_mfa_disable_state(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<MfaDisableState>, sqlx::Error> {
    sqlx::query_as::<_, MfaDisableState>(
        r"
        SELECT mfa_enabled, mfa_secret, password_hash, mfa_required
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
pub async fn disable_mfa(pool: &PgPool, user_id: uuid::Uuid) -> Result<(), sqlx::Error> {
    let query = r"
        UPDATE admin_users
        SET mfa_enabled = false,
            mfa_secret = NULL,
            mfa_backup_codes = NULL,
            mfa_verified_at = NULL,
            mfa_attempts = 0,
            mfa_locked_until = NULL,
            updated_at = NOW()
        WHERE id = $1
    ";
    sqlx::query(query).bind(user_id).execute(pool).await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_mfa_verify_state(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<MfaVerifyState>, sqlx::Error> {
    sqlx::query_as::<_, MfaVerifyState>(
        r"
        SELECT mfa_enabled, mfa_secret, mfa_backup_codes, mfa_attempts, mfa_locked_until, created_at
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
pub async fn update_mfa_attempts(
    pool: &PgPool,
    user_id: uuid::Uuid,
    attempts: i32,
    locked_until: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    let query = r"
        UPDATE admin_users
        SET mfa_attempts = $1,
            mfa_locked_until = $2,
            updated_at = NOW()
        WHERE id = $3
    ";
    sqlx::query(query)
        .bind(attempts)
        .bind(locked_until)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn remove_backup_code(
    pool: &PgPool,
    user_id: uuid::Uuid,
    remaining_codes: serde_json::Value,
) -> Result<(), sqlx::Error> {
    let query = r"
        UPDATE admin_users
        SET mfa_backup_codes = $1,
            updated_at = NOW()
        WHERE id = $2
    ";
    sqlx::query(query)
        .bind(remaining_codes)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn reset_mfa_verify_state(pool: &PgPool, user_id: uuid::Uuid) -> Result<(), sqlx::Error> {
    let query = r"
        UPDATE admin_users
        SET mfa_attempts = 0,
            mfa_locked_until = NULL,
            mfa_verified_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
    ";
    sqlx::query(query).bind(user_id).execute(pool).await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_mfa_status(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<MfaStatusRow>, sqlx::Error> {
    sqlx::query_as::<_, MfaStatusRow>(
        r"
        SELECT mfa_enabled, mfa_verified_at, mfa_backup_codes
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
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_backup_codes(
    pool: &PgPool,
    user_id: uuid::Uuid,
    hashed_codes: &[String],
) -> Result<(), sqlx::Error> {
    let query = r"
        UPDATE admin_users
        SET mfa_backup_codes = $1,
            updated_at = NOW()
        WHERE id = $2
    ";
    sqlx::query(query)
        .bind(serde_json::json!(hashed_codes))
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_mfa_required_setup_state(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<MfaRequiredSetupState>, sqlx::Error> {
    sqlx::query_as::<_, MfaRequiredSetupState>(
        r"
        SELECT au.mfa_enabled, au.mfa_required, au.mfa_secret, au.role,
               ms.mfa_enabled_globally, ms.mfa_required_for_roles
        FROM admin_users au
        CROSS JOIN mfa_settings ms
        WHERE au.id = $1 AND ms.id = '00000000-0000-0000-0000-000000000102'
    ",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_mfa_required_enable_state(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<Option<MfaRequiredEnableState>, sqlx::Error> {
    sqlx::query_as::<_, MfaRequiredEnableState>(
        r"
        SELECT au.mfa_enabled, au.mfa_required, au.mfa_secret, au.role, au.created_at,
               ms.mfa_enabled_globally, ms.mfa_required_for_roles
        FROM admin_users au
        CROSS JOIN mfa_settings ms
        WHERE au.id = $1 AND ms.id = '00000000-0000-0000-0000-000000000102'
    ",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}
