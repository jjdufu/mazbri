use chrono::{TimeZone, Utc};
use ipnetwork::IpNetwork;
use mazbri_core::num::SaturatingFrom;
use mazbri_rpc::RpcError;
use mazbri_rpc::ratelimit::{
    AckControlCommandsRequest, AckControlCommandsResponse, ApiLimitRule as RpcApiLimitRule,
    ControlCommand, GetControlCommandsRequest, GetControlCommandsResponse,
    GetPolicySnapshotsRequest, GetPolicySnapshotsResponse, IngestUsageDeltasRequest,
    IngestUsageDeltasResponse, PolicySnapshot, RateLimitService, UsageDelta,
};
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tarpc::context::Context;
use tracing::{debug, warn};
use uuid::Uuid;

fn parse_tenant_id(tenant_id: &str) -> Result<Uuid, String> {
    Uuid::parse_str(tenant_id).map_err(|e| {
        format!(
            "Invalid tenant_id '{tenant_id}': {e}. Expected format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
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
        field: field.map(std::string::ToString::to_string),
    }
}

fn rpc_database_error(error: impl std::fmt::Display) -> RpcError {
    RpcError::DatabaseError {
        message: error.to_string(),
    }
}

/// Parse `api_limits` JSON: `path -> {qps,burst}` into RPC rules.
///
/// Only the object form is accepted. Entries that fail to deserialize or carry
/// neither a positive `qps` nor a positive `burst` are skipped.
fn parse_api_limits_json(value: &Value) -> HashMap<String, RpcApiLimitRule> {
    let Some(obj) = value.as_object() else {
        return HashMap::new();
    };
    let mut out = HashMap::with_capacity(obj.len());
    for (key, entry) in obj {
        let Value::Object(_) = entry else {
            continue;
        };
        let Ok(rule) = serde_json::from_value::<RpcApiLimitRule>(entry.clone()) else {
            continue;
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
    let api_limits = parse_api_limits_json(&api_limits_value);
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
    /// 统一审计写入端（未配置 `[rate_limit.audit.database]` 时为 `None`）。
    audit_sink: Option<mazbri_audit::sink::AuditSink>,
}

/// 已校验的用量增量：`(delta, tenant_id, window_start)`。
type ValidatedDelta<'a> = (&'a UsageDelta, Uuid, chrono::DateTime<Utc>);

fn validate_usage_deltas(
    deltas: &[UsageDelta],
) -> Result<Vec<ValidatedDelta<'_>>, IngestUsageDeltasResponse> {
    let mut validated_deltas = Vec::with_capacity(deltas.len());
    for (idx, delta) in deltas.iter().enumerate() {
        let tenant_id = match Uuid::parse_str(&delta.tenant_id) {
            Ok(uuid) => uuid,
            Err(error) => {
                return Err(IngestUsageDeltasResponse {
                    success: false,
                    ingested: i64::saturating_from(idx),
                    error: Some(rpc_validation(
                        format!("invalid tenant id: {error}"),
                        Some("tenant_id"),
                    )),
                });
            }
        };
        let window_start = match unix_to_ts(delta.window_start_unix) {
            Ok(ts) => ts,
            Err(error) => {
                return Err(IngestUsageDeltasResponse {
                    success: false,
                    ingested: i64::saturating_from(idx),
                    error: Some(rpc_validation(error, Some("window_start_unix"))),
                });
            }
        };
        validated_deltas.push((delta, tenant_id, window_start));
    }

    Ok(validated_deltas)
}

const MAX_DIMENSION_KEY_LEN: usize = 768;

/// Parse optional `client_ip` into a canonical host [`IpNetwork`] for `PostgreSQL` `INET` columns.
/// Non-IP strings (e.g. `"not-an-ip"`, `"unknown"`, empty, or arbitrary CIDRs) safely yield `None`
/// (`NULL` in SQL) so a malformed header value never aborts a multi-row transaction.
fn parse_client_ip_inet(raw: Option<&str>) -> Option<IpNetwork> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let ip = trimmed.parse::<IpAddr>().ok()?;
    let canonical = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        IpAddr::V4(v4) => IpAddr::V4(v4),
    };
    Some(IpNetwork::from(canonical))
}

