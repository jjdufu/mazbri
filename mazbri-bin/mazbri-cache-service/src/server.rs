use crate::tarpc_service::CacheTarpcService;
use anyhow::Result;
use mazbri_cache::{CacheConfig, CacheService};
use mazbri_core::config_watch::{CACHE_L1_VERSION_KEY, ConfigWatchConfig, spawn_config_watch};
use mazbri_rpc::cache::CacheService as CacheRpcService;
use mazbri_tls::TlsAcceptor;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

pub async fn serve(
    addr: SocketAddr,
    db: Arc<PgPool>,
    platform_watch: Option<(Arc<PgPool>, mazbri_core::DatabaseConfig, ConfigWatchConfig)>,
    cache_config: CacheConfig,
    redis: mazbri_core::redis::RedisClient,
    tls_acceptor: Option<TlsAcceptor>,
    max_connections: u32,
) -> Result<()> {
    info!(
        "Starting tarpc cache server on {} (max connections: {})",
        addr, max_connections
    );
    let cache_service = CacheService::new(db, cache_config.clone(), redis.clone());
    let cache = Arc::clone(cache_service.cache());
    let ttl_manager = Arc::clone(cache_service.ttl_manager());

    // Cross-replica L1 eviction via Redis Pub/Sub (always active even without platform_database).
    {
        let cache_for_pubsub = Arc::clone(&cache);
        let channel = redis.cache_l1_invalidation_channel();
        let _ = redis.spawn_pubsub_listener(channel, "cache_service_l1_inv", move |payload| {
            let cache_inner = Arc::clone(&cache_for_pubsub);
            tokio::spawn(async move {
                let target = payload.trim();
                if target.is_empty() || target == "*" {
                    cache_inner.clear_all_memory().await;
                } else {
                    cache_inner.invalidate_config_cache(target);
                    let _ = cache_inner.memory_cache().clear_tenant(target).await;
                    let _ = cache_inner.memory_cache().clear_tenant("__shared__").await;
                }
            });
        });
    }

    let platform_db = if let Some((platform_pool, listen_db, watch_cfg)) = platform_watch {
        let cache_for_watch = Arc::clone(&cache);
        spawn_config_watch(
            (*platform_pool).clone(),
            listen_db,
            watch_cfg,
            CACHE_L1_VERSION_KEY,
            "cache_service_l1_version",
            move || {
                let cache_inner = Arc::clone(&cache_for_watch);
                async move {
                    cache_inner.clear_all_memory().await;
                }
            },
        );
        Some(platform_pool)
    } else {
        None
    };

    let service = CacheTarpcService::new(cache, ttl_manager, platform_db);
    mazbri_service::run_tarpc_server!(
        addr,
        service,
        tls_acceptor,
        max_connections,
        |svc: CacheTarpcService, ch| {
            mazbri_core::tarpc_middleware::trace::execute_traced(ch, svc.serve())
        }
    )
    .await
    .map_err(|e| anyhow::anyhow!("tarpc server error: {e}"))?;
    info!("缓存服务关闭完成");
    Ok(())
}
