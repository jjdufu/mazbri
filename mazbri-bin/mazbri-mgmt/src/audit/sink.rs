//! 审计记录写入：有界队列 + 单写线程批量落库 + 重试 + 本地 JSONL 兜底 + 停机排空。

use super::types::AuditRecord;
use mazbri_core::metrics::{MetricsBuilder, REGISTRY, register_collector_once};
use prometheus::{CounterVec, IntGauge};
use sqlx::{Postgres, QueryBuilder};
use std::future::Future;
use std::io::Write as _;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

pub static AUDIT_QUEUE_DEPTH: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge("audit_queue_depth", "Pending mgmt audit records in queue")
});

/// stage: `batch|sync`
pub static AUDIT_WRITE_FAILED_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "audit_write_failed_total",
        "Failed mgmt audit database write attempts",
        &["stage"],
    )
});

/// reason: `queue_full|queue_closed|db_error`
pub static AUDIT_FALLBACK_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "audit_fallback_total",
        "Mgmt audit records written to the local fallback file",
        &["reason"],
    )
});

/// # Errors
///
/// 指标注册失败时返回错误。
pub fn register_audit_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(AUDIT_QUEUE_DEPTH.clone()))?;
    register_collector_once(&REGISTRY, Box::new(AUDIT_WRITE_FAILED_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(AUDIT_FALLBACK_TOTAL.clone()))?;
    Ok(())
}

/// 落库错误分类。
#[derive(Debug)]
pub enum StoreError {
    /// 外键冲突（操作人账号已被删除）：清空 `user_id` 后可重试。
    ForeignKey,
    Other(String),
}

type StoreFuture<'a> = Pin<Box<dyn Future<Output = Result<(), StoreError>> + Send + 'a>>;

/// 审计存储抽象（生产为 Postgres，测试可替换为内存实现）。
pub trait AuditStore: Send + Sync + 'static {
    fn insert_batch<'a>(&'a self, records: &'a [AuditRecord]) -> StoreFuture<'a>;
}

/// `mazbri_admin.audit_logs` 存储。
pub struct PgAuditStore {
    db: Arc<sqlx::PgPool>,
}

impl PgAuditStore {
    #[must_use]
    pub fn new(db: Arc<sqlx::PgPool>) -> Self {
        Self { db }
    }
}

impl AuditStore for PgAuditStore {
    fn insert_batch<'a>(&'a self, records: &'a [AuditRecord]) -> StoreFuture<'a> {
        Box::pin(async move {
            if records.is_empty() {
                return Ok(());
            }
            let mut builder: QueryBuilder<'_, Postgres> = QueryBuilder::new(
                "INSERT INTO audit_logs (created_at, user_id, actor_type, actor_username, \
                 actor_role, action, resource_type, resource_id, details, status, error_message, \
                 failure_reason, ip_address, user_agent, old_value, new_value, request_id, \
                 method, path, status_code, duration_ms, request_summary) ",
            );
            builder.push_values(records, |mut row, record| {
                row.push_bind(record.created_at)
                    .push_bind(record.user_id)
                    .push_bind(&record.actor_type)
                    .push_bind(&record.actor_username)
                    .push_bind(&record.actor_role)
                    .push_bind(&record.action)
                    .push_bind(&record.resource_type)
                    .push_bind(&record.resource_id)
                    .push_bind(&record.details)
                    .push_bind(record.status.as_str())
                    .push_bind(&record.error_message)
                    .push_bind(&record.failure_reason)
                    .push_bind(&record.ip_address)
                    .push_bind(&record.user_agent)
                    .push_bind(&record.old_value)
                    .push_bind(&record.new_value)
                    .push_bind(&record.request_id)
                    .push_bind(&record.method)
                    .push_bind(&record.path)
                    .push_bind(record.status_code)
                    .push_bind(record.duration_ms)
                    .push_bind(&record.request_summary);
            });
            builder
                .build()
                .execute(&*self.db)
                .await
                .map(|_| ())
                .map_err(|error| match &error {
                    sqlx::Error::Database(db) if db.code().as_deref() == Some("23503") => {
                        StoreError::ForeignKey
                    }
                    _ => StoreError::Other(error.to_string()),
                })
        })
    }
}

/// 本地 JSONL 兜底文件（记录已脱敏）。
pub struct FallbackWriter {
    path: Option<PathBuf>,
    lock: Mutex<()>,
}

