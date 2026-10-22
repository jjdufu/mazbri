use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ControlCommandRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub policy_version: i64,
    pub command_type: String,
    pub command_payload: serde_json::Value,
    pub effective_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub acknowledged_by: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CreateControlCommandParams<'a> {
    pub tenant_id: Uuid,
    pub policy_version: i64,
    pub command_type: &'a str,
    pub command_payload: &'a serde_json::Value,
    pub effective_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by: Option<&'a str>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_control_command(
    db: &PgPool,
    params: &CreateControlCommandParams<'_>,
) -> Result<ControlCommandRow, sqlx::Error> {
    sqlx::query_as::<_, ControlCommandRow>(
        r"
        INSERT INTO rate_limit_control_commands (
            tenant_id, policy_version, command_type, command_payload,
            effective_at, expires_at, created_by
        ) VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING
            id, tenant_id, policy_version, command_type, command_payload,
            effective_at, expires_at, created_by, created_at, acknowledged_by
        ",
    )
    .bind(params.tenant_id)
    .bind(params.policy_version)
    .bind(params.command_type)
    .bind(params.command_payload)
    .bind(params.effective_at)
    .bind(params.expires_at)
    .bind(params.created_by)
    .fetch_one(db)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_control_commands(
    db: &PgPool,
    tenant_id: Uuid,
    limit: i64,
) -> Result<Vec<ControlCommandRow>, sqlx::Error> {
    sqlx::query_as::<_, ControlCommandRow>(
        r"
        SELECT
            id, tenant_id, policy_version, command_type, command_payload,
            effective_at, expires_at, created_by, created_at, acknowledged_by
        FROM rate_limit_control_commands
        WHERE tenant_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        ",
    )
    .bind(tenant_id)
    .bind(limit.clamp(1, 500))
    .fetch_all(db)
    .await
}

/// Cancel by setting `expires_at` = now (idempotent if already expired).
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn cancel_control_command(
    db: &PgPool,
    tenant_id: Uuid,
    command_id: Uuid,
) -> Result<Option<ControlCommandRow>, sqlx::Error> {
    sqlx::query_as::<_, ControlCommandRow>(
        r"
        UPDATE rate_limit_control_commands
        SET expires_at = LEAST(COALESCE(expires_at, NOW()), NOW())
        WHERE id = $1 AND tenant_id = $2
        RETURNING
            id, tenant_id, policy_version, command_type, command_payload,
            effective_at, expires_at, created_by, created_at, acknowledged_by
        ",
    )
    .bind(command_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_latest_policy_version(db: &PgPool, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
    let version: Option<i64> = sqlx::query_scalar(
        r"
        SELECT version FROM rate_limit_tenant_policies WHERE tenant_id = $1
        ",
    )
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;
    Ok(version.unwrap_or(0))
}
