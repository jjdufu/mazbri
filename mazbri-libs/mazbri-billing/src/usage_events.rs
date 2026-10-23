//! L1 billing durability: usage charge events go through Redpanda (Kafka API).
//!
//! * Gateway: [`UsageEventProducer::produce`] is awaited **before** the HTTP response is
//!   returned (acks=all, bounded by `produce_timeout_ms`). On failure the gateway falls back
//!   to the existing direct outbox insert + in-memory deferred retry queue.
//! * Billing: [`UsageEventConsumer`] reads every partition and inserts the events into
//!   `usage_outbox` with `ON CONFLICT (event_id) DO NOTHING`; the consumed offset is stored in
//!   `usage_event_offsets` **in the same DB transaction** (offset committed only after the DB
//!   commit, so redelivery is possible but duplicates are absorbed by the `event_id` key).
//!
//! rskafka has no idempotent producer / consumer groups: idempotency is provided by the
//! `event_id` unique key (outbox) and `usage_record_id` unique key (charge queue), and offsets
//! live in Postgres. A per-partition advisory lock keeps one consumer per partition.
use crate::service::UsageOutboxEvent;
use chrono::Utc;
use rskafka::client::partition::{Compression, OffsetAt, PartitionClient, UnknownTopicHandling};
use rskafka::client::{ClientBuilder, Credentials, SaslConfig};
use rskafka::record::Record;
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

/// `[usage_events]` (billing) / `[billing.usage_events]` (gateway).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UsageEventsConfig {
    /// Gateway: `"redpanda"` = produce before response (fallback: outbox); `"outbox"` = legacy.
    /// Billing: consumer runs when `"redpanda"`.
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

/// Stable partition for a tenant key (FNV-1a), so one tenant's events stay ordered.
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

#[derive(Clone)]
pub struct UsageEventProducer {
    partitions: Arc<Vec<Arc<PartitionClient>>>,
    timeout: Duration,
}

impl UsageEventProducer {
    /// # Errors
    /// Broker unreachable, SASL failure or unknown topic.
    pub async fn connect(cfg: &UsageEventsConfig) -> Result<Self, String> {
        let partitions = cfg.partition_clients().await?;
        info!(topic = %cfg.topic, partitions = partitions.len(), "usage event producer connected (acks=all)");
        Ok(Self {
            partitions: Arc::new(partitions),
            timeout: Duration::from_millis(cfg.produce_timeout_ms.max(1)),
        })
    }

    /// Produce one event (key = tenant id, acks=all). Returns the offset.
    ///
    /// # Errors
    /// Serialization, broker error or timeout; the caller falls back to the outbox path.
    pub async fn produce(&self, event: &UsageOutboxEvent) -> Result<i64, String> {
        let key = event.tenant_id.to_string().into_bytes();
        let n = i32::try_from(self.partitions.len()).unwrap_or(1);
        let idx = usize::try_from(partition_for(&key, n)).unwrap_or(0);
        let value = serde_json::to_vec(event).map_err(|e| e.to_string())?;
        let mut headers = BTreeMap::new();
        headers.insert(
            "event_id".to_string(),
            event.event_id.to_string().into_bytes(),
        );
        let record = Record {
            key: Some(key),
            value: Some(value),
            headers,
            timestamp: Utc::now(),
        };
        let pc = &self.partitions[idx];
        let offsets = tokio::time::timeout(
            self.timeout,
            pc.produce(vec![record], Compression::NoCompression),
        )
        .await
        .map_err(|_| format!("produce timeout after {:?}", self.timeout))?
        .map_err(|e| e.to_string())?;
        offsets
            .first()
            .copied()
            .ok_or_else(|| "no offset returned".to_string())
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

    /// Spawns the per-partition loops; they stop when `shutdown` flips to `true`.
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
            let mut handles = Vec::new();
            for pc in clients {
                let (cfg, db, sd) = (self.cfg.clone(), self.db.clone(), shutdown.clone());
                handles.push(tokio::spawn(partition_loop(cfg, db, pc, sd)));
            }
            for h in handles {
                let _ = h.await;
            }
            info!("usage event consumer stopped");
        })
    }
}

async fn partition_loop(
    cfg: UsageEventsConfig,
    db: PgPool,
    pc: Arc<PartitionClient>,
    mut shutdown: watch::Receiver<bool>,
) {
    let partition = pc.partition();
    // One consumer per partition across billing replicas (session advisory lock).
    let lock_key =
        i64::from(partition_for(cfg.topic.as_bytes(), i32::MAX)) * 1000 + i64::from(partition);
    let Ok(mut lock_conn) = db.acquire().await else {
        error!(
            partition,
            "usage event consumer: cannot acquire lock connection"
        );
        return;
    };
    loop {
        let got: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(lock_key)
            .fetch_one(&mut *lock_conn)
            .await
            .unwrap_or(false);
        if got {
            break;
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(5)) => {}
            _ = shutdown.changed() => return,
        }
    }
    let mut offset = match load_offset(&db, &cfg.topic, partition).await {
        Some(o) => o,
        None => pc.get_offset(OffsetAt::Earliest).await.unwrap_or(0),
    };
    info!(partition, offset, "usage event partition consumer running");
    while !*shutdown.borrow() {
        let fetched = tokio::select! {
            r = pc.fetch_records(offset, 1..cfg.fetch_max_bytes.max(1024), cfg.fetch_max_wait_ms.max(10)) => r,
            _ = shutdown.changed() => break,
        };
        let (records, _hwm) = match fetched {
            Ok(r) => r,
            Err(rskafka::client::error::Error::ServerError {
                protocol_error: rskafka::client::error::ProtocolError::OffsetOutOfRange,
                ..
            }) => {
                let earliest = pc.get_offset(OffsetAt::Earliest).await.unwrap_or(0);
                warn!(
                    partition,
                    offset, earliest, "offset out of range; resetting to earliest"
                );
                offset = earliest;
                continue;
            }
            Err(e) => {
                warn!(partition, error = %e, "usage event fetch failed; retry");
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };
        let records: Vec<_> = records.into_iter().filter(|r| r.offset >= offset).collect();
        let Some(last) = records.last().map(|r| r.offset) else {
            continue;
        };
        match store_batch(&db, &cfg.topic, partition, &records, last + 1).await {
            Ok(inserted) => {
                debug!(
                    partition,
                    n = records.len(),
                    inserted,
                    next = last + 1,
                    "usage events stored"
                );
                offset = last + 1;
            }
            Err(e) => {
                warn!(partition, error = %e, "usage event batch store failed; will refetch");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(lock_key)
        .execute(&mut *lock_conn)
        .await;
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
) -> Result<u64, String> {
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
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
    fn offset_commit_is_upsert() {
        assert!(STORE_OFFSET_SQL.contains("ON CONFLICT (topic, partition)"));
    }

    #[test]
    fn default_sink_is_outbox() {
        let c = UsageEventsConfig::default();
        assert!(!c.redpanda_enabled());
    }
}
