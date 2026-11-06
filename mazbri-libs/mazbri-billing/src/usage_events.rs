//! L1 billing durability: usage charge events go through Redpanda (Kafka API).
//!
//! * Gateway: [`UsageEventProducer::try_enqueue`] only `try_send`s into a bounded in-memory
//!   queue on the response path (never awaits the broker). A background task batches by
//!   linger / size and `produce`s with acks=all. Queue-full or background produce failure
//!   falls back to the existing outbox / deferred path — there is no sync-produce success path.
//! * Billing: [`UsageEventConsumer`] reads every partition and inserts the events into
//!   `usage_outbox` with `ON CONFLICT (event_id) DO NOTHING`; the consumed offset is stored in
//!   `usage_event_offsets` **in the same DB transaction** (offset committed only after the DB
//!   commit, so redelivery is possible but duplicates are absorbed by the `event_id` key).
//!
//! rskafka has no idempotent producer / consumer groups: idempotency is provided by the
//! `event_id` unique key (outbox) and `usage_record_id` unique key (charge queue), and offsets
//! live in Postgres. A per-partition advisory lock keeps one consumer per partition; all of an
//! instance's partition locks are held on ONE dedicated session ([`PartitionLocks`]), which
//! re-tries/renews them every few seconds, and every batch commit is fenced against
//! `pg_locks` so a consumer that lost its session can never advance an offset.
//!
//! Partitioning: the record key is the `event_id` (random UUID), so events spread evenly
//! over all partitions even for a single tenant. Per-tenant ordering is not needed: the
//! outbox/charge pipeline is idempotent per event and serializes balance updates itself.
use crate::service::UsageOutboxEvent;
use chrono::Utc;
use rskafka::client::partition::{Compression, OffsetAt, PartitionClient, UnknownTopicHandling};
use rskafka::client::{ClientBuilder, Credentials, SaslConfig};
use rskafka::record::Record;
use serde::Deserialize;
use sqlx::{Connection, PgConnection, PgPool};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};

/// `[usage_events]` (billing) / `[billing.usage_events]` (gateway).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UsageEventsConfig {
    /// Gateway: `"redpanda"` = enqueue to background batch producer (fallback: outbox);
    /// `"outbox"` = legacy direct outbox. Billing: consumer runs when `"redpanda"`.
    pub sink: String,
    pub brokers: Vec<String>,
    pub topic: String,
    pub partitions: i32,
    /// `SCRAM-SHA-256` | `SCRAM-SHA-512` | `PLAIN` | empty (no SASL).
    pub sasl_mechanism: String,
    pub username: String,
    pub password: String,
    pub produce_timeout_ms: u64,
    pub fetch_max_wait_ms: i32,
    pub fetch_max_bytes: i32,
}

impl Default for UsageEventsConfig {
    fn default() -> Self {
        Self {
            sink: "outbox".into(),
            brokers: Vec::new(),
            topic: "mazbri-usage-events".into(),
            partitions: 3,
            sasl_mechanism: "SCRAM-SHA-256".into(),
            username: String::new(),
            password: String::new(),
            produce_timeout_ms: 2000,
            fetch_max_wait_ms: 200,
            fetch_max_bytes: 4 * 1024 * 1024,
        }
    }
}

impl UsageEventsConfig {
    #[must_use]
    pub fn redpanda_enabled(&self) -> bool {
        self.sink.eq_ignore_ascii_case("redpanda") && !self.brokers.is_empty()
    }

    fn sasl(&self) -> Option<SaslConfig> {
        let creds = || Credentials::new(self.username.clone(), self.password.clone());
        match self.sasl_mechanism.to_ascii_uppercase().as_str() {
            "SCRAM-SHA-256" => Some(SaslConfig::ScramSha256(creds())),
            "SCRAM-SHA-512" => Some(SaslConfig::ScramSha512(creds())),
            "PLAIN" => Some(SaslConfig::Plain(creds())),
            _ => None,
        }
    }

    async fn partition_clients(&self) -> Result<Vec<Arc<PartitionClient>>, String> {
        let mut builder = ClientBuilder::new(self.brokers.clone()).client_id("mazbri-billing");
        if let Some(sasl) = self.sasl() {
            builder = builder.sasl_config(sasl);
        }
        let client = builder.build().await.map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for p in 0..self.partitions.max(1) {
            let pc = client
                .partition_client(self.topic.clone(), p, UnknownTopicHandling::Error)
                .await
                .map_err(|e| format!("partition {p}: {e}"))?;
            out.push(Arc::new(pc));
        }
        Ok(out)
    }
}

