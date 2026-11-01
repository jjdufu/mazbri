//! 告警完善：静默 / 抑制 / 值班 / 通知渠道 CRUD。
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

const SECRET_KEYS: &[&str] = &["token", "secret", "webhook_url", "access_token", "password"];

/// 渠道密钥加密用的应用级密钥材料（非用户口令）；生产可改为 bootstrap 注入。
fn channel_key() -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&Sha256::digest(b"mazbri-notify-channel-v1"));
    out
}

fn encrypt_blob(plain: &str) -> Result<String, AlertOpsError> {
    let key = Aes256Gcm::new_from_slice(&channel_key())
        .map_err(|_| AlertOpsError::Validation("加密初始化失败".into()))?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = key
        .encrypt(nonce, plain.as_bytes())
        .map_err(|_| AlertOpsError::Validation("加密失败".into()))?;
    let mut packed = nonce_bytes.to_vec();
    packed.extend_from_slice(&ct);
    Ok(hex::encode(packed))
}

fn split_channel_config(mut config: Value) -> Result<(Value, Option<String>), AlertOpsError> {
    let Some(obj) = config.as_object_mut() else {
        return Err(AlertOpsError::Validation("config 必须是对象".into()));
    };
    let mut secrets = serde_json::Map::new();
    for key in SECRET_KEYS {
        if let Some(v) = obj.remove(*key) {
            if v.is_null() {
                continue;
            }
            secrets.insert((*key).to_string(), v);
        }
    }
    let secret_enc = if secrets.is_empty() {
        None
    } else {
        Some(encrypt_blob(&Value::Object(secrets).to_string())?)
    };
    Ok((Value::Object(obj.clone()), secret_enc))
}

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

