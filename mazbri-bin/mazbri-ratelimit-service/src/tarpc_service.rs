use chrono::{TimeZone, Utc};
use mazbri_rpc::RpcError;
use mazbri_rpc::ratelimit::{
    AckControlCommandsRequest, AckControlCommandsResponse, ApiLimitRule as RpcApiLimitRule,
    ControlCommand, GetControlCommandsRequest, GetControlCommandsResponse,
    GetPolicySnapshotsRequest, GetPolicySnapshotsResponse, IngestUsageDeltasRequest,
    IngestUsageDeltasResponse, PolicySnapshot, RateLimitService,
};
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use std::collections::HashMap;
use std::sync::Arc;
use tarpc::context::Context;
use tracing::{debug, warn};
use uuid::Uuid;

fn parse_tenant_id(tenant_id: &str) -> Result<Uuid, String> {
    Uuid::parse_str(tenant_id).map_err(|e| {
        format!(
            "Invalid tenant_id '{}': {}. Expected format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
            tenant_id, e
        )
    })
}

fn now_unix() -> i64 {
    Utc::now().timestamp()
}

fn ts_to_unix(ts: chrono::DateTime<Utc>) -> i64 {
    ts.timestamp()
}

fn unix_to_ts(unix: i64) -> Result<chrono::DateTime<Utc>, String> {
    Utc.timestamp_opt(unix, 0)
        .single()
        .ok_or_else(|| format!("invalid unix timestamp: {unix}"))
}

fn rpc_validation(message: impl Into<String>, field: Option<&str>) -> RpcError {
    RpcError::Validation {
        message: message.into(),
        field: field.map(|value| value.to_string()),
    }
}

fn rpc_database_error(error: impl std::fmt::Display) -> RpcError {
    RpcError::DatabaseError {
        message: error.to_string(),
    }
}

/// Normalize `api_limits` JSON from mgmt/Web (`path -> number`) and object form
/// (`path -> {qps,burst}`) into RPC rules.
///
/// Bare numbers map to `{ qps: N, burst: N }` so shorthand overrides actually
/// tighten both rate and burst. Leaving `burst` unset made the gateway fall
/// back to **tenant_burst** (often ≫ api qps), which silently skipped `api_qps`
/// denials until the plan burst was exhausted (T-21 / productization §4.2).
fn parse_api_limits_json(value: Value) -> HashMap<String, RpcApiLimitRule> {
    let Some(obj) = value.as_object() else {
        return HashMap::new();
    };
    let mut out = HashMap::with_capacity(obj.len());
    for (key, entry) in obj {
        let rule = match entry {
            Value::Number(n) => {
                let qps = n
                    .as_u64()
                    .or_else(|| n.as_i64().filter(|v| *v > 0).map(|v| v as u64));
                RpcApiLimitRule {
                    qps,
                    // Shorthand: same capacity as qps (not tenant_burst fallback).
                    burst: qps,
                }
            }
            Value::Object(_) => serde_json::from_value::<RpcApiLimitRule>(entry.clone())
                .unwrap_or_default(),
            _ => continue,
        };
        if rule.qps.unwrap_or(0) > 0 || rule.burst.unwrap_or(0) > 0 {
            out.insert(key.clone(), rule);
        }
    }
    out
}

fn policy_row_to_snapshot(row: &sqlx::postgres::PgRow) -> Result<PolicySnapshot, sqlx::Error> {
    let created_at: chrono::DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: chrono::DateTime<Utc> = row.try_get("updated_at")?;
    let api_limits_value: Value = row.try_get("api_limits")?;
    let api_limits = parse_api_limits_json(api_limits_value);
    Ok(PolicySnapshot {
        id: row.try_get::<Uuid, _>("id")?.to_string(),
        tenant_id: row.try_get::<Uuid, _>("tenant_id")?.to_string(),
        version: row.try_get("version")?,
        enabled: row.try_get("enabled")?,
        enforcement_mode: row.try_get("enforcement_mode")?,
        stale_policy: row.try_get("stale_policy")?,
        max_snapshot_age_secs: row.try_get("max_snapshot_age_secs")?,
        tenant_qps: row.try_get("tenant_qps")?,
        tenant_burst: row.try_get("tenant_burst")?,
        client_ip_qps: row.try_get("client_ip_qps")?,
        client_ip_burst: row.try_get("client_ip_burst")?,
        bandwidth_bps: row.try_get("bandwidth_bps")?,
        api_limits,
        description: row.try_get("description")?,
        updated_by: row.try_get("updated_by")?,
        created_at_unix: ts_to_unix(created_at),
        updated_at_unix: ts_to_unix(updated_at),
    })
}

