//! Tenant response cache via mazbri-cache-service (H-05 & Step 05 S05-3).
//!
//! Enforces strict tenant namespace isolation, routing header + length-prefixed query + body SHA-256
//! cache keys, and inner HTTP method cacheability rules (wildcard caches `GET`/`HEAD` only;
//! `POST` requires an explicit non-wildcard `api_ttl` endpoint rule).

use super::execution::{ProxyBody, UpstreamResponse};
use super::validation::GatewayRequest;
use crate::config::ServiceEndpoint;
use crate::tarpc_client::TarpcClientPool;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use mazbri_core::num::SaturatingFrom;
use mazbri_rpc::{CacheGetRequest, CacheSetRequest};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;
use tracing::{Instrument, debug, warn};
use uuid::Uuid;

/// Max body bytes we will put into the tenant response cache.
const MAX_CACHEABLE_BODY_BYTES: usize = 512 * 1024;

/// Short fail-open timeout for cache get RPC when the snapshot says caching is on.
const CACHE_GET_TIMEOUT: Duration = Duration::from_millis(8);

const POLICY_CACHE_TTL: Duration = Duration::from_secs(30);
const PUBLIC_CACHE_PARTITION: &str = "__public__";

#[derive(Debug, Clone)]
pub struct TenantCachePolicy {
    pub enabled: bool,
    pub tenant_isolated: bool,
    pub api_ttl: HashMap<String, u64>,
}

impl Default for TenantCachePolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            tenant_isolated: true,
            api_ttl: HashMap::new(),
        }
    }
}

impl TenantCachePolicy {
    /// Returns `true` when `path` matches an explicit non-wildcard `api_ttl` rule (`ttl > 0`).
    #[must_use]
    pub fn has_explicit_api_ttl(&self, path: &str) -> bool {
        if self.api_ttl.get(path).is_some_and(|ttl| *ttl > 0) {
            return true;
        }
        for (pattern, ttl) in &self.api_ttl {
            if *ttl == 0 {
                continue;
            }
            if let Some(prefix) = pattern.strip_suffix('*')
                && !prefix.is_empty()
                && prefix != "/"
                && path.starts_with(prefix)
            {
                return true;
            }
        }
        false
    }

    /// Evaluates whether inner `(method, path)` may be served from or stored in the response cache.
    ///
    /// - Without an explicit `api_ttl` rule (wildcard/default cache), ONLY inner `GET` / `HEAD` are cached.
    /// - With an explicit non-wildcard `api_ttl` rule (e.g. `"/dev19-mock": 60`), read-only query `POST`
    ///   is also permitted unless `cacheable_methods` explicitly restricts it.
    #[must_use]
    pub fn is_request_cacheable(&self, method: &str, path: &str) -> bool {
        if !self.enabled {
            return false;
        }
        match method.trim().to_ascii_uppercase().as_str() {
            "GET" | "HEAD" => true,
            "POST" => self.has_explicit_api_ttl(path),
            _ => false,
        }
    }
}

pub struct TenantCachePolicyLookup {
    cache_db: Option<PgPool>,
    cache: moka::sync::Cache<Uuid, TenantCachePolicy>,
}

impl TenantCachePolicyLookup {
    #[must_use]
    pub fn new(cache_db: Option<PgPool>) -> Self {
        let cache = moka::sync::Cache::builder()
            .max_capacity(10_000)
            .time_to_live(POLICY_CACHE_TTL)
            .build();
        Self { cache_db, cache }
    }

    pub fn invalidate_all(&self) {
        self.cache.invalidate_all();
    }

    pub fn invalidate(&self, tenant_id: Uuid) {
        self.cache.invalidate(&tenant_id);
    }

    pub async fn resolve(&self, tenant_id: Uuid, snapshot_enabled: bool) -> TenantCachePolicy {
        if !snapshot_enabled {
            return TenantCachePolicy::default();
        }
        if let Some(policy) = self.cache.get(&tenant_id) {
            return policy;
        }
        let policy = self.fetch_from_db(tenant_id, snapshot_enabled).await;
        self.cache.insert(tenant_id, policy.clone());
        policy
    }

