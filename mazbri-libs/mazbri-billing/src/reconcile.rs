//! L3 billing reconciliation: gateway access log (`ClickHouse`) vs `usage_records`.
//!
//! For a closed window `[start, end)` (end <= now - `close_lag_secs`), processed in
//! `chunk_secs` slices:
//! * forward: every access-log row with `billable = 1` whose `event_id` has no
//!   `usage_records` row is cross-checked against `usage_outbox`:
//!   - `status = 'dead_letter'` is classified as an outbox dead-letter anomaly (`outbox_dead_letter`)
//!     awaiting operator action rather than a missing pipeline event;
//!   - already queued outbox rows (`pending` / `processing` / `failed`) are skipped;
//!   - genuinely absent events are rebuilt as `UsageOutboxEvent` and inserted into `usage_outbox`
//!     with `ON CONFLICT (event_id) DO NOTHING`.
//! * reverse: `usage_records` created in the slice whose `event_id` is absent from the access
//!   log (slice widened by `skew_secs`) are recorded in `billing_reconcile_anomalies`
//!   (no automatic action).
use crate::calculator::CalculationResult;
use crate::service::UsageOutboxEvent;
use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{error, info, warn};
use uuid::Uuid;

const RECLAIM_STALE_RUNNING_SQL: &str = "UPDATE billing_reconcile_runs \
     SET status = 'requested', started_at = NULL, \
         error = COALESCE(error, 'reclaimed stale running (>15m)') \
     WHERE status = 'running' \
       AND started_at < NOW() - INTERVAL '15 minutes'";

const ALLOWED_CH_TABLE_NAME: &str = "request_logs";
const FORBIDDEN_CH_SCHEMAS: &[&str] = &["system", "information_schema", "INFORMATION_SCHEMA"];

/// Validate that `table` is a safe `ClickHouse` identifier targeting the whitelisted
/// `request_logs` table (`request_logs` or `<schema>.request_logs`).
///
/// # Errors
///
/// Returns an error string when `table` is empty, contains non-identifier characters,
/// targets a forbidden schema, or does not reference `request_logs`.
pub fn validate_clickhouse_table_ident(table: &str) -> Result<&str, String> {
    let valid_ident = |seg: &str| -> bool {
        !seg.is_empty()
            && seg.len() <= 64
            && seg
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            && seg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    };

    if table.is_empty() || table.len() > 128 || table.trim() != table {
        return Err("clickhouse_table must be a non-empty identifier without whitespace".into());
    }
    let parts: Vec<&str> = table.split('.').collect();
    match parts.as_slice() {
        [tbl] => {
            if !valid_ident(tbl) || *tbl != ALLOWED_CH_TABLE_NAME {
                return Err(format!(
                    "clickhouse_table '{table}' is not in identifier whitelist ({ALLOWED_CH_TABLE_NAME})"
                ));
            }
        }
        [schema, tbl] => {
            if !valid_ident(schema)
                || FORBIDDEN_CH_SCHEMAS
                    .iter()
                    .any(|s| schema.eq_ignore_ascii_case(s))
                || !valid_ident(tbl)
                || *tbl != ALLOWED_CH_TABLE_NAME
            {
                return Err(format!(
                    "clickhouse_table '{table}' is not in identifier whitelist (<schema>.{ALLOWED_CH_TABLE_NAME})"
                ));
            }
        }
        _ => {
            return Err(format!(
                "clickhouse_table '{table}' must be '{ALLOWED_CH_TABLE_NAME}' or '<schema>.{ALLOWED_CH_TABLE_NAME}'"
            ));
        }
    }
    Ok(table)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForwardDisposition {
    AlreadyRecorded,
    OutboxDeadLetter,
    OutboxInFlight,
    MissingReenqueue,
}

fn classify_forward_event(
    in_usage_records: bool,
    outbox_status: Option<&str>,
) -> ForwardDisposition {
    if in_usage_records {
        return ForwardDisposition::AlreadyRecorded;
    }
    match outbox_status {
        Some("dead_letter") => ForwardDisposition::OutboxDeadLetter,
        Some("pending" | "processing" | "failed") => ForwardDisposition::OutboxInFlight,
        // Absent from outbox (or anomalously marked 'processed' without usage_records): re-enqueue
        _ => ForwardDisposition::MissingReenqueue,
    }
}

/// `[reconcile]` in the billing config.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ReconcileConfig {
    pub enabled: bool,
    /// Scheduled window length and cadence.
    pub window_secs: i64,
    /// A window is only reconciled once `now - close_lag_secs >= window_end`.
    pub close_lag_secs: i64,
    pub chunk_secs: i64,
    pub skew_secs: i64,
    /// How often the job looks for requested runs / due windows.
    pub poll_secs: u64,
    pub clickhouse_url: String,
    pub clickhouse_user: String,
    pub clickhouse_password: String,
    pub clickhouse_table: String,
}

