use super::metrics::LOCAL_RATELIMIT_USAGE_BUFFER_DROPPED_TOTAL;
use mazbri_rpc::ratelimit::UsageDelta;
use parking_lot::Mutex;
use std::mem;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct UsageDeltaRecord {
    pub delta: UsageDelta,
}

#[derive(Clone)]
pub struct UsageDeltaBuffer {
    inner: Arc<Mutex<Vec<UsageDeltaRecord>>>,
    max_entries: usize,
}

impl UsageDeltaBuffer {
    pub fn new(max_entries: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::with_capacity(max_entries.min(1024)))),
            max_entries: max_entries.max(1),
        }
    }

    /// Record a usage delta. When the buffer is full:
    /// - non-critical allow deltas are dropped (metric++);
    /// - deny/degraded deltas drop the oldest entry to make room (metric++).
    ///
    /// Either path never affects the already-made HTTP decision.
    pub fn record(&self, delta: UsageDelta) {
        let mut guard = self.inner.lock();
        if guard.len() >= self.max_entries {
            let critical = delta.denied_count > 0 || delta.degraded;
            if critical {
                // Drop oldest to retain the denial signal for control-plane ingest.
                if !guard.is_empty() {
                    guard.remove(0);
                    LOCAL_RATELIMIT_USAGE_BUFFER_DROPPED_TOTAL
                        .with_label_values(&["evict_oldest"])
                        .inc();
                }
            } else {
                LOCAL_RATELIMIT_USAGE_BUFFER_DROPPED_TOTAL
                    .with_label_values(&["reject_new"])
                    .inc();
                return;
            }
        }
        guard.push(UsageDeltaRecord { delta });
    }

    pub fn drain(&self) -> Vec<UsageDeltaRecord> {
        let mut guard = self.inner.lock();
        mem::take(&mut *guard)
    }
}