fn format_dimension_key(
    product_id: &str,
    api_endpoint: &str,
    client_ip: Option<IpNetwork>,
    policy_version: i64,
) -> String {
    let ip_part = client_ip.map_or_else(String::new, |net| net.ip().to_string());
    let mut key = format!("{product_id}|{api_endpoint}|{ip_part}|{policy_version}");
    if key.len() > MAX_DIMENSION_KEY_LEN {
        key.truncate(key.floor_char_boundary(MAX_DIMENSION_KEY_LEN));
    }
    key
}

struct UsageDeltaUnnestBatch {
    gateway_ids: Vec<String>,
    tenant_ids: Vec<Uuid>,
    policy_versions: Vec<i64>,
    product_ids: Vec<String>,
    api_endpoints: Vec<String>,
    client_ips: Vec<Option<IpNetwork>>,
    window_starts: Vec<chrono::DateTime<Utc>>,
    window_secs: Vec<i32>,
    allowed_counts: Vec<i64>,
    denied_counts: Vec<i64>,
    bytes_in: Vec<i64>,
    bytes_out: Vec<i64>,
    snapshot_ages_ms: Vec<i64>,
    degraded_flags: Vec<bool>,
    degrade_reasons: Vec<Option<String>>,
}

impl UsageDeltaUnnestBatch {
    /// Number of SQL bind parameters used by the `UNNEST` statement (constant 15, independent of row count).
    const SQL_BIND_PARAM_COUNT: usize = 15;

