use crate::error::ApiError;
use crate::services::license::ensure_license_issuance_allowed;
use mazbri_core::crypto::supplier_secrets::{
    RootMaterialKeyring, clear_root_material_keyring, install_root_material_keyring,
};
use mazbri_core::crypto::tenant_key::{RootProtectionKeyring, RootProtectionMaterialBundle};
use mazbri_core::{CertServiceConfig, get_cert_service_token};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

const ROOT_MATERIAL_CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
struct CachedEntry<T> {
    value: T,
    fetched_at: Instant,
}

struct RootCache<T> {
    entries: RwLock<HashMap<String, CachedEntry<T>>>,
    refresh_gate: Mutex<()>,
    ttl: Duration,
}

impl<T: Clone> RootCache<T> {
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

    async fn get_or_fetch<F, Fut>(&self, base_url: &str, fetch: F) -> Result<T, ApiError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, ApiError>>,
    {
        let key = Self::normalize_key(base_url);
        {
            let guard = self.entries.read().await;
            if let Some(entry) = guard.get(&key)
                && entry.fetched_at.elapsed() < self.ttl
            {
                return Ok(entry.value.clone());
            }
        }

        let _gate = self.refresh_gate.lock().await;
        {
            let guard = self.entries.read().await;
            if let Some(entry) = guard.get(&key)
                && entry.fetched_at.elapsed() < self.ttl
            {
                return Ok(entry.value.clone());
            }
        }

        let value = fetch().await?;
        let mut guard = self.entries.write().await;
        guard.insert(
            key,
            CachedEntry {
                value: value.clone(),
                fetched_at: Instant::now(),
            },
        );
        Ok(value)
    }

    async fn invalidate(&self) {
        let mut guard = self.entries.write().await;
        guard.clear();
    }
}

type RootMaterialCache = RootCache<RootProtectionMaterialBundle>;
type RootKeyringCache = RootCache<RootProtectionKeyring>;

static ROOT_MATERIAL_CACHE: LazyLock<RootMaterialCache> =
    LazyLock::new(|| RootMaterialCache::new(ROOT_MATERIAL_CACHE_TTL));
static ROOT_KEYRING_CACHE: LazyLock<RootKeyringCache> =
    LazyLock::new(|| RootKeyringCache::new(ROOT_MATERIAL_CACHE_TTL));

/// 主动清空根授权保护材料短缓存与进程级密钥环（导入/更新/Rekey 根许可证后调用）。
pub async fn invalidate_root_material_cache() {
    ROOT_MATERIAL_CACHE.invalidate().await;
    ROOT_KEYRING_CACHE.invalidate().await;
    clear_root_material_keyring();
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

/// 获取完整多版本根保护密钥环（进程内 60s 短缓存，启动期零预拉，按需懒加载）。
///
/// # Errors
///
/// `cert-service` 网络请求失败、尚未导入许可证或序列化失败时返回错误。
pub async fn fetch_root_protection_keyring_cached(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
) -> Result<RootProtectionKeyring, ApiError> {
    ROOT_KEYRING_CACHE
        .get_or_fetch(base_url, || async {
            mazbri_core::crypto::tenant_key::fetch_root_protection_keyring(client, base_url, token)
                .await
                .map_err(|error| {
                    ApiError::internal(format!("Failed to fetch root protection keyring: {error}"))
                })
        })
        .await
}

/// 强制失效缓存并重新拉取、装载进程共享 `RootMaterialKeyring`（许可证导入或 `Rekey` 后立即调用）。
///
/// # Errors
///
/// 拉取或构造密钥环失败时返回错误。
pub async fn refresh_root_material_keyring(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
) -> Result<Arc<RootMaterialKeyring>, ApiError> {
    invalidate_root_material_cache().await;
    let protection = fetch_root_protection_keyring_cached(client, base_url, token).await?;
    let keyring = RootMaterialKeyring::from_protection_keyring(&protection)
        .map_err(|e| ApiError::internal(format!("Invalid root material keyring: {e}")))?;
    Ok(install_root_material_keyring(keyring))
}

fn resolve_cert_token() -> Result<String, ApiError> {
    get_cert_service_token()
        .map_err(|e| ApiError::internal(format!("Cert service token unavailable: {e}")))
}

/// 按需懒加载并装载可用于新凭据/私钥加密写入的 `RootMaterialKeyring`。
///
/// 若当前尚未导入商业许可证或许可证处于禁止新签发状态（宽限期满/吊销），直接返回 `403 LICENSE_ISSUANCE_NOT_ALLOWED`。
///
/// # Errors
///
/// 许可证禁止写入或根密钥环拉取失败时返回错误。
pub async fn ensure_root_material_keyring_for_write(
    client: &reqwest::Client,
    cert_cfg: &CertServiceConfig,
) -> Result<Arc<RootMaterialKeyring>, ApiError> {
    let token = resolve_cert_token()?;
    ensure_root_material_keyring_for_write_raw(client, &cert_cfg.url, &token).await
}

/// 按显式 `(base_url, token)` 懒加载写入态 `RootMaterialKeyring`。
///
/// # Errors
///
/// 许可证禁止新签发或根密钥环拉取失败时返回错误。
pub async fn ensure_root_material_keyring_for_write_raw(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
) -> Result<Arc<RootMaterialKeyring>, ApiError> {
    ensure_license_issuance_allowed(client, base_url, token).await?;
    let protection = fetch_root_protection_keyring_cached(client, base_url, token).await?;
    if !protection.issuance_allowed {
        return Err(ApiError::license_issuance_not_allowed());
    }
    let keyring = RootMaterialKeyring::from_protection_keyring(&protection)
        .map_err(|e| ApiError::internal(format!("Invalid root material keyring: {e}")))?;
    Ok(install_root_material_keyring(keyring))
}

/// 按需懒加载并装载可用于历史凭据/租户密钥解密读取的 `RootMaterialKeyring`（支持许可证宽限期解密）。
///
/// # Errors
///
/// 根密钥环拉取或构造失败时返回错误。
pub async fn ensure_root_material_keyring_for_read(
    client: &reqwest::Client,
    cert_cfg: &CertServiceConfig,
) -> Result<Arc<RootMaterialKeyring>, ApiError> {
    let token = resolve_cert_token()?;
    ensure_root_material_keyring_for_read_raw(client, &cert_cfg.url, &token).await
}

/// 按显式 `(base_url, token)` 懒加载读取态 `RootMaterialKeyring`。
///
/// # Errors
///
/// 根密钥环拉取或构造失败时返回错误。
pub async fn ensure_root_material_keyring_for_read_raw(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
) -> Result<Arc<RootMaterialKeyring>, ApiError> {
    let protection = fetch_root_protection_keyring_cached(client, base_url, token).await?;
    let keyring = RootMaterialKeyring::from_protection_keyring(&protection)
        .map_err(|e| ApiError::internal(format!("Invalid root material keyring: {e}")))?;
    Ok(install_root_material_keyring(keyring))
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
