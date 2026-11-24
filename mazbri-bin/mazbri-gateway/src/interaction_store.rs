use chrono::{DateTime, Utc};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::{AttemptStatus, InteractionEvent};
use sqlx::PgPool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::{Notify, mpsc};
use tracing::{error, info, warn};
use uuid::Uuid;

const INTERACTION_QUEUE_CAPACITY: usize = 32_768;
const INTERACTION_BATCH_MAX: usize = 128;
const INTERACTION_LINGER: Duration = Duration::from_millis(5);

struct QueuedInteraction {
    pool: Arc<PgPool>,
    event: InteractionEvent,
}

struct InteractionBatcher {
    tx: mpsc::Sender<QueuedInteraction>,
    flush_notify: Arc<Notify>,
}

static BATCHER: OnceLock<InteractionBatcher> = OnceLock::new();
static PENDING_EVENTS: AtomicUsize = AtomicUsize::new(0);

fn batcher() -> &'static InteractionBatcher {
    BATCHER.get_or_init(|| {
        let (tx, rx) = mpsc::channel(INTERACTION_QUEUE_CAPACITY);
        let flush_notify = Arc::new(Notify::new());
        let notify_for_worker = Arc::clone(&flush_notify);
        tokio::spawn(async move {
            run_batch_worker(rx, notify_for_worker).await;
        });
        InteractionBatcher { tx, flush_notify }
    })
}

/// 将交互事件推入有界异步批写队列（请求热路径零克隆、零逐请求 `tokio::spawn`）。
pub fn enqueue_interaction_event(pool: &Arc<PgPool>, event: InteractionEvent) {
    PENDING_EVENTS.fetch_add(1, Ordering::AcqRel);
    let item = QueuedInteraction {
        pool: Arc::clone(pool),
        event,
    };
    if let Err(err) = batcher().tx.try_send(item) {
        PENDING_EVENTS.fetch_sub(1, Ordering::AcqRel);
        let dropped = match err {
            mpsc::error::TrySendError::Full(v) | mpsc::error::TrySendError::Closed(v) => v,
        };
        warn!(
            event_id = %dropped.event.id,
            request_id = %dropped.event.request_id,
            "Interaction event queue full or closed; dropping best-effort interaction record"
        );
    }
}