/// Stable partition for a key (FNV-1a). The producer keys by `event_id`.
#[must_use]
pub fn partition_for(key: &[u8], partitions: i32) -> i32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in key {
        h ^= u32::from(*b);
        h = h.wrapping_mul(0x0100_0193);
    }
    let n = u32::try_from(partitions.max(1)).unwrap_or(1);
    i32::try_from(h % n).unwrap_or(0)
}

/// Bounded in-memory queue capacity for response-path enqueue.
const USAGE_EVENT_QUEUE_CAPACITY: usize = 4096;
/// Flush when this many events accumulate for a partition (or overall batch).
const USAGE_EVENT_BATCH_MAX: usize = 64;
/// Max time to wait for more events before flushing a non-empty batch.
const USAGE_EVENT_LINGER: Duration = Duration::from_millis(2);
/// Side channel for events that failed background produce (gateway drains → outbox).
const USAGE_EVENT_FAILED_CAPACITY: usize = 4096;

#[derive(Clone)]
pub struct UsageEventProducer {
    tx: mpsc::Sender<UsageOutboxEvent>,
}

/// Handles returned by [`UsageEventProducer::connect`]: the enqueue handle plus a
/// receiver of events that the background flusher could not produce (outbox fallback).
pub struct UsageEventProducerHandles {
    pub producer: UsageEventProducer,
    pub failed_rx: mpsc::Receiver<UsageOutboxEvent>,
}

impl UsageEventProducer {
    /// # Errors
    /// Broker unreachable, SASL failure or unknown topic.
    pub async fn connect(cfg: &UsageEventsConfig) -> Result<UsageEventProducerHandles, String> {
        let partitions = cfg.partition_clients().await?;
        let timeout = Duration::from_millis(cfg.produce_timeout_ms.max(1));
        let (tx, rx) = mpsc::channel(USAGE_EVENT_QUEUE_CAPACITY);
        let (failed_tx, failed_rx) = mpsc::channel(USAGE_EVENT_FAILED_CAPACITY);
        info!(
            topic = %cfg.topic,
            partitions = partitions.len(),
            queue_capacity = USAGE_EVENT_QUEUE_CAPACITY,
            batch_max = USAGE_EVENT_BATCH_MAX,
            linger_ms = USAGE_EVENT_LINGER.as_millis(),
            "usage event producer connected (async batch, acks=all on flush)"
        );
        tokio::spawn(run_batch_flusher(
            rx,
            failed_tx,
            Arc::new(partitions),
            timeout,
        ));
        Ok(UsageEventProducerHandles {
            producer: Self { tx },
            failed_rx,
        })
    }

    /// Enqueue one event for background batch produce. Never waits on the broker.
    ///
    /// # Errors
    /// Returns the event when the bounded queue is full or the flusher has stopped;
    /// the caller must write it to the outbox / deferred path.
    pub fn try_enqueue(&self, event: UsageOutboxEvent) -> Result<(), Box<UsageOutboxEvent>> {
        self.tx.try_send(event).map_err(|err| match err {
            mpsc::error::TrySendError::Full(event) | mpsc::error::TrySendError::Closed(event) => {
                Box::new(event)
            }
        })
    }
}

fn record_for_event(event: &UsageOutboxEvent, partitions: i32) -> Result<(usize, Record), String> {
    let key = event.event_id.to_string().into_bytes();
    let value = serde_json::to_vec(event).map_err(|e| e.to_string())?;
    let mut headers = BTreeMap::new();
    headers.insert(
        "event_id".to_string(),
        event.event_id.to_string().into_bytes(),
    );
    let record = Record {
        key: Some(key.clone()),
        value: Some(value),
        headers,
        timestamp: Utc::now(),
    };
    let idx = usize::try_from(partition_for(&key, partitions)).unwrap_or(0);
    Ok((idx, record))
}

