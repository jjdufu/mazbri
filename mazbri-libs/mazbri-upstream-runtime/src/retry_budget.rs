//! 进程内重试预算：限制「认证被拒重试」「WS 陈旧连接重试」在故障期放大上游流量。
//!
//! 预算按上游（供应商）分桶，语义对应 Envoy 按 cluster 的 `retry_budget`：
//! 一个上游整体失效（例如重启后全部令牌作废）只消耗它自己的额度，
//! 不会挤占同一时刻其它上游的恢复重试。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// 滑动窗口重试预算：窗口内重试数 ≤ max(`min_per_window`, `ratio` × 首次请求数)。
///
/// 用「当前窗口 + 上一窗口按剩余时间加权」的两桶近似滑动窗口，避免在窗口
/// 边界处额度突然清零或翻倍。
#[derive(Debug)]
pub struct RetryBudget {
    ratio: f64,
    min_per_window: u32,
    window: Duration,
    state: Mutex<BudgetWindow>,
}

#[derive(Debug)]
struct BudgetWindow {
    started: Instant,
    requests: u64,
    retries: u64,
    prev_requests: u64,
    prev_retries: u64,
}

impl BudgetWindow {
    fn roll(&mut self, now: Instant, window: Duration) {
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed < window {
            return;
        }
        if elapsed < window * 2 {
            self.prev_requests = self.requests;
            self.prev_retries = self.retries;
            self.started += window;
        } else {
            self.prev_requests = 0;
            self.prev_retries = 0;
            self.started = now;
        }
        self.requests = 0;
        self.retries = 0;
    }

    /// 上一窗口的计数按「仍落在滑动窗口内的比例」折算。
    fn weighted(&self, now: Instant, window: Duration, current: u64, previous: u64) -> f64 {
        let into = now.saturating_duration_since(self.started).as_secs_f64();
        let weight = (1.0 - into / window.as_secs_f64()).clamp(0.0, 1.0);
        to_f64(current) + to_f64(previous) * weight
    }
}

fn to_f64(n: u64) -> f64 {
    // 计数远小于 2^52，换算无精度损失。
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

impl RetryBudget {
    #[must_use]
    pub fn new(ratio: f64, min_per_window: u32, window: Duration) -> Self {
        Self::new_at(ratio, min_per_window, window, Instant::now())
    }

    fn new_at(ratio: f64, min_per_window: u32, window: Duration, now: Instant) -> Self {
        Self {
            ratio,
            min_per_window,
            window,
            state: Mutex::new(BudgetWindow {
                started: now,
                requests: 0,
                retries: 0,
                prev_requests: 0,
                prev_retries: 0,
            }),
        }
    }

    /// 记一次首次请求（每次 invoke 调用一次）。
    pub fn record_request(&self) {
        self.record_request_at(Instant::now());
    }

    /// 申请一次重试额度；通过时计入重试数。
    #[must_use]
    pub fn try_acquire(&self) -> bool {
        self.try_acquire_at(Instant::now())
    }

    fn record_request_at(&self, now: Instant) {
        let mut st = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        st.roll(now, self.window);
        st.requests += 1;
    }

    fn try_acquire_at(&self, now: Instant) -> bool {
        let mut st = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        st.roll(now, self.window);
        let requests = st.weighted(now, self.window, st.requests, st.prev_requests);
        let retries = st.weighted(now, self.window, st.retries, st.prev_retries);
        let allowed = (self.ratio * requests).max(f64::from(self.min_per_window));
        if retries + 1.0 <= allowed {
            st.retries += 1;
            true
        } else {
            false
        }
    }
}

/// 按上游键（供应商 id）懒建的 [`RetryBudget`] 集合，各桶参数相同、计数独立。
///
/// 桶数量等于出现过的供应商数，随配置有界，不做淘汰。
#[derive(Debug)]
pub struct RetryBudgets {
    ratio: f64,
    min_per_window: u32,
    window: Duration,
    buckets: Mutex<HashMap<String, Arc<RetryBudget>>>,
}

impl RetryBudgets {
    #[must_use]
    pub fn new(ratio: f64, min_per_window: u32, window: Duration) -> Self {
        Self {
            ratio,
            min_per_window,
            window,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// 取（必要时新建）该上游的预算桶。
    #[must_use]
    pub fn for_upstream(&self, upstream: &str) -> Arc<RetryBudget> {
        let mut buckets = self.buckets.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(b) = buckets.get(upstream) {
            return b.clone();
        }
        let b = Arc::new(RetryBudget::new(
            self.ratio,
            self.min_per_window,
            self.window,
        ));
        buckets.insert(upstream.to_owned(), b.clone());
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstreams_have_independent_budgets() {
        let budgets = RetryBudgets::new(0.2, 3, WINDOW);
        let a = budgets.for_upstream("supplier-a");
        let b = budgets.for_upstream("supplier-b");
        let now = Instant::now();
        assert_eq!(run(&a, now, 3), 3);
        assert_eq!(run(&a, now, 1), 0, "supplier-a exhausted");
        assert_eq!(run(&b, now, 3), 3, "supplier-b keeps its own minimum");
        assert!(Arc::ptr_eq(&a, &budgets.for_upstream("supplier-a")));
    }

    const WINDOW: Duration = Duration::from_secs(10);

    fn run(budget: &RetryBudget, now: Instant, requests: u32) -> u32 {
        let mut granted = 0;
        for _ in 0..requests {
            budget.record_request_at(now);
            if budget.try_acquire_at(now) {
                granted += 1;
            }
        }
        granted
    }

    #[test]
    fn ratio_caps_retries() {
        let now = Instant::now();
        let budget = RetryBudget::new_at(0.2, 0, WINDOW, now);
        assert_eq!(run(&budget, now, 100), 20);
    }

    #[test]
    fn min_per_window_covers_low_traffic() {
        let now = Instant::now();
        let budget = RetryBudget::new_at(0.2, 3, WINDOW, now);
        assert_eq!(run(&budget, now, 2), 2);
        assert_eq!(run(&budget, now, 1), 1, "third retry still within min=3");
        assert_eq!(run(&budget, now, 1), 0, "ratio*4 < 4 and min exhausted");
    }

    #[test]
    fn budget_recovers_after_window_rolls() {
        let now = Instant::now();
        let budget = RetryBudget::new_at(0.2, 0, WINDOW, now);
        assert_eq!(run(&budget, now, 10), 2);
        assert!(!budget.try_acquire_at(now), "exhausted in current window");
        let later = now + WINDOW * 3;
        assert_eq!(run(&budget, later, 10), 2, "fresh window after idle gap");
    }

    #[test]
    fn previous_window_is_weighted_near_boundary() {
        let now = Instant::now();
        let budget = RetryBudget::new_at(0.5, 0, WINDOW, now);
        assert_eq!(run(&budget, now, 10), 5);
        // 刚进入下一窗口：上一窗口仍几乎全额计入，不会立刻放出整窗额度。
        let just_after = now + WINDOW + Duration::from_millis(1);
        assert_eq!(run(&budget, just_after, 1), 0);
        // 下一窗口过半：上一窗口折半，额度部分恢复。
        let halfway = now + WINDOW + WINDOW / 2;
        assert!(run(&budget, halfway, 4) >= 1);
    }
}
