//! WebSocket unary 连接池：按上游键复用，全局/每键信号量硬限流。
//!
//! # 池键
//! `(base_url, ws_path, auth_fingerprint)`。
//! `auth_fingerprint` 来自握手用的稳定 headers/query（AppliedAuth + spec.request），
//! **不含** per-request 的 `X-Request-Id`（关联 id 只走帧内 `id`）。
//! 鉴权轮换 → 指纹变 → 新键；旧键空闲连接按 `idle_ttl` 回收。
//!
//! # 超限
//! `try_acquire` 失败立即返回错误，**禁止**无限排队，**禁止**池满回退短连。
//!
//! # 空闲回收
//! 归还时若空闲数已达 `max_idle_per_key` 则丢弃；借出时丢弃超过 `idle_ttl` 的连接。
//!
//! # Binary 帧
//! 适配器层对 binary 严格失败（见 `ws_unary`）；本池不解析帧载荷。

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::types::ExecuteError;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Clone)]
pub struct WsPoolConfig {
    pub handshake_timeout: Duration,
    /// 全局同时进行的 `ws_unary` 调用上限。
    pub max_concurrent: u32,
    /// 同一池键同时进行的调用上限。
    pub max_concurrent_per_key: u32,
    /// 每键最多保留的空闲连接数。
    pub max_idle_per_key: u32,
    /// 空闲超过此时长的连接在借出时丢弃。
    pub idle_ttl: Duration,
}

impl Default for WsPoolConfig {
    fn default() -> Self {
        Self {
            handshake_timeout: Duration::from_secs(10),
            max_concurrent: 32,
            max_concurrent_per_key: 8,
            max_idle_per_key: 4,
            idle_ttl: Duration::from_secs(60),
        }
    }
}

#[derive(Clone, Eq)]
pub struct PoolKey {
    pub base_url: String,
    pub path: String,
    pub auth_fp: String,
}

impl PartialEq for PoolKey {
    fn eq(&self, other: &Self) -> bool {
        self.base_url == other.base_url
            && self.path == other.path
            && self.auth_fp == other.auth_fp
    }
}

impl Hash for PoolKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.base_url.hash(state);
        self.path.hash(state);
        self.auth_fp.hash(state);
    }
}

/// 对握手 headers + query 做稳定指纹（排序后 SHA-256 hex）。
#[must_use]
pub fn auth_fingerprint(headers: &[(String, String)], query: &[(String, String)]) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(headers.len() + query.len());
    for (k, v) in headers {
        parts.push(format!("h:{}={}", k.to_ascii_lowercase(), v));
    }
    for (k, v) in query {
        parts.push(format!("q:{k}={v}"));
    }
    parts.sort();
    let mut hasher = Sha256::new();
    for p in &parts {
        hasher.update(p.as_bytes());
        hasher.update([0]);
    }
    hex::encode(hasher.finalize())
}

struct IdleConn {
    stream: WsStream,
    last_used: Instant,
}

struct KeySlot {
    idle: VecDeque<IdleConn>,
    sem: Arc<Semaphore>,
}

struct WsPoolInner {
    cfg: WsPoolConfig,
    keys: Mutex<HashMap<PoolKey, KeySlot>>,
    global_sem: Arc<Semaphore>,
    handshakes: AtomicUsize,
    reuses: AtomicUsize,
    rejects: AtomicUsize,
}

/// 进程内共享的 `ws_unary` 池（bypass 与 mgmt 试跑同实现）。
#[derive(Clone)]
pub struct WsPool {
    inner: Arc<WsPoolInner>,
}

impl WsPool {
    #[must_use]
    pub fn new(cfg: WsPoolConfig) -> Self {
        let global = cfg.max_concurrent.max(1) as usize;
        Self {
            inner: Arc::new(WsPoolInner {
                global_sem: Arc::new(Semaphore::new(global)),
                cfg,
                keys: Mutex::new(HashMap::new()),
                handshakes: AtomicUsize::new(0),
                reuses: AtomicUsize::new(0),
                rejects: AtomicUsize::new(0),
            }),
        }
    }

    #[must_use]
    pub fn config(&self) -> &WsPoolConfig {
        &self.inner.cfg
    }

    #[must_use]
    pub fn handshakes(&self) -> usize {
        self.inner.handshakes.load(Ordering::SeqCst)
    }

    #[must_use]
    pub fn reuses(&self) -> usize {
        self.inner.reuses.load(Ordering::SeqCst)
    }

    #[must_use]
    pub fn rejects(&self) -> usize {
        self.inner.rejects.load(Ordering::SeqCst)
    }