fn command_row_to_command(row: &sqlx::postgres::PgRow) -> Result<ControlCommand, sqlx::Error> {
    let effective_at: chrono::DateTime<Utc> = row.try_get("effective_at")?;
    let expires_at: Option<chrono::DateTime<Utc>> = row.try_get("expires_at")?;
    let created_at: chrono::DateTime<Utc> = row.try_get("created_at")?;
    let command_payload = row.try_get::<Value, _>("command_payload")?.to_string();
    let acknowledged_by = row.try_get::<Value, _>("acknowledged_by")?.to_string();
    Ok(ControlCommand {
        id: row.try_get::<Uuid, _>("id")?.to_string(),
        tenant_id: row.try_get::<Uuid, _>("tenant_id")?.to_string(),
        policy_version: row.try_get("policy_version")?,
        command_type: row.try_get("command_type")?,
        command_payload,
        effective_at_unix: ts_to_unix(effective_at),
        expires_at_unix: expires_at.map(ts_to_unix),
        created_by: row.try_get("created_by")?,
        created_at_unix: ts_to_unix(created_at),
        acknowledged_by,
    })
}

#[derive(Clone)]
pub struct RateLimitTarpcService {
    pool: Arc<PgPool>,
    audit_enabled: bool,
}

impl RateLimitTarpcService {
    pub fn new(db: Arc<PgPool>, audit_enabled: bool) -> Self {
        Self {
            pool: db,
            audit_enabled,
        }
    }
}

