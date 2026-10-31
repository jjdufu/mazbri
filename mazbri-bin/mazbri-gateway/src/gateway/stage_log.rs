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

/// 阶段 span 的 ID，写进同一阶段日志的 `trace_id` / `span_id`；未启用导出时为空串。
#[derive(Debug, Default, Clone)]
pub struct StageSpanIds {
    pub trace_id: String,
    pub span_id: String,
}

/// 网关阶段 span：名称即 `stage_name`，耗时与阶段日志的 `stage_ms` 同源。
///
/// 阶段日志有采样，span 不采样：每个请求的每个阶段都补记一个已结束的 span。
#[must_use]
pub struct StageSpan {
    span: mazbri_core::trace_spans::CompletedSpan,
    stage: &'static str,
    failed: bool,
}

pub fn stage_span(stage: &'static str, stage_ms: u64) -> StageSpan {
    StageSpan {
        span: mazbri_core::trace_spans::CompletedSpan::from_millis(stage, stage_ms),
        stage,
        failed: false,
    }
}

impl StageSpan {
    pub fn product(mut self, product_id: &str) -> Self {
        self.span = self.span.attr("product_id", product_id.to_string());
        self
    }

    pub fn tenant(mut self, tenant_code: &str) -> Self {
        self.span = self.span.attr("tenant_code", tenant_code.to_string());
        self
    }

    pub fn mode(mut self, mode: &str) -> Self {
        self.span = self.span.attr("mode", mode.to_string());
        self
    }

    /// 本阶段出错：span status=error，`error_stage` 属性取阶段名（与访问日志 `error_stage` 一致）。
    pub fn failed(mut self) -> Self {
        self.failed = true;
        self
    }

    /// 以当前 span 为父节点记录。
    #[must_use]
    pub fn record(self) -> StageSpanIds {
        let parent = mazbri_core::trace_spans::current_context();
        self.record_with_parent(&parent)
    }

    /// 以给定上下文为父节点记录（脱离请求 span 的异步任务用）。
    #[must_use]
    pub fn record_with_parent(
        self,
        parent: &mazbri_core::trace_spans::OtelContext,
    ) -> StageSpanIds {
        let span = if self.failed {
            self.span
                .attr("error_stage", self.stage)
                .error(format!("{} failed", self.stage))
        } else {
            self.span
        };
        span.record(parent)
            .map(|span_context| StageSpanIds {
                trace_id: span_context.trace_id().to_string(),
                span_id: span_context.span_id().to_string(),
            })
            .unwrap_or_default()
    }
}