impl FallbackWriter {
    #[must_use]
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    pub fn write(&self, records: &[AuditRecord], reason: &str) {
        AUDIT_FALLBACK_TOTAL
            .with_label_values(&[reason])
            .inc_by(f64::from(u32::try_from(records.len()).unwrap_or(u32::MAX)));
        if let Err(error) = self.append(records) {
            for record in records {
                let line = serde_json::to_string(record).unwrap_or_default();
                tracing::error!(
                    target: "audit",
                    error = %error,
                    reason,
                    record = %line,
                    "审计兜底文件写入失败，记录输出到日志"
                );
            }
        } else {
            tracing::warn!(
                target: "audit",
                count = records.len(),
                reason,
                "审计记录已写入兜底文件"
            );
        }
    }

    fn append(&self, records: &[AuditRecord]) -> std::io::Result<()> {
        let Some(path) = self.path.as_ref() else {
            return Err(std::io::Error::other("fallback file not configured"));
        };
        let _guard = match self.lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        let mut buffer = Vec::new();
        for record in records {
            serde_json::to_writer(&mut buffer, record).map_err(std::io::Error::other)?;
            buffer.push(b'\n');
        }
        file.write_all(&buffer)?;
        file.flush()
    }
}

/// 写入参数。
#[derive(Debug, Clone)]
pub struct SinkOptions {
    pub queue_capacity: usize,
    pub batch_size: usize,
    pub flush_interval: Duration,
    pub retry_backoff: Duration,
    pub max_attempts: u32,
    pub sync_timeout: Duration,
}

impl Default for SinkOptions {
    fn default() -> Self {
        Self {
            queue_capacity: 10_000,
            batch_size: 100,
            flush_interval: Duration::from_millis(200),
            retry_backoff: Duration::from_millis(100),
            max_attempts: 3,
            sync_timeout: Duration::from_secs(2),
        }
    }
}

/// 审计写入端（可廉价克隆）。
#[derive(Clone)]
pub struct AuditSink {
    tx: mpsc::Sender<AuditRecord>,
    store: Arc<dyn AuditStore>,
    fallback: Arc<FallbackWriter>,
    options: Arc<SinkOptions>,
}

/// 写线程句柄：停机时关闭队列并等待排空。
pub struct AuditWriterHandle {
    shutdown: Option<oneshot::Sender<()>>,
    join: JoinHandle<()>,
}

impl AuditWriterHandle {
    /// 关闭队列、写完已入队记录；超时后放弃等待（剩余记录随进程退出丢失并告警）。
    pub async fn shutdown(mut self, timeout: Duration) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        match tokio::time::timeout(timeout, &mut self.join).await {
            Ok(Ok(())) => tracing::info!("审计写入队列已排空"),
            Ok(Err(error)) => tracing::error!(error = %error, "审计写线程异常退出"),
            Err(_) => {
                tracing::error!(
                    pending = AUDIT_QUEUE_DEPTH.get(),
                    "审计写入队列排空超时，剩余记录未落库"
                );
                self.join.abort();
            }
        }
    }
}

impl AuditSink {
    /// 启动单写线程（需在 tokio 运行时内调用）。
    #[must_use]
    pub fn start(
        store: Arc<dyn AuditStore>,
        fallback: Arc<FallbackWriter>,
        options: SinkOptions,
    ) -> (Self, AuditWriterHandle) {
        let (tx, rx) = mpsc::channel(options.queue_capacity.max(1));
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let options = Arc::new(options);
        let writer = Writer {
            store: Arc::clone(&store),
            fallback: Arc::clone(&fallback),
            options: Arc::clone(&options),
        };
        let join = tokio::spawn(writer.run(rx, shutdown_rx));
        (
            Self {
                tx,
                store,
                fallback,
                options,
            },
            AuditWriterHandle {
                shutdown: Some(shutdown_tx),
                join,
            },
        )
    }

    /// 异步入队；队列满或已关闭时写兜底文件（不阻塞请求）。
    pub fn enqueue(&self, record: AuditRecord) {
        match self.tx.try_send(record) {
            Ok(()) => AUDIT_QUEUE_DEPTH.inc(),
            Err(mpsc::error::TrySendError::Full(record)) => {
                self.fallback.write(&[record], "queue_full");
            }
            Err(mpsc::error::TrySendError::Closed(record)) => {
                self.fallback.write(&[record], "queue_closed");
            }
        }
    }