impl RateLimitService for RateLimitTarpcService {
    async fn get_policy_snapshots(
        self,
        _context: Context,
        request: GetPolicySnapshotsRequest,
    ) -> GetPolicySnapshotsResponse {
        let page_size = request.page_size.clamp(1, 1000) as i64;
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
            SELECT id, tenant_id, version, enabled, enforcement_mode, stale_policy,
                   max_snapshot_age_secs, tenant_qps, tenant_burst,
                   client_ip_qps, client_ip_burst, bandwidth_bps,
                   api_limits, description, updated_by, created_at, updated_at
            FROM rate_limit_tenant_policies
            WHERE version > 
            "#,
        );
        builder.push_bind(request.min_version.max(0));
        if let Some(tenant_id) = request.tenant_id.as_deref() {
            let tenant_uuid = match parse_tenant_id(tenant_id) {
                Ok(uuid) => uuid,
                Err(error) => {
                    return GetPolicySnapshotsResponse {
                        snapshots: vec![],
                        server_time_unix: now_unix(),
                        error: Some(rpc_validation(error, Some("tenant_id"))),
                    };
                }
            };
            builder.push(" AND tenant_id = ");
            builder.push_bind(tenant_uuid);
        }
        if !request.include_disabled {
            builder.push(" AND enabled = true");
        }
        builder.push(" ORDER BY updated_at DESC LIMIT ");
        builder.push_bind(page_size);
        let query = builder.build();
        match query.fetch_all(&*self.pool).await {
            Ok(rows) => {
                let mut snapshots = Vec::with_capacity(rows.len());
                for row in rows {
                    match policy_row_to_snapshot(&row) {
                        Ok(snapshot) => snapshots.push(snapshot),
                        Err(error) => {
                            warn!(error = % error, "Failed to parse rate limit policy row");
                        }
                    }
                }
                GetPolicySnapshotsResponse {
                    snapshots,
                    server_time_unix: now_unix(),
                    error: None,
                }
            }
            Err(error) => GetPolicySnapshotsResponse {
                snapshots: vec![],
                server_time_unix: now_unix(),
                error: Some(rpc_database_error(error)),
            },
        }
    }

    async fn ingest_usage_deltas(
        self,
        _context: Context,
        request: IngestUsageDeltasRequest,
    ) -> IngestUsageDeltasResponse {
        if request.deltas.is_empty() {
            return IngestUsageDeltasResponse {
                success: true,
                ingested: 0,
                error: None,
            };
        }

        let mut validated_deltas = Vec::with_capacity(request.deltas.len());
        for (idx, delta) in request.deltas.iter().enumerate() {
            let tenant_id = match Uuid::parse_str(&delta.tenant_id) {
                Ok(uuid) => uuid,
                Err(error) => {
                    return IngestUsageDeltasResponse {
                        success: false,
                        ingested: idx as i64,
                        error: Some(rpc_validation(format!("invalid tenant id: {error}"), Some("tenant_id"))),
                    };
                }
            };
            let window_start = match unix_to_ts(delta.window_start_unix) {
                Ok(ts) => ts,
                Err(error) => {
                    return IngestUsageDeltasResponse {
                        success: false,
                        ingested: idx as i64,
                        error: Some(rpc_validation(error, Some("window_start_unix"))),
                    };
                }
            };
            validated_deltas.push((delta, tenant_id, window_start));
        }

        let mut tx = match self.pool.begin().await {
            Ok(tx) => tx,
            Err(error) => {
                return IngestUsageDeltasResponse {
                    success: false,
                    ingested: 0,
                    error: Some(rpc_database_error(error)),
                };
            }
        };

        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
            INSERT INTO rate_limit_usage_deltas (
                gateway_id, tenant_id, policy_version, product_id, api_endpoint, client_ip,
                window_start, window_secs, allowed_count, denied_count, bytes_in, bytes_out,
                snapshot_age_ms, degraded, degrade_reason
            ) VALUES 
            "#,
        );
        for (i, (delta, tenant_id, window_start)) in validated_deltas.iter().enumerate() {
            if i > 0 {
                builder.push(", ");
            }
            builder.push("(");
            builder.push_bind(&delta.gateway_id);
            builder.push(", ");
            builder.push_bind(tenant_id);
            builder.push(", ");
            builder.push_bind(delta.policy_version);
            builder.push(", ");
            builder.push_bind(&delta.product_id);
            builder.push(", ");
            builder.push_bind(&delta.api_endpoint);
            builder.push(", ");
            let client_ip = delta.client_ip.as_deref().unwrap_or("").trim();
            let client_ip_opt = if client_ip.is_empty() { None } else { Some(client_ip) };
            builder.push_bind(client_ip_opt);
            builder.push("::inet, ");
            builder.push_bind(*window_start);
            builder.push(", ");
            builder.push_bind(delta.window_secs);
            builder.push(", ");
            builder.push_bind(delta.allowed_count);
            builder.push(", ");
            builder.push_bind(delta.denied_count);
            builder.push(", ");
            builder.push_bind(delta.bytes_in);
            builder.push(", ");
            builder.push_bind(delta.bytes_out);
            builder.push(", ");
            builder.push_bind(delta.snapshot_age_ms);
            builder.push(", ");
            builder.push_bind(delta.degraded);
            builder.push(", ");
            builder.push_bind(&delta.degrade_reason);
            builder.push(")");
        }
        if let Err(error) = builder.build().execute(&mut *tx).await {
            return IngestUsageDeltasResponse {
                success: false,
                ingested: 0,
                error: Some(rpc_database_error(error)),
            };
        }

        type CounterKey = (Uuid, String, chrono::DateTime<Utc>, i32);
        struct AggregatedCounter {
            policy_version: i64,
            allowed_count: i64,
            denied_count: i64,
            bytes_in: i64,
            bytes_out: i64,
        }
        let mut aggregated_counters: HashMap<CounterKey, AggregatedCounter> = HashMap::new();
        for (delta, tenant_id, window_start) in &validated_deltas {
            let client_ip = delta.client_ip.clone().unwrap_or_default();
            let dimension_key = format!(
                "{}|{}|{}|{}",
                delta.product_id, delta.api_endpoint, client_ip, delta.policy_version
            );
            let key = (*tenant_id, dimension_key, *window_start, delta.window_secs);
            let entry = aggregated_counters.entry(key).or_insert(AggregatedCounter {
                policy_version: delta.policy_version,
                allowed_count: 0,
                denied_count: 0,
                bytes_in: 0,
                bytes_out: 0,
            });
            entry.policy_version = entry.policy_version.max(delta.policy_version);
            entry.allowed_count += delta.allowed_count;
            entry.denied_count += delta.denied_count;
            entry.bytes_in += delta.bytes_in;
            entry.bytes_out += delta.bytes_out;
        }

        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
            INSERT INTO rate_limit_global_counters (
                tenant_id, policy_version, dimension_key, window_start, window_secs,
                allowed_count, denied_count, bytes_in, bytes_out, gateway_count
            ) VALUES 
            "#,
        );
        for (i, (key, value)) in aggregated_counters.iter().enumerate() {
            if i > 0 {
                builder.push(", ");
            }
            builder.push("(");
            builder.push_bind(key.0); 
            builder.push(", ");
            builder.push_bind(value.policy_version);
            builder.push(", ");
            builder.push_bind(&key.1); 
            builder.push(", ");
            builder.push_bind(key.2); 
            builder.push(", ");
            builder.push_bind(key.3); 
            builder.push(", ");
            builder.push_bind(value.allowed_count);
            builder.push(", ");
            builder.push_bind(value.denied_count);
            builder.push(", ");
            builder.push_bind(value.bytes_in);
            builder.push(", ");
            builder.push_bind(value.bytes_out);
            builder.push(", 1)");
        }
        builder.push(
            r#"
            ON CONFLICT (tenant_id, dimension_key, window_start, window_secs)
            DO UPDATE SET
                policy_version = EXCLUDED.policy_version,
                allowed_count = rate_limit_global_counters.allowed_count + EXCLUDED.allowed_count,
                denied_count = rate_limit_global_counters.denied_count + EXCLUDED.denied_count,
                bytes_in = rate_limit_global_counters.bytes_in + EXCLUDED.bytes_in,
                bytes_out = rate_limit_global_counters.bytes_out + EXCLUDED.bytes_out,
                gateway_count = GREATEST(rate_limit_global_counters.gateway_count, EXCLUDED.gateway_count),
                updated_at = NOW()
            "#,
        );
        if let Err(error) = builder.build().execute(&mut *tx).await {
            return IngestUsageDeltasResponse {
                success: false,
                ingested: 0,
                error: Some(rpc_database_error(error)),
            };
        }

        if self.audit_enabled {
            let mut audit_events = Vec::new();
            for (delta, tenant_id, _window_start) in &validated_deltas {
                if delta.degraded || delta.denied_count > 0 {
                    audit_events.push((delta, tenant_id));
                }
            }
            if !audit_events.is_empty() {
                let mut builder = QueryBuilder::<Postgres>::new(
                    r#"
                    INSERT INTO rate_limit_events (
                        tenant_id, gateway_id, request_id, policy_version, scope, scope_key,
                        product_id, api_endpoint, client_ip, allowed, reason, local_decision,
                        degraded, current_count, limit_value, retry_after_ms
                    ) VALUES 
                    "#,
                );
                for (i, (delta, tenant_id)) in audit_events.iter().enumerate() {
                    if i > 0 {
                        builder.push(", ");
                    }
                    let client_ip = delta.client_ip.as_deref().unwrap_or("").trim();
                    let client_ip_opt = if client_ip.is_empty() { None } else { Some(client_ip.to_string()) };
                    let dimension_key = format!(
                        "{}|{}|{}|{}",
                        delta.product_id,
                        delta.api_endpoint,
                        delta.client_ip.as_deref().unwrap_or(""),
                        delta.policy_version
                    );
                    builder.push("(");
                    builder.push_bind(*tenant_id);
                    builder.push(", ");
                    builder.push_bind(&delta.gateway_id);
                    builder.push(", NULL, ");
                    builder.push_bind(delta.policy_version);
                    builder.push(", ");
                    builder.push_bind("tenant");
                    builder.push(", ");
                    builder.push_bind(dimension_key);
                    builder.push(", ");
                    builder.push_bind(&delta.product_id);
                    builder.push(", ");
                    builder.push_bind(&delta.api_endpoint);
                    builder.push(", ");
                    builder.push_bind(client_ip_opt);
                    builder.push("::inet, ");
                    builder.push_bind(delta.denied_count == 0);
                    builder.push(", ");
                    let reason = if delta.degraded {
                        delta
                            .degrade_reason
                            .clone()
                            .unwrap_or_else(|| "degraded".to_string())
                    } else {
                        "usage_delta".to_string()
                    };
                    builder.push_bind(reason);
                    builder.push(", true, ");
                    builder.push_bind(delta.degraded);
                    builder.push(", ");
                    builder.push_bind(delta.allowed_count + delta.denied_count);
                    builder.push(", ");
                    builder.push_bind(delta.allowed_count + delta.denied_count);
                    builder.push(", 0)");
                }
                if let Err(error) = builder.build().execute(&mut *tx).await {
                    return IngestUsageDeltasResponse {
                        success: false,
                        ingested: 0,
                        error: Some(rpc_database_error(error)),
                    };
                }
            }
        }

        if let Err(error) = tx.commit().await {
            return IngestUsageDeltasResponse {
                success: false,
                ingested: 0,
                error: Some(rpc_database_error(error)),
            };
        }

        let ingested = request.deltas.len() as i64;
        IngestUsageDeltasResponse {
            success: true,
            ingested,
            error: None,
        }
    }

    async fn get_control_commands(
        self,
        _context: Context,
        request: GetControlCommandsRequest,
    ) -> GetControlCommandsResponse {
        let page_size = request.page_size.clamp(1, 1000) as i64;
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
            SELECT id, tenant_id, policy_version, command_type, command_payload,
                   effective_at, expires_at, created_by, created_at, acknowledged_by
            FROM rate_limit_control_commands
            WHERE effective_at <= NOW()
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
        );
        if let Some(tenant_id) = request.tenant_id.as_deref() {
            let tenant_uuid = match parse_tenant_id(tenant_id) {
                Ok(uuid) => uuid,
                Err(error) => {
                    return GetControlCommandsResponse {
                        commands: vec![],
                        server_time_unix: now_unix(),
                        error: Some(rpc_validation(error, Some("tenant_id"))),
                    };
                }
            };
            builder.push(" AND tenant_id = ");
            builder.push_bind(tenant_uuid);
        }
        // Stable order: newest effective first so clients can apply latest-wins deterministically.
        builder.push(" ORDER BY effective_at DESC, created_at DESC LIMIT ");
        builder.push_bind(page_size);
        match builder.build().fetch_all(&*self.pool).await {
            Ok(rows) => {
                let mut commands = Vec::with_capacity(rows.len());
                for row in rows {
                    match command_row_to_command(&row) {
                        Ok(command) => commands.push(command),
                        Err(error) => warn!(error = % error, "Failed to parse control command row"),
                    }
                }
                GetControlCommandsResponse {
                    commands,
                    server_time_unix: now_unix(),
                    error: None,
                }
            }
            Err(error) => GetControlCommandsResponse {
                commands: vec![],
                server_time_unix: now_unix(),
                error: Some(rpc_database_error(error)),
            },
        }
    }

    async fn ack_control_commands(
        self,
        _context: Context,
        request: AckControlCommandsRequest,
    ) -> AckControlCommandsResponse {
        if request.gateway_id.trim().is_empty() {
            return AckControlCommandsResponse {
                success: false,
                acknowledged: 0,
                error: Some(rpc_validation("gateway_id is required", Some("gateway_id"))),
            };
        }
        if request.command_ids.is_empty() {
            return AckControlCommandsResponse {
                success: true,
                acknowledged: 0,
                error: None,
            };
        }
        let mut command_uuids = Vec::with_capacity(request.command_ids.len());
        for command_id in &request.command_ids {
            let command_uuid = match Uuid::parse_str(command_id) {
                Ok(uuid) => uuid,
                Err(error) => {
                    return AckControlCommandsResponse {
                        success: false,
                        acknowledged: 0,
                        error: Some(rpc_validation(
                            format!("invalid command id: {error}"),
                            Some("command_ids"),
                        )),
                    };
                }
            };
            command_uuids.push(command_uuid);
        }
        let result = sqlx::query(
            r#"
            UPDATE rate_limit_control_commands
            SET acknowledged_by = COALESCE(acknowledged_by, '{}'::jsonb)
                || jsonb_build_object($2, to_jsonb(NOW()))
            WHERE id = ANY($1)
            "#,
        )
        .bind(&command_uuids)
        .bind(&request.gateway_id)
        .execute(&*self.pool)
        .await;
        let acknowledged = match result {
            Ok(result) => result.rows_affected() as i64,
            Err(error) => {
                return AckControlCommandsResponse {
                    success: false,
                    acknowledged: 0,
                    error: Some(rpc_database_error(error)),
                };
            }
        };
        debug!(
            gateway_id = % request.gateway_id,
            acknowledged,
            "Acknowledged rate limit control commands"
        );
        AckControlCommandsResponse {
            success: true,
            acknowledged,
            error: None,
        }
    }

    async fn health(self, _context: Context) -> mazbri_rpc::HealthResponse {
        mazbri_rpc::HealthResponse {
            status: "ok".to_string(),
            service: "ratelimit-service".to_string(),
            timestamp: Utc::now().timestamp(),
        }
    }
}