#[derive(Debug, Serialize)]
pub struct OncallSchedule {
    pub id: Uuid,
    pub name: String,
    pub timezone: String,
    pub layers: Value,
    pub escalation: Value,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct OncallInput {
    pub name: String,
    #[serde(default = "default_tz")]
    pub timezone: String,
    #[serde(default)]
    pub layers: Value,
    #[serde(default)]
    pub escalation: Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct NotifyChannel {
    pub id: Uuid,
    pub name: String,
    #[serde(rename = "type")]
    pub channel_type: String,
    pub config: Value,
    pub has_secret: bool,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ChannelInput {
    pub name: String,
    #[serde(rename = "type")]
    pub channel_type: String,
    #[serde(default)]
    pub config: Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}
fn default_tz() -> String {
    "Asia/Shanghai".into()
}

fn require_object(v: &Value, field: &str) -> Result<(), AlertOpsError> {
    if v.as_object().is_some() {
        Ok(())
    } else {
        Err(AlertOpsError::Validation(format!("{field} 必须是 JSON 对象")))
    }
}

fn require_array(v: &Value, field: &str) -> Result<(), AlertOpsError> {
    if v.as_array().is_some() || v.is_null() {
        Ok(())
    } else {
        Err(AlertOpsError::Validation(format!("{field} 必须是 JSON 数组")))
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

// ---- oncall ----

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn list_oncalls(pool: &PgPool) -> Result<Vec<OncallSchedule>, AlertOpsError> {
    let rows = sqlx::query(
        "SELECT id, name, timezone, layers, escalation, enabled, created_at, updated_at \
         FROM oncall_schedules ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_oncall).collect()
}

fn row_oncall(r: &sqlx::postgres::PgRow) -> Result<OncallSchedule, AlertOpsError> {
    Ok(OncallSchedule {
        id: r.try_get("id")?,
        name: r.try_get("name")?,
        timezone: r.try_get("timezone")?,
        layers: r.try_get("layers")?,
        escalation: r.try_get("escalation")?,
        enabled: r.try_get("enabled")?,
        created_at: r.try_get("created_at")?,
        updated_at: r.try_get("updated_at")?,
    })
}

fn validate_oncall(input: &mut OncallInput) -> Result<(), AlertOpsError> {
    input.name = input.name.trim().to_string();
    if input.name.is_empty() || input.name.chars().count() > 128 {
        return Err(AlertOpsError::Validation("名称长度 1..=128".into()));
    }
    if input.timezone.trim().is_empty() || input.timezone.chars().count() > 64 {
        return Err(AlertOpsError::Validation("timezone 非法".into()));
    }
    if input.layers.is_null() {
        input.layers = json!([]);
    }
    if input.escalation.is_null() {
        input.escalation = json!([]);
    }
    require_array(&input.layers, "layers")?;
    require_array(&input.escalation, "escalation")?;
    Ok(())
}

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn create_oncall(pool: &PgPool, mut input: OncallInput) -> Result<OncallSchedule, AlertOpsError> {
    validate_oncall(&mut input)?;
    let row = sqlx::query(
        "INSERT INTO oncall_schedules (name, timezone, layers, escalation, enabled) \
         VALUES ($1,$2,$3,$4,$5) \
         RETURNING id, name, timezone, layers, escalation, enabled, created_at, updated_at",
    )
    .bind(&input.name)
    .bind(&input.timezone)
    .bind(&input.layers)
    .bind(&input.escalation)
    .bind(input.enabled)
    .fetch_one(pool)
    .await
    .map_err(map_unique)?;
    row_oncall(&row)
}

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn update_oncall(
    pool: &PgPool,
    id: Uuid,
    mut input: OncallInput,
) -> Result<OncallSchedule, AlertOpsError> {
    validate_oncall(&mut input)?;
    let row = sqlx::query(
        "UPDATE oncall_schedules SET name=$2, timezone=$3, layers=$4, escalation=$5, enabled=$6, updated_at=now() \
         WHERE id=$1 \
         RETURNING id, name, timezone, layers, escalation, enabled, created_at, updated_at",
    )
    .bind(id)
    .bind(&input.name)
    .bind(&input.timezone)
    .bind(&input.layers)
    .bind(&input.escalation)
    .bind(input.enabled)
    .fetch_optional(pool)
    .await
    .map_err(map_unique)?;
    row.map(|r| row_oncall(&r)).transpose()?.ok_or(AlertOpsError::NotFound("值班表不存在"))
}

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn delete_oncall(pool: &PgPool, id: Uuid) -> Result<(), AlertOpsError> {
    let n = sqlx::query("DELETE FROM oncall_schedules WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AlertOpsError::NotFound("值班表不存在"));
    }
    Ok(())
}

// ---- channels ----

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn list_channels(pool: &PgPool) -> Result<Vec<NotifyChannel>, AlertOpsError> {
    let rows = sqlx::query(
        "SELECT id, name, type, config, (secret_enc IS NOT NULL) AS has_secret, enabled, created_at, updated_at \
         FROM notify_channels ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_channel).collect()
}

fn row_channel(r: &sqlx::postgres::PgRow) -> Result<NotifyChannel, AlertOpsError> {
    Ok(NotifyChannel {
        id: r.try_get("id")?,
        name: r.try_get("name")?,
        channel_type: r.try_get("type")?,
        config: r.try_get("config")?,
        has_secret: r.try_get("has_secret")?,
        enabled: r.try_get("enabled")?,
        created_at: r.try_get("created_at")?,
        updated_at: r.try_get("updated_at")?,
    })
}

fn validate_channel_type(t: &str) -> Result<(), AlertOpsError> {
    match t {
        "email" | "webhook" | "dingtalk" | "feishu" | "wecom" => Ok(()),
        _ => Err(AlertOpsError::Validation(
            "type 必须是 email|webhook|dingtalk|feishu|wecom".into(),
        )),
    }
}

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn create_channel(pool: &PgPool, mut input: ChannelInput) -> Result<NotifyChannel, AlertOpsError> {
    input.name = input.name.trim().to_string();
    if input.name.is_empty() || input.name.chars().count() > 128 {
        return Err(AlertOpsError::Validation("名称长度 1..=128".into()));
    }
    validate_channel_type(&input.channel_type)?;
    require_object(&input.config, "config")?;
    let (config, secret_enc) = split_channel_config(input.config)?;
    let row = sqlx::query(
        "INSERT INTO notify_channels (name, type, config, secret_enc, enabled) \
         VALUES ($1,$2,$3,$4,$5) \
         RETURNING id, name, type, config, (secret_enc IS NOT NULL) AS has_secret, enabled, created_at, updated_at",
    )
    .bind(&input.name)
    .bind(&input.channel_type)
    .bind(&config)
    .bind(secret_enc)
    .bind(input.enabled)
    .fetch_one(pool)
    .await
    .map_err(map_unique)?;
    row_channel(&row)
}

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn update_channel(
    pool: &PgPool,
    id: Uuid,
    mut input: ChannelInput,
) -> Result<NotifyChannel, AlertOpsError> {
    input.name = input.name.trim().to_string();
    if input.name.is_empty() || input.name.chars().count() > 128 {
        return Err(AlertOpsError::Validation("名称长度 1..=128".into()));
    }
    validate_channel_type(&input.channel_type)?;
    require_object(&input.config, "config")?;
    let (config, secret_enc) = split_channel_config(input.config)?;
    // 若本次未提交敏感字段，保留原 secret_enc
    let row = if let Some(enc) = secret_enc {
        sqlx::query(
            "UPDATE notify_channels SET name=$2, type=$3, config=$4, secret_enc=$5, enabled=$6, updated_at=now() \
             WHERE id=$1 \
             RETURNING id, name, type, config, (secret_enc IS NOT NULL) AS has_secret, enabled, created_at, updated_at",
        )
        .bind(id)
        .bind(&input.name)
        .bind(&input.channel_type)
        .bind(&config)
        .bind(enc)
        .bind(input.enabled)
        .fetch_optional(pool)
        .await
    } else {
        sqlx::query(
            "UPDATE notify_channels SET name=$2, type=$3, config=$4, enabled=$5, updated_at=now() \
             WHERE id=$1 \
             RETURNING id, name, type, config, (secret_enc IS NOT NULL) AS has_secret, enabled, created_at, updated_at",
        )
        .bind(id)
        .bind(&input.name)
        .bind(&input.channel_type)
        .bind(&config)
        .bind(input.enabled)
        .fetch_optional(pool)
        .await
    }
    .map_err(map_unique)?;
    row.map(|r| row_channel(&r)).transpose()?.ok_or(AlertOpsError::NotFound("通知渠道不存在"))
}

/// # Errors
///
/// 校验失败、记录不存在、名称冲突或数据库错误时返回 [`AlertOpsError`]。
pub async fn delete_channel(pool: &PgPool, id: Uuid) -> Result<(), AlertOpsError> {
    let n = sqlx::query("DELETE FROM notify_channels WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AlertOpsError::NotFound("通知渠道不存在"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_moves_secrets_out_of_config() {
        let (pub_cfg, enc) = split_channel_config(json!({
            "emails": ["a@b.c"],
            "token": "secret-token",
            "webhook_url": "https://example.com/hook"
        }))
        .expect("ok");
        assert!(enc.is_some());
        assert_eq!(pub_cfg.get("emails").and_then(|v| v[0].as_str()), Some("a@b.c"));
        assert!(pub_cfg.get("token").is_none());
        assert!(pub_cfg.get("webhook_url").is_none());
    }

    #[test]
    fn encrypt_roundtrip_length() {
        let enc = encrypt_blob(r#"{"token":"x"}"#).expect("enc");
        assert!(enc.len() > 24);
        assert!(hex::decode(&enc).is_ok());
    }
}