/// 优雅停机排空：唤醒批写循环并等待在途批次落库（受 `timeout` 上限保护）。
pub async fn drain_on_shutdown(timeout: Duration) {
    let pending = PENDING_EVENTS.load(Ordering::Acquire);
    if pending == 0 {
        return;
    }
    if let Some(b) = BATCHER.get() {
        b.flush_notify.notify_one();
    }
    info!(
        pending,
        timeout_ms = mazbri_core::num::millis_u64(timeout),
        "Draining gateway interaction events before exit"
    );
    let deadline = Instant::now() + timeout;
    loop {
        if PENDING_EVENTS.load(Ordering::Acquire) == 0 {
            info!("Gateway interaction event drain complete");
            return;
        }
        if Instant::now() >= deadline {
            let left = PENDING_EVENTS.load(Ordering::Acquire);
            error!(
                remaining = left,
                "Gateway interaction event drain timed out"
            );
            return;
        }
        if let Some(b) = BATCHER.get() {
            b.flush_notify.notify_one();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn run_batch_worker(
    mut rx: mpsc::Receiver<QueuedInteraction>,
    flush_notify: Arc<Notify>,
) {
    let mut batch: Vec<InteractionEvent> = Vec::with_capacity(INTERACTION_BATCH_MAX);
    while let Some(first) = rx.recv().await {
        let pool = first.pool;
        batch.push(first.event);
        let linger_deadline = tokio::time::Instant::now() + INTERACTION_LINGER;
        while batch.len() < INTERACTION_BATCH_MAX {
            match rx.try_recv() {
                Ok(next) => batch.push(next.event),
                Err(mpsc::error::TryRecvError::Disconnected) => break,
                Err(mpsc::error::TryRecvError::Empty) => {
                    tokio::select! {
                        biased;
                        maybe_next = rx.recv() => {
                            if let Some(next) = maybe_next {
                                batch.push(next.event);
                            } else {
                                break;
                            }
                        }
                        () = flush_notify.notified() => break,
                        () = tokio::time::sleep_until(linger_deadline) => break,
                    }
                }
            }
        }
        let count = batch.len();
        if let Err(batch_err) = persist_interaction_batch(&pool, &batch).await {
            warn!(
                batch_size = count,
                error = %batch_err,
                "Batch insert for interaction_events failed; falling back to single-row inserts"
            );
            for event in &batch {
                if let Err(single_err) = persist_interaction_event(&pool, event).await {
                    warn!(
                        event_id = %event.id,
                        request_id = %event.request_id,
                        error = %single_err,
                        "Failed to persist gateway interaction event"
                    );
                }
            }
        }
        batch.clear();
        PENDING_EVENTS.fetch_sub(count, Ordering::AcqRel);
    }
}

struct EventBatchColumns {
    ids: Vec<Uuid>,
    request_ids: Vec<String>,
    tenant_ids: Vec<String>,
    product_ids: Vec<String>,
    start_times: Vec<DateTime<Utc>>,
    final_statuses: Vec<String>,
}

struct AttemptBatchColumns {
    event_ids: Vec<Uuid>,
    channel_ids: Vec<String>,
    supplier_ids: Vec<Option<Uuid>>,
    supplier_codes: Vec<Option<String>>,
    supplier_names: Vec<Option<String>>,
    channel_codes: Vec<Option<String>>,
    channel_names: Vec<Option<String>>,
    endpoint_urls: Vec<Option<String>>,
    attempt_orders: Vec<i32>,
    start_times: Vec<DateTime<Utc>>,
    end_times: Vec<Option<DateTime<Utc>>>,
    statuses: Vec<Option<&'static str>>,
    error_messages: Vec<Option<String>>,
}

fn unpack_batch_columns(
    events: &[InteractionEvent],
) -> Result<(EventBatchColumns, AttemptBatchColumns), String> {
    let n = events.len();
    let total_attempts: usize = events.iter().map(|e| e.attempts.len()).sum();
    let mut ev = EventBatchColumns {
        ids: Vec::with_capacity(n),
        request_ids: Vec::with_capacity(n),
        tenant_ids: Vec::with_capacity(n),
        product_ids: Vec::with_capacity(n),
        start_times: Vec::with_capacity(n),
        final_statuses: Vec::with_capacity(n),
    };
    let mut at = AttemptBatchColumns {
        event_ids: Vec::with_capacity(total_attempts),
        channel_ids: Vec::with_capacity(total_attempts),
        supplier_ids: Vec::with_capacity(total_attempts),
        supplier_codes: Vec::with_capacity(total_attempts),
        supplier_names: Vec::with_capacity(total_attempts),
        channel_codes: Vec::with_capacity(total_attempts),
        channel_names: Vec::with_capacity(total_attempts),
        endpoint_urls: Vec::with_capacity(total_attempts),
        attempt_orders: Vec::with_capacity(total_attempts),
        start_times: Vec::with_capacity(total_attempts),
        end_times: Vec::with_capacity(total_attempts),
        statuses: Vec::with_capacity(total_attempts),
        error_messages: Vec::with_capacity(total_attempts),
    };
    for event in events {
        ev.ids.push(event.id);
        ev.request_ids.push(event.request_id.clone());
        ev.tenant_ids.push(event.tenant_id.clone());
        ev.product_ids.push(event.product_id.clone());
        ev.start_times.push(event.start_time);
        ev.final_statuses.push(
            event
                .final_status
                .as_deref()
                .unwrap_or("unknown")
                .to_string(),
        );
        for (index, attempt) in event.attempts.iter().enumerate() {
            let parsed_supplier_id = match attempt.supplier_id.as_deref() {
                Some(raw) if !raw.is_empty() => Some(
                    Uuid::parse_str(raw)
                        .map_err(|e| format!("invalid supplier_id UUID '{raw}': {e}"))?,
                ),
                _ => None,
            };
            let (status, error_message) = attempt_status_parts(attempt.status.as_ref());
            at.event_ids.push(event.id);
            at.channel_ids.push(attempt.channel_id.clone());
            at.supplier_ids.push(parsed_supplier_id);
            at.supplier_codes.push(attempt.supplier_code.clone());
            at.supplier_names.push(attempt.supplier_name.clone());
            at.channel_codes.push(attempt.channel_code.clone());
            at.channel_names.push(attempt.channel_name.clone());
            at.endpoint_urls.push(attempt.endpoint_url.clone());
            at.attempt_orders.push(i32::saturating_from(index + 1));
            at.start_times.push(attempt.start_time);
            at.end_times.push(attempt.end_time);
            at.statuses.push(status);
            at.error_messages.push(error_message);
        }
    }
    Ok((ev, at))
}

async fn persist_interaction_batch(
    pool: &PgPool,
    events: &[InteractionEvent],
) -> Result<(), String> {
    if events.is_empty() {
        return Ok(());
    }
    let (ev, at) = unpack_batch_columns(events)?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query(
        r"
        INSERT INTO interaction_events (
            id, request_id, tenant_id, product_id, start_time, final_status
        )
        SELECT * FROM UNNEST(
            $1::uuid[],
            $2::text[],
            $3::text[],
            $4::text[],
            $5::timestamptz[],
            $6::text[]
        )
        ON CONFLICT (id) DO NOTHING
        ",
    )
    .bind(&ev.ids)
    .bind(&ev.request_ids)
    .bind(&ev.tenant_ids)
    .bind(&ev.product_ids)
    .bind(&ev.start_times)
    .bind(&ev.final_statuses)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    if !at.event_ids.is_empty() {
        sqlx::query(
            r"
            INSERT INTO interaction_event_attempts (
                interaction_event_id,
                channel_id,
                supplier_id,
                supplier_code,
                supplier_name,
                channel_code,
                channel_name,
                endpoint_url,
                attempt_order,
                start_time,
                end_time,
                status,
                error_message
            )
            SELECT * FROM UNNEST(
                $1::uuid[],
                $2::text[],
                $3::uuid[],
                $4::text[],
                $5::text[],
                $6::text[],
                $7::text[],
                $8::text[],
                $9::int4[],
                $10::timestamptz[],
                $11::timestamptz[],
                $12::text[],
                $13::text[]
            )
            ON CONFLICT (interaction_event_id, attempt_order) DO NOTHING
            ",
        )
        .bind(&at.event_ids)
        .bind(&at.channel_ids)
        .bind(&at.supplier_ids)
        .bind(&at.supplier_codes)
        .bind(&at.supplier_names)
        .bind(&at.channel_codes)
        .bind(&at.channel_names)
        .bind(&at.endpoint_urls)
        .bind(&at.attempt_orders)
        .bind(&at.start_times)
        .bind(&at.end_times)
        .bind(&at.statuses)
        .bind(&at.error_messages)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    tx.commit().await.map_err(|e| e.to_string())
}

/// 单条交互事件落库（供测试及批量异常回退路径调用）。
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn persist_interaction_event(
    pool: &PgPool,
    event: &InteractionEvent,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let final_status = event.final_status.as_deref().unwrap_or("unknown");
    sqlx::query(
        r"
        INSERT INTO interaction_events (
            id, request_id, tenant_id, product_id, start_time, final_status
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (id) DO NOTHING
        ",
    )
    .bind(event.id)
    .bind(&event.request_id)
    .bind(&event.tenant_id)
    .bind(&event.product_id)
    .bind(event.start_time)
    .bind(final_status)
    .execute(&mut *tx)
    .await?;
    for (index, attempt) in event.attempts.iter().enumerate() {
        let (status, error_message) = attempt_status_parts(attempt.status.as_ref());
        sqlx::query(
            r"
            INSERT INTO interaction_event_attempts (
                interaction_event_id,
                channel_id,
                supplier_id,
                supplier_code,
                supplier_name,
                channel_code,
                channel_name,
                endpoint_url,
                attempt_order,
                start_time,
                end_time,
                status,
                error_message
            )
            VALUES ($1, $2, $3::uuid, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (interaction_event_id, attempt_order) DO NOTHING
            ",
        )
        .bind(event.id)
        .bind(&attempt.channel_id)
        .bind(attempt.supplier_id.as_deref())
        .bind(attempt.supplier_code.as_deref())
        .bind(attempt.supplier_name.as_deref())
        .bind(attempt.channel_code.as_deref())
        .bind(attempt.channel_name.as_deref())
        .bind(attempt.endpoint_url.as_deref())
        .bind(i32::saturating_from(index + 1))
        .bind(attempt.start_time)
        .bind(attempt.end_time)
        .bind(status)
        .bind(error_message)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

fn attempt_status_parts(status: Option<&AttemptStatus>) -> (Option<&'static str>, Option<String>) {
    match status {
        Some(AttemptStatus::Success) => (Some("success"), None),
        Some(AttemptStatus::Timeout) => (Some("timeout"), None),
        Some(AttemptStatus::Failed(error)) => (Some("failed"), Some(error.clone())),
        None => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_core::interaction::UpstreamAttempt;

    #[test]
    fn unpack_batch_columns_aligns_events_and_attempts() {
        let sid = Uuid::now_v7().to_string();
        let mut e1 = InteractionEvent::new("req-1", "t-1", "prod-1");
        e1.record_upstream_attempt(
            "ch-1",
            UpstreamAttempt {
                supplier_id: &sid,
                supplier_code: "S1",
                supplier_name: "Supplier 1",
                channel_code: "C1",
                channel_name: "Channel 1",
                endpoint_url: "https://api.example.com",
            },
        );
        e1.mark_last_attempt(AttemptStatus::Success);
        e1.mark_final("success");

        let mut e2 = InteractionEvent::new("req-2", "t-2", "prod-2");
        e2.mark_final("no_route");

        let (ev, at) = unpack_batch_columns(&[e1, e2]).unwrap();
        assert_eq!(ev.ids.len(), 2);
        assert_eq!(ev.final_statuses, vec!["success", "no_route"]);
        assert_eq!(at.event_ids.len(), 1);
        assert_eq!(at.attempt_orders, vec![1]);
        assert_eq!(at.statuses, vec![Some("success")]);
    }

    #[tokio::test]
    async fn drain_returns_immediately_when_queue_idle() {
        let start = Instant::now();
        drain_on_shutdown(Duration::from_secs(1)).await;
        assert!(start.elapsed() < Duration::from_millis(100));
    }
}
