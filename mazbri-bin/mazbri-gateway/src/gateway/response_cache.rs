//! Tenant response cache via mazbri-cache-service (H-05).
//!
//! Reads/writes proxy responses when the tenant has `cache_configs.enabled`.
//! Fail-open on any cache error so proxy availability is not coupled to cache.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use mazbri_core::num::SaturatingFrom;
use super::execution::{ProxyBody, UpstreamResponse};
use super::validation::GatewayRequest;
use crate::config::ServiceEndpoint;
use crate::tarpc_client::TarpcClientPool;
use mazbri_rpc::{CacheGetRequest, CacheSetRequest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

/// Max body bytes we will put into the tenant response cache.
const MAX_CACHEABLE_BODY_BYTES: usize = 512 * 1024;

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
            ProxyBody::JsonRaw(bytes) => {
                let value = serde_json::from_slice(bytes.as_ref()).ok()?;
                CachedBody::Json(value)
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
                let len = serde_json::to_vec(&v).map_or(0, |b| b.len() as u64);
                (ProxyBody::JsonValue(v), len)
            }
            CachedBody::Text(s) => {
                let len = s.len() as u64;
                (ProxyBody::Text(s), len)
            }
            CachedBody::Binary(encoded) => {
                match STANDARD.decode(encoded) {
                    Ok(bytes) => {
                        let len = bytes.len() as u64;
                        (ProxyBody::Binary(bytes), len)
                    }
                    Err(_) => (ProxyBody::Empty, 0),
                }
            }
        };
        UpstreamResponse {
            status: self.status,
            body,
            error: self.error,
            content_type: self.content_type,
            body_bytes_len,
        }
    }
}

pub(crate) fn build_proxy_cache_key(req: &GatewayRequest) -> String {
    let mut hasher = Sha256::new();
    hasher.update(req.product_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(req.method.to_ascii_uppercase().as_bytes());
    hasher.update(b"\0");
    hasher.update(req.path.as_bytes());
    hasher.update(b"\0");
    if let Some(query) = &req.query {
        let mut pairs: Vec<_> = query.iter().collect();
        pairs.sort_by(|a, b| a.0.cmp(b.0));
        for (k, v) in pairs {
            hasher.update(k.as_bytes());
            hasher.update(b"=");
            hasher.update(v.as_bytes());
            hasher.update(b"&");
        }
    }
    hasher.update(b"\0");
    if let Some(body) = &req.body {
        if let Ok(bytes) = serde_json::to_vec(body) {
            hasher.update(&bytes);
        }
    }
    format!("proxy:{}", hex::encode(hasher.finalize()))
}

pub(crate) async fn try_get_cached_response(
    pool: &TarpcClientPool,
    endpoint: &ServiceEndpoint,
    tenant_id: &str,
    cache_key: &str,
) -> Option<CachedUpstreamResponse> {
    let client = match pool
        .get_cache_client(endpoint.url())
        .await
    {
        Ok(c) => c,
        Err(e) => {
            debug!(error = %e, "Proxy response cache get: client unavailable (fail-open)");
            return None;
        }
    };
    let response = match client
        .get(
            tarpc::context::current(),
            CacheGetRequest {
                tenant_id: tenant_id.to_string(),
                key: cache_key.to_string(),
            },
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            debug!(error = %e, "Proxy response cache get: RPC failed (fail-open)");
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
    match serde_json::from_slice::<CachedUpstreamResponse>(&response.value) {
        Ok(cached) => Some(cached),
        Err(e) => {
            warn!(error = %e, "Proxy response cache get: decode failed");
            None
        }
    }
}

pub(crate) async fn try_put_cached_response(
    pool: &TarpcClientPool,
    endpoint: &ServiceEndpoint,
    tenant_id: &str,
    cache_key: &str,
    response: &UpstreamResponse,
    path: &str,
) {
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
    let client = match pool
        .get_cache_client(endpoint.url())
        .await
    {
        Ok(c) => c,
        Err(e) => {
            debug!(error = %e, "Proxy response cache set: client unavailable (fail-open)");
            return;
        }
    };
    // ttl_seconds=0 → cache-service uses tenant default_ttl from cache_configs.
    match client
        .set(
            tarpc::context::current(),
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
            let msg = r.error.as_ref().map(mazbri_rpc::RpcError::message).unwrap_or_default();
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
