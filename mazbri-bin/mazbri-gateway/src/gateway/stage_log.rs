//! Gateway stage 日志采样：热路径降频，慢请求/失败/摘要全量保留。

use std::sync::atomic::{AtomicU64, Ordering};

/// 默认每 N 条 info 阶段日志打 1 条（失败/慢请求仍全量）。
const DEFAULT_SAMPLE_N: u64 = 20;
const SLOW_STAGE_MS: u64 = 10;

static STAGE_SEQ: AtomicU64 = AtomicU64::new(0);

#[inline]
pub fn should_log_stage_info(stage_ms: u64) -> bool {
    if stage_ms >= SLOW_STAGE_MS {
        return true;
    }
    let n = STAGE_SEQ.fetch_add(1, Ordering::Relaxed);
    n.is_multiple_of(DEFAULT_SAMPLE_N)
}

/// 平台摘要、余额拒绝等始终打。
#[inline]
#[must_use]
pub fn always_log_stage() -> bool {
    true
}