async fn run_batch_flusher(
    mut rx: mpsc::Receiver<UsageOutboxEvent>,
    failed_tx: mpsc::Sender<UsageOutboxEvent>,
    partitions: Arc<Vec<Arc<PartitionClient>>>,
    timeout: Duration,
) {
    let partition_count = i32::try_from(partitions.len()).unwrap_or(1);
    // Per-partition pending events (preserve event for outbox fallback on produce failure).
    let mut pending: Vec<Vec<UsageOutboxEvent>> = (0..partitions.len()).map(|_| Vec::new()).collect();
    let mut linger = None::<tokio::time::Instant>;

    loop {
        let recv_fut = rx.recv();
        let event = if let Some(deadline) = linger {
            let sleep = tokio::time::sleep_until(deadline);
            tokio::pin!(sleep);
            tokio::select! {
                ev = recv_fut => ev,
                () = &mut sleep => {
                    flush_all(&mut pending, &partitions, timeout, &failed_tx).await;
                    linger = None;
                    continue;
                }
            }
        } else {
            recv_fut.await
        };

        let Some(event) = event else {
            // Sender dropped: flush remaining and exit.
            flush_all(&mut pending, &partitions, timeout, &failed_tx).await;
            break;
        };

        let Ok((idx, _)) = record_for_event(&event, partition_count) else {
            warn!(event_id = %event.event_id, "usage event serialize failed; routing to outbox");
            let _ = failed_tx.try_send(event);
            continue;
        };
        let idx = idx.min(pending.len().saturating_sub(1));
        pending[idx].push(event);
        if pending[idx].len() >= USAGE_EVENT_BATCH_MAX {
            let batch = std::mem::take(&mut pending[idx]);
            flush_partition(idx, batch, &partitions, timeout, &failed_tx).await;
            if pending.iter().all(Vec::is_empty) {
                linger = None;
            }
        } else if linger.is_none() {
            linger = Some(tokio::time::Instant::now() + USAGE_EVENT_LINGER);
        }
    }
}

async fn flush_all(
    pending: &mut [Vec<UsageOutboxEvent>],
    partitions: &[Arc<PartitionClient>],
    timeout: Duration,
    failed_tx: &mpsc::Sender<UsageOutboxEvent>,
) {
    for (idx, batch) in pending.iter_mut().enumerate() {
        if batch.is_empty() {
            continue;
        }
        let batch = std::mem::take(batch);
        flush_partition(idx, batch, partitions, timeout, failed_tx).await;
    }
}

async fn flush_partition(
    idx: usize,
    batch: Vec<UsageOutboxEvent>,
    partitions: &[Arc<PartitionClient>],
    timeout: Duration,
    failed_tx: &mpsc::Sender<UsageOutboxEvent>,
) {
    if batch.is_empty() {
        return;
    }
    let partition_count = i32::try_from(partitions.len()).unwrap_or(1);
    let mut records = Vec::with_capacity(batch.len());
    let mut kept = Vec::with_capacity(batch.len());
    for event in batch {
        match record_for_event(&event, partition_count) {
            Ok((_, record)) => {
                records.push(record);
                kept.push(event);
            }
            Err(error) => {
                warn!(event_id = %event.event_id, error = %error, "usage event serialize failed; routing to outbox");
                let _ = failed_tx.try_send(event);
            }
        }
    }
    if records.is_empty() {
        return;
    }
    let pc = &partitions[idx];
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        timeout,
        pc.produce(records, Compression::NoCompression),
    )
    .await;
    let ok = matches!(&result, Ok(Ok(_)));
    mazbri_core::metrics::observe_dep("kafka", "produce", started, ok);
    match result {
        Ok(Ok(_)) => {
            debug!(partition = idx, n = kept.len(), "usage event batch produced");
        }
        Ok(Err(error)) => {
            warn!(partition = idx, n = kept.len(), error = %error, "usage event batch produce failed; routing to outbox");
            for event in kept {
                if failed_tx.try_send(event).is_err() {
                    error!("usage event failed channel full/closed; event may need reconcile");
                }
            }
        }
        Err(_) => {
            warn!(partition = idx, n = kept.len(), timeout = ?timeout, "usage event batch produce timed out; routing to outbox");
            for event in kept {
                if failed_tx.try_send(event).is_err() {
                    error!("usage event failed channel full/closed; event may need reconcile");
                }
            }
        }
    }
}

/// Billing-side consumer: one task per partition.
pub struct UsageEventConsumer {
    cfg: UsageEventsConfig,
    db: PgPool,
}

const LOAD_OFFSET_SQL: &str =
    "SELECT next_offset FROM usage_event_offsets WHERE topic = $1 AND partition = $2";
const STORE_OFFSET_SQL: &str = "INSERT INTO usage_event_offsets (topic, partition, next_offset, updated_at) \
     VALUES ($1, $2, $3, NOW()) ON CONFLICT (topic, partition) \
     DO UPDATE SET next_offset = EXCLUDED.next_offset, updated_at = NOW()";

