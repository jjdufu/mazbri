//! 请求级重试台账：一次客户请求内每一次重发（渠道切换、配置未命中重发、传输重试、
//! bypass 内部换票 / 换连接）各记一个原因，访问日志 `retry_count` / `retry_reasons` 都从这里取。
//!
//! 对标 Envoy `%UPSTREAM_REQUEST_ATTEMPT_COUNT%`、Kong `tries[]`：一次请求一行访问日志，
//! 重试明细挂在同一行上。台账是 tokio task-local，由 [`scope_retry_ledger`] 在拒绝日志层
//! （包住鉴权、限流与处理函数）建立，成功和失败两条访问日志路径读到的是同一份。

use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};

tokio::task_local! {
    static RETRY_LEDGER: RetryLedger;
}

#[derive(Debug, Clone, Default)]
pub struct RetryLedger(Arc<Mutex<Vec<String>>>);

impl RetryLedger {
    fn push(&self, reason: String) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(reason);
    }

    /// 已记下的重试原因（按发生顺序）。
    #[must_use]
    pub fn reasons(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// 在新台账作用域内运行 `future`，返回结果与台账。
pub async fn scope_retry_ledger<F: Future>(future: F) -> (F::Output, RetryLedger) {
    let ledger = RetryLedger::default();
    let output = RETRY_LEDGER.scope(ledger.clone(), future).await;
    (output, ledger)
}

/// 记一次重发；不在请求作用域内（如单测、后台任务）时忽略。
pub fn note_retry(reason: &str) {
    let _ = RETRY_LEDGER.try_with(|ledger| ledger.push(reason.to_string()));
}

/// 记 bypass 透传的内部重试：每个原因一项；原因少于次数时用 `upstream_internal` 补齐。
pub fn note_internal_retries(count: u32, reasons: &[String]) {
    let _ = RETRY_LEDGER.try_with(|ledger| {
        for reason in reasons {
            ledger.push(reason.clone());
        }
        let missing = (count as usize).saturating_sub(reasons.len());
        for _ in 0..missing {
            ledger.push("upstream_internal".to_string());
        }
    });
}

/// 当前请求已记下的重试原因；不在作用域内时为空。
#[must_use]
pub fn current_retry_reasons() -> Vec<String> {
    RETRY_LEDGER
        .try_with(RetryLedger::reasons)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ledger_collects_reasons_in_order() {
        let ((), ledger) = scope_retry_ledger(async {
            note_retry("config_cache_miss");
            note_internal_retries(2, &["auth_rejected:http_status".to_string()]);
            note_retry("channel_switch");
            assert_eq!(current_retry_reasons().len(), 4);
        })
        .await;
        assert_eq!(
            ledger.reasons(),
            vec![
                "config_cache_miss",
                "auth_rejected:http_status",
                "upstream_internal",
                "channel_switch"
            ]
        );
    }

    #[test]
    fn outside_scope_is_a_no_op() {
        note_retry("transport");
        assert_eq!(current_retry_reasons(), Vec::<String>::new());
    }
}
