use chrono::{DateTime, Utc};
use sqlx::PgPool;
/// MFA 全局参数（TOTP 发行方/位数/周期、备用码数量）。
///
/// 是否要码由操作类别决定，不再读取角色级全局策略列。
#[derive(Debug, sqlx::FromRow)]
pub struct MfaSettingsRow {
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
        SELECT totp_issuer, totp_digits, totp_period, backup_codes_count, updated_at
        FROM mfa_settings WHERE id = '00000000-0000-0000-0000-000000000102'
    ",
    )
    .fetch_one(pool)
    .await
}
pub struct UpdateMfaSettingsParams<'a> {
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
    sqlx::query_as::<_, MfaSettingsRow>(
        r"
        UPDATE mfa_settings SET
            totp_issuer = COALESCE($1, totp_issuer),
            totp_digits = COALESCE($2, totp_digits), totp_period = COALESCE($3, totp_period),
            backup_codes_count = COALESCE($4, backup_codes_count), updated_at = NOW(), updated_by = $5::uuid
        WHERE id = '00000000-0000-0000-0000-000000000102'
        RETURNING totp_issuer, totp_digits, totp_period, backup_codes_count, updated_at
    ",
    )
    .bind(&params.totp_issuer)
    .bind(params.totp_digits)
    .bind(params.totp_period)
    .bind(params.backup_codes_count)
    .bind(params.updated_by)
    .fetch_one(pool)
    .await
}
