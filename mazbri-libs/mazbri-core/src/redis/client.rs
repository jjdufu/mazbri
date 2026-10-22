use crate::redis::config::RedisConfig;
use crate::redis::error::{RedisError, Result};
use crate::redis::keys::{
    fallback_key, gateway_binding_nonce_key, idempotency_key, nonce_key, permission_key,
    rate_limit_key,
};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use redis::cluster::ClusterClient;
use redis::cluster_async::ClusterConnection;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

#[derive(Clone)]
enum InnerConnection {
    Standalone(Box<ConnectionManager>),
    Cluster(ClusterConnection),
}

impl redis::aio::ConnectionLike for InnerConnection {
    fn req_packed_command<'a>(
        &'a mut self,
        cmd: &'a redis::Cmd,
    ) -> redis::RedisFuture<'a, redis::Value> {
        match self {
            InnerConnection::Standalone(conn) => conn.req_packed_command(cmd),
            InnerConnection::Cluster(conn) => conn.req_packed_command(cmd),
        }
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        cmd: &'a redis::Pipeline,
        offset: usize,
        count: usize,
    ) -> redis::RedisFuture<'a, Vec<redis::Value>> {
        match self {
            InnerConnection::Standalone(conn) => conn.req_packed_commands(cmd, offset, count),
            InnerConnection::Cluster(conn) => conn.req_packed_commands(cmd, offset, count),
        }
    }

    fn get_db(&self) -> i64 {
        match self {
            InnerConnection::Standalone(conn) => conn.get_db(),
            InnerConnection::Cluster(conn) => conn.get_db(),
        }
    }
}

enum InnerClient {
    Standalone {
        client: Arc<redis::Client>,
        managers: Arc<[ConnectionManager]>,
        next_manager_index: Arc<AtomicUsize>,
    },
    Cluster {
        client: Arc<ClusterClient>,
        connection: ClusterConnection,
    },
}

#[derive(Clone)]
pub struct RedisClient {
    inner: Arc<InnerClient>,
    key_prefix: Arc<str>,
    connect_timeout: Duration,
    command_timeout: Duration,
}