impl Default for ReconcileConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            window_secs: 3600,
            close_lag_secs: 600,
            chunk_secs: 60,
            skew_secs: 300,
            poll_secs: 5,
            clickhouse_url: String::new(),
            clickhouse_user: "default".into(),
            clickhouse_password: String::new(),
            clickhouse_table: "mazbri_logs.request_logs".into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct AccessRow {
    event_id: String,
    request_id: String,
    tenant_id: String,
    product_id: String,
    method: String,
    path: String,
    status_code: u16,
    request_bytes: u64,
    response_bytes: u64,
    duration_ms: u64,
    price: String,
    billing_code: i16,
}

#[derive(Deserialize)]
struct Id {
    event_id: String,
}

#[derive(Default, Debug)]
struct RunStats {
    billable: i64,
    missing: i64,
    reenqueued: i64,
    dead_letter: i64,
    reverse: i64,
}

pub struct ReconcileJob {
    cfg: ReconcileConfig,
    db: PgPool,
    http: reqwest::Client,
}

/// Floor `t` to a multiple of `step` seconds.
#[must_use]
pub fn align_down(t: i64, step: i64) -> i64 {
    let step = step.max(1);
    t.div_euclid(step) * step
}

/// Next scheduled window after `last_end` (or the latest closed window), if closed at `now`.
#[must_use]
pub fn next_due_window(
    last_end: Option<i64>,
    now: i64,
    window: i64,
    lag: i64,
) -> Option<(i64, i64)> {
    let window = window.max(60);
    let closed_until = align_down(now - lag, window);
    let start = last_end.unwrap_or(closed_until - window);
    let end = start + window;
    (end <= closed_until).then_some((start, end))
}

