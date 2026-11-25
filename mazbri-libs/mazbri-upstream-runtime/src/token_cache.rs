//! Token cache with cancellation-safe singleflight, shared by production and probe paths.

use chrono::{DateTime, Utc};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use subtle::ConstantTimeEq;
use tokio::sync::watch;

const DEFAULT_LEADER_FETCH_TIMEOUT: Duration = Duration::from_secs(15);

type FlightPayload<V> = Option<Result<V, String>>;
type FlightReceiver<V> = watch::Receiver<FlightPayload<V>>;
type FlightSender<V> = watch::Sender<FlightPayload<V>>;

struct LeaderCleanupGuard<'a, K, V>
where
    K: std::hash::Hash + Eq + Clone,
{
    in_flight: &'a Mutex<HashMap<K, FlightReceiver<V>>>,
    key: K,
    tx: Option<FlightSender<V>>,
}

impl<K, V> LeaderCleanupGuard<'_, K, V>
where
    K: std::hash::Hash + Eq + Clone,
{
    fn finish(&mut self, outcome: Result<V, String>) {
        self.in_flight.lock().remove(&self.key);
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Some(outcome));
        }
    }
}

impl<K, V> Drop for LeaderCleanupGuard<'_, K, V>
where
    K: std::hash::Hash + Eq + Clone,
{
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            self.in_flight.lock().remove(&self.key);
            let _ = tx.send(Some(Err("singleflight leader task aborted".to_string())));
        }
    }
}

struct GroupInner<K, V> {
    in_flight: Mutex<HashMap<K, FlightReceiver<V>>>,
    leader_timeout: Duration,
}

/// Cancellation-safe singleflight group: the leader executes inside an independent
/// `tokio::spawn` task bounded by `leader_timeout`, so cancelling one short-deadline
/// caller never aborts the in-flight fetch or strands concurrent waiters.
pub struct Group<K, V> {
    inner: Arc<GroupInner<K, V>>,
}

impl<K, V> Group<K, V>
where
    K: std::hash::Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    #[must_use]
    pub fn new() -> Self {
        Self::with_timeout(DEFAULT_LEADER_FETCH_TIMEOUT)
    }

    #[must_use]
    pub fn with_timeout(leader_timeout: Duration) -> Self {
        Self {
            inner: Arc::new(GroupInner {
                in_flight: Mutex::new(HashMap::new()),
                leader_timeout: leader_timeout.max(Duration::from_millis(10)),
            }),
        }
    }

    /// # Errors
    ///
    /// Returns an error if the leader task fails, times out, or aborts.
    pub async fn work<F, Fut>(&self, key: &K, f: F) -> Result<V, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, String>> + Send + 'static,
    {
        let mut rx = {
            let mut map = self.inner.in_flight.lock();
            if let Some(existing) = map.get(key) {
                existing.clone()
            } else {
                let (tx, rx) = watch::channel(None);
                map.insert(key.clone(), rx.clone());
                let group_inner = Arc::clone(&self.inner);
                let owned_key = key.clone();
                let leader_fut = f();
                tokio::spawn(async move {
                    let mut guard = LeaderCleanupGuard {
                        in_flight: &group_inner.in_flight,
                        key: owned_key,
                        tx: Some(tx),
                    };
                    let res =
                        match tokio::time::timeout(group_inner.leader_timeout, leader_fut).await {
                            Ok(res) => res,
                            Err(_) => Err(format!(
                                "token fetch timed out after {}ms",
                                group_inner.leader_timeout.as_millis()
                            )),
                        };
                    guard.finish(res);
                });
                rx
            }
        };

        loop {
            if let Some(ref res) = *rx.borrow_and_update() {
                return res.clone();
            }
            if rx.changed().await.is_err() {
                if let Some(ref res) = *rx.borrow() {
                    return res.clone();
                }
                return Err("singleflight leader task aborted".to_string());
            }
        }
    }
}

impl<K, V> Default for Group<K, V>
where
    K: std::hash::Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct CachedToken {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

/// [`TokenCache::evict_if_current_gated`] 的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleEvict {
    /// 缓存仍是被拒令牌，预算放行，已作废。
    Evicted,
    /// 缓存仍是被拒令牌，预算不放行，未作废。
    Denied,
    /// 缓存已换新或已被并发请求作废。
    Superseded,
}

#[derive(Default)]
struct CacheState {
    entries: HashMap<String, (CachedToken, u64)>,
    generations: HashMap<String, u64>,
}

struct TokenCacheInner {
    state: RwLock<CacheState>,
    flight_group: Group<String, CachedToken>,
    refresh_skew: chrono::Duration,
}

