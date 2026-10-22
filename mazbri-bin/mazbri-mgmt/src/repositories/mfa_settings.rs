use chrono::{DateTime, Utc};
use sqlx::PgPool;
#[derive(Debug, sqlx::FromRow)]
pub struct MfaSettingsRow {
    pub mfa_enabled_globally: bool,
    pub mfa_required_for_all: bool,
    #[sqlx(default)]
    pub mfa_required_for_roles: Vec<String>,
    pub totp_issuer: String,
    pub totp_digits: i32,
    pub totp_period: i32,
    pub backup_codes_count: i32,
    pub updated_at: Option<DateTime<Utc>>,
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_mfa_settings(pool: &PgPool) -> Result<MfaSettingsRow, sqlx::Error> {
    sqlx::query_as::<_, MfaSettingsRow>(
        r"
        SELECT mfa_enabled_globally, mfa_required_for_all, mfa_required_for_roles,
            totp_issuer, totp_digits, totp_period, backup_codes_count, updated_at
        FROM mfa_settings WHERE id = '00000000-0000-0000-0000-000000000102'
    ",
    )
    .fetch_one(pool)
    .await
}
pub struct UpdateMfaSettingsParams<'a> {
    pub mfa_enabled_globally: Option<bool>,
    pub mfa_required_for_all: Option<bool>,
    pub mfa_required_for_roles: Option<Vec<String>>,
    pub totp_issuer: Option<String>,
    pub totp_digits: Option<i32>,
    pub totp_period: Option<i32>,
    pub backup_codes_count: Option<i32>,
    pub updated_by: &'a str,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_mfa_settings(
    pool: &PgPool,
    params: UpdateMfaSettingsParams<'_>,
) -> Result<MfaSettingsRow, sqlx::Error> {
    sqlx::query_as::<
        _,
        MfaSettingsRow,
    >(
            r"
        UPDATE mfa_settings SET
            mfa_enabled_globally = COALESCE($1, mfa_enabled_globally), mfa_required_for_all = COALESCE($2, mfa_required_for_all),
            mfa_required_for_roles = COALESCE($3, mfa_required_for_roles), totp_issuer = COALESCE($4, totp_issuer),
            totp_digits = COALESCE($5, totp_digits), totp_period = COALESCE($6, totp_period),
            backup_codes_count = COALESCE($7, backup_codes_count), updated_at = NOW(), updated_by = $8::uuid
        WHERE id = '00000000-0000-0000-0000-000000000102'
        RETURNING mfa_enabled_globally, mfa_required_for_all, mfa_required_for_roles,
            totp_issuer, totp_digits, totp_period, backup_codes_count, updated_at
    ",
        )
        .bind(params.mfa_enabled_globally)
        .bind(params.mfa_required_for_all)
        .bind(&params.mfa_required_for_roles)
        .bind(&params.totp_issuer)
        .bind(params.totp_digits)
        .bind(params.totp_period)
        .bind(params.backup_codes_count)
        .bind(params.updated_by)
        .fetch_one(pool)
        .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn clear_all_user_mfa(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let query = r"
        UPDATE admin_users SET mfa_enabled = false, mfa_secret = NULL, mfa_backup_codes = NULL,
            mfa_verified_at = NULL, mfa_attempts = 0, mfa_locked_until = NULL, updated_at = NOW()
        WHERE mfa_enabled = true
    ";
    let result = sqlx::query(query).execute(pool).await?;
    Ok(result.rows_affected())
}