impl RedisClient {
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        Self::connect_with_config(&RedisConfig {
            url: url.to_string(),
            ..Default::default()
        })
        .await
    }

    pub async fn connect_with_config(config: &RedisConfig) -> anyhow::Result<Self> {
        let connect_timeout = Duration::from_millis(config.connect_timeout_ms.max(1));
        let command_timeout = Duration::from_millis(config.command_timeout_ms.max(1));

        let url_str = if !config.url.trim().is_empty() {
            config.url.clone()
        } else if let Some(ref host) = config.host {
            let port = config.port.unwrap_or(6379);
            let db = config.db.unwrap_or(0);
            let scheme = if config.is_cluster {
                if config.use_tls {
                    "rediss+cluster"
                } else {
                    "redis+cluster"
                }
            } else {
                if config.use_tls { "rediss" } else { "redis" }
            };
            if let Some(ref password) = config.password {
                format!("{scheme}://:{password}@{host}:{port}/{db}")
            } else {
                format!("{scheme}://{host}:{port}/{db}")
            }
        } else {
            anyhow::bail!("Redis configuration must specify either 'url' or 'host'");
        };

        let is_cluster =
            url_str.starts_with("redis+cluster://") || url_str.starts_with("rediss+cluster://");

        let inner = if is_cluster {
            let clean_url = url_str
                .replace("redis+cluster://", "redis://")
                .replace("rediss+cluster://", "rediss://");
            let client = Arc::new(ClusterClient::new(vec![clean_url.as_str()])?);
            let connection = timeout(connect_timeout, client.get_async_connection())
                .await
                .map_err(|_| anyhow::anyhow!("redis cluster connection timed out"))??;
            InnerClient::Cluster { client, connection }
        } else {
            let client = Arc::new(redis::Client::open(url_str.as_str())?);
            let pool_size = config.pool_max_size.clamp(1, 8);
            let mut managers = Vec::with_capacity(pool_size);
            for _ in 0..pool_size {
                let manager = timeout(connect_timeout, ConnectionManager::new((*client).clone()))
                    .await
                    .map_err(|_| anyhow::anyhow!("redis connection timed out"))??;
                managers.push(manager);
            }
            InnerClient::Standalone {
                client,
                managers: Arc::from(managers.into_boxed_slice()),
                next_manager_index: Arc::new(AtomicUsize::new(0)),
            }
        };

        Ok(Self {
            inner: Arc::new(inner),
            key_prefix: Arc::from(config.key_prefix.as_str()),
            connect_timeout,
            command_timeout,
        })
    }

    #[must_use]
    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }

    fn connection(&self) -> InnerConnection {
        match &*self.inner {
            InnerClient::Standalone {
                managers,
                next_manager_index,
                ..
            } => {
                let len = managers.len();
                debug_assert!(len > 0);
                let index = next_manager_index.fetch_add(1, Ordering::Relaxed) % len;
                InnerConnection::Standalone(Box::new(managers[index].clone()))
            }
            InnerClient::Cluster { connection, .. } => InnerConnection::Cluster(connection.clone()),
        }
    }

    async fn fresh_connection(&self) -> Result<InnerConnection> {
        match &*self.inner {
            InnerClient::Standalone { client, .. } => {
                let conn = timeout(
                    self.connect_timeout,
                    ConnectionManager::new((**client).clone()),
                )
                .await
                .map_err(|_| RedisError::Timeout("CONNECT"))?
                .map_err(RedisError::from)?;
                Ok(InnerConnection::Standalone(Box::new(conn)))
            }
            InnerClient::Cluster { client, .. } => {
                let conn = timeout(self.connect_timeout, client.get_async_connection())
                    .await
                    .map_err(|_| RedisError::Timeout("CONNECT"))?
                    .map_err(RedisError::from)?;
                Ok(InnerConnection::Cluster(conn))
            }
        }
    }

    fn should_retry_connection(error: &redis::RedisError) -> bool {
        let message = error.to_string().to_lowercase();
        message.contains("broken pipe")
            || message.contains("connection reset")
            || message.contains("connection refused")
            || message.contains("connection closed")
            || message.contains("server closed the connection")
            || message.contains("already shutdown")
    }

    #[must_use]
    pub fn nonce_key(&self, tenant_id: Uuid, key_version: &str, nonce: &str) -> String {
        nonce_key(&self.key_prefix, tenant_id, key_version, nonce)
    }

    #[must_use]
    pub fn cache_key(&self, tenant_id: &str, cache_key_value: &str) -> String {
        crate::redis::keys::cache_key(&self.key_prefix, tenant_id, cache_key_value)
    }

    #[must_use]
    pub fn permission_key(&self, tenant_id: Uuid, product_id: &str) -> String {
        permission_key(&self.key_prefix, tenant_id, product_id)
    }

    pub async fn set_nonce_if_absent(
        &self,
        tenant_id: Uuid,
        key_version: &str,
        nonce: &str,
        ttl_seconds: i32,
    ) -> Result<bool> {
        let ttl_seconds = ttl_seconds.max(1) as usize;
        let key = self.nonce_key(tenant_id, key_version, nonce);
        self.set_nx_ex(&key, b"1", ttl_seconds).await
    }

    pub async fn set_gateway_binding_nonce_if_absent(
        &self,
        prefix: &str,
        tenant_id: Uuid,
        nonce: &str,
        ttl_seconds: i32,
    ) -> Result<bool> {
        let ttl_seconds = ttl_seconds.max(1) as usize;
        let key = gateway_binding_nonce_key(prefix, tenant_id, nonce);
        self.set_nx_ex(&key, b"1", ttl_seconds).await
    }

    #[must_use]
    pub fn fallback_key(&self, cache_key: &str) -> String {
        fallback_key(&self.key_prefix, cache_key)
    }

    #[must_use]
    pub fn idempotency_key(&self, fingerprint: &str) -> String {
        idempotency_key(&self.key_prefix, fingerprint)
    }

    #[must_use]
    pub fn rate_limit_key(&self, parts: &[&str]) -> String {
        rate_limit_key(&self.key_prefix, parts)
    }

    async fn run<T, F, Fut>(&self, op: &'static str, mut f: F) -> Result<T>
    where
        F: FnMut(InnerConnection) -> Fut,
        Fut: Future<Output = redis::RedisResult<T>>,
    {
        async fn execute<T, Fut>(op: &'static str, command_timeout: Duration, fut: Fut) -> Result<T>
        where
            Fut: Future<Output = redis::RedisResult<T>>,
        {
            timeout(command_timeout, fut)
                .await
                .map_err(|_| RedisError::Timeout(op))?
                .map_err(RedisError::from)
        }

        let first = execute(op, self.command_timeout, f(self.connection())).await;
        match first {
            Ok(value) => Ok(value),
            Err(RedisError::Connection(error)) if Self::should_retry_connection(&error) => {
                let fresh = self.fresh_connection().await?;
                execute(op, self.command_timeout, f(fresh)).await
            }
            Err(error) => Err(error),
        }
    }

    pub async fn ping(&self) -> Result<()> {
        self.run("PING", |mut conn| async move {
            let _: String = redis::cmd("PING").query_async(&mut conn).await?;
            Ok(())
        })
        .await
    }

    pub async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.run("GET", |mut conn| async move { conn.get(key).await })
            .await
    }

    pub async fn set_ex_bytes(&self, key: &str, value: &[u8], ttl_secs: usize) -> Result<()> {
        self.run("SET EX", |mut conn| async move {
            let _: () = conn.set_ex(key, value, ttl_secs as u64).await?;
            Ok(())
        })
        .await
    }

    pub async fn set_nx_ex(&self, key: &str, value: &[u8], ttl_secs: usize) -> Result<bool> {
        self.run("SET NX EX", |mut conn| async move {
            let result: Option<String> = redis::cmd("SET")
                .arg(key)
                .arg(value)
                .arg("NX")
                .arg("EX")
                .arg(ttl_secs)
                .query_async(&mut conn)
                .await?;
            Ok(result.is_some())
        })
        .await
    }

    pub async fn del(&self, key: &str) -> Result<u64> {
        self.run("DEL", |mut conn| async move { conn.del(key).await })
            .await
    }

    pub async fn exists(&self, key: &str) -> Result<bool> {
        self.run("EXISTS", |mut conn| async move { conn.exists(key).await })
            .await
    }

    pub async fn mget_bytes(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>> {
        self.run("MGET", |mut conn| async move {
            redis::cmd("MGET").arg(keys).query_async(&mut conn).await
        })
        .await
    }

    pub async fn eval_i64(&self, script: &str, keys: &[String], args: &[String]) -> Result<i64> {
        self.run("EVAL", |mut conn| async move {
            let mut command = redis::cmd("EVAL");
            command.arg(script).arg(keys.len());
            for key in keys {
                command.arg(key);
            }
            for arg in args {
                command.arg(arg);
            }
            command.query_async(&mut conn).await
        })
        .await
    }

    pub async fn eval_bool(&self, script: &str, keys: &[String], args: &[String]) -> Result<bool> {
        self.run("EVAL", |mut conn| async move {
            let mut command = redis::cmd("EVAL");
            command.arg(script).arg(keys.len());
            for key in keys {
                command.arg(key);
            }
            for arg in args {
                command.arg(arg);
            }
            command.query_async(&mut conn).await
        })
        .await
    }

    pub async fn ttl(&self, key: &str) -> Result<i64> {
        self.run("TTL", |mut conn| async move { conn.ttl(key).await })
            .await
    }

    pub async fn pttl(&self, key: &str) -> Result<i64> {
        self.run("PTTL", |mut conn| async move { conn.pttl(key).await })
            .await
    }

    pub async fn scan_match(&self, pattern: &str) -> Result<Vec<String>> {
        self.run("SCAN", |mut conn| async move {
            let mut cursor: u64 = 0;
            let mut keys = Vec::new();
            loop {
                let (next_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                    .arg(cursor)
                    .arg("MATCH")
                    .arg(pattern)
                    .arg("COUNT")
                    .arg(1000)
                    .query_async(&mut conn)
                    .await?;
                keys.extend(batch);
                if next_cursor == 0 {
                    break;
                }
                cursor = next_cursor;
            }
            Ok(keys)
        })
        .await
    }

    pub async fn memory_usage(&self, key: &str) -> Result<u64> {
        self.run("MEMORY USAGE", |mut conn| async move {
            redis::cmd("MEMORY")
                .arg("USAGE")
                .arg(key)
                .query_async(&mut conn)
                .await
        })
        .await
    }

    pub async fn dbsize(&self) -> Result<u64> {
        self.run("DBSIZE", |mut conn| async move {
            redis::cmd("DBSIZE").query_async(&mut conn).await
        })
        .await
    }

    pub async fn del_multiple(&self, keys: &[String]) -> Result<u64> {
        if keys.is_empty() {
            return Ok(0);
        }
        self.run("DEL", |mut conn| async move {
            redis::cmd("DEL").arg(keys).query_async(&mut conn).await
        })
        .await
    }

    pub async fn memory_usages(&self, keys: &[String]) -> Result<Vec<Option<u64>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        self.run("MEMORY USAGES", |mut conn| async move {
            let mut pipe = redis::pipe();
            for key in keys {
                pipe.cmd("MEMORY").arg("USAGE").arg(key);
            }
            pipe.query_async(&mut conn).await
        })
        .await
    }
}