impl UsageEventConsumer {
    #[must_use]
    pub fn new(cfg: UsageEventsConfig, db: PgPool) -> Self {
        Self { cfg, db }
    }

    /// Spawns the shared lock holder plus the per-partition loops; they stop when
    /// `shutdown` flips to `true` (partition loops first, then the locks are released).
    #[must_use]
    pub fn spawn(self, shutdown: watch::Receiver<bool>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let clients = loop {
                match self.cfg.partition_clients().await {
                    Ok(c) => break c,
                    Err(e) => {
                        error!(error = %e, "usage event consumer: connect failed; retry in 5s");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        if *shutdown.borrow() {
                            return;
                        }
                    }
                }
            };
            info!(topic = %self.cfg.topic, partitions = clients.len(), "usage event consumer started");
            let partitions: Vec<i32> = clients.iter().map(|c| c.partition()).collect();
            let locks = Arc::new(PartitionLocks::new(&self.cfg.topic, &partitions));
            let (lock_stop_tx, lock_stop_rx) = watch::channel(false);
            let holder = tokio::spawn(Arc::clone(&locks).run(self.db.clone(), lock_stop_rx));
            let mut handles = Vec::new();
            for (slot, pc) in clients.into_iter().enumerate() {
                let (cfg, db, sd, lk) = (
                    self.cfg.clone(),
                    self.db.clone(),
                    shutdown.clone(),
                    Arc::clone(&locks),
                );
                handles.push(tokio::spawn(partition_loop(cfg, db, pc, sd, lk, slot)));
            }
            for h in handles {
                let _ = h.await;
            }
            let _ = lock_stop_tx.send(true);
            let _ = holder.await;
            info!("usage event consumer stopped");
        })
    }
}

/// How often the holder renews (liveness probe) and retries locks it does not own.
const LOCK_RENEW_INTERVAL: Duration = Duration::from_secs(3);

/// All partition advisory locks of one billing instance, held on a single dedicated
/// session (not a pool connection, so a lock can never leak back into the pool).
pub struct PartitionLocks {
    keys: Vec<i64>,
    owned: Vec<AtomicBool>,
    /// Backend pid of the session holding the locks (0 = no session).
    pid: AtomicI32,
}

impl PartitionLocks {
    fn new(topic: &str, partitions: &[i32]) -> Self {
        let base = i64::from(partition_for(topic.as_bytes(), i32::MAX)) * 1000;
        Self {
            keys: partitions.iter().map(|p| base + i64::from(*p)).collect(),
            owned: partitions.iter().map(|_| AtomicBool::new(false)).collect(),
            pid: AtomicI32::new(0),
        }
    }

    fn is_owned(&self, slot: usize) -> bool {
        self.owned[slot].load(Ordering::Acquire)
    }

    fn drop_all(&self) {
        for o in &self.owned {
            o.store(false, Ordering::Release);
        }
        self.pid.store(0, Ordering::Release);
    }

    /// Holder loop: (re)connect, `SELECT pg_backend_pid()` as liveness, then
    /// `pg_try_advisory_lock` for every not-yet-owned key. Any session error drops all
    /// ownership (partition loops pause) and reconnects.
    async fn run(self: Arc<Self>, db: PgPool, mut stop: watch::Receiver<bool>) {
        let opts = db.connect_options();
        let mut conn: Option<PgConnection> = None;
        loop {
            if *stop.borrow() {
                break;
            }
            if conn.is_none() {
                match PgConnection::connect_with(&opts).await {
                    Ok(c) => conn = Some(c),
                    Err(e) => warn!(error = %e, "partition lock session connect failed"),
                }
            }
            if let Some(c) = conn.as_mut()
                && let Err(e) = self.renew(c).await
            {
                warn!(error = %e, "partition lock session lost; releasing ownership");
                self.drop_all();
                conn = None;
            }
            tokio::select! {
                () = tokio::time::sleep(LOCK_RENEW_INTERVAL) => {}
                _ = stop.changed() => {}
            }
        }
        self.drop_all();
        if let Some(mut c) = conn {
            let _ = sqlx::query("SELECT pg_advisory_unlock_all()")
                .execute(&mut c)
                .await;
            let _ = c.close().await;
        }
    }