    /// 借出一条连接：优先空闲复用，否则新建握手。超限立即失败。
    ///
    /// # Errors
    ///
    /// 全局或每键并发已满、锁中毒、握手超时或连接失败时返回 [`ExecuteError`]。
    pub async fn checkout(
        &self,
        key: PoolKey,
        ws_url: &str,
        handshake_headers: &[(String, String)],
    ) -> Result<WsConnGuard, ExecuteError> {
        let global_permit = self.inner.global_sem.clone().try_acquire_owned().map_err(|_| {
            self.inner.rejects.fetch_add(1, Ordering::SeqCst);
            ExecuteError::AdapterFailed(format!(
                "ws concurrency limit exceeded (ws_max_concurrent={})",
                self.inner.cfg.max_concurrent
            ))
        })?;

        let key_sem = {
            let mut map = self
                .inner
                .keys
                .lock()
                .map_err(|_| ExecuteError::AdapterFailed("ws pool lock poisoned".into()))?;
            let per = self.inner.cfg.max_concurrent_per_key.max(1) as usize;
            let slot = map.entry(key.clone()).or_insert_with(|| KeySlot {
                idle: VecDeque::new(),
                sem: Arc::new(Semaphore::new(per)),
            });
            slot.sem.clone()
        };

        let Ok(key_permit) = key_sem.try_acquire_owned() else {
            self.inner.rejects.fetch_add(1, Ordering::SeqCst);
            drop(global_permit);
            return Err(ExecuteError::AdapterFailed(format!(
                "ws per-key concurrency limit exceeded (ws_max_concurrent_per_key={})",
                self.inner.cfg.max_concurrent_per_key
            )));
        };

        if let Some(stream) = self.take_idle(&key)? {
            self.inner.reuses.fetch_add(1, Ordering::SeqCst);
            return Ok(WsConnGuard {
                stream: Some(stream),
                pool: self.inner.clone(),
                key,
                _global: global_permit,
                _key: key_permit,
                broken: false,
            });
        }

        let stream = self.connect(ws_url, handshake_headers).await?;
        self.inner.handshakes.fetch_add(1, Ordering::SeqCst);
        Ok(WsConnGuard {
            stream: Some(stream),
            pool: self.inner.clone(),
            key,
            _global: global_permit,
            _key: key_permit,
            broken: false,
        })
    }

    fn take_idle(&self, key: &PoolKey) -> Result<Option<WsStream>, ExecuteError> {
        let mut map = self
            .inner
            .keys
            .lock()
            .map_err(|_| ExecuteError::AdapterFailed("ws pool lock poisoned".into()))?;
        let Some(slot) = map.get_mut(key) else {
            return Ok(None);
        };
        let ttl = self.inner.cfg.idle_ttl;
        let now = Instant::now();
        while let Some(idle) = slot.idle.pop_front() {
            if now.duration_since(idle.last_used) > ttl {
                continue;
            }
            return Ok(Some(idle.stream));
        }
        Ok(None)
    }

    async fn connect(
        &self,
        ws_url: &str,
        handshake_headers: &[(String, String)],
    ) -> Result<WsStream, ExecuteError> {
        let mut req = ws_url
            .into_client_request()
            .map_err(|e| ExecuteError::AdapterFailed(format!("ws request: {e}")))?;
        for (k, v) in handshake_headers {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(k.as_bytes()),
                HeaderValue::from_str(v),
            ) {
                req.headers_mut().insert(name, val);
            }
        }
        let connect = tokio_tungstenite::connect_async(req);
        let (ws, _resp) = tokio::time::timeout(self.inner.cfg.handshake_timeout, connect)
            .await
            .map_err(|_| ExecuteError::UpstreamTransport("ws handshake timeout".into()))?
            .map_err(|e| ExecuteError::UpstreamTransport(format!("ws connect: {e}")))?;
        Ok(ws)
    }

    fn checkin(&self, key: PoolKey, stream: WsStream) {
        let Ok(mut map) = self.inner.keys.lock() else {
            return;
        };
        let max_idle = self.inner.cfg.max_idle_per_key.max(1) as usize;
        let per = self.inner.cfg.max_concurrent_per_key.max(1) as usize;
        let slot = map.entry(key).or_insert_with(|| KeySlot {
            idle: VecDeque::new(),
            sem: Arc::new(Semaphore::new(per)),
        });
        if slot.idle.len() >= max_idle {
            return;
        }
        slot.idle.push_back(IdleConn {
            stream,
            last_used: Instant::now(),
        });
    }
}

/// 借出的连接；Drop 时归还或丢弃，并释放信号量。
pub struct WsConnGuard {
    stream: Option<WsStream>,
    pool: Arc<WsPoolInner>,
    key: PoolKey,
    _global: OwnedSemaphorePermit,
    _key: OwnedSemaphorePermit,
    broken: bool,
}

impl WsConnGuard {
    /// # Errors
    ///
    /// 连接已被取走时返回错误。
    pub fn stream(&mut self) -> Result<&mut WsStream, ExecuteError> {
        self.stream
            .as_mut()
            .ok_or_else(|| ExecuteError::AdapterFailed("ws connection missing".into()))
    }

    /// 标记坏连：Drop 时不归还池。
    pub fn mark_broken(&mut self) {
        self.broken = true;
    }
}

impl Drop for WsConnGuard {
    fn drop(&mut self) {
        if self.broken {
            let _ = self.stream.take();
            return;
        }
        if let Some(stream) = self.stream.take() {
            let pool = WsPool {
                inner: self.pool.clone(),
            };
            pool.checkin(self.key.clone(), stream);
        }
    }
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;

    #[test]
    fn fingerprint_stable_under_reorder() {
        let a = auth_fingerprint(
            &[("Authorization".into(), "Bearer x".into())],
            &[("a".into(), "1".into()), ("b".into(), "2".into())],
        );
        let b = auth_fingerprint(
            &[("Authorization".into(), "Bearer x".into())],
            &[("b".into(), "2".into()), ("a".into(), "1".into())],
        );
        assert_eq!(a, b);
        let c = auth_fingerprint(
            &[("Authorization".into(), "Bearer y".into())],
            &[("a".into(), "1".into()), ("b".into(), "2".into())],
        );
        assert_ne!(a, c);
    }
}
