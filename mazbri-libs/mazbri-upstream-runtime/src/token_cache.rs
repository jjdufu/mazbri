//! Token cache with singleflight, shared by production and probe paths.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::future::Future;
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, RwLock, watch};

type FlightReceiver<V> = watch::Receiver<Option<Result<V, String>>>;

pub struct Group<K, V> {
    in_flight: Mutex<HashMap<K, FlightReceiver<V>>>,
}

impl<K, V> Group<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    pub fn new() -> Self {
        Self {
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    pub async fn work<F, Fut>(&self, key: &K, f: F) -> Result<V, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, String>>,
    {
        let mut map = self.in_flight.lock().await;
        if let Some(rx) = map.get(key) {
            let mut rx = rx.clone();
            drop(map);
            loop {
                if let Some(ref res) = *rx.borrow() {
                    return res.clone();
                }
                if rx.changed().await.is_err() {
                    return Err("Singleflight sender dropped".to_string());
                }
            }
        }

        let (tx, rx) = watch::channel(None);
        map.insert(key.clone(), rx);
        drop(map);

        let res = f().await;

        let mut map = self.in_flight.lock().await;
        map.remove(key);
        let _ = tx.send(Some(res.clone()));
        drop(map);

        res
    }
}

impl<K, V> Default for Group<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
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

/// 进程内令牌缓存：命中未过期（扣除提前量）的令牌直接返回，否则经 singleflight 换票。
pub struct TokenCache {
    cache: RwLock<HashMap<String, CachedToken>>,
    flight_group: Group<String, CachedToken>,
    refresh_skew: chrono::Duration,
}

impl TokenCache {
    /// `refresh_skew`：令牌剩余有效期不足该值即视为过期（来自运行时配置）。
    #[must_use]
    pub fn new(refresh_skew: std::time::Duration) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            flight_group: Group::new(),
            refresh_skew: chrono::Duration::from_std(refresh_skew).unwrap_or(chrono::Duration::MAX),
        }
    }

    /// 提前视为过期的余量。
    #[must_use]
    pub fn refresh_skew(&self) -> std::time::Duration {
        self.refresh_skew.to_std().unwrap_or_default()
    }

    fn is_fresh(&self, cached: &CachedToken, now: DateTime<Utc>) -> bool {
        cached.expires_at > now + self.refresh_skew
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_token<F, Fut>(&self, cache_key: &str, fetch_fn: F) -> Result<String, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<CachedToken, String>>,
    {
        {
            let guard = self.cache.read().await;
            if let Some(cached) = guard.get(cache_key)
                && self.is_fresh(cached, Utc::now())
            {
                return Ok(cached.token.clone());
            }
        }

        let cached = self
            .flight_group
            .work(&cache_key.to_string(), || async move {
                {
                    let guard = self.cache.read().await;
                    if let Some(cached) = guard.get(cache_key)
                        && self.is_fresh(cached, Utc::now())
                    {
                        return Ok(cached.clone());
                    }
                }

                let new_cached = fetch_fn().await?;
                Ok(new_cached)
            })
            .await?;

        let mut guard = self.cache.write().await;
        guard.insert(cache_key.to_string(), cached.clone());

        Ok(cached.token)
    }

    /// 仅当缓存中的令牌仍是 `failed` 时删除，返回是否删除（常量时间比较）。
    ///
    /// 并发 N 个请求同时被拒时只有第一个真正删除；其余看到的已是新令牌或空，
    /// 直接走 [`Self::get_token`]，不会误删别的请求刚换回来的新令牌。
    pub async fn evict_if_current(&self, cache_key: &str, failed: &str) -> bool {
        let mut guard = self.cache.write().await;
        match guard.get(cache_key) {
            Some(c) if bool::from(c.token.as_bytes().ct_eq(failed.as_bytes())) => {
                guard.remove(cache_key);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

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
}
