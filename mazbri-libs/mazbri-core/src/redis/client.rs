use crate::num::SaturatingFrom;
use crate::redis::config::RedisConfig;
use crate::redis::error::{RedisError, Result};
use crate::redis::keys::{
    billing_balance_invalidation_channel, billing_balance_key, cache_l1_invalidation_channel,
    fallback_key, gateway_binding_nonce_key, nonce_key, permission_key, rate_limit_key,
    tenant_idempotency_key,
};
use futures::StreamExt;
use redis::AsyncCommands;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use redis::cluster::{ClusterClient, ClusterClientBuilder};
use redis::cluster_async::ClusterConnection;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
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
        manager_config: ConnectionManagerConfig,
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

/// redis 1.x 给异步连接加了库内默认超时（连接 1s、响应 500ms）。这里显式设成与
/// `RedisConfig` 一致：连接超时 = `connect_timeout_ms`，响应超时 = `command_timeout_ms`，
/// 后台重连同样受连接超时约束。外层 `tokio::time::timeout` 仍是权威截止时间，
/// 库内超时与之等值，只作兜底，不会把配置的命令超时截短。
fn manager_config(connect_timeout: Duration, command_timeout: Duration) -> ConnectionManagerConfig {
    ConnectionManagerConfig::new()
        .set_connection_timeout(Some(connect_timeout))
        .set_response_timeout(Some(command_timeout))
}