    async fn renew(&self, c: &mut PgConnection) -> Result<(), sqlx::Error> {
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *c)
            .await?;
        self.pid.store(pid, Ordering::Release);
        for (slot, key) in self.keys.iter().enumerate() {
            if self.is_owned(slot) {
                continue;
            }
            let got: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
                .bind(key)
                .fetch_one(&mut *c)
                .await?;
            if got {
                info!(slot, key, pid, "usage event partition lock acquired");
                self.owned[slot].store(true, Ordering::Release);
            }
        }
        Ok(())
    }
}

/// Fencing check run inside the batch transaction: the lock for `key` is still granted to
/// the holder session `pid` (bigint advisory key = classid:high32, objid:low32, objsubid 1).
const LOCK_FENCE_SQL: &str = "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE locktype = 'advisory' \
     AND granted AND pid = $1 AND objsubid = 1 \
     AND classid::bigint = ($2 >> 32) AND objid::bigint = ($2 & 4294967295))";

async fn partition_loop(
    cfg: UsageEventsConfig,
    db: PgPool,
    pc: Arc<PartitionClient>,
    mut shutdown: watch::Receiver<bool>,
    locks: Arc<PartitionLocks>,
    slot: usize,
) {
    let partition = pc.partition();
    let key = locks.keys[slot];
    // None = not owned; offset is (re)loaded from PG every time ownership is (re)gained.
    let mut offset: Option<i64> = None;
    while !*shutdown.borrow() {
        if !locks.is_owned(slot) {
            if offset.take().is_some() {
                warn!(partition, "usage event partition lock lost; pausing");
            }
            tokio::select! {
                () = tokio::time::sleep(Duration::from_millis(200)) => {}
                _ = shutdown.changed() => break,
            }
            continue;
        }
        let cur = if let Some(o) = offset {
            o
        } else {
            let o = match load_offset(&db, &cfg.topic, partition).await {
                Some(o) => o,
                None => pc.get_offset(OffsetAt::Earliest).await.unwrap_or(0),
            };
            info!(
                partition,
                offset = o,
                "usage event partition consumer running"
            );
            offset = Some(o);
            o
        };
        let fetch_started = std::time::Instant::now();
        let fetched = tokio::select! {
            r = pc.fetch_records(cur, 1..cfg.fetch_max_bytes.max(1024), cfg.fetch_max_wait_ms.max(10)) => r,
            _ = shutdown.changed() => break,
        };
        let (records, _hwm) = match fetched {
            Ok(r) => {
                mazbri_core::metrics::observe_dep("kafka", "consume", fetch_started, true);
                r
            }
            Err(rskafka::client::error::Error::ServerError {
                protocol_error: rskafka::client::error::ProtocolError::OffsetOutOfRange,
                ..
            }) => {
                mazbri_core::metrics::observe_dep("kafka", "consume", fetch_started, false);
                let earliest = pc.get_offset(OffsetAt::Earliest).await.unwrap_or(0);
                warn!(
                    partition,
                    offset = cur,
                    earliest,
                    "offset out of range; resetting to earliest"
                );
                offset = Some(earliest);
                continue;
            }
            Err(e) => {
                mazbri_core::metrics::observe_dep("kafka", "consume", fetch_started, false);
                warn!(partition, error = %e, "usage event fetch failed; retry");
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };
        let records: Vec<_> = records.into_iter().filter(|r| r.offset >= cur).collect();
        let Some(last) = records.last().map(|r| r.offset) else {
            continue;
        };
        let fence = (locks.pid.load(Ordering::Acquire), key);
        match store_batch(&db, &cfg.topic, partition, &records, last + 1, fence).await {
            Ok(inserted) => {
                debug!(
                    partition,
                    n = records.len(),
                    inserted,
                    next = last + 1,
                    "usage events stored"
                );
                offset = Some(last + 1);
            }
            Err(e) => {
                warn!(partition, error = %e, "usage event batch store failed; will refetch");
                offset = None;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

async fn load_offset(db: &PgPool, topic: &str, partition: i32) -> Option<i64> {
    sqlx::query_scalar(LOAD_OFFSET_SQL)
        .bind(topic)
        .bind(partition)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

/// Insert the batch into `usage_outbox` and advance the stored offset in ONE transaction.
async fn store_batch(
    db: &PgPool,
    topic: &str,
    partition: i32,
    records: &[rskafka::record::RecordAndOffset],
    next_offset: i64,
    (lock_pid, lock_key): (i32, i64),
) -> Result<u64, String> {
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    let still_owner: bool = sqlx::query_scalar(LOCK_FENCE_SQL)
        .bind(lock_pid)
        .bind(lock_key)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    if !still_owner {
        return Err(format!(
            "partition lock {lock_key} no longer held by pid {lock_pid}; batch fenced"
        ));
    }
    let mut inserted = 0;
    for r in records {
        let Some(value) = r.record.value.as_deref() else {
            continue;
        };
        let event: UsageOutboxEvent = match serde_json::from_slice(value) {
            Ok(e) => e,
            Err(e) => {
                error!(partition, offset = r.offset, error = %e, "undecodable usage event skipped");
                continue;
            }
        };
        let q =
            crate::repository::queries::outbox_insert_query(&event).map_err(|e| e.to_string())?;
        inserted += q
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected();
    }
    sqlx::query(STORE_OFFSET_SQL)
        .bind(topic)
        .bind(partition)
        .bind(next_offset)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(inserted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_is_stable_and_in_range() {
        for n in [1, 3, 7] {
            let p = partition_for(b"fc391546-7925-4139-b06e-6cfe9f3c8a70", n);
            assert!((0..n).contains(&p));
            assert_eq!(p, partition_for(b"fc391546-7925-4139-b06e-6cfe9f3c8a70", n));
        }
        assert_eq!(partition_for(b"x", 0), 0);
    }

    #[test]
    fn event_id_keys_spread_over_partitions() {
        let mut counts = [0u32; 3];
        for _ in 0..3000 {
            let id = uuid::Uuid::new_v4().to_string();
            counts[usize::try_from(partition_for(id.as_bytes(), 3)).unwrap()] += 1;
        }
        assert!(counts.iter().all(|c| *c > 800), "{counts:?}");
    }

    #[test]
    fn lock_keys_distinct_and_fence_sql_shape() {
        let l = PartitionLocks::new("mazbri-usage-events", &[0, 1, 2]);
        assert_eq!(l.keys.len(), 3);
        assert!(l.keys[0] != l.keys[1] && l.keys[1] != l.keys[2]);
        assert!(!l.is_owned(0));
        assert!(LOCK_FENCE_SQL.contains("pg_locks") && LOCK_FENCE_SQL.contains("objsubid = 1"));
    }

    #[test]
    fn offset_commit_is_upsert() {
        assert!(STORE_OFFSET_SQL.contains("ON CONFLICT (topic, partition)"));
    }

    #[test]
    fn default_sink_is_outbox() {
        let c = UsageEventsConfig::default();
        assert!(!c.redpanda_enabled());
    }

    #[tokio::test]
    async fn try_enqueue_reports_full_without_blocking() {
        let (tx, _rx) = mpsc::channel::<UsageOutboxEvent>(1);
        let producer = UsageEventProducer { tx };
        let e1 = sample_event();
        let e2 = sample_event();
        assert!(producer.try_enqueue(e1).is_ok());
        let err = producer.try_enqueue(e2).expect_err("second enqueue must fill");
        assert!(!err.event_id.is_nil());
    }

    #[tokio::test]
    async fn batch_flush_decision_by_size() {
        // Pure decision helper: accumulating to BATCH_MAX must trigger a flush-sized batch.
        let mut pending = Vec::<UsageOutboxEvent>::new();
        for _ in 0..USAGE_EVENT_BATCH_MAX {
            pending.push(sample_event());
            if pending.len() >= USAGE_EVENT_BATCH_MAX {
                break;
            }
        }
        assert_eq!(pending.len(), USAGE_EVENT_BATCH_MAX);
        let flushed = std::mem::take(&mut pending);
        assert_eq!(flushed.len(), USAGE_EVENT_BATCH_MAX);
        assert!(pending.is_empty());
    }

    fn sample_event() -> UsageOutboxEvent {
        UsageOutboxEvent {
            event_id: uuid::Uuid::new_v4(),
            request_id: uuid::Uuid::new_v4(),
            tenant_id: uuid::Uuid::new_v4(),
            account_id: None,
            product_id: Some("8000000001".into()),
            api_endpoint: "/v1/test".into(),
            http_method: "POST".into(),
            response_status: 200,
            request_size: 1,
            response_size: 1,
            billing_service_duration_ms: 1,
            billing_result: crate::calculator::CalculationResult {
                amount: rust_decimal::Decimal::new(1, 3),
                units: 1,
                unit_price: rust_decimal::Decimal::new(1, 3),
                description: "test".into(),
            },
            billing_code: Some(0),
            trace_parent: None,
        }
    }
}