impl ReconcileJob {
    #[must_use]
    pub fn new(cfg: ReconcileConfig, db: PgPool) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();
        Self { cfg, db, http }
    }

    #[must_use]
    pub fn spawn(self, mut shutdown: watch::Receiver<bool>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            info!(
                window_secs = self.cfg.window_secs,
                close_lag_secs = self.cfg.close_lag_secs,
                "billing reconcile job started"
            );
            loop {
                if let Err(e) = self.tick().await {
                    warn!(error = %e, "reconcile tick failed");
                }
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))) => {}
                    _ = shutdown.changed() => break,
                }
            }
        })
    }

    async fn tick(&self) -> Result<(), String> {
        validate_clickhouse_table_ident(&self.cfg.clickhouse_table)?;
        self.reclaim_stale_running().await?;
        self.schedule_due().await?;
        while let Some((id, start, end)) = self.claim().await? {
            info!(run_id = id, %start, %end, "reconcile run started");
            match self.run_window(id, start, end).await {
                Ok(s) => {
                    sqlx::query(
                        "UPDATE billing_reconcile_runs \
                         SET status='done', billable_count=$2, missing_count=$3, \
                             reenqueued_count=$4, reverse_count=$5, finished_at=NOW() \
                         WHERE id=$1 AND status='running'",
                    )
                    .bind(id)
                    .bind(s.billable)
                    .bind(s.missing)
                    .bind(s.reenqueued)
                    .bind(s.reverse)
                    .execute(&self.db)
                    .await
                    .map_err(|e| e.to_string())?;
                    info!(
                        run_id = id,
                        billable = s.billable,
                        missing = s.missing,
                        reenqueued = s.reenqueued,
                        dead_letter = s.dead_letter,
                        reverse = s.reverse,
                        "reconcile run done"
                    );
                }
                Err(e) => {
                    error!(run_id = id, error = %e, "reconcile run failed");
                    sqlx::query(
                        "UPDATE billing_reconcile_runs \
                         SET status='failed', error=$2, finished_at=NOW() \
                         WHERE id=$1 AND status='running'",
                    )
                    .bind(id)
                    .bind(&e)
                    .execute(&self.db)
                    .await
                    .map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }

    async fn reclaim_stale_running(&self) -> Result<u64, String> {
        let res = sqlx::query(RECLAIM_STALE_RUNNING_SQL)
            .execute(&self.db)
            .await
            .map_err(|e| e.to_string())?;
        let reclaimed = res.rows_affected();
        if reclaimed > 0 {
            warn!(
                reclaimed,
                "reclaimed stale billing_reconcile_runs stuck in running (>15m)"
            );
        }
        Ok(reclaimed)
    }

    async fn schedule_due(&self) -> Result<(), String> {
        let last: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT max(window_end) FROM billing_reconcile_runs WHERE trigger = 'schedule'",
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| e.to_string())?;
        let now = Utc::now().timestamp();
        if let Some((s, e)) = next_due_window(
            last.map(|t| t.timestamp()),
            now,
            self.cfg.window_secs,
            self.cfg.close_lag_secs,
        ) {
            sqlx::query("INSERT INTO billing_reconcile_runs (window_start, window_end, trigger) VALUES ($1, $2, 'schedule') ON CONFLICT DO NOTHING")
                .bind(ts(s)).bind(ts(e)).execute(&self.db).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    async fn claim(&self) -> Result<Option<(i64, DateTime<Utc>, DateTime<Utc>)>, String> {
        sqlx::query_as(
            "UPDATE billing_reconcile_runs SET status='running', started_at=NOW()
             WHERE id = (SELECT id FROM billing_reconcile_runs WHERE status='requested'
                         ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED)
             RETURNING id, window_start, window_end",
        )
        .fetch_optional(&self.db)
        .await
        .map_err(|e| e.to_string())
    }

    async fn run_window(
        &self,
        run_id: i64,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<RunStats, String> {
        let mut stats = RunStats::default();
        let chunk = ChronoDuration::seconds(self.cfg.chunk_secs.max(1));
        let mut s = start;
        while s < end {
            let e = (s + chunk).min(end);
            self.forward(run_id, s, e, &mut stats).await?;
            self.reverse(run_id, s, e, &mut stats).await?;
            s = e;
        }
        Ok(stats)
    }

    async fn ch_query<T: for<'de> Deserialize<'de>>(&self, sql: String) -> Result<Vec<T>, String> {
        let body = self
            .http
            .post(&self.cfg.clickhouse_url)
            .basic_auth(
                &self.cfg.clickhouse_user,
                Some(&self.cfg.clickhouse_password),
            )
            .body(sql)
            .send()
            .await
            .map_err(|e| format!("clickhouse: {e}"))?;
        let status = body.status();
        let text = body.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!(
                "clickhouse {status}: {}",
                text.chars().take(300).collect::<String>()
            ));
        }
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(|e| format!("clickhouse row: {e}")))
            .collect()
    }

    async fn forward(
        &self,
        run_id: i64,
        s: DateTime<Utc>,
        e: DateTime<Utc>,
        stats: &mut RunStats,
    ) -> Result<(), String> {
        let table = validate_clickhouse_table_ident(&self.cfg.clickhouse_table)?;
        let sql = format!(
            "SELECT event_id, request_id, tenant_id, product_id, method, path, status_code, request_bytes, response_bytes, duration_ms, price, billing_code \
             FROM {table} FINAL WHERE service = 'mazbri-gateway' AND billable = 1 AND event_id != '' \
             AND timestamp >= toDateTime64('{s}', 9, 'UTC') AND timestamp < toDateTime64('{e}', 9, 'UTC') FORMAT JSONEachRow",
            s = ch_ts(s),
            e = ch_ts(e)
        );
        let rows: Vec<AccessRow> = self.ch_query(sql).await?;
        stats.billable += i64::try_from(rows.len()).unwrap_or(i64::MAX);
        if rows.is_empty() {
            return Ok(());
        }
        let ids: Vec<Uuid> = rows
            .iter()
            .filter_map(|r| Uuid::parse_str(&r.event_id).ok())
            .collect();
        let found: Vec<Uuid> =
            sqlx::query_scalar("SELECT event_id FROM usage_records WHERE event_id = ANY($1)")
                .bind(&ids)
                .fetch_all(&self.db)
                .await
                .map_err(|e| e.to_string())?;
        let found: HashSet<Uuid> = found.into_iter().collect();

        let missing_candidates: Vec<Uuid> =
            ids.into_iter().filter(|id| !found.contains(id)).collect();
        let outbox_statuses: HashMap<Uuid, String> = if missing_candidates.is_empty() {
            HashMap::new()
        } else {
            let outbox_rows: Vec<(Uuid, String)> = sqlx::query_as(
                "SELECT event_id, status::text FROM usage_outbox WHERE event_id = ANY($1)",
            )
            .bind(&missing_candidates)
            .fetch_all(&self.db)
            .await
            .map_err(|e| e.to_string())?;
            outbox_rows.into_iter().collect()
        };

        for r in rows {
            let Some(event) = rebuild_event(&r) else {
                warn!(event_id = %r.event_id, "access row not rebuildable; skipped");
                continue;
            };
            match classify_forward_event(
                found.contains(&event.event_id),
                outbox_statuses.get(&event.event_id).map(String::as_str),
            ) {
                ForwardDisposition::AlreadyRecorded | ForwardDisposition::OutboxInFlight => {}
                ForwardDisposition::OutboxDeadLetter => {
                    stats.dead_letter += 1;
                    warn!(
                        event_id = %event.event_id,
                        tenant_id = %event.tenant_id,
                        "reconcile: billable event in usage_outbox dead_letter (awaiting manual resolution)"
                    );
                    sqlx::query(
                        "INSERT INTO billing_reconcile_anomalies (run_id, kind, event_id, usage_record_id) \
                         VALUES ($1, 'outbox_dead_letter', $2, NULL) \
                         ON CONFLICT (kind, event_id) DO NOTHING",
                    )
                    .bind(run_id)
                    .bind(event.event_id)
                    .execute(&self.db)
                    .await
                    .map_err(|e| e.to_string())?;
                }
                ForwardDisposition::MissingReenqueue => {
                    stats.missing += 1;
                    let q = crate::repository::queries::outbox_insert_query(&event)
                        .map_err(|e| e.to_string())?;
                    let n = q
                        .execute(&self.db)
                        .await
                        .map_err(|e| e.to_string())?
                        .rows_affected();
                    stats.reenqueued += i64::try_from(n).unwrap_or(0);
                    if n > 0 {
                        warn!(
                            event_id = %event.event_id,
                            tenant_id = %event.tenant_id,
                            amount = %event.billing_result.amount,
                            "reconcile: billable request missing in billing; re-enqueued to usage_outbox"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    async fn reverse(
        &self,
        run_id: i64,
        s: DateTime<Utc>,
        e: DateTime<Utc>,
        stats: &mut RunStats,
    ) -> Result<(), String> {
        let table = validate_clickhouse_table_ident(&self.cfg.clickhouse_table)?;
        let recs: Vec<(i64, Uuid)> = sqlx::query_as(
            "SELECT id, event_id FROM usage_records WHERE recorded_at >= $1 AND recorded_at < $2 AND event_id IS NOT NULL",
        )
        .bind(s)
        .bind(e)
        .fetch_all(&self.db)
        .await
        .map_err(|e| e.to_string())?;
        if recs.is_empty() {
            return Ok(());
        }
        let skew = ChronoDuration::seconds(self.cfg.skew_secs.max(0));
        let sql = format!(
            "SELECT event_id FROM {table} WHERE service = 'mazbri-gateway' AND event_id != '' \
             AND timestamp >= toDateTime64('{s}', 9, 'UTC') AND timestamp < toDateTime64('{e}', 9, 'UTC') FORMAT JSONEachRow",
            s = ch_ts(s - skew),
            e = ch_ts(e + skew)
        );
        let seen: HashSet<String> = self
            .ch_query::<Id>(sql)
            .await?
            .into_iter()
            .map(|r| r.event_id)
            .collect();
        for (usage_id, event_id) in recs {
            if seen.contains(&event_id.to_string()) {
                continue;
            }
            stats.reverse += 1;
            sqlx::query("INSERT INTO billing_reconcile_anomalies (run_id, kind, event_id, usage_record_id) VALUES ($1, 'missing_in_access_log', $2, $3) ON CONFLICT (kind, event_id) DO NOTHING")
                .bind(run_id).bind(event_id).bind(usage_id)
                .execute(&self.db).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn ts(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).single().unwrap_or_else(Utc::now)
}

fn ch_ts(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%d %H:%M:%S%.6f").to_string()
}

fn rebuild_event(r: &AccessRow) -> Option<UsageOutboxEvent> {
    let price = Decimal::from_str(&r.price).ok()?;
    if price < Decimal::ZERO {
        return None;
    }
    Some(UsageOutboxEvent {
        event_id: Uuid::parse_str(&r.event_id).ok()?,
        request_id: Uuid::parse_str(&r.request_id).unwrap_or_else(|_| Uuid::nil()),
        tenant_id: Uuid::parse_str(&r.tenant_id).ok()?,
        account_id: None,
        product_id: Some(r.product_id.clone()),
        api_endpoint: r.path.clone(),
        http_method: r.method.clone(),
        response_status: r.status_code,
        request_size: i64::try_from(r.request_bytes).unwrap_or(0),
        response_size: i64::try_from(r.response_bytes).unwrap_or(0),
        billing_service_duration_ms: i32::try_from(r.duration_ms).unwrap_or(i32::MAX),
        billing_result: CalculationResult {
            amount: price,
            units: 1,
            unit_price: price,
            description: format!(
                "reconcile backfill request_id={} endpoint={}",
                r.request_id, r.path
            ),
        },
        billing_code: Some(r.billing_code),
        trace_parent: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_window_only_when_closed() {
        let now = 10 * 3600 + 15 * 60;
        assert_eq!(
            next_due_window(None, now, 3600, 600),
            Some((9 * 3600, 10 * 3600))
        );
        assert_eq!(next_due_window(Some(10 * 3600), now, 3600, 600), None);
        assert_eq!(
            next_due_window(Some(8 * 3600), now, 3600, 600),
            Some((8 * 3600, 9 * 3600))
        );
    }

    #[test]
    fn rebuild_from_access_row() {
        let r = AccessRow {
            event_id: "8b9ad0d6-6d4f-4bd5-9c1e-0d7f6c3f1a11".into(),
            request_id: "x".into(),
            tenant_id: "fc391546-7925-4139-b06e-6cfe9f3c8a70".into(),
            product_id: "8000000001".into(),
            method: "POST".into(),
            path: "/verify".into(),
            status_code: 200,
            request_bytes: 1,
            response_bytes: 2,
            duration_ms: 3,
            price: "0.001".into(),
            billing_code: 1,
        };
        let e = rebuild_event(&r).expect("rebuild");
        assert_eq!(
            e.billing_result.amount,
            Decimal::from_str("0.001").expect("dec")
        );
        assert_eq!(e.request_id, Uuid::nil());
        assert!(e.account_id.is_none());
    }

    #[test]
    fn clickhouse_table_whitelist_rejects_injection_and_non_request_logs() {
        assert_eq!(
            validate_clickhouse_table_ident("mazbri_logs.request_logs").expect("default"),
            "mazbri_logs.request_logs"
        );
        assert_eq!(
            validate_clickhouse_table_ident("request_logs").expect("unqualified"),
            "request_logs"
        );
        assert_eq!(
            validate_clickhouse_table_ident("audit_ch_1.request_logs").expect("custom db"),
            "audit_ch_1.request_logs"
        );

        for bad in [
            "",
            " mazbri_logs.request_logs",
            "mazbri_logs.request_logs; DROP TABLE x",
            "mazbri_logs.request_logs --",
            "system.request_logs",
            "system.users",
            "mazbri_logs.other_table",
            "a.b.request_logs",
            "1bad.request_logs",
            "mazbri-logs.request_logs",
        ] {
            assert!(
                validate_clickhouse_table_ident(bad).is_err(),
                "expected '{bad}' to be rejected"
            );
        }
    }

    #[test]
    fn forward_disposition_separates_dead_letter_and_inflight_from_missing() {
        assert_eq!(
            classify_forward_event(true, Some("dead_letter")),
            ForwardDisposition::AlreadyRecorded
        );
        assert_eq!(
            classify_forward_event(false, Some("dead_letter")),
            ForwardDisposition::OutboxDeadLetter
        );
        assert_eq!(
            classify_forward_event(false, Some("pending")),
            ForwardDisposition::OutboxInFlight
        );
        assert_eq!(
            classify_forward_event(false, Some("processing")),
            ForwardDisposition::OutboxInFlight
        );
        assert_eq!(
            classify_forward_event(false, Some("failed")),
            ForwardDisposition::OutboxInFlight
        );
        assert_eq!(
            classify_forward_event(false, None),
            ForwardDisposition::MissingReenqueue
        );
        assert_eq!(
            classify_forward_event(false, Some("processed")),
            ForwardDisposition::MissingReenqueue
        );
    }

    #[test]
    fn stale_running_reclaim_uses_15_minute_window() {
        assert!(RECLAIM_STALE_RUNNING_SQL.contains("WHERE status = 'running'"));
        assert!(RECLAIM_STALE_RUNNING_SQL.contains("INTERVAL '15 minutes'"));
        assert!(RECLAIM_STALE_RUNNING_SQL.contains("SET status = 'requested'"));
    }
}
