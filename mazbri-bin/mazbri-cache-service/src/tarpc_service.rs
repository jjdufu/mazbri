use mazbri_cache::{Cache, RedisCache, TtlManager};
use mazbri_rpc::RpcError;
use mazbri_rpc::cache::{
    CacheService, CacheStats, ClearRequest, ClearResponse, DeleteRequest, DeleteResponse,
    ExistsRequest, ExistsResponse, GetMultiRequest, GetMultiResponse, GetRequest, GetResponse,
    GetStatsRequest, GetStatsResponse, HealthResponse, SetMultiRequest, SetMultiResponse,
    SetRequest, SetResponse,
};
use sqlx::types::chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tarpc::context::Context;
#[derive(Clone)]
pub struct CacheTarpcService {
    cache: Arc<RedisCache>,
    ttl_manager: Arc<TtlManager>,
}
impl CacheTarpcService {
    pub fn new(cache: Arc<RedisCache>, ttl_manager: Arc<TtlManager>) -> Self {
        Self { cache, ttl_manager }
    }
}
impl CacheService for CacheTarpcService {
    async fn get(self, _context: Context, req: GetRequest) -> GetResponse {
        match self.cache.get(&req.tenant_id, &req.key).await {
            Ok(Some(entry)) => GetResponse {
                found: true,
                value: entry.value,
                ttl_remaining: entry.ttl as i64,
                error: None,
            },
            Ok(None) => GetResponse {
                found: false,
                value: Vec::new(),
                ttl_remaining: 0,
                error: None,
            },
            Err(e) => {
                let rpc_err = RpcError::from(&e);
                GetResponse {
                    found: false,
                    value: Vec::new(),
                    ttl_remaining: 0,
                    error: Some(rpc_err),
                }
            }
        }
    }
    async fn set(self, _context: Context, req: SetRequest) -> SetResponse {
        let ttl = Duration::from_secs(req.ttl_seconds.max(0) as u64);
        let endpoint = if req.endpoint.is_empty() {
            None
        } else {
            Some(req.endpoint.as_str())
        };
        match self
            .cache
            .set_with_endpoint(&req.tenant_id, &req.key, req.value, ttl, endpoint)
            .await
        {
            Ok(()) => SetResponse {
                success: true,
                error: None,
            },
            Err(e) => {
                let rpc_err = RpcError::from(&e);
                SetResponse {
                    success: false,
                    error: Some(rpc_err),
                }
            }
        }
    }
    async fn delete(self, _context: Context, req: DeleteRequest) -> DeleteResponse {
        let exists = match self.cache.exists(&req.tenant_id, &req.key).await {
            Ok(exists) => exists,
            Err(e) => {
                let rpc_err = RpcError::from(&e);
                return DeleteResponse {
                    success: false,
                    was_found: false,
                    error: Some(rpc_err),
                };
            }
        };
        if !exists {
            return DeleteResponse {
                success: true,
                was_found: false,
                error: None,
            };
        }
        match self.cache.delete(&req.tenant_id, &req.key).await {
            Ok(()) => DeleteResponse {
                success: true,
                was_found: true,
                error: None,
            },
            Err(e) => {
                let rpc_err = RpcError::from(&e);
                DeleteResponse {
                    success: false,
                    was_found: false,
                    error: Some(rpc_err),
                }
            }
        }
    }
    async fn clear(self, _context: Context, req: ClearRequest) -> ClearResponse {
        match self.cache.clear_tenant_with_count(&req.tenant_id).await {
            Ok(entries_cleared) => ClearResponse {
                success: true,
                entries_cleared: entries_cleared as i32,
                error: None,
            },
            Err(e) => {
                let rpc_err = RpcError::from(&e);
                ClearResponse {
                    success: false,
                    entries_cleared: 0,
                    error: Some(rpc_err),
                }
            }
        }
    }
    async fn exists(self, _context: Context, req: ExistsRequest) -> ExistsResponse {
        match self.cache.exists(&req.tenant_id, &req.key).await {
            Ok(exists) => ExistsResponse {
                exists,
                error: None,
            },
            Err(e) => {
                let rpc_err = RpcError::from(&e);
                ExistsResponse {
                    exists: false,
                    error: Some(rpc_err),
                }
            }
        }
    }
    async fn get_multi(self, _context: Context, req: GetMultiRequest) -> GetMultiResponse {
        let keys: Vec<String> = req.keys.clone();
        match self.cache.get_multi(&req.tenant_id, &keys).await {
            Ok(results) => {
                let mut values = std::collections::HashMap::new();
                let mut missing_keys = Vec::new();
                for (key, result) in req.keys.into_iter().zip(results) {
                    match result {
                        Some(entry) => {
                            values.insert(key, entry.value);
                        }
                        None => {
                            missing_keys.push(key);
                        }
                    }
                }
                GetMultiResponse {
                    values,
                    missing_keys,
                    error: None,
                }
            }
            Err(e) => {
                let rpc_err = RpcError::from(&e);
                GetMultiResponse {
                    values: std::collections::HashMap::new(),
                    missing_keys: Vec::new(),
                    error: Some(rpc_err),
                }
            }
        }
    }
    async fn set_multi(self, _context: Context, req: SetMultiRequest) -> SetMultiResponse {
        let entries: Vec<(String, Vec<u8>, Duration)> = req
            .entries
            .into_iter()
            .map(|(key, entry)| {
                let ttl = Duration::from_secs(entry.ttl_seconds as u64);
                (key, entry.value, ttl)
            })
            .collect();
        let count = entries.len() as i32;
        match self.cache.set_multi(&req.tenant_id, entries).await {
            Ok(()) => SetMultiResponse {
                success: true,
                entries_set: count,
                error: None,
            },
            Err(e) => {
                let rpc_err = RpcError::from(&e);
                SetMultiResponse {
                    success: false,
                    entries_set: 0,
                    error: Some(rpc_err),
                }
            }
        }
    }
    async fn get_stats(self, _context: Context, req: GetStatsRequest) -> GetStatsResponse {
        let stats = if req.tenant_id.is_empty() {
            self.ttl_manager.get_global_stats().await
        } else {
            self.ttl_manager.get_stats(&req.tenant_id).await
        };
        match stats {
            Ok(cache_stats) => {
                let rpc_stats = CacheStats {
                    total_entries: cache_stats.total_entries as i64,
                    total_size_bytes: cache_stats.total_size as i64,
                    hit_count: 0,
                    miss_count: 0,
                    hit_rate: 0.0,
                    eviction_count: cache_stats.expired_entries as i64,
                };
                GetStatsResponse {
                    stats: Some(rpc_stats),
                    error: None,
                }
            }
            Err(e) => {
                let rpc_err = RpcError::from(&e);
                GetStatsResponse {
                    stats: None,
                    error: Some(rpc_err),
                }
            }
        }
    }
    async fn health(self, _context: Context) -> HealthResponse {
        HealthResponse {
            status: "ok".to_string(),
            service: "cache-service".to_string(),
            timestamp: Utc::now().timestamp(),
        }
    }
}