    /// 关键事件同步落库；失败或超时写兜底文件。返回是否成功落库。
    pub async fn write_now(&self, record: AuditRecord) -> bool {
        let mut records = vec![record];
        let attempt = tokio::time::timeout(
            self.options.sync_timeout,
            insert_fixing_fk(self.store.as_ref(), &mut records),
        )
        .await;
        match attempt {
            Ok(Ok(())) => true,
            Ok(Err(error)) => {
                AUDIT_WRITE_FAILED_TOTAL.with_label_values(&["sync"]).inc();
                tracing::error!(target: "audit", error = %error, "关键审计记录同步落库失败");
                self.fallback.write(&records, "db_error");
                false
            }
            Err(_) => {
                AUDIT_WRITE_FAILED_TOTAL.with_label_values(&["sync"]).inc();
                tracing::error!(target: "audit", "关键审计记录同步落库超时");
                self.fallback.write(&records, "db_error");
                false
            }
        }
    }
}

/// 插入；外键冲突时清空 `user_id`（保留 `actor_username`）再试一次。
async fn insert_fixing_fk(
    store: &dyn AuditStore,
    records: &mut [AuditRecord],
) -> Result<(), String> {
    match store.insert_batch(records).await {
        Ok(()) => Ok(()),
        Err(StoreError::ForeignKey) => {
            for record in records.iter_mut() {
                record.user_id = None;
            }
            store
                .insert_batch(records)
                .await
                .map_err(|error| match error {
                    StoreError::ForeignKey => "foreign key violation".to_string(),
                    StoreError::Other(message) => message,
                })
        }
        Err(StoreError::Other(message)) => Err(message),
    }
}

struct Writer {
    store: Arc<dyn AuditStore>,
    fallback: Arc<FallbackWriter>,
    options: Arc<SinkOptions>,
}

impl Writer {
    async fn run(self, mut rx: mpsc::Receiver<AuditRecord>, shutdown: oneshot::Receiver<()>) {
        let mut shutdown = Some(shutdown);
        let mut batch = Vec::with_capacity(self.options.batch_size);
        loop {
            let first = if let Some(signal) = shutdown.as_mut() {
                tokio::select! {
                    biased;
                    record = rx.recv() => record,
                    _ = signal => {
                        // 停止接收新记录，继续排空已入队的记录。
                        rx.close();
                        shutdown = None;
                        continue;
                    }
                }
            } else {
                rx.recv().await
            };
            let Some(first) = first else {
                break;
            };
            AUDIT_QUEUE_DEPTH.dec();
            batch.push(first);
            let deadline = tokio::time::Instant::now() + self.options.flush_interval;
            while batch.len() < self.options.batch_size {
                match tokio::time::timeout_at(deadline, rx.recv()).await {
                    Ok(Some(record)) => {
                        AUDIT_QUEUE_DEPTH.dec();
                        batch.push(record);
                    }
                    Ok(None) | Err(_) => break,
                }
            }
            self.flush(&mut batch).await;
        }
    }