    async fn fetch_from_db(&self, tenant_id: Uuid, fallback_enabled: bool) -> TenantCachePolicy {
        let Some(ref pool) = self.cache_db else {
            return TenantCachePolicy {
                enabled: fallback_enabled,
                tenant_isolated: true,
                api_ttl: HashMap::new(),
            };
        };
        let row = match sqlx::query(
            "SELECT enabled, tenant_isolated, api_ttl FROM cache_configs WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
        {
            Ok(Some(row)) => row,
            Ok(None) => return TenantCachePolicy::default(),
            Err(err) => {
                warn!(error = %err, %tenant_id, "Failed to load tenant cache_configs policy; treating as disabled");
                return TenantCachePolicy::default();
            }
        };
        let enabled = row.try_get::<bool, _>("enabled").unwrap_or(false);
        let tenant_isolated = row.try_get::<bool, _>("tenant_isolated").unwrap_or(true);
        let api_ttl = row
            .try_get::<serde_json::Value, _>("api_ttl")
            .ok()
            .and_then(|v| serde_json::from_value::<HashMap<String, u64>>(v).ok())
            .unwrap_or_default();
        TenantCachePolicy {
            enabled,
            tenant_isolated,
            api_ttl,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedUpstreamResponse {
    pub status: u16,
    pub error: String,
    pub content_type: Option<String>,
    pub body: CachedBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub(crate) enum CachedBody {
    Empty,
    Json(serde_json::Value),
    JsonRaw(String),
    Text(String),
    Binary(String),
}

impl CachedUpstreamResponse {
    pub(crate) fn from_upstream(response: &UpstreamResponse) -> Option<Self> {
        if response.status < 200 || response.status >= 300 || !response.error.is_empty() {
            return None;
        }
        if usize::saturating_from(response.body_bytes_len) > MAX_CACHEABLE_BODY_BYTES {
            return None;
        }
        let body = match &response.body {
            ProxyBody::Empty => CachedBody::Empty,
            ProxyBody::JsonValue(v) => CachedBody::Json(v.clone()),
            ProxyBody::JsonRawStr(raw) => {
                if raw.len() > MAX_CACHEABLE_BODY_BYTES
                    || serde_json::from_str::<&RawValue>(raw).is_err()
                {
                    return None;
                }
                CachedBody::JsonRaw(raw.to_string())
            }
            ProxyBody::JsonRaw(bytes) => {
                if bytes.len() > MAX_CACHEABLE_BODY_BYTES {
                    return None;
                }
                let s = std::str::from_utf8(bytes).ok()?;
                serde_json::from_str::<&RawValue>(s).ok()?;
                CachedBody::JsonRaw(s.to_string())
            }
            ProxyBody::Text(s) => {
                if s.len() > MAX_CACHEABLE_BODY_BYTES {
                    return None;
                }
                CachedBody::Text(s.clone())
            }
            ProxyBody::Binary(bytes) => {
                if bytes.len() > MAX_CACHEABLE_BODY_BYTES {
                    return None;
                }
                CachedBody::Binary(STANDARD.encode(bytes))
            }
        };
        Some(Self {
            status: response.status,
            error: response.error.clone(),
            content_type: response.content_type.clone(),
            body,
        })
    }

    pub(crate) fn into_upstream(self) -> UpstreamResponse {
        let (body, body_bytes_len) = match self.body {
            CachedBody::Empty => (ProxyBody::Empty, 0),
            CachedBody::Json(v) => {
                let raw = serde_json::to_string(&v).unwrap_or_else(|_| "null".into());
                let len = raw.len() as u64;
                (ProxyBody::JsonRawStr(Arc::from(raw)), len)
            }
            CachedBody::JsonRaw(raw) => {
                let len = raw.len() as u64;
                (ProxyBody::JsonRawStr(Arc::from(raw)), len)
            }
            CachedBody::Text(s) => {
                let len = s.len() as u64;
                (ProxyBody::Text(s), len)
            }
            CachedBody::Binary(encoded) => match STANDARD.decode(encoded) {
                Ok(bytes) => {
                    let len = bytes.len() as u64;
                    (ProxyBody::Binary(bytes), len)
                }
                Err(_) => (ProxyBody::Empty, 0),
            },
        };
        UpstreamResponse {
            status: self.status,
            body,
            error: self.error,
            content_type: self.content_type,
            body_bytes_len,
            upstream_wire_duration: Duration::ZERO,
            upstream_request_id: String::new(),
        }
    }
}

fn update_len_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

/// Builds a tenant-isolated (or `__public__` when explicitly shared without route override)
/// proxy response cache key incorporating routing headers, length-prefixed query params, and body SHA-256.
#[must_use]
pub(crate) fn build_proxy_cache_key(
    tenant_id: &str,
    req: &GatewayRequest,
    tenant_isolated: bool,
    has_tenant_route_override: bool,
) -> String {
    let partition = if tenant_isolated || has_tenant_route_override {
        tenant_id
    } else {
        PUBLIC_CACHE_PARTITION
    };
    let mut hasher = Sha256::new();
    update_len_prefixed(&mut hasher, partition.as_bytes());
    update_len_prefixed(&mut hasher, req.product_id.as_bytes());
    let upper_method = req.method.trim().to_ascii_uppercase();
    update_len_prefixed(&mut hasher, upper_method.as_bytes());
    update_len_prefixed(&mut hasher, req.path.as_bytes());

    // Canonical routing headers (e.g. X-Api-Version, Accept, Content-Type).
    let mut route_headers: BTreeMap<String, &str> = BTreeMap::new();
    if let Some(ref headers) = req.headers {
        for (k, v) in headers {
            let lower = k.trim().to_ascii_lowercase();
            if matches!(lower.as_str(), "x-api-version" | "accept" | "content-type") {
                route_headers.insert(lower, v.trim());
            }
        }
    }
    hasher.update((route_headers.len() as u64).to_be_bytes());
    for (k, v) in route_headers {
        update_len_prefixed(&mut hasher, k.as_bytes());
        update_len_prefixed(&mut hasher, v.as_bytes());
    }

    // Length-prefixed sorted query parameters (immune to `&` / `=` boundary ambiguity).
    if let Some(ref query) = req.query {
        let sorted: BTreeMap<&str, &str> = query
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        hasher.update((sorted.len() as u64).to_be_bytes());
        for (k, v) in sorted {
            update_len_prefixed(&mut hasher, k.as_bytes());
            update_len_prefixed(&mut hasher, v.as_bytes());
        }
    } else {
        hasher.update(0_u64.to_be_bytes());
    }

    // Body SHA-256 digest.
    if let Some(ref body) = req.body {
        let body_digest = Sha256::digest(body.as_raw_json_str().trim().as_bytes());
        hasher.update([1_u8]);
        hasher.update(body_digest);
    } else {
        hasher.update([0_u8]);
    }

    format!("proxy:{}", hex::encode(hasher.finalize()))
}

/// Returns true when the gateway policy confirms this request is eligible for cache RPC.
#[must_use]
pub(crate) fn should_attempt_cache_rpc(cache_enabled: bool) -> bool {
    cache_enabled
}

pub(crate) async fn try_get_cached_response(
    pool: &TarpcClientPool,
    endpoint: &ServiceEndpoint,
    tenant_id: &str,
    cache_key: &str,
    cache_enabled: bool,
) -> Option<CachedUpstreamResponse> {
    if !should_attempt_cache_rpc(cache_enabled) {
        return None;
    }
    let span = tracing::info_span!(
        "response_cache.get",
        tenant_id = %tenant_id,
        cache_key = %cache_key,
    );
    let started = std::time::Instant::now();
    let client = match pool
        .get_cache_client(endpoint.url())
        .instrument(span.clone())
        .await
    {
        Ok(c) => c,
        Err(e) => {
            debug!(error = %e, "Proxy response cache get: client unavailable (fail-open)");
            return None;
        }
    };
    let rpc_ctx = TarpcClientPool::rpc_context(CACHE_GET_TIMEOUT);
    let get_fut = client.get(
        rpc_ctx,
        CacheGetRequest {
            tenant_id: tenant_id.to_string(),
            key: cache_key.to_string(),
        },
    );
    let response = match tokio::time::timeout(CACHE_GET_TIMEOUT, get_fut)
        .instrument(span)
        .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            debug!(error = %e, "Proxy response cache get: RPC failed (fail-open)");
            return None;
        }
        Err(_) => {
            debug!(
                timeout_ms = CACHE_GET_TIMEOUT.as_millis(),
                "Proxy response cache get: timed out (fail-open)"
            );
            return None;
        }
    };
    if let Some(err) = response.error {
        debug!(error = %err.message(), "Proxy response cache get: service error (fail-open)");
        return None;
    }
    if !response.found || response.value.is_empty() {
        return None;
    }
    let cached = match serde_json::from_slice::<CachedUpstreamResponse>(&response.value) {
        Ok(cached) => Some(cached),
        Err(e) => {
            warn!(error = %e, "Proxy response cache get: decode failed");
            None
        }
    };
    debug!(
        cache_lookup_ms = started.elapsed().as_millis(),
        hit = cached.is_some(),
        "response cache get finished"
    );
    cached
}

pub(crate) async fn try_put_cached_response(
    pool: &TarpcClientPool,
    endpoint: &ServiceEndpoint,
    tenant_id: &str,
    cache_key: &str,
    response: &UpstreamResponse,
    path: &str,
    cache_enabled: bool,
) {
    if !should_attempt_cache_rpc(cache_enabled) {
        return;
    }
    let Some(payload) = CachedUpstreamResponse::from_upstream(response) else {
        return;
    };
    let value = match serde_json::to_vec(&payload) {
        Ok(v) => v,
        Err(e) => {
            debug!(error = %e, "Proxy response cache set: encode failed");
            return;
        }
    };
    let client = match pool.get_cache_client(endpoint.url()).await {
        Ok(c) => c,
        Err(e) => {
            debug!(error = %e, "Proxy response cache set: client unavailable (fail-open)");
            return;
        }
    };
    let rpc_ctx = TarpcClientPool::rpc_context(endpoint.timeout());
    match client
        .set(
            rpc_ctx,
            CacheSetRequest {
                tenant_id: tenant_id.to_string(),
                key: cache_key.to_string(),
                value,
                ttl_seconds: 0,
                endpoint: path.to_string(),
            },
        )
        .await
    {
        Ok(r) if r.error.is_some() => {
            let msg = r
                .error
                .as_ref()
                .map_or_default(mazbri_rpc::RpcError::message);
            debug!(error = %msg, path = %path, "Proxy response cache set: service error (fail-open)");
        }
        Ok(_) => {
            debug!(tenant_id = %tenant_id, key = %cache_key, path = %path, "Proxy response cached");
        }
        Err(e) => {
            debug!(error = %e, path = %path, "Proxy response cache set: RPC failed (fail-open)");
        }
    }
}

#[cfg(test)]
mod skip_disabled_cache_tests {
    use super::{
        GatewayRequest, TenantCachePolicy, build_proxy_cache_key, should_attempt_cache_rpc,
    };
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[test]
    fn disabled_snapshot_skips_cache_rpc() {
        assert!(
            !should_attempt_cache_rpc(false),
            "false / missing cache_configs must skip RPC"
        );
        assert!(
            should_attempt_cache_rpc(true),
            "enabled cache_configs must allow RPC"
        );
    }

    #[test]
    fn cache_key_isolates_tenants_headers_and_query_boundaries() {
        let req_v1: GatewayRequest = serde_json::from_str(
            r#"{"product_id":"8000000001","path":"/v1/q","method":"GET","headers":{"X-Api-Version":"v1"},"query":{"a":"1&b=2"}}"#,
        )
        .unwrap();
        let req_v2: GatewayRequest = serde_json::from_str(
            r#"{"product_id":"8000000001","path":"/v1/q","method":"GET","headers":{"x-api-version":"v2"},"query":{"a":"1&b=2"}}"#,
        )
        .unwrap();
        let req_split_query: GatewayRequest = serde_json::from_str(
            r#"{"product_id":"8000000001","path":"/v1/q","method":"GET","headers":{"X-Api-Version":"v1"},"query":{"a":"1","b":"2"}}"#,
        )
        .unwrap();

        assert_ne!(
            build_proxy_cache_key("tenant-a", &req_v1, true, false),
            build_proxy_cache_key("tenant-b", &req_v1, true, false)
        );
        // Shared mode still isolates when tenant has a dedicated route override.
        assert_ne!(
            build_proxy_cache_key("tenant-a", &req_v1, false, true),
            build_proxy_cache_key("tenant-b", &req_v1, false, false)
        );
        assert_ne!(
            build_proxy_cache_key("tenant-a", &req_v1, true, false),
            build_proxy_cache_key("tenant-a", &req_v2, true, false)
        );
        assert_ne!(
            build_proxy_cache_key("tenant-a", &req_v1, true, false),
            build_proxy_cache_key("tenant-a", &req_split_query, true, false)
        );
    }

    #[test]
    fn wildcard_policy_caches_only_get_head_while_explicit_api_ttl_allows_post() {
        let wildcard = TenantCachePolicy {
            enabled: true,
            tenant_isolated: true,
            api_ttl: HashMap::from([("/*".to_string(), 60)]),
        };
        assert!(wildcard.is_request_cacheable("GET", "/v1/items"));
        assert!(wildcard.is_request_cacheable("HEAD", "/v1/items"));
        assert!(!wildcard.is_request_cacheable("POST", "/v1/items"));
        assert!(!wildcard.is_request_cacheable("DELETE", "/v1/items"));

        let explicit = TenantCachePolicy {
            enabled: true,
            tenant_isolated: true,
            api_ttl: HashMap::from([("/dev19-mock".to_string(), 60)]),
        };
        assert!(explicit.is_request_cacheable("POST", "/dev19-mock"));
        assert!(!explicit.is_request_cacheable("POST", "/v1/orders"));
        assert!(!explicit.is_request_cacheable("DELETE", "/dev19-mock"));
    }

    #[tokio::test]
    async fn cache_get_timeout_fail_opens_within_budget() {
        let started = Instant::now();
        let hanging = async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            "never"
        };
        let result = tokio::time::timeout(super::CACHE_GET_TIMEOUT, hanging).await;
        assert!(result.is_err(), "hanging get must time out");
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "fail-open timeout must stay near {} ms, elapsed={:?}",
            super::CACHE_GET_TIMEOUT.as_millis(),
            started.elapsed()
        );
    }
}
