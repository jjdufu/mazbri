//! 运行时内部事件（认证被拒、换票重试、作废令牌、死连接、重试预算）的观测出口。
//!
//! 运行时只产出闭集标签的事件；服务（bypass 等）实现 [`RuntimeObserver`] 映射为
//! 自己的指标。日志在运行时内统一打，令牌只以 [`token_fingerprint`] 出现。

use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::Duration;

use mazbri_core::AdapterKind;
use mazbri_core::rate_limited_log::RateLimitedLog;
use sha2::{Digest, Sha256};

use crate::types::{AuthRejectTrigger, RetryReason};

/// 重试 / 跳过重试的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryOutcome {
    Ok,
    /// 认证重试后仍被拒。
    RejectedAgain,
    /// 重试轮以其他错误失败。
    Failed,
    SkippedDisabled,
    SkippedBudget,
}

impl RetryOutcome {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::RejectedAgain => "rejected_again",
            Self::Failed => "failed",
            Self::SkippedDisabled => "skipped_disabled",
            Self::SkippedBudget => "skipped_budget",
        }
    }
}

/// 识别出死 WS 连接的阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsStaleStage {
    /// 借出前存活检查丢弃。
    Checkout,
    /// 复用连接发送后、回复前断开。
    Send,
}

impl WsStaleStage {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Checkout => "checkout",
            Self::Send => "send",
        }
    }
}

/// 重试预算的使用方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryKind {
    Auth,
    WsStale,
}

impl RetryKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::WsStale => "ws_stale",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEvent {
    /// 一轮调用按规则判定为认证被拒（每轮记一次）。
    AuthRejected {
        adapter: AdapterKind,
        trigger: AuthRejectTrigger,
    },
    /// 认证重试的结局（含跳过）。
    AuthRetry {
        adapter: AdapterKind,
        result: RetryOutcome,
    },
    /// 比较后作废：`true` 已作废，`false` 缓存已被并发请求换新。
    TokenEvict {
        evicted: bool,
    },
    WsStale {
        stage: WsStaleStage,
    },
    /// WS 陈旧连接重试的结局（不含 `RejectedAgain`）。
    WsStaleRetry {
        result: RetryOutcome,
    },
    BudgetExhausted {
        kind: RetryKind,
    },
}

/// 事件接收方，须为非阻塞实现（在请求路径上同步调用）。
pub trait RuntimeObserver: Send + Sync {
    fn on_event(&self, event: RuntimeEvent);
}

/// 默认观测者：丢弃所有事件。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopObserver;

impl RuntimeObserver for NoopObserver {
    fn on_event(&self, _event: RuntimeEvent) {}
}

/// 单次 invoke 内发生的内部重试（认证换票、WS 换新连接）。
#[derive(Debug, Default)]
pub struct RetryLog(Mutex<Vec<RetryReason>>);

impl RetryLog {
    pub fn push(&self, reason: RetryReason) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(reason);
    }

    #[must_use]
    pub fn take(&self) -> Vec<RetryReason> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// 令牌指纹：SHA-256 前 8 位十六进制，仅用于日志关联，不可逆推令牌。
#[must_use]
pub fn token_fingerprint(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex::encode(&digest[..4])
}

/// 重试预算耗尽告警：同一渠道每 10 秒最多一条。
static BUDGET_WARN: LazyLock<RateLimitedLog> =
    LazyLock::new(|| RateLimitedLog::new(Duration::from_secs(10)));

pub(crate) fn warn_budget_exhausted(channel_id: &str, kind: RetryKind) {
    if BUDGET_WARN.allow(&format!("{}:{channel_id}", kind.label())) {
        tracing::warn!(
            target: mazbri_core::trace_spans::SPAN_EVENT_TARGET,
            event = "upstream.retry_budget_exhausted",
            channel_id,
            kind = kind.label(),
            "retry budget exhausted, retry skipped"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_fingerprint_is_short_and_stable() {
        let fp = token_fingerprint("secret-token");
        assert_eq!(fp.len(), 8);
        assert_eq!(fp, token_fingerprint("secret-token"));
        assert_ne!(fp, token_fingerprint("other-token"));
        assert!(!fp.contains("secret"));
    }

    #[test]
    fn retry_log_collects_and_drains() {
        let log = RetryLog::default();
        log.push(RetryReason::StaleConnection);
        log.push(RetryReason::AuthRejected(AuthRejectTrigger::BodyMatch));
        assert_eq!(
            log.take(),
            vec![
                RetryReason::StaleConnection,
                RetryReason::AuthRejected(AuthRejectTrigger::BodyMatch)
            ]
        );
        assert_eq!(log.take(), Vec::<RetryReason>::new());
    }
}