impl TokenCacheInner {
    fn is_fresh(&self, cached: &CachedToken, now: DateTime<Utc>) -> bool {
        cached.expires_at > now + self.refresh_skew
    }
}

/// 进程内令牌缓存：命中未过期（扣除提前量）的令牌直接返回，否则经取消安全的 singleflight 换票。
pub struct TokenCache {
    inner: Arc<TokenCacheInner>,
}

impl TokenCache {
    /// `refresh_skew`：令牌剩余有效期不足该值即视为过期（来自运行时配置）。
    #[must_use]
    pub fn new(refresh_skew: Duration) -> Self {
        Self::with_fetch_timeout(refresh_skew, DEFAULT_LEADER_FETCH_TIMEOUT)
    }

    #[must_use]
    pub fn with_fetch_timeout(refresh_skew: Duration, fetch_timeout: Duration) -> Self {
        Self {
            inner: Arc::new(TokenCacheInner {
                state: RwLock::new(CacheState::default()),
                flight_group: Group::with_timeout(fetch_timeout),
                refresh_skew: chrono::Duration::from_std(refresh_skew)
                    .unwrap_or(chrono::Duration::MAX),
            }),
        }
    }

    /// 提前视为过期的余量。
    #[must_use]
    pub fn refresh_skew(&self) -> Duration {
        self.inner.refresh_skew.to_std().unwrap_or_default()
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_token<F, Fut>(&self, cache_key: &str, fetch_fn: F) -> Result<String, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<CachedToken, String>> + Send + 'static,
    {
        self.get_token_timed(cache_key, fetch_fn)
            .await
            .map(|(token, _)| token)
    }

    /// 获取令牌并返回因未命中缓存而在外部换票请求上等待的真实时长（缓存命中时为 0）。
    ///
    /// 仅由后台 Leader 任务按 `generation` 单点写入缓存条目，杜绝慢速 Follower 回写被驱逐的脏令牌。
    ///
    /// # Errors
    ///
    /// 换票请求或响应解析失败时返回错误。
    pub async fn get_token_timed<F, Fut>(
        &self,
        cache_key: &str,
        fetch_fn: F,
    ) -> Result<(String, Duration), String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<CachedToken, String>> + Send + 'static,
    {
        {
            let guard = self.inner.state.read();
            if let Some((cached, _)) = guard.entries.get(cache_key)
                && self.inner.is_fresh(cached, Utc::now())
            {
                return Ok((cached.token.clone(), Duration::ZERO));
            }
        }

        let wait_started = std::time::Instant::now();
        let key_lookup = cache_key.to_string();
        let key_owned = cache_key.to_string();
        let inner = Arc::clone(&self.inner);
        let cached = self
            .inner
            .flight_group
            .work(&key_lookup, || {
                let fetch_fut = fetch_fn();
                async move {
                    let start_gen = {
                        let guard = inner.state.read();
                        if let Some((cached, _)) = guard.entries.get(&key_owned)
                            && inner.is_fresh(cached, Utc::now())
                        {
                            return Ok(cached.clone());
                        }
                        guard.generations.get(&key_owned).copied().unwrap_or(0)
                    };

                    let new_cached = fetch_fut.await?;
                    {
                        let mut guard = inner.state.write();
                        let cur_gen = guard.generations.get(&key_owned).copied().unwrap_or(0);
                        if cur_gen == start_gen {
                            guard
                                .entries
                                .insert(key_owned, (new_cached.clone(), start_gen));
                        }
                    }
                    Ok(new_cached)
                }
            })
            .await?;
        let waited = wait_started.elapsed();
        Ok((cached.token, waited))
    }

    /// 认证被拒后的「作废令牌」决策（原子，写锁内完成）。
    pub async fn evict_if_current_gated(
        &self,
        cache_key: &str,
        failed: &str,
        gate: impl FnOnce() -> bool,
    ) -> StaleEvict {
        let decision = {
            let mut guard = self.inner.state.write();
            match guard.entries.get(cache_key) {
                Some((c, _)) if bool::from(c.token.as_bytes().ct_eq(failed.as_bytes())) => {
                    if gate() {
                        guard.entries.remove(cache_key);
                        let next_gen = guard
                            .generations
                            .get(cache_key)
                            .copied()
                            .unwrap_or(0)
                            .saturating_add(1);
                        guard.generations.insert(cache_key.to_string(), next_gen);
                        StaleEvict::Evicted
                    } else {
                        StaleEvict::Denied
                    }
                }
                _ => StaleEvict::Superseded,
            }
        };
        tokio::task::consume_budget().await;
        decision
    }

    /// 仅当缓存中的令牌仍是 `failed` 时删除，返回是否删除（常量时间比较）。
    pub async fn evict_if_current(&self, cache_key: &str, failed: &str) -> bool {
        self.evict_if_current_gated(cache_key, failed, || true)
            .await
            == StaleEvict::Evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn token(t: &str, ttl_secs: i64) -> CachedToken {
        CachedToken {
            token: t.into(),
            expires_at: Utc::now() + chrono::Duration::seconds(ttl_secs),
        }
    }

    async fn seed(cache: &TokenCache, key: &str, t: CachedToken) {
        let want = t.token.clone();
        let got = cache.get_token(key, || async move { Ok(t) }).await.unwrap();
        assert_eq!(got, want);
    }

    #[tokio::test]
    async fn evict_if_current_only_removes_matching_token() {
        let cache = TokenCache::new(Duration::from_secs(30));
        seed(&cache, "k", token("A", 600)).await;
        assert!(
            !cache.evict_if_current("k", "B").await,
            "缓存为 A、传入 B 不删"
        );
        assert!(
            cache.evict_if_current("k", "A").await,
            "缓存为 A、传入 A 删除"
        );
        assert!(!cache.evict_if_current("k", "A").await, "缓存为空不删");
    }

    #[tokio::test]
    async fn concurrent_evict_then_get_fetches_once() {
        let cache = Arc::new(TokenCache::new(Duration::from_secs(30)));
        seed(&cache, "k", token("A", 600)).await;
        let fetches = Arc::new(AtomicUsize::new(0));
        let mut joins = Vec::new();
        for _ in 0..16 {
            let cache = cache.clone();
            let fetches = fetches.clone();
            joins.push(tokio::spawn(async move {
                cache.evict_if_current("k", "A").await;
                cache
                    .get_token("k", || async move {
                        fetches.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        Ok(token("B", 600))
                    })
                    .await
                    .unwrap()
            }));
        }
        for j in joins {
            assert_eq!(j.await.unwrap(), "B");
        }
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn refresh_skew_from_config_decides_freshness() {
        let cache = TokenCache::new(Duration::from_secs(30));
        seed(&cache, "near", token("OLD", 29)).await;
        let got = cache
            .get_token("near", || async { Ok(token("NEW", 600)) })
            .await
            .unwrap();
        assert_eq!(got, "NEW", "剩余 29s < skew 30s 触发换票");

        seed(&cache, "far", token("KEEP", 31)).await;
        let got = cache
            .get_token("far", || async { Ok(token("NEW", 600)) })
            .await
            .unwrap();
        assert_eq!(got, "KEEP", "剩余 31s > skew 30s 命中缓存");
        assert_eq!(cache.refresh_skew(), Duration::from_secs(30));
    }

    #[tokio::test]
    async fn cancelling_short_timeout_caller_does_not_drop_singleflight_for_peers() {
        let cache = Arc::new(TokenCache::new(Duration::from_secs(30)));
        let fetches = Arc::new(AtomicUsize::new(0));

        // Caller 1 initiates a 150ms token fetch, but cancels after 30ms.
        let c1 = Arc::clone(&cache);
        let f1 = Arc::clone(&fetches);
        let caller1 = tokio::spawn(async move {
            tokio::time::timeout(
                Duration::from_millis(30),
                c1.get_token("shared-key", move || async move {
                    f1.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok(token("FRESH-TOK", 600))
                }),
            )
            .await
        });

        // Give caller 1 a moment to become the singleflight leader, then launch 10 followers.
        tokio::time::sleep(Duration::from_millis(10)).await;
        let mut followers = Vec::new();
        for _ in 0..10 {
            let cf = Arc::clone(&cache);
            let ff = Arc::clone(&fetches);
            followers.push(tokio::spawn(async move {
                cf.get_token("shared-key", move || async move {
                    ff.fetch_add(1, Ordering::SeqCst);
                    Ok(token("UNEXPECTED", 600))
                })
                .await
            }));
        }

        assert!(caller1.await.expect("join c1").is_err(), "caller 1 timed out");
        for f in followers {
            let res = f.await.expect("join follower");
            assert_eq!(res.as_deref(), Ok("FRESH-TOK"));
        }
        assert_eq!(fetches.load(Ordering::SeqCst), 1);

        // Subsequent call hits the populated cache entry immediately.
        let hit = cache
            .get_token_timed("shared-key", || async { Err("must hit cache".into()) })
            .await
            .expect("cache hit");
        assert_eq!(hit.0, "FRESH-TOK");
        assert_eq!(hit.1, Duration::ZERO);
    }
}
