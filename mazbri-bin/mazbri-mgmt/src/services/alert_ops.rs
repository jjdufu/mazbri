//! 告警完善：静默 / 抑制 CRUD。
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AlertOpsError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    NotFound(&'static str),
    #[error("{0}")]
    Conflict(&'static str),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Serialize)]
pub struct Silence {
    pub id: Uuid,
    pub matchers: Value,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub created_by: String,
    pub comment: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SilenceInput {
    pub matchers: Value,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    #[serde(default)]
    pub comment: String,
}

#[derive(Debug, Serialize)]
pub struct InhibitRule {
    pub id: Uuid,
    pub name: String,
    pub source_matchers: Value,
    pub target_matchers: Value,
    pub equal_labels: Vec<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct InhibitInput {
    pub name: String,
    pub source_matchers: Value,
    pub target_matchers: Value,
    #[serde(default)]
    pub equal_labels: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn require_object(v: &Value, field: &str) -> Result<(), AlertOpsError> {
    if v.as_object().is_some() {
        Ok(())
    } else {
        Err(AlertOpsError::Validation(format!("{field} 必须是 JSON 对象")))
    }
}

// ---- silences ----

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn list_silences(pool: &PgPool) -> Result<Vec<Silence>, AlertOpsError> {
    let rows = sqlx::query(
        "SELECT id, matchers, starts_at, ends_at, created_by, comment, created_at \
         FROM alert_silences ORDER BY starts_at DESC LIMIT 200",
    )
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|r| {
            Ok(Silence {
                id: r.try_get("id")?,
                matchers: r.try_get("matchers")?,
                starts_at: r.try_get("starts_at")?,
                ends_at: r.try_get("ends_at")?,
                created_by: r.try_get("created_by")?,
                comment: r.try_get("comment")?,
                created_at: r.try_get("created_at")?,
            })
        })
        .collect()
}

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn create_silence(
    pool: &PgPool,
    input: SilenceInput,
    created_by: &str,
) -> Result<Silence, AlertOpsError> {
    require_object(&input.matchers, "matchers")?;
    if input.ends_at <= input.starts_at {
        return Err(AlertOpsError::Validation("ends_at 必须晚于 starts_at".into()));
    }
    if input.comment.chars().count() > 500 {
        return Err(AlertOpsError::Validation("comment 过长".into()));
    }
    let row = sqlx::query(
        "INSERT INTO alert_silences (matchers, starts_at, ends_at, created_by, comment) \
         VALUES ($1,$2,$3,$4,$5) \
         RETURNING id, matchers, starts_at, ends_at, created_by, comment, created_at",
    )
    .bind(&input.matchers)
    .bind(input.starts_at)
    .bind(input.ends_at)
    .bind(created_by)
    .bind(input.comment)
    .fetch_one(pool)
    .await?;
    Ok(Silence {
        id: row.try_get("id")?,
        matchers: row.try_get("matchers")?,
        starts_at: row.try_get("starts_at")?,
        ends_at: row.try_get("ends_at")?,
        created_by: row.try_get("created_by")?,
        comment: row.try_get("comment")?,
        created_at: row.try_get("created_at")?,
    })
}

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn delete_silence(pool: &PgPool, id: Uuid) -> Result<(), AlertOpsError> {
    let n = sqlx::query("DELETE FROM alert_silences WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AlertOpsError::NotFound("静默不存在"));
    }
    Ok(())
}

// ---- inhibit ----

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn list_inhibits(pool: &PgPool) -> Result<Vec<InhibitRule>, AlertOpsError> {
    let rows = sqlx::query(
        "SELECT id, name, source_matchers, target_matchers, equal_labels, enabled, created_at, updated_at \
         FROM alert_inhibit_rules ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_inhibit).collect()
}

fn row_inhibit(r: &sqlx::postgres::PgRow) -> Result<InhibitRule, AlertOpsError> {
    Ok(InhibitRule {
        id: r.try_get("id")?,
        name: r.try_get("name")?,
        source_matchers: r.try_get("source_matchers")?,
        target_matchers: r.try_get("target_matchers")?,
        equal_labels: r.try_get("equal_labels")?,
        enabled: r.try_get("enabled")?,
        created_at: r.try_get("created_at")?,
        updated_at: r.try_get("updated_at")?,
    })
}

fn validate_inhibit(input: &InhibitInput) -> Result<(), AlertOpsError> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 128 {
        return Err(AlertOpsError::Validation("名称长度 1..=128".into()));
    }
    require_object(&input.source_matchers, "source_matchers")?;
    require_object(&input.target_matchers, "target_matchers")?;
    Ok(())
}

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn create_inhibit(pool: &PgPool, mut input: InhibitInput) -> Result<InhibitRule, AlertOpsError> {
    validate_inhibit(&input)?;
    input.name = input.name.trim().to_string();
    let row = sqlx::query(
        "INSERT INTO alert_inhibit_rules (name, source_matchers, target_matchers, equal_labels, enabled) \
         VALUES ($1,$2,$3,$4,$5) \
         RETURNING id, name, source_matchers, target_matchers, equal_labels, enabled, created_at, updated_at",
    )
    .bind(&input.name)
    .bind(&input.source_matchers)
    .bind(&input.target_matchers)
    .bind(&input.equal_labels)
    .bind(input.enabled)
    .fetch_one(pool)
    .await
    .map_err(map_unique)?;
    row_inhibit(&row)
}

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn update_inhibit(
    pool: &PgPool,
    id: Uuid,
    mut input: InhibitInput,
) -> Result<InhibitRule, AlertOpsError> {
    validate_inhibit(&input)?;
    input.name = input.name.trim().to_string();
    let row = sqlx::query(
        "UPDATE alert_inhibit_rules SET name=$2, source_matchers=$3, target_matchers=$4, \
         equal_labels=$5, enabled=$6, updated_at=now() WHERE id=$1 \
         RETURNING id, name, source_matchers, target_matchers, equal_labels, enabled, created_at, updated_at",
    )
    .bind(id)
    .bind(&input.name)
    .bind(&input.source_matchers)
    .bind(&input.target_matchers)
    .bind(&input.equal_labels)
    .bind(input.enabled)
    .fetch_optional(pool)
    .await
    .map_err(map_unique)?;
    row.map(|r| row_inhibit(&r)).transpose()?.ok_or(AlertOpsError::NotFound("抑制规则不存在"))
}

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn delete_inhibit(pool: &PgPool, id: Uuid) -> Result<(), AlertOpsError> {
    let n = sqlx::query("DELETE FROM alert_inhibit_rules WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AlertOpsError::NotFound("抑制规则不存在"));
    }
    Ok(())
}


fn map_unique(e: sqlx::Error) -> AlertOpsError {
    if let sqlx::Error::Database(db) = &e
        && db.constraint().is_some()
    {
        return AlertOpsError::Conflict("名称已存在");
    }
    AlertOpsError::Db(e)
}
