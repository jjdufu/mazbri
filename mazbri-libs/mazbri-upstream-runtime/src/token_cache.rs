//! Token cache with singleflight, shared by production and probe paths.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::future::Future;
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

pub struct TokenCache {
    cache: RwLock<HashMap<String, CachedToken>>,
    flight_group: Group<String, CachedToken>,
}

impl TokenCache {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            flight_group: Group::new(),
        }
    }

    pub async fn get_token<F, Fut>(&self, cache_key: &str, fetch_fn: F) -> Result<String, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<CachedToken, String>>,
    {
        let now = Utc::now();

        {
            let guard = self.cache.read().await;
            if let Some(cached) = guard.get(cache_key) {
                if cached.expires_at > now + chrono::Duration::seconds(30) {
                    return Ok(cached.token.clone());
                }
            }
        }

        let cached = self
            .flight_group
            .work(&cache_key.to_string(), || async move {
                {
                    let guard = self.cache.read().await;
                    if let Some(cached) = guard.get(cache_key) {
                        if cached.expires_at > Utc::now() + chrono::Duration::seconds(30) {
                            return Ok(cached.clone());
                        }
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

    pub async fn evict_token(&self, cache_key: &str) {
        let mut guard = self.cache.write().await;
        guard.remove(cache_key);
    }
}

impl Default for TokenCache {
    fn default() -> Self {
        Self::new()
    }
}