fn redis_metric_op(op: &'static str) -> &'static str {
    match op {
        "GET" | "MGET" | "EXISTS" | "TTL" | "PTTL" | "MEMORY USAGE" | "DBSIZE" | "SCAN" => "get",
        "SET EX" | "SET NX EX" | "DEL" | "PUBLISH" => "set",
        "PING" => "ping",
        "MEMORY USAGES" | "EVALSHA PIPELINE" => "pipeline",
        _ => "execute",
    }
}

fn is_noscript_error(error: &redis::RedisError) -> bool {
    error.code() == Some("NOSCRIPT")
        || error.to_string().to_ascii_uppercase().contains("NOSCRIPT")
}

pub(crate) fn build_redis_url_from_config(config: &RedisConfig) -> anyhow::Result<String> {
    if !config.url.trim().is_empty() {
        return Ok(config.url.clone());
    }
    let Some(ref host) = config.host else {
        anyhow::bail!("Redis configuration must specify either 'url' or 'host'");
    };
    let port = config.port.unwrap_or(6379);
    let db = config.db.unwrap_or(0);
    let scheme = if config.is_cluster {
        if config.use_tls {
            "rediss+cluster"
        } else {
            "redis+cluster"
        }
    } else if config.use_tls {
        "rediss"
    } else {
        "redis"
    };
    if let Some(ref password) = config.password {
        let encoded_pw = urlencoding::encode(password);
        Ok(format!("{scheme}://:{encoded_pw}@{host}:{port}/{db}"))
    } else {
        Ok(format!("{scheme}://{host}:{port}/{db}"))
    }
}

impl RedisClient {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        Self::connect_with_config(&RedisConfig {
            url: url.to_string(),
            ..Default::default()
        })
        .await
    }

    /// 只解析 URL、不打开 TCP 连接。供不访问 Redis 的单元测试构造客户端。
    ///
    /// # Errors
    ///
    /// URL 无法解析时返回错误。
    pub fn unconnected(url: &str) -> anyhow::Result<Self> {
        let client = Arc::new(redis::Client::open(url)?);
        Ok(Self {
            inner: Arc::new(InnerClient::Standalone {
                client,
                manager_config: manager_config(Duration::from_secs(1), Duration::from_secs(1)),
                managers: Arc::from(Vec::<ConnectionManager>::new()),
                next_manager_index: Arc::new(AtomicUsize::new(0)),
            }),
            key_prefix: Arc::from(""),
            connect_timeout: Duration::from_secs(1),
            command_timeout: Duration::from_secs(1),
        })
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn connect_with_config(config: &RedisConfig) -> anyhow::Result<Self> {
        let connect_timeout = Duration::from_millis(config.connect_timeout_ms.max(1));
        let command_timeout = Duration::from_millis(config.command_timeout_ms.max(1));
        let url_str = build_redis_url_from_config(config)?;

        let is_cluster =
            url_str.starts_with("redis+cluster://") || url_str.starts_with("rediss+cluster://");

        let inner = if is_cluster {
            let clean_url = url_str
                .replace("redis+cluster://", "redis://")
                .replace("rediss+cluster://", "rediss://");
            let client = Arc::new(
                ClusterClientBuilder::new(vec![clean_url.as_str()])
                    .connection_timeout(connect_timeout)
                    .response_timeout(command_timeout)
                    .build()?,
            );
            let connection = timeout(connect_timeout, client.get_async_connection())
                .await
                .map_err(|_| anyhow::anyhow!("redis cluster connection timed out"))??;
            InnerClient::Cluster { client, connection }
        } else {
            let client = Arc::new(redis::Client::open(url_str.as_str())?);
            let manager_config = manager_config(connect_timeout, command_timeout);
            let pool_size = config.pool_max_size.clamp(1, 256);
            let mut managers = Vec::with_capacity(pool_size);
            for _ in 0..pool_size {
                let manager = timeout(
                    connect_timeout,
                    ConnectionManager::new_with_config((*client).clone(), manager_config.clone()),
                )
                .await
                .map_err(|_| anyhow::anyhow!("redis connection timed out"))??;
                managers.push(manager);
            }
            InnerClient::Standalone {
                client,
                manager_config,
                managers: Arc::from(managers.into_boxed_slice()),
                next_manager_index: Arc::new(AtomicUsize::new(0)),
            }
        };

        let inner = Arc::new(inner);
        Self::spawn_keepalive(&inner, command_timeout);

        Ok(Self {
            inner,
            key_prefix: Arc::from(config.key_prefix.as_str()),
            connect_timeout,
            command_timeout,
        })
    }

    fn spawn_keepalive(inner: &Arc<InnerClient>, command_timeout: Duration) {
        const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
        if matches!(&**inner, InnerClient::Standalone { managers, .. } if managers.is_empty()) {
            return;
        }
        let weak = Arc::downgrade(inner);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(KEEPALIVE_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // Skip immediate tick right after initial connect.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let Some(inner) = weak.upgrade() else {
                    break;
                };
                match &*inner {
                    InnerClient::Standalone { managers, .. } => {
                        for manager in managers.iter() {
                            let mut conn = manager.clone();
                            let _ = timeout(
                                command_timeout,
                                redis::cmd("PING").query_async::<String>(&mut conn),
                            )
                            .await;
                        }
                    }
                    InnerClient::Cluster { connection, .. } => {
                        let mut conn = connection.clone();
                        let _ = timeout(
                            command_timeout,
                            redis::cmd("PING").query_async::<String>(&mut conn),
                        )
                        .await;
                    }
                }
            }
        });
    }

    #[must_use]
    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }

    #[must_use]
    pub fn has_active_connections(&self) -> bool {
        match &*self.inner {
            InnerClient::Standalone { managers, .. } => !managers.is_empty(),
            InnerClient::Cluster { .. } => true,
        }
    }

    fn connection(&self) -> Result<InnerConnection> {
        match &*self.inner {
            InnerClient::Standalone {
                managers,
                next_manager_index,
                ..
            } => {
                let len = managers.len();
                if len == 0 {
                    return Err(RedisError::InvalidInput(
                        "redis client has no active connections".to_string(),
                    ));
                }
                let index = next_manager_index.fetch_add(1, Ordering::Relaxed) % len;
                Ok(InnerConnection::Standalone(Box::new(
                    managers[index].clone(),
                )))
            }
            InnerClient::Cluster { connection, .. } => {
                Ok(InnerConnection::Cluster(connection.clone()))
            }
        }
    }

    async fn fresh_connection(&self) -> Result<InnerConnection> {
        match &*self.inner {
            InnerClient::Standalone {
                client,
                manager_config,
                ..
            } => {
                let conn = timeout(
                    self.connect_timeout,
                    ConnectionManager::new_with_config((**client).clone(), manager_config.clone()),
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

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn set_nonce_if_absent(
        &self,
        tenant_id: Uuid,
        key_version: &str,
        nonce: &str,
        ttl_seconds: i32,
    ) -> Result<bool> {
        let ttl_seconds = usize::saturating_from(ttl_seconds.max(1));
        let key = self.nonce_key(tenant_id, key_version, nonce);
        self.set_nx_ex(&key, b"1", ttl_seconds).await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn set_gateway_binding_nonce_if_absent(
        &self,
        prefix: &str,
        tenant_id: Uuid,
        nonce: &str,
        ttl_seconds: i32,
    ) -> Result<bool> {
        let ttl_seconds = usize::saturating_from(ttl_seconds.max(1));
        let key = gateway_binding_nonce_key(prefix, tenant_id, nonce);
        self.set_nx_ex(&key, b"1", ttl_seconds).await
    }

    /// Coalesced single-RTT check & consume for both gateway request-binding nonce and
    /// transport-envelope nonce via Redis pipeline. Returns `(binding_inserted, envelope_inserted)`.
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn set_binding_and_envelope_nonces_if_absent(
        &self,
        prefix: &str,
        tenant_id: Uuid,
        binding_nonce: &str,
        key_version: &str,
        envelope_nonce: &str,
        ttl_seconds: i32,
    ) -> Result<(bool, bool)> {
        let ttl_seconds = usize::saturating_from(ttl_seconds.max(1));
        let binding_key = gateway_binding_nonce_key(prefix, tenant_id, binding_nonce);
        let envelope_key = self.nonce_key(tenant_id, key_version, envelope_nonce);
        self.set_nx_ex_pair(&binding_key, &envelope_key, ttl_seconds)
            .await
    }

    #[must_use]
    pub fn fallback_key(&self, cache_key: &str) -> String {
        fallback_key(&self.key_prefix, cache_key)
    }

    #[must_use]
    pub fn tenant_idempotency_key(
        &self,
        tenant_id: Uuid,
        product_id: &str,
        method: &str,
        path: &str,
        fingerprint: &str,
    ) -> String {
        tenant_idempotency_key(
            &self.key_prefix,
            tenant_id,
            product_id,
            method,
            path,
            fingerprint,
        )
    }

    #[must_use]
    pub fn billing_balance_key(&self, tenant_id: Uuid) -> String {
        billing_balance_key(&self.key_prefix, tenant_id)
    }

    #[must_use]
    pub fn billing_balance_invalidation_channel(&self) -> String {
        billing_balance_invalidation_channel(&self.key_prefix)
    }

    #[must_use]
    pub fn cache_l1_invalidation_channel(&self) -> String {
        cache_l1_invalidation_channel(&self.key_prefix)
    }

    #[must_use]
    pub fn rate_limit_key(&self, parts: &[&str]) -> String {
        rate_limit_key(&self.key_prefix, parts)
    }

    async fn execute_timed<T, Fut>(op: &'static str, command_timeout: Duration, fut: Fut) -> Result<T>
    where
        Fut: Future<Output = redis::RedisResult<T>>,
    {
        timeout(command_timeout, fut)
            .await
            .map_err(|_| RedisError::Timeout(op))?
            .map_err(RedisError::from)
    }

    fn record_command_telemetry<T>(op: &'static str, started: Instant, result: &Result<T>) {
        let metric_op = redis_metric_op(op);
        crate::metrics::observe_dep("redis", metric_op, started, result.is_ok());
        let span = crate::trace_spans::CompletedSpan::new(
            format!("redis.{}", op.to_ascii_lowercase().replace(' ', "_")),
            started.elapsed(),
        )
        .client()
        .attr("db.system", "redis")
        .attr("db.operation", op);
        match result {
            Ok(_) => {
                let _ = span.record_under_current();
            }
            Err(error) => {
                let _ = span.error(error.to_string()).record_under_current();
            }
        }
    }

    /// Read-only / naturally idempotent command execution: retries once on broken connection.
    async fn run_idempotent<T, F, Fut>(&self, op: &'static str, mut f: F) -> Result<T>
    where
        F: FnMut(InnerConnection) -> Fut,
        Fut: Future<Output = redis::RedisResult<T>>,
    {
        let started = Instant::now();
        let conn = self.connection()?;
        let first = Self::execute_timed(op, self.command_timeout, f(conn)).await;
        let result = match first {
            Ok(value) => Ok(value),
            Err(RedisError::Connection(error)) if Self::should_retry_connection(&error) => {
                match self.fresh_connection().await {
                    Ok(fresh) => Self::execute_timed(op, self.command_timeout, f(fresh)).await,
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        };
        Self::record_command_telemetry(op, started, &result);
        result
    }

    /// Stateful / non-idempotent command execution (`SET NX EX`, `EVAL`, `PUBLISH`, etc.):
    /// never auto-retries after sending on wire so non-idempotent commands cannot execute twice.
    async fn run_once<T, F, Fut>(&self, op: &'static str, f: F) -> Result<T>
    where
        F: FnOnce(InnerConnection) -> Fut,
        Fut: Future<Output = redis::RedisResult<T>>,
    {
        let started = Instant::now();
        let conn = self.connection()?;
        let result = Self::execute_timed(op, self.command_timeout, f(conn)).await;
        Self::record_command_telemetry(op, started, &result);
        result
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn ping(&self) -> Result<()> {
        self.run_idempotent("PING", |mut conn| async move {
            let _: String = redis::cmd("PING").query_async(&mut conn).await?;
            Ok(())
        })
        .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.run_idempotent("GET", |mut conn| async move { conn.get(key).await })
            .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn set_ex_bytes(&self, key: &str, value: &[u8], ttl_secs: usize) -> Result<()> {
        let ttl_u64 = ttl_secs as u64;
        self.run_idempotent("SET EX", |mut conn| async move {
            let _: () = conn.set_ex(key, value, ttl_u64).await?;
            Ok(())
        })
        .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn set_nx_ex(&self, key: &str, value: &[u8], ttl_secs: usize) -> Result<bool> {
        self.run_once("SET NX EX", |mut conn| async move {
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

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn set_nx_ex_pair(
        &self,
        key1: &str,
        key2: &str,
        ttl_secs: usize,
    ) -> Result<(bool, bool)> {
        self.run_once("SET NX EX", |mut conn| async move {
            let mut pipe = redis::pipe();
            pipe.cmd("SET")
                .arg(key1)
                .arg(b"1")
                .arg("NX")
                .arg("EX")
                .arg(ttl_secs)
                .cmd("SET")
                .arg(key2)
                .arg(b"1")
                .arg("NX")
                .arg("EX")
                .arg(ttl_secs);
            let (r1, r2): (Option<String>, Option<String>) = pipe.query_async(&mut conn).await?;
            Ok((r1.is_some(), r2.is_some()))
        })
        .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn del(&self, key: &str) -> Result<u64> {
        self.run_idempotent("DEL", |mut conn| async move { conn.del(key).await })
            .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn exists(&self, key: &str) -> Result<bool> {
        self.run_idempotent("EXISTS", |mut conn| async move { conn.exists(key).await })
            .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn mget_bytes(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>> {
        self.run_idempotent("MGET", |mut conn| async move {
            redis::cmd("MGET").arg(keys).query_async(&mut conn).await
        })
        .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn eval_i64(&self, script: &str, keys: &[String], args: &[String]) -> Result<i64> {
        self.run_once("EVAL", |mut conn| async move {
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

    /// Evaluates a Lua script that returns either an integer (`Some(i64)`) or `nil`/`false` (`None`).
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn eval_opt_i64(
        &self,
        script: &str,
        keys: &[String],
        args: &[String],
    ) -> Result<Option<i64>> {
        self.run_once("EVAL", |mut conn| async move {
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

    /// Executes a single-key, single-integer-argument Lua script across `keys_and_args` using
    /// a single Redis Pipeline + `EVALSHA` (falling back to `SCRIPT LOAD` + retry once on `NOSCRIPT`).
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn evalsha_batch_opt_i64(
        &self,
        script: &str,
        keys_and_args: &[(String, i64)],
    ) -> Result<Vec<Option<i64>>> {
        if keys_and_args.is_empty() {
            return Ok(Vec::new());
        }
        let script_obj = redis::Script::new(script);
        let sha1 = script_obj.get_hash().to_owned();
        self.run_once("EVALSHA PIPELINE", |mut conn| async move {
            match &mut conn {
                InnerConnection::Standalone(standalone) => {
                    let mut pipe = redis::pipe();
                    for (key, arg) in keys_and_args {
                        pipe.cmd("EVALSHA")
                            .arg(&sha1)
                            .arg(1usize)
                            .arg(key)
                            .arg(*arg);
                    }
                    match pipe.query_async::<Vec<Option<i64>>>(standalone.as_mut()).await {
                        Ok(values) => Ok(values),
                        Err(err) if is_noscript_error(&err) => {
                            let _: String = redis::cmd("SCRIPT")
                                .arg("LOAD")
                                .arg(script)
                                .query_async(standalone.as_mut())
                                .await?;
                            pipe.query_async::<Vec<Option<i64>>>(standalone.as_mut())
                                .await
                        }
                        Err(err) => Err(err),
                    }
                }
                InnerConnection::Cluster(cluster) => {
                    let mut out = Vec::with_capacity(keys_and_args.len());
                    for (key, arg) in keys_and_args {
                        let res = redis::cmd("EVALSHA")
                            .arg(&sha1)
                            .arg(1usize)
                            .arg(key)
                            .arg(*arg)
                            .query_async::<Option<i64>>(cluster)
                            .await;
                        let val = match res {
                            Ok(v) => v,
                            Err(err) if is_noscript_error(&err) => {
                                redis::cmd("EVAL")
                                    .arg(script)
                                    .arg(1usize)
                                    .arg(key)
                                    .arg(*arg)
                                    .query_async::<Option<i64>>(cluster)
                                    .await?
                            }
                            Err(err) => return Err(err),
                        };
                        out.push(val);
                    }
                    Ok(out)
                }
            }
        })
        .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn eval_bool(&self, script: &str, keys: &[String], args: &[String]) -> Result<bool> {
        self.run_once("EVAL", |mut conn| async move {
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

    /// Publishes a UTF-8 message to a Redis Pub/Sub channel.
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn publish(&self, channel: &str, payload: &str) -> Result<u64> {
        self.run_once("PUBLISH", |mut conn| async move {
            redis::cmd("PUBLISH")
                .arg(channel)
                .arg(payload)
                .query_async(&mut conn)
                .await
        })
        .await
    }

    /// Spawns a resilient background Redis Pub/Sub listener for `channel`.
    /// For unconnected test clients (`managers.is_empty()`), returns immediately without connecting.
    pub fn spawn_pubsub_listener<F>(
        &self,
        channel: String,
        label: &'static str,
        on_message: F,
    ) -> Option<tokio::task::JoinHandle<()>>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        let client = match &*self.inner {
            InnerClient::Standalone {
                client, managers, ..
            } => {
                if managers.is_empty() {
                    return None;
                }
                Arc::clone(client)
            }
            InnerClient::Cluster { .. } => {
                tracing::info!(
                    label,
                    channel = %channel,
                    "Redis cluster mode active; relying on short L1 TTL without standalone pubsub"
                );
                return None;
            }
        };
        let connect_timeout = self.connect_timeout;
        Some(tokio::spawn(async move {
            let mut backoff_secs = 1u64;
            loop {
                let pubsub_res = timeout(connect_timeout, client.get_async_pubsub()).await;
                let mut pubsub = match pubsub_res {
                    Ok(Ok(ps)) => ps,
                    Ok(Err(error)) => {
                        tracing::warn!(
                            label,
                            channel = %channel,
                            error = %error,
                            "Redis pubsub connect failed; retrying"
                        );
                        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                        backoff_secs = (backoff_secs * 2).min(30);
                        continue;
                    }
                    Err(_) => {
                        tracing::warn!(
                            label,
                            channel = %channel,
                            "Redis pubsub connect timed out; retrying"
                        );
                        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                        backoff_secs = (backoff_secs * 2).min(30);
                        continue;
                    }
                };
                if let Err(error) = pubsub.subscribe(&channel).await {
                    tracing::warn!(
                        label,
                        channel = %channel,
                        error = %error,
                        "Redis pubsub subscribe failed; retrying"
                    );
                    tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(30);
                    continue;
                }
                tracing::info!(label, channel = %channel, "Subscribed to Redis invalidation channel");
                backoff_secs = 1;
                let mut stream = pubsub.on_message();
                while let Some(msg) = stream.next().await {
                    if let Ok(payload) = msg.get_payload::<String>() {
                        on_message(payload);
                    }
                }
                tracing::warn!(label, channel = %channel, "Redis pubsub stream ended; reconnecting");
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
            }
        }))
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn ttl(&self, key: &str) -> Result<i64> {
        self.run_idempotent("TTL", |mut conn| async move { conn.ttl(key).await })
            .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn pttl(&self, key: &str) -> Result<i64> {
        self.run_idempotent("PTTL", |mut conn| async move { conn.pttl(key).await })
            .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn scan_match(&self, pattern: &str) -> Result<Vec<String>> {
        self.run_idempotent("SCAN", |mut conn| async move {
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

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn memory_usage(&self, key: &str) -> Result<u64> {
        self.run_idempotent("MEMORY USAGE", |mut conn| async move {
            redis::cmd("MEMORY")
                .arg("USAGE")
                .arg(key)
                .query_async(&mut conn)
                .await
        })
        .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn dbsize(&self) -> Result<u64> {
        self.run_idempotent("DBSIZE", |mut conn| async move {
            redis::cmd("DBSIZE").query_async(&mut conn).await
        })
        .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn del_multiple(&self, keys: &[String]) -> Result<u64> {
        if keys.is_empty() {
            return Ok(0);
        }
        self.run_idempotent("DEL", |mut conn| async move {
            redis::cmd("DEL").arg(keys).query_async(&mut conn).await
        })
        .await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn memory_usages(&self, keys: &[String]) -> Result<Vec<Option<u64>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        self.run_idempotent("MEMORY USAGES", |mut conn| async move {
            let mut pipe = redis::pipe();
            for key in keys {
                pipe.cmd("MEMORY").arg("USAGE").arg(key);
            }
            pipe.query_async(&mut conn).await
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::{RedisClient, build_redis_url_from_config, manager_config};
    use crate::redis::config::RedisConfig;
    use std::time::Duration;

    #[test]
    fn manager_config_uses_configured_timeouts_not_library_defaults() {
        let config = manager_config(Duration::from_millis(1500), Duration::from_millis(2000));
        assert_eq!(
            config.connection_timeout(),
            Some(Duration::from_millis(1500))
        );
        assert_eq!(config.response_timeout(), Some(Duration::from_millis(2000)));
    }

    #[test]
    fn password_with_special_chars_is_percent_encoded_into_valid_redis_url() {
        let cfg = RedisConfig {
            url: String::new(),
            host: Some("redis.internal".to_string()),
            port: Some(6379),
            db: Some(2),
            password: Some("p@ss:w/rd?#1%".to_string()),
            ..RedisConfig::default()
        };
        let url = build_redis_url_from_config(&cfg).expect("build url");
        assert!(url.contains("p%40ss%3Aw%2Frd%3F%231%25@redis.internal:6379/2"));
        redis::Client::open(url.as_str()).expect("redis::Client::open must accept encoded password");
    }

    #[tokio::test]
    async fn unconnected_client_returns_error_instead_of_panicking_on_empty_pool() {
        let client = RedisClient::unconnected("redis://127.0.0.1:6379").expect("unconnected");
        let err = client.get_bytes("k").await.expect_err("must return Err");
        assert!(err.to_string().contains("no active connections"));
        let err_nx = client
            .set_nx_ex("k", b"1", 10)
            .await
            .expect_err("must return Err");
        assert!(err_nx.to_string().contains("no active connections"));
    }
}
