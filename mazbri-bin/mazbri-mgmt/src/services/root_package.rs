use crate::error::ApiError;
use mazbri_core::crypto::tenant_key::RootProtectionMaterialBundle;
use std::collections::HashMap;
use std::future::Future;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

const ROOT_MATERIAL_CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
struct CachedRootBundle {
    bundle: RootProtectionMaterialBundle,
    fetched_at: Instant,
}

struct RootMaterialCache {
    entries: RwLock<HashMap<String, CachedRootBundle>>,
    refresh_gate: Mutex<()>,
    ttl: Duration,
}

impl RootMaterialCache {
    fn new(ttl: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            refresh_gate: Mutex::new(()),
            ttl,
        }
    }

    fn normalize_key(base_url: &str) -> String {
        base_url.trim().trim_end_matches('/').to_string()
    }

    async fn get_or_fetch<F, Fut>(
        &self,
        base_url: &str,
        fetch: F,
    ) -> Result<RootProtectionMaterialBundle, ApiError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<RootProtectionMaterialBundle, ApiError>>,
    {
        let key = Self::normalize_key(base_url);
        {
            let guard = self.entries.read().await;
            if let Some(entry) = guard.get(&key)
                && entry.fetched_at.elapsed() < self.ttl
            {
                return Ok(entry.bundle.clone());
            }
        }

        let _gate = self.refresh_gate.lock().await;
        {
            let guard = self.entries.read().await;
            if let Some(entry) = guard.get(&key)
                && entry.fetched_at.elapsed() < self.ttl
            {
                return Ok(entry.bundle.clone());
            }
        }

        let bundle = fetch().await?;
        let mut guard = self.entries.write().await;
        guard.insert(
            key,
            CachedRootBundle {
                bundle: bundle.clone(),
                fetched_at: Instant::now(),
            },
        );
        Ok(bundle)
    }

    async fn invalidate(&self) {
        let mut guard = self.entries.write().await;
        guard.clear();
    }
}

static ROOT_MATERIAL_CACHE: LazyLock<RootMaterialCache> =
    LazyLock::new(|| RootMaterialCache::new(ROOT_MATERIAL_CACHE_TTL));

/// 主动清空根授权保护材料短缓存（导入/更新根许可证后调用）。
pub async fn invalidate_root_material_cache() {
    ROOT_MATERIAL_CACHE.invalidate().await;
}

/// 获取根授权保护材料包（进程内 60s 短缓存，消除租户与 API Key 批量初始化重复内网连读）。
///
/// # Errors
///
/// 底层依赖（网络、序列化或参数校验）失败时返回错误。
pub async fn fetch_root_material_bundle(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
) -> Result<RootProtectionMaterialBundle, ApiError> {
    ROOT_MATERIAL_CACHE
        .get_or_fetch(base_url, || async {
            mazbri_core::crypto::tenant_key::fetch_root_protection_material_bundle(
                client, base_url, token,
            )
            .await
            .map_err(|error| {
                ApiError::internal(format!(
                    "Failed to fetch root protection materials: {error}"
                ))
            })
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn sample_bundle(ver: &str) -> RootProtectionMaterialBundle {
        RootProtectionMaterialBundle {
            current_key_version: ver.to_string(),
            wrapping_key_id: "wrap-1".to_string(),
            master_key: "master".to_string(),
            integrity_key: "integrity".to_string(),
        }
    }

    #[tokio::test]
    async fn caches_bundle_within_ttl_and_invalidates_on_demand() {
        let cache = RootMaterialCache::new(Duration::from_secs(60));
        let calls = Arc::new(AtomicUsize::new(0));

        let b1 = cache
            .get_or_fetch("http://cert:9100/", || {
                let calls = Arc::clone(&calls);
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(sample_bundle("v1"))
                }
            })
            .await
            .expect("first fetch");
        assert_eq!(b1.current_key_version, "v1");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let b2 = cache
            .get_or_fetch("http://cert:9100", || {
                let calls = Arc::clone(&calls);
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(sample_bundle("v2"))
                }
            })
            .await
            .expect("cached fetch");
        assert_eq!(b2.current_key_version, "v1");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        cache.invalidate().await;
        let b3 = cache
            .get_or_fetch("http://cert:9100", || {
                let calls = Arc::clone(&calls);
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(sample_bundle("v2"))
                }
            })
            .await
            .expect("post-invalidate fetch");
        assert_eq!(b3.current_key_version, "v2");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