    async fn flush(&self, batch: &mut Vec<AuditRecord>) {
        let mut backoff = self.options.retry_backoff;
        for attempt in 1..=self.options.max_attempts.max(1) {
            match insert_fixing_fk(self.store.as_ref(), batch).await {
                Ok(()) => {
                    batch.clear();
                    return;
                }
                Err(error) => {
                    AUDIT_WRITE_FAILED_TOTAL.with_label_values(&["batch"]).inc();
                    tracing::warn!(
                        target: "audit",
                        attempt,
                        count = batch.len(),
                        error = %error,
                        "审计批量落库失败"
                    );
                    if attempt < self.options.max_attempts {
                        tokio::time::sleep(backoff).await;
                        backoff = backoff.saturating_mul(4);
                    }
                }
            }
        }
        self.fallback.write(batch, "db_error");
        batch.clear();
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::{AuditStore, StoreError, StoreFuture};
    use crate::audit::types::AuditRecord;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// 内存存储：可配置前 N 次失败或外键冲突。
    #[derive(Default)]
    pub struct MemoryStore {
        pub rows: Mutex<Vec<AuditRecord>>,
        pub fail_times: AtomicU32,
        pub fk_times: AtomicU32,
        pub calls: AtomicU32,
    }

    impl MemoryStore {
        pub fn rows(&self) -> Vec<AuditRecord> {
            self.rows.lock().unwrap().clone()
        }
    }

    /// 计数大于 0 时减一并返回 true。
    fn take_one(counter: &AtomicU32) -> bool {
        let mut current = counter.load(Ordering::SeqCst);
        while current > 0 {
            match counter.compare_exchange(current, current - 1, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
        false
    }

    impl AuditStore for MemoryStore {
        fn insert_batch<'a>(&'a self, records: &'a [AuditRecord]) -> StoreFuture<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                if take_one(&self.fk_times) {
                    return Err(StoreError::ForeignKey);
                }
                if take_one(&self.fail_times) {
                    return Err(StoreError::Other("simulated failure".into()));
                }
                self.rows.lock().unwrap().extend_from_slice(records);
                Ok(())
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::MemoryStore;
    use super::{AuditSink, FallbackWriter, SinkOptions};
    use crate::audit::types::{AuditLogRequest, AuditRecord};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    fn record() -> AuditRecord {
        AuditRecord::from_request(AuditLogRequest {
            user_id: Some(uuid::Uuid::new_v4()),
            ..AuditLogRequest::default()
        })
    }

    fn options() -> SinkOptions {
        SinkOptions {
            queue_capacity: 4,
            batch_size: 3,
            flush_interval: Duration::from_millis(20),
            retry_backoff: Duration::from_millis(1),
            max_attempts: 3,
            sync_timeout: Duration::from_secs(1),
        }
    }

    fn temp_file(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mazbri-audit-{name}-{}.jsonl",
            uuid::Uuid::new_v4()
        ))
    }

    fn read_lines(path: &std::path::Path) -> Vec<AuditRecord> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[tokio::test]
    async fn batches_and_drains_on_shutdown() {
        let store = Arc::new(MemoryStore::default());
        let (sink, handle) = AuditSink::start(
            store.clone(),
            Arc::new(FallbackWriter::new(None)),
            options(),
        );
        for _ in 0..4 {
            sink.enqueue(record());
        }
        handle.shutdown(Duration::from_secs(2)).await;
        assert_eq!(store.rows().len(), 4);
        // 关闭后入队的记录走兜底而不是丢失。
        let path = temp_file("closed");
        let late = AuditSink {
            fallback: Arc::new(FallbackWriter::new(Some(path.clone()))),
            ..sink
        };
        late.enqueue(record());
        assert_eq!(read_lines(&path).len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn retries_transient_failures() {
        let store = Arc::new(MemoryStore::default());
        store.fail_times.store(2, Ordering::SeqCst);
        let (sink, handle) = AuditSink::start(
            store.clone(),
            Arc::new(FallbackWriter::new(None)),
            options(),
        );
        sink.enqueue(record());
        handle.shutdown(Duration::from_secs(2)).await;
        assert_eq!(store.rows().len(), 1);
        assert_eq!(store.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn persistent_failure_goes_to_fallback_file() {
        let store = Arc::new(MemoryStore::default());
        store.fail_times.store(100, Ordering::SeqCst);
        let path = temp_file("dbdown");
        let (sink, handle) = AuditSink::start(
            store.clone(),
            Arc::new(FallbackWriter::new(Some(path.clone()))),
            options(),
        );
        sink.enqueue(record());
        sink.enqueue(record());
        handle.shutdown(Duration::from_secs(2)).await;
        assert!(store.rows().is_empty());
        assert_eq!(read_lines(&path).len(), 2);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn full_queue_spills_to_fallback() {
        let store = Arc::new(MemoryStore::default());
        let path = temp_file("full");
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let sink = AuditSink {
            tx,
            store: store.clone(),
            fallback: Arc::new(FallbackWriter::new(Some(path.clone()))),
            options: Arc::new(options()),
        };
        sink.enqueue(record());
        sink.enqueue(record());
        assert_eq!(read_lines(&path).len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn sync_write_clears_user_on_fk_violation() {
        let store = Arc::new(MemoryStore::default());
        store.fk_times.store(1, Ordering::SeqCst);
        let (sink, handle) = AuditSink::start(
            store.clone(),
            Arc::new(FallbackWriter::new(None)),
            options(),
        );
        let mut rec = record();
        rec.actor_username = Some("deleted-admin".into());
        assert!(sink.write_now(rec).await);
        let rows = store.rows();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].user_id.is_none());
        assert_eq!(rows[0].actor_username.as_deref(), Some("deleted-admin"));
        handle.shutdown(Duration::from_secs(1)).await;
    }

    #[tokio::test]
    async fn sync_write_failure_falls_back() {
        let store = Arc::new(MemoryStore::default());
        store.fail_times.store(5, Ordering::SeqCst);
        let path = temp_file("sync");
        let (sink, handle) = AuditSink::start(
            store.clone(),
            Arc::new(FallbackWriter::new(Some(path.clone()))),
            options(),
        );
        assert!(!sink.write_now(record()).await);
        assert_eq!(read_lines(&path).len(), 1);
        handle.shutdown(Duration::from_secs(1)).await;
        let _ = std::fs::remove_file(path);
    }
}
