use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
/// 字母大小写要求。
#[derive(Debug, Clone, Copy, serde::Serialize, sqlx::FromRow)]
pub struct LetterCaseRules {
    pub require_uppercase: bool,
    pub require_lowercase: bool,
}
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct PasswordPolicyRow {
    pub min_length: i32,
    pub max_length: i32,
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub letter_case: LetterCaseRules,
    pub require_digit: bool,
    pub require_special_char: bool,
    pub special_chars: String,
    pub max_age_days: i32,
    pub warning_days: i32,
    pub prevent_reuse_count: i32,
    pub max_login_attempts: i32,
    pub lockout_duration_minutes: i32,
    pub updated_at: Option<DateTime<Utc>>,
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_password_policy(pool: &PgPool) -> Result<PasswordPolicyRow, sqlx::Error> {
    sqlx::query_as::<_, PasswordPolicyRow>(
        r"
        SELECT min_length, max_length, require_uppercase, require_lowercase, require_digit,
            require_special_char, special_chars, max_age_days, warning_days, prevent_reuse_count,
            max_login_attempts, lockout_duration_minutes, updated_at
        FROM password_policy WHERE id = '00000000-0000-0000-0000-000000000101'
    ",
    )
    .fetch_one(pool)
    .await
}
pub struct UpdatePasswordPolicyParams<'a> {
    pub min_length: Option<i32>,
    pub max_length: Option<i32>,
    pub require_uppercase: Option<bool>,
    pub require_lowercase: Option<bool>,
    pub require_digit: Option<bool>,
    pub require_special_char: Option<bool>,
    pub special_chars: Option<String>,
    pub max_age_days: Option<i32>,
    pub warning_days: Option<i32>,
    pub prevent_reuse_count: Option<i32>,
    pub max_login_attempts: Option<i32>,
    pub lockout_duration_minutes: Option<i32>,
    pub updated_by: &'a str,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_password_policy(
    pool: &PgPool,
    params: UpdatePasswordPolicyParams<'_>,
) -> Result<PasswordPolicyRow, sqlx::Error> {
    sqlx::query_as::<
        _,
        PasswordPolicyRow,
    >(
            r"
        UPDATE password_policy SET
            min_length = COALESCE($1, min_length), max_length = COALESCE($2, max_length),
            require_uppercase = COALESCE($3, require_uppercase), require_lowercase = COALESCE($4, require_lowercase),
            require_digit = COALESCE($5, require_digit), require_special_char = COALESCE($6, require_special_char),
            special_chars = COALESCE($7, special_chars), max_age_days = COALESCE($8, max_age_days),
            warning_days = COALESCE($9, warning_days), prevent_reuse_count = COALESCE($10, prevent_reuse_count),
            max_login_attempts = COALESCE($11, max_login_attempts), lockout_duration_minutes = COALESCE($12, lockout_duration_minutes),
            updated_at = NOW(), updated_by = $13::uuid
        WHERE id = '00000000-0000-0000-0000-000000000101'
        RETURNING min_length, max_length, require_uppercase, require_lowercase, require_digit,
            require_special_char, special_chars, max_age_days, warning_days, prevent_reuse_count,
            max_login_attempts, lockout_duration_minutes, updated_at
    ",
        )
        .bind(params.min_length)
        .bind(params.max_length)
        .bind(params.require_uppercase)
        .bind(params.require_lowercase)
        .bind(params.require_digit)
        .bind(params.require_special_char)
        .bind(&params.special_chars)
        .bind(params.max_age_days)
        .bind(params.warning_days)
        .bind(params.prevent_reuse_count)
        .bind(params.max_login_attempts)
        .bind(params.lockout_duration_minutes)
        .bind(params.updated_by)
        .fetch_one(pool)
        .await
}
/// 读取策略；缺失或读取失败时返回 `None`（调用方回落默认值）。
pub async fn load_policy_fields(pool: &PgPool) -> Option<PasswordPolicyRow> {
    get_password_policy(pool).await.ok()
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_recent_password_hashes(
    pool: &PgPool,
    user_id: uuid::Uuid,
    limit: i32,
) -> Result<Vec<String>, sqlx::Error> {
    let query = r"
        SELECT password_hash FROM password_history
        WHERE user_id = $1
        ORDER BY changed_at DESC
        LIMIT $2
    ";
    let rows = sqlx::query(query)
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| r.get("password_hash")).collect())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_password_history(
    pool: &PgPool,
    user_id: uuid::Uuid,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    let query = r"
        INSERT INTO password_history (id, user_id, password_hash, changed_at)
        VALUES ($1, $2, $3, NOW())
    ";
    let history_id = uuid::Uuid::new_v4();
    sqlx::query(query)
        .bind(history_id)
        .bind(user_id)
        .bind(password_hash)
        .execute(pool)
        .await?;
    Ok(())
}
pub async fn cleanup_old_password_history(pool: &PgPool, user_id: uuid::Uuid, keep_count: i32) {
    let query = r"
        DELETE FROM password_history
        WHERE user_id = $1
        AND id NOT IN (
            SELECT id FROM password_history
            WHERE user_id = $1
            ORDER BY changed_at DESC
            LIMIT $2
        )
    ";
    sqlx::query(query)
        .bind(user_id)
        .bind(keep_count)
        .execute(pool)
        .await
        .ok();
}