    fn from_validated(validated_deltas: &[ValidatedDelta<'_>]) -> Self {
        let len = validated_deltas.len();
        let mut batch = Self {
            gateway_ids: Vec::with_capacity(len),
            tenant_ids: Vec::with_capacity(len),
            policy_versions: Vec::with_capacity(len),
            product_ids: Vec::with_capacity(len),
            api_endpoints: Vec::with_capacity(len),
            client_ips: Vec::with_capacity(len),
            window_starts: Vec::with_capacity(len),
            window_secs: Vec::with_capacity(len),
            allowed_counts: Vec::with_capacity(len),
            denied_counts: Vec::with_capacity(len),
            bytes_in: Vec::with_capacity(len),
            bytes_out: Vec::with_capacity(len),
            snapshot_ages_ms: Vec::with_capacity(len),
            degraded_flags: Vec::with_capacity(len),
            degrade_reasons: Vec::with_capacity(len),
        };
        for (delta, tenant_id, window_start) in validated_deltas {
            batch.gateway_ids.push(delta.gateway_id.clone());
            batch.tenant_ids.push(*tenant_id);
            batch.policy_versions.push(delta.policy_version);
            batch.product_ids.push(delta.product_id.clone());
            batch.api_endpoints.push(delta.api_endpoint.clone());
            batch
                .client_ips
                .push(parse_client_ip_inet(delta.client_ip.as_deref()));
            batch.window_starts.push(*window_start);
            batch.window_secs.push(delta.window_secs);
            batch.allowed_counts.push(delta.allowed_count);
            batch.denied_counts.push(delta.denied_count);
            batch.bytes_in.push(delta.bytes_in);
            batch.bytes_out.push(delta.bytes_out);
            batch.snapshot_ages_ms.push(delta.snapshot_age_ms);
            batch.degraded_flags.push(delta.degraded);
            batch.degrade_reasons.push(delta.degrade_reason.clone());
        }
        batch
    }
}

async fn insert_usage_deltas(
    conn: &mut sqlx::PgConnection,
    validated_deltas: &[ValidatedDelta<'_>],
) -> Result<(), sqlx::Error> {
    if validated_deltas.is_empty() {
        return Ok(());
    }
    const {
        assert!(UsageDeltaUnnestBatch::SQL_BIND_PARAM_COUNT < 65_535);
    }
    let batch = UsageDeltaUnnestBatch::from_validated(validated_deltas);
    sqlx::query(
        r"
        INSERT INTO rate_limit_usage_deltas (
            gateway_id, tenant_id, policy_version, product_id, api_endpoint, client_ip,
            window_start, window_secs, allowed_count, denied_count, bytes_in, bytes_out,
            snapshot_age_ms, degraded, degrade_reason
        )
        SELECT * FROM UNNEST(
            $1::text[],
            $2::uuid[],
            $3::bigint[],
            $4::text[],
            $5::text[],
            $6::inet[],
            $7::timestamptz[],
            $8::int4[],
            $9::bigint[],
            $10::bigint[],
            $11::bigint[],
            $12::bigint[],
            $13::bigint[],
            $14::bool[],
            $15::text[]
        )
        ",
    )
    .bind(&batch.gateway_ids)
    .bind(&batch.tenant_ids)
    .bind(&batch.policy_versions)
    .bind(&batch.product_ids)
    .bind(&batch.api_endpoints)
    .bind(&batch.client_ips)
    .bind(&batch.window_starts)
    .bind(&batch.window_secs)
    .bind(&batch.allowed_counts)
    .bind(&batch.denied_counts)
    .bind(&batch.bytes_in)
    .bind(&batch.bytes_out)
    .bind(&batch.snapshot_ages_ms)
    .bind(&batch.degraded_flags)
    .bind(&batch.degrade_reasons)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

async fn upsert_global_counters(
    conn: &mut sqlx::PgConnection,
    validated_deltas: &[ValidatedDelta<'_>],
) -> Result<(), sqlx::Error> {
    let mut aggregated_counters: HashMap<CounterKey, AggregatedCounter> = HashMap::new();
    for (delta, tenant_id, window_start) in validated_deltas {
        let parsed_ip = parse_client_ip_inet(delta.client_ip.as_deref());
        let dimension_key = format_dimension_key(
            &delta.product_id,
            &delta.api_endpoint,
            parsed_ip,
            delta.policy_version,
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

    if aggregated_counters.is_empty() {
        return Ok(());
    }

    let len = aggregated_counters.len();
    let mut tenant_ids = Vec::with_capacity(len);
    let mut policy_versions = Vec::with_capacity(len);
    let mut dimension_keys = Vec::with_capacity(len);
    let mut window_starts = Vec::with_capacity(len);
    let mut window_secs = Vec::with_capacity(len);
    let mut allowed_counts = Vec::with_capacity(len);
    let mut denied_counts = Vec::with_capacity(len);
    let mut bytes_in = Vec::with_capacity(len);
    let mut bytes_out = Vec::with_capacity(len);

    for (key, value) in aggregated_counters {
        tenant_ids.push(key.0);
        policy_versions.push(value.policy_version);
        dimension_keys.push(key.1);
        window_starts.push(key.2);
        window_secs.push(key.3);
        allowed_counts.push(value.allowed_count);
        denied_counts.push(value.denied_count);
        bytes_in.push(value.bytes_in);
        bytes_out.push(value.bytes_out);
    }

    sqlx::query(
        r"
        INSERT INTO rate_limit_global_counters (
            tenant_id, policy_version, dimension_key, window_start, window_secs,
            allowed_count, denied_count, bytes_in, bytes_out, gateway_count
        )
        SELECT
            u.tenant_id, u.policy_version, u.dimension_key, u.window_start, u.window_secs,
            u.allowed_count, u.denied_count, u.bytes_in, u.bytes_out, 1
        FROM UNNEST(
            $1::uuid[],
            $2::bigint[],
            $3::text[],
            $4::timestamptz[],
            $5::int4[],
            $6::bigint[],
            $7::bigint[],
            $8::bigint[],
            $9::bigint[]
        ) AS u(
            tenant_id, policy_version, dimension_key, window_start, window_secs,
            allowed_count, denied_count, bytes_in, bytes_out
        )
        ON CONFLICT (tenant_id, dimension_key, window_start, window_secs)
        DO UPDATE SET
            policy_version = EXCLUDED.policy_version,
            allowed_count = rate_limit_global_counters.allowed_count + EXCLUDED.allowed_count,
            denied_count = rate_limit_global_counters.denied_count + EXCLUDED.denied_count,
            bytes_in = rate_limit_global_counters.bytes_in + EXCLUDED.bytes_in,
            bytes_out = rate_limit_global_counters.bytes_out + EXCLUDED.bytes_out,
            gateway_count = GREATEST(rate_limit_global_counters.gateway_count, EXCLUDED.gateway_count),
            updated_at = NOW()
        ",
    )
    .bind(&tenant_ids)
    .bind(&policy_versions)
    .bind(&dimension_keys)
    .bind(&window_starts)
    .bind(&window_secs)
    .bind(&allowed_counts)
    .bind(&denied_counts)
    .bind(&bytes_in)
    .bind(&bytes_out)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

async fn insert_usage_audit_events(
    conn: &mut sqlx::PgConnection,
    validated_deltas: &[ValidatedDelta<'_>],
) -> Result<(), sqlx::Error> {
    let mut tenant_ids = Vec::new();
    let mut gateway_ids = Vec::new();
    let mut policy_versions = Vec::new();
    let mut scope_keys = Vec::new();
    let mut product_ids = Vec::new();
    let mut api_endpoints = Vec::new();
    let mut client_ips = Vec::new();
    let mut allowed_flags = Vec::new();
    let mut reasons = Vec::new();
    let mut degraded_flags = Vec::new();
    let mut counts = Vec::new();

    for (delta, tenant_id, _window_start) in validated_deltas {
        if !(delta.degraded || delta.denied_count > 0) {
            continue;
        }
        let parsed_ip = parse_client_ip_inet(delta.client_ip.as_deref());
        let dimension_key = format_dimension_key(
            &delta.product_id,
            &delta.api_endpoint,
            parsed_ip,
            delta.policy_version,
        );
        let reason = if delta.degraded {
            delta
                .degrade_reason
                .clone()
                .unwrap_or_else(|| "degraded".to_string())
        } else {
            "usage_delta".to_string()
        };
        let total = delta.allowed_count.saturating_add(delta.denied_count);

        tenant_ids.push(*tenant_id);
        gateway_ids.push(delta.gateway_id.clone());
        policy_versions.push(delta.policy_version);
        scope_keys.push(dimension_key);
        product_ids.push(delta.product_id.clone());
        api_endpoints.push(delta.api_endpoint.clone());
        client_ips.push(parsed_ip);
        allowed_flags.push(delta.denied_count == 0);
        reasons.push(reason);
        degraded_flags.push(delta.degraded);
        counts.push(total);
    }

    if tenant_ids.is_empty() {
        return Ok(());
    }

    sqlx::query(
        r"
        INSERT INTO rate_limit_events (
            tenant_id, gateway_id, request_id, policy_version, scope, scope_key,
            product_id, api_endpoint, client_ip, allowed, reason, local_decision,
            degraded, current_count, limit_value, retry_after_ms
        )
        SELECT
            u.tenant_id, u.gateway_id, NULL, u.policy_version, 'tenant', u.scope_key,
            u.product_id, u.api_endpoint, u.client_ip, u.allowed, u.reason, true,
            u.degraded, u.total_count, u.total_count, 0
        FROM UNNEST(
            $1::uuid[],
            $2::text[],
            $3::bigint[],
            $4::text[],
            $5::text[],
            $6::text[],
            $7::inet[],
            $8::bool[],
            $9::text[],
            $10::bool[],
            $11::bigint[]
        ) AS u(
            tenant_id, gateway_id, policy_version, scope_key,
            product_id, api_endpoint, client_ip, allowed,
            reason, degraded, total_count
        )
        ",
    )
    .bind(&tenant_ids)
    .bind(&gateway_ids)
    .bind(&policy_versions)
    .bind(&scope_keys)
    .bind(&product_ids)
    .bind(&api_endpoints)
    .bind(&client_ips)
    .bind(&allowed_flags)
    .bind(&reasons)
    .bind(&degraded_flags)
    .bind(&counts)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

type CounterKey = (Uuid, String, chrono::DateTime<Utc>, i32);
struct AggregatedCounter {
    policy_version: i64,
    allowed_count: i64,
    denied_count: i64,
    bytes_in: i64,
    bytes_out: i64,
}
impl RateLimitTarpcService {
    #[must_use]
    pub fn new(
        db: Arc<PgPool>,
        audit_enabled: bool,
        audit_sink: Option<mazbri_audit::sink::AuditSink>,
    ) -> Self {
        Self {
            pool: db,
            audit_enabled,
            audit_sink,
        }
    }

    /// 控制命令确认写入统一审计（异步入队，不阻塞 RPC）。
    fn audit_command_ack(
        &self,
        gateway_id: &str,
        command_ids: &[String],
        outcome: Result<i64, (&str, &str)>,
    ) {
        if let Some(sink) = &self.audit_sink {
            sink.enqueue(crate::audit::command_ack_record(
                gateway_id,
                command_ids,
                outcome,
            ));
        }
    }
}

impl RateLimitService for RateLimitTarpcService {
    async fn get_policy_snapshots(
        self,
        _context: Context,
        request: GetPolicySnapshotsRequest,
    ) -> GetPolicySnapshotsResponse {
        let page_size = i64::from(request.page_size.clamp(1, 1000));
        let mut builder = QueryBuilder::<Postgres>::new(
            r"
            SELECT id, tenant_id, version, enabled, enforcement_mode, stale_policy,
                   max_snapshot_age_secs, tenant_qps, tenant_burst,
                   client_ip_qps, client_ip_burst, bandwidth_bps,
                   api_limits, description, updated_by, created_at, updated_at
            FROM rate_limit_tenant_policies
            WHERE version > 
            ",
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

        let validated_deltas = match validate_usage_deltas(&request.deltas) {
            Ok(validated) => validated,
            Err(response) => return response,
        };
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
        let persisted = async {
            insert_usage_deltas(&mut tx, &validated_deltas).await?;
            upsert_global_counters(&mut tx, &validated_deltas).await?;
            if self.audit_enabled {
                insert_usage_audit_events(&mut tx, &validated_deltas).await?;
            }
            tx.commit().await
        }
        .await;
        if let Err(error) = persisted {
            return IngestUsageDeltasResponse {
                success: false,
                ingested: 0,
                error: Some(rpc_database_error(error)),
            };
        }

        let ingested = i64::saturating_from(request.deltas.len());
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
        let page_size = i64::from(request.page_size.clamp(1, 1000));
        let mut builder = QueryBuilder::<Postgres>::new(
            r"
            SELECT id, tenant_id, policy_version, command_type, command_payload,
                   effective_at, expires_at, created_by, created_at, acknowledged_by
            FROM rate_limit_control_commands
            WHERE effective_at <= NOW()
              AND (expires_at IS NULL OR expires_at > NOW())
            ",
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
                    self.audit_command_ack(
                        &request.gateway_id,
                        &request.command_ids,
                        Err(("client_error", "invalid command id")),
                    );
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
            r"
            UPDATE rate_limit_control_commands
            SET acknowledged_by = COALESCE(acknowledged_by, '{}'::jsonb)
                || jsonb_build_object($2, to_jsonb(NOW()))
            WHERE id = ANY($1)
            ",
        )
        .bind(&command_uuids)
        .bind(&request.gateway_id)
        .execute(&*self.pool)
        .await;
        let acknowledged = match result {
            Ok(result) => i64::saturating_from(result.rows_affected()),
            Err(error) => {
                self.audit_command_ack(
                    &request.gateway_id,
                    &request.command_ids,
                    Err(("db_error", "database error")),
                );
                return AckControlCommandsResponse {
                    success: false,
                    acknowledged: 0,
                    error: Some(rpc_database_error(error)),
                };
            }
        };
        self.audit_command_ack(&request.gateway_id, &request.command_ids, Ok(acknowledged));
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

    fn health(
        self,
        _context: Context,
    ) -> impl std::future::Future<Output = mazbri_rpc::HealthResponse> {
        std::future::ready(mazbri_rpc::HealthResponse {
            status: "ok".to_string(),
            service: "ratelimit-service".to_string(),
            timestamp: Utc::now().timestamp(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn api_limits_object_form_only() {
        let parsed = parse_api_limits_json(&json!({
            "/a": { "qps": 10, "burst": 20 },
            "/b": 5,
            "/c": { "qps": 0, "burst": 0 },
            "/d": "nope"
        }));
        assert_eq!(parsed.len(), 1);
        let rule = parsed.get("/a").expect("/a");
        assert_eq!(rule.qps, Some(10));
        assert_eq!(rule.burst, Some(20));
    }

    #[test]
    fn parse_client_ip_inet_maps_invalid_strings_to_none_and_canonicalizes_mapped_v6() {
        for bad in [
            None,
            Some(""),
            Some("   "),
            Some("not-an-ip"),
            Some("unknown"),
            Some("10.0.0.0/8"),
            Some("999.999.999.999"),
        ] {
            assert_eq!(parse_client_ip_inet(bad), None, "bad={bad:?}");
        }

        assert_eq!(
            parse_client_ip_inet(Some("198.51.100.7")),
            Some("198.51.100.7/32".parse().expect("v4 host"))
        );
        assert_eq!(
            parse_client_ip_inet(Some("::ffff:198.51.100.7")),
            Some("198.51.100.7/32".parse().expect("mapped v4 host"))
        );
        assert_eq!(
            parse_client_ip_inet(Some("2001:db8::42")),
            Some("2001:db8::42/128".parse().expect("v6 host"))
        );
    }

    #[test]
    fn usage_delta_unnest_batch_handles_6000_rows_and_malformed_client_ips() {
        let tenant_uuid = Uuid::new_v4();
        let tenant_str = tenant_uuid.to_string();
        let deltas: Vec<UsageDelta> = (0..6_000_i64)
            .map(|idx| UsageDelta {
                gateway_id: "gw-1".to_string(),
                tenant_id: tenant_str.clone(),
                policy_version: 3,
                product_id: "prod-1".to_string(),
                api_endpoint: format!("/v1/items/{idx}"),
                client_ip: match idx % 4 {
                    0 => Some("not-an-ip".to_string()),
                    1 => Some("198.51.100.12".to_string()),
                    2 => Some("::ffff:192.0.2.9".to_string()),
                    _ => None,
                },
                window_start_unix: 1_760_000_000 + idx,
                window_secs: 60,
                allowed_count: 10,
                denied_count: i64::from(idx % 2 == 0),
                bytes_in: 128,
                bytes_out: 256,
                snapshot_age_ms: 5,
                degraded: false,
                degrade_reason: None,
            })
            .collect();

        let validated = validate_usage_deltas(&deltas).expect("validate 6000 deltas");
        assert_eq!(validated.len(), 6_000);

        let batch = UsageDeltaUnnestBatch::from_validated(&validated);
        assert_eq!(batch.tenant_ids.len(), 6_000);
        assert_eq!(batch.gateway_ids.len(), 6_000);
        assert_eq!(batch.client_ips.len(), 6_000);
        const {
            assert!(UsageDeltaUnnestBatch::SQL_BIND_PARAM_COUNT < 65_535);
        }
        assert_eq!(batch.client_ips[0], None);
        assert_eq!(
            batch.client_ips[1],
            Some("198.51.100.12/32".parse().expect("v4"))
        );
        assert_eq!(
            batch.client_ips[2],
            Some("192.0.2.9/32".parse().expect("mapped v4"))
        );
        assert_eq!(batch.client_ips[3], None);
    }
}
