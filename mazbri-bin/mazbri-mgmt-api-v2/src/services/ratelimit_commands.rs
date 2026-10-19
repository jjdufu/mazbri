use crate::repositories::ratelimit_commands as repo;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Max temporary deny duration (seconds) — 24h hard cap against mis-ops.
const MAX_DENY_FOR_SECS: i64 = 24 * 3600;
/// Max quota correction lifetime (seconds) — 7 days.
const MAX_QUOTA_EXPIRES_SECS: i64 = 7 * 24 * 3600;
/// Default list page size.
const DEFAULT_LIST_LIMIT: i64 = 50;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateControlCommandRequest {
    pub command_type: String,
    /// Absolute deny deadline (unix seconds). Mutually exclusive with deny_for_secs.
    #[serde(default)]
    pub deny_until_unix: Option<i64>,
    /// Relative deny duration from now. Mutually exclusive with deny_until_unix.
    #[serde(default)]
    pub deny_for_secs: Option<i64>,
    /// Quota multiplier in \[0.1, 10.0\].
    #[serde(default)]
    pub quota_multiplier: Option<f64>,
    /// When the command expires (seconds from now). Required for quota_correction.
    #[serde(default)]
    pub expires_in_secs: Option<i64>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ControlCommandResponse {
    pub id: String,
    pub tenant_id: String,
    pub tenant_code: String,
    pub policy_version: i64,
    pub command_type: String,
    pub command_payload: serde_json::Value,
    pub effective_at: chrono::DateTime<Utc>,
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
    pub acknowledged_by: serde_json::Value,
    /// Derived: active when effective_at <= now and (expires null or > now).
    pub status: String,
}

#[derive(Debug)]
pub enum ControlCommandError {
    Validation(String),
    NotFound(String),
    Internal(String),
}

fn row_to_response(
    row: &repo::ControlCommandRow,
    tenant_code: String,
    now: chrono::DateTime<Utc>,
) -> ControlCommandResponse {
    let active =
        row.effective_at <= now && row.expires_at.map(|expires| expires > now).unwrap_or(true);
    ControlCommandResponse {
        id: row.id.to_string(),
        tenant_id: row.tenant_id.to_string(),
        tenant_code,
        policy_version: row.policy_version,
        command_type: row.command_type.clone(),
        command_payload: row.command_payload.clone(),
        effective_at: row.effective_at,
        expires_at: row.expires_at,
        created_by: row.created_by.clone(),
        created_at: row.created_at,
        acknowledged_by: row.acknowledged_by.clone(),
        status: if active { "active" } else { "expired" }.to_string(),
    }
}

fn validate_and_build_payload(
    req: &CreateControlCommandRequest,
) -> Result<(String, serde_json::Value, Option<chrono::DateTime<Utc>>), ControlCommandError> {
    let now = Utc::now();
    match req.command_type.as_str() {
        "deny_until" => {
            let (deny_until_unix, expires_at) = match (req.deny_until_unix, req.deny_for_secs) {
                (Some(until), None) => {
                    if until <= now.timestamp() {
                        return Err(ControlCommandError::Validation(
                            "deny_until_unix must be in the future".to_string(),
                        ));
                    }
                    let max_until = now.timestamp() + MAX_DENY_FOR_SECS;
                    if until > max_until {
                        return Err(ControlCommandError::Validation(format!(
                            "deny_until_unix must not exceed {MAX_DENY_FOR_SECS}s from now"
                        )));
                    }
                    (until, chrono::DateTime::from_timestamp(until, 0))
                }
                (None, Some(secs)) => {
                    if secs <= 0 {
                        return Err(ControlCommandError::Validation(
                            "deny_for_secs must be greater than 0".to_string(),
                        ));
                    }
                    if secs > MAX_DENY_FOR_SECS {
                        return Err(ControlCommandError::Validation(format!(
                            "deny_for_secs must not exceed {MAX_DENY_FOR_SECS}"
                        )));
                    }
                    let until = now.timestamp() + secs;
                    (until, Some(now + Duration::seconds(secs)))
                }
                (Some(_), Some(_)) => {
                    return Err(ControlCommandError::Validation(
                        "provide either deny_until_unix or deny_for_secs, not both".to_string(),
                    ));
                }
                (None, None) => {
                    return Err(ControlCommandError::Validation(
                        "deny_until requires deny_until_unix or deny_for_secs".to_string(),
                    ));
                }
            };
            let mut payload = serde_json::json!({ "deny_until_unix": deny_until_unix });
            if let Some(reason) = req.reason.as_ref() {
                payload["reason"] = serde_json::Value::String(reason.clone());
            }
            Ok(("deny_until".to_string(), payload, expires_at))
        }
        "quota_correction" => {
            let multiplier = req.quota_multiplier.ok_or_else(|| {
                ControlCommandError::Validation(
                    "quota_correction requires quota_multiplier".to_string(),
                )
            })?;
            if !(0.1..=10.0).contains(&multiplier) {
                return Err(ControlCommandError::Validation(
                    "quota_multiplier must be between 0.1 and 10.0".to_string(),
                ));
            }
            let expires_in = req.expires_in_secs.ok_or_else(|| {
                ControlCommandError::Validation(
                    "quota_correction requires expires_in_secs".to_string(),
                )
            })?;
            if expires_in <= 0 {
                return Err(ControlCommandError::Validation(
                    "expires_in_secs must be greater than 0".to_string(),
                ));
            }
            if expires_in > MAX_QUOTA_EXPIRES_SECS {
                return Err(ControlCommandError::Validation(format!(
                    "expires_in_secs must not exceed {MAX_QUOTA_EXPIRES_SECS}"
                )));
            }
            let mut payload = serde_json::json!({ "quota_multiplier": multiplier });
            if let Some(reason) = req.reason.as_ref() {
                payload["reason"] = serde_json::Value::String(reason.clone());
            }
            Ok((
                "quota_correction".to_string(),
                payload,
                Some(now + Duration::seconds(expires_in)),
            ))
        }
        "refresh_policy" => {
            // Short-lived so gateways do not re-fire forever; 60s is enough for all replicas.
            let mut payload = serde_json::json!({});
            if let Some(reason) = req.reason.as_ref() {
                payload["reason"] = serde_json::Value::String(reason.clone());
            }
            Ok((
                "refresh_policy".to_string(),
                payload,
                Some(now + Duration::seconds(60)),
            ))
        }
        other => Err(ControlCommandError::Validation(format!(
            "unsupported command_type '{other}'; expected deny_until, quota_correction, or refresh_policy"
        ))),
    }
}

pub async fn create_control_command(
    platform_db: &PgPool,
    ratelimit_db: &PgPool,
    tenant_id: Uuid,
    created_by: &str,
    req: &CreateControlCommandRequest,
) -> Result<ControlCommandResponse, ControlCommandError> {
    let identities = crate::repositories::tenant::get_tenant_identities(platform_db, &[tenant_id])
        .await
        .map_err(|e| ControlCommandError::Internal(format!("Failed to fetch tenant: {e}")))?;
    let (tenant_code, _) = identities
        .get(&tenant_id)
        .cloned()
        .ok_or_else(|| ControlCommandError::NotFound(format!("Tenant not found: {tenant_id}")))?;

    let (command_type, payload, expires_at) = validate_and_build_payload(req)?;
    let policy_version = repo::get_latest_policy_version(ratelimit_db, tenant_id)
        .await
        .map_err(|e| {
            ControlCommandError::Internal(format!("Failed to read policy version: {e}"))
        })?;

    let params = repo::CreateControlCommandParams {
        tenant_id,
        policy_version,
        command_type: &command_type,
        command_payload: &payload,
        effective_at: Utc::now(),
        expires_at,
        created_by: Some(created_by),
    };
    let row = repo::insert_control_command(ratelimit_db, &params)
        .await
        .map_err(|e| ControlCommandError::Internal(format!("Failed to insert command: {e}")))?;
    Ok(row_to_response(&row, tenant_code, Utc::now()))
}

pub async fn list_control_commands(
    platform_db: &PgPool,
    ratelimit_db: &PgPool,
    tenant_id: Uuid,
    limit: Option<i64>,
) -> Result<Vec<ControlCommandResponse>, ControlCommandError> {
    let identities = crate::repositories::tenant::get_tenant_identities(platform_db, &[tenant_id])
        .await
        .map_err(|e| ControlCommandError::Internal(format!("Failed to fetch tenant: {e}")))?;
    let (tenant_code, _) = identities
        .get(&tenant_id)
        .cloned()
        .ok_or_else(|| ControlCommandError::NotFound(format!("Tenant not found: {tenant_id}")))?;

    let rows =
        repo::list_control_commands(ratelimit_db, tenant_id, limit.unwrap_or(DEFAULT_LIST_LIMIT))
            .await
            .map_err(|e| ControlCommandError::Internal(format!("Failed to list commands: {e}")))?;
    let now = Utc::now();
    Ok(rows
        .iter()
        .map(|row| row_to_response(row, tenant_code.clone(), now))
        .collect())
}

pub async fn cancel_control_command(
    platform_db: &PgPool,
    ratelimit_db: &PgPool,
    tenant_id: Uuid,
    command_id: Uuid,
) -> Result<ControlCommandResponse, ControlCommandError> {
    let identities = crate::repositories::tenant::get_tenant_identities(platform_db, &[tenant_id])
        .await
        .map_err(|e| ControlCommandError::Internal(format!("Failed to fetch tenant: {e}")))?;
    let (tenant_code, _) = identities
        .get(&tenant_id)
        .cloned()
        .ok_or_else(|| ControlCommandError::NotFound(format!("Tenant not found: {tenant_id}")))?;

    let row = repo::cancel_control_command(ratelimit_db, tenant_id, command_id)
        .await
        .map_err(|e| ControlCommandError::Internal(format!("Failed to cancel command: {e}")))?
        .ok_or_else(|| ControlCommandError::NotFound(format!("Command not found: {command_id}")))?;
    Ok(row_to_response(&row, tenant_code, Utc::now()))
}
