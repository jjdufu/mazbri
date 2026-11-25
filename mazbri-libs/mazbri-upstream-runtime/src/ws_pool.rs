//! WebSocket unary 连接池：按上游键复用，全局/每键信号量硬限流，取消安全默认熔断脏连。

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{FutureExt, SinkExt, StreamExt};
use mazbri_core::UpstreamTlsSpec;
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};

use crate::ssrf::SsrfChecker;
use crate::types::ExecuteError;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Maximum inbound WebSocket frame / message size (16 MiB).
pub const MAX_WS_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
const MAX_CACHED_WS_TLS_CONFIGS: usize = 128;
const DEFAULT_EVICTION_SWEEP_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct WsPoolConfig {
    pub handshake_timeout: Duration,
    /// 全局同时进行的 `ws_unary` 调用上限。
    pub max_concurrent: u32,
    /// 同一池键同时进行的调用上限。
    pub max_concurrent_per_key: u32,
    /// 每键最多保留的空闲连接数。
    pub max_idle_per_key: u32,
    /// 空闲超过此时长的连接在借出和后台巡检时丢弃。
    pub idle_ttl: Duration,
    /// 借出空闲连接前做非阻塞存活检查。
    pub liveness_check_on_checkout: bool,
}

impl Default for WsPoolConfig {
    fn default() -> Self {
        Self {
            handshake_timeout: Duration::from_secs(10),
            max_concurrent: 32,
            max_concurrent_per_key: 8,
            max_idle_per_key: 4,
            idle_ttl: Duration::from_secs(60),
            liveness_check_on_checkout: true,
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

#[derive(Debug)]
struct InsecureSkipServerCertVerifier {
    supported: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl InsecureSkipServerCertVerifier {
    fn new() -> Self {
        Self {
            supported: rustls::crypto::aws_lc_rs::default_provider()
                .signature_verification_algorithms,
        }
    }
}

impl rustls::client::danger::ServerCertVerifier for InsecureSkipServerCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.supported)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.supported)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.supported.supported_schemes()
    }
}

struct WsPoolInner {
    cfg: WsPoolConfig,
    keys: Mutex<HashMap<PoolKey, KeySlot>>,
    tls_configs: Mutex<HashMap<[u8; 32], Arc<rustls::ClientConfig>>>,
    global_sem: Arc<Semaphore>,
    handshakes: AtomicUsize,
    reuses: AtomicUsize,
    rejects: AtomicUsize,
    stale_discards: AtomicUsize,
}

impl WsPoolInner {
    fn evict_expired_idle(&self) {
        let Ok(mut map) = self.keys.lock() else {
            return;
        };
        let ttl = self.cfg.idle_ttl;
        let max_per_key = self.cfg.max_concurrent_per_key.max(1) as usize;
        let now = Instant::now();
        map.retain(|_, slot| {
            slot.idle
                .retain(|conn| now.duration_since(conn.last_used) <= ttl);
            !slot.idle.is_empty() || slot.sem.available_permits() < max_per_key
        });
    }
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
        let sweep_interval = cfg
            .idle_ttl
            .min(DEFAULT_EVICTION_SWEEP_INTERVAL)
            .max(Duration::from_millis(50));
        let inner = Arc::new(WsPoolInner {
            global_sem: Arc::new(Semaphore::new(global)),
            cfg,
            keys: Mutex::new(HashMap::new()),
            tls_configs: Mutex::new(HashMap::new()),
            handshakes: AtomicUsize::new(0),
            reuses: AtomicUsize::new(0),
            rejects: AtomicUsize::new(0),
            stale_discards: AtomicUsize::new(0),
        });
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let weak = Arc::downgrade(&inner);
            handle.spawn(async move {
                let mut ticker = tokio::time::interval(sweep_interval);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    let Some(strong) = weak.upgrade() else {
                        break;
                    };
                    strong.evict_expired_idle();
                }
            });
        }
        Self { inner }
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

    /// 借出前存活检查判死并丢弃的空闲连接数。
    #[must_use]
    pub fn stale_discards(&self) -> usize {
        self.inner.stale_discards.load(Ordering::SeqCst)
    }

    /// Active key slots currently tracked in the pool (for diagnostics/tests).
    #[must_use]
    pub fn active_key_slots(&self) -> usize {
        self.inner.keys.lock().map_or(0, |m| m.len())
    }

    /// 借出一条连接：优先空闲复用（按配置先做存活检查），否则新建握手。超限立即失败。
    ///
    /// # Errors
    ///
    /// 全局或每键并发已满、锁中毒、SSRF 拦截、握手超时或连接失败时返回 [`ExecuteError`]。
    pub async fn checkout(
        &self,
        key: PoolKey,
        ws_url: &str,
        handshake_headers: &[(String, String)],
        ssrf: &SsrfChecker,
    ) -> Result<WsConnGuard, ExecuteError> {
        self.checkout_with_tls(key, ws_url, handshake_headers, ssrf, None)
            .await
    }

    /// 同 [`Self::checkout`]，但按 [`UpstreamTlsSpec`] 注入自定义 CA / 客户端 mTLS 证书。
    ///
    /// # Errors
    ///
    /// 同 [`Self::checkout`]。
    pub async fn checkout_with_tls(
        &self,
        key: PoolKey,
        ws_url: &str,
        handshake_headers: &[(String, String)],
        ssrf: &SsrfChecker,
        tls: Option<&UpstreamTlsSpec>,
    ) -> Result<WsConnGuard, ExecuteError> {
        let permits = self.acquire_permits(&key)?;
        let mut discarded = 0usize;
        while let Some(mut stream) = self.take_idle(&key)? {
            if self.inner.cfg.liveness_check_on_checkout
                && matches!(probe_alive(&mut stream).await, Liveness::Dead)
            {
                self.inner.stale_discards.fetch_add(1, Ordering::SeqCst);
                discarded += 1;
                continue;
            }
            self.inner.reuses.fetch_add(1, Ordering::SeqCst);
            let mut guard = self.guard(key, stream, permits, true);
            guard.discarded = discarded;
            return Ok(guard);
        }
        let stream = self.connect(ws_url, handshake_headers, ssrf, tls).await?;
        self.inner.handshakes.fetch_add(1, Ordering::SeqCst);
        let mut guard = self.guard(key, stream, permits, false);
        guard.discarded = discarded;
        Ok(guard)
    }

    /// 跳过空闲连接直接握手（复用连接回复前断开后的重试专用），仍受全局与每键并发约束。
    ///
    /// # Errors
    ///
    /// 同 [`Self::checkout`]。
    pub async fn checkout_fresh(
        &self,
        key: PoolKey,
        ws_url: &str,
        handshake_headers: &[(String, String)],
        ssrf: &SsrfChecker,
    ) -> Result<WsConnGuard, ExecuteError> {
        self.checkout_fresh_with_tls(key, ws_url, handshake_headers, ssrf, None)
            .await
    }

    /// 同 [`Self::checkout_fresh`]，支持 [`UpstreamTlsSpec`]。
    ///
    /// # Errors
    ///
    /// 同 [`Self::checkout`]。
    pub async fn checkout_fresh_with_tls(
        &self,
        key: PoolKey,
        ws_url: &str,
        handshake_headers: &[(String, String)],
        ssrf: &SsrfChecker,
        tls: Option<&UpstreamTlsSpec>,
    ) -> Result<WsConnGuard, ExecuteError> {
        let permits = self.acquire_permits(&key)?;
        let stream = self.connect(ws_url, handshake_headers, ssrf, tls).await?;
        self.inner.handshakes.fetch_add(1, Ordering::SeqCst);
        Ok(self.guard(key, stream, permits, false))
    }

    fn acquire_permits(&self, key: &PoolKey) -> Result<Permits, ExecuteError> {
        let global = self
            .inner
            .global_sem
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
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

        let Ok(per_key) = key_sem.try_acquire_owned() else {
            self.inner.rejects.fetch_add(1, Ordering::SeqCst);
            return Err(ExecuteError::AdapterFailed(format!(
                "ws per-key concurrency limit exceeded (ws_max_concurrent_per_key={})",
                self.inner.cfg.max_concurrent_per_key
            )));
        };
        Ok(Permits { global, per_key })
    }

    /// `broken` defaults to `true` so any mid-flight timeout/cancellation/panic destroys
    /// the socket on Drop unless the caller explicitly calls [`WsConnGuard::mark_clean`].
    fn guard(&self, key: PoolKey, stream: WsStream, permits: Permits, reused: bool) -> WsConnGuard {
        WsConnGuard {
            stream: Some(stream),
            pool: self.inner.clone(),
            key,
            _global: permits.global,
            _key: permits.per_key,
            broken: true,
            reused,
            discarded: 0,
        }
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

    fn resolve_tls_connector(
        &self,
        scheme: &str,
        tls: Option<&UpstreamTlsSpec>,
    ) -> Result<Option<Connector>, ExecuteError> {
        if scheme != "wss" {
            return Ok(None);
        }
        let Some(spec) = tls.filter(|t| !t.is_empty()) else {
            return Ok(None);
        };
        let fp = spec.fingerprint_bytes();
        if let Ok(guard) = self.inner.tls_configs.lock()
            && let Some(cached) = guard.get(&fp)
        {
            return Ok(Some(Connector::Rustls(Arc::clone(cached))));
        }

        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let versions_builder = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| ExecuteError::SpecInvalid(format!("ws rustls versions: {e}")))?;

        let client_auth = parse_ws_client_auth(spec)?;
        let Some(built_cfg) = build_ws_rustls_config(spec, versions_builder, client_auth)? else {
            return Ok(None);
        };

        let arc_cfg = Arc::new(built_cfg);
        if let Ok(mut guard) = self.inner.tls_configs.lock() {
            if guard.len() >= MAX_CACHED_WS_TLS_CONFIGS {
                guard.clear();
            }
            guard.insert(fp, Arc::clone(&arc_cfg));
        }
        Ok(Some(Connector::Rustls(arc_cfg)))
    }

    async fn connect(
        &self,
        ws_url: &str,
        handshake_headers: &[(String, String)],
        ssrf: &SsrfChecker,
        tls: Option<&UpstreamTlsSpec>,
    ) -> Result<WsStream, ExecuteError> {
        let parsed = url::Url::parse(ws_url)
            .map_err(|e| ExecuteError::AdapterFailed(format!("ws url: {e}")))?;
        let scheme = parsed.scheme();
        if scheme != "ws" && scheme != "wss" {
            return Err(ExecuteError::SsrfBlocked(format!(
                "WS URL scheme must be ws/wss, got '{scheme}'"
            )));
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| ExecuteError::SsrfBlocked("Missing host string in WS URL".into()))?;
        let port = parsed
            .port()
            .unwrap_or(if scheme == "wss" { 443 } else { 80 });
        let checked_addrs = ssrf
            .resolve_checked_socket_addrs(host, port)
            .await
            .map_err(ExecuteError::SsrfBlocked)?;
        let connector = self.resolve_tls_connector(scheme, tls)?;
        let ws_cfg = WebSocketConfig::default()
            .max_message_size(Some(MAX_WS_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_WS_MESSAGE_BYTES));

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
        let handshake = async {
            let tcp = TcpStream::connect(checked_addrs.as_slice())
                .await
                .map_err(|e| ExecuteError::UpstreamTransport(format!("ws connect: {e}")))?;
            let _ = tcp.set_nodelay(true);
            let (ws, _resp) = tokio_tungstenite::client_async_tls_with_config(
                req,
                tcp,
                Some(ws_cfg),
                connector,
            )
            .await
            .map_err(|e| ExecuteError::UpstreamTransport(format!("ws connect: {e}")))?;
            Ok(ws)
        };
        tokio::time::timeout(self.inner.cfg.handshake_timeout, handshake)
            .await
            .map_err(|_| ExecuteError::UpstreamTransport("ws handshake timeout".into()))?
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

type WsClientAuthPair = (
    Vec<rustls::pki_types::CertificateDer<'static>>,
    rustls::pki_types::PrivateKeyDer<'static>,
);

fn parse_ws_client_auth(spec: &UpstreamTlsSpec) -> Result<Option<WsClientAuthPair>, ExecuteError> {
    let cert_pem = spec
        .client_cert
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let key_pem = spec
        .client_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let Some((c_pem, k_pem)) = cert_pem.zip(key_pem) else {
        return Ok(None);
    };
    let mut c_reader = Cursor::new(c_pem.as_bytes());
    let chain: Vec<rustls::pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(&mut c_reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ExecuteError::SpecInvalid(format!("invalid ws client_cert PEM: {e}")))?;
    if chain.is_empty() {
        return Err(ExecuteError::SpecInvalid(
            "ws client_cert PEM contains no certificates".into(),
        ));
    }
    let mut k_reader = Cursor::new(k_pem.as_bytes());
    let key = rustls_pemfile::private_key(&mut k_reader)
        .map_err(|e| ExecuteError::SpecInvalid(format!("invalid ws client_key PEM: {e}")))?
        .ok_or_else(|| {
            ExecuteError::SpecInvalid("ws client_key PEM contains no private key".into())
        })?;
    Ok(Some((chain, key)))
}

fn build_ws_rustls_config(
    spec: &UpstreamTlsSpec,
    versions_builder: rustls::ConfigBuilder<rustls::ClientConfig, rustls::WantsVerifier>,
    client_auth: Option<WsClientAuthPair>,
) -> Result<Option<rustls::ClientConfig>, ExecuteError> {
    if spec.insecure_skip_verify {
        let verifier = Arc::new(InsecureSkipServerCertVerifier::new());
        let dangerous = versions_builder
            .dangerous()
            .with_custom_certificate_verifier(verifier);
        let cfg = if let Some((chain, key)) = client_auth {
            dangerous
                .with_client_auth_cert(chain, key)
                .map_err(|e| ExecuteError::SpecInvalid(format!("ws client mTLS auth: {e}")))?
        } else {
            dangerous.with_no_client_auth()
        };
        return Ok(Some(cfg));
    }
    let Some(ca_pem) = spec
        .ca_cert
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return Ok(None);
    };
    let mut reader = Cursor::new(ca_pem.as_bytes());
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ExecuteError::SpecInvalid(format!("invalid ws ca_cert PEM: {e}")))?;
    if certs.is_empty() {
        return Err(ExecuteError::SpecInvalid(
            "ws ca_cert PEM contains no certificates".into(),
        ));
    }
    let mut root_store = rustls::RootCertStore::empty();
    for cert in certs {
        root_store
            .add(cert)
            .map_err(|e| ExecuteError::SpecInvalid(format!("invalid ws CA cert: {e}")))?;
    }
    let with_roots = versions_builder.with_root_certificates(root_store);
    let cfg = if let Some((chain, key)) = client_auth {
        with_roots
            .with_client_auth_cert(chain, key)
            .map_err(|e| ExecuteError::SpecInvalid(format!("ws client mTLS auth: {e}")))?
    } else {
        with_roots.with_no_client_auth()
    };
    Ok(Some(cfg))
}

struct Permits {
    global: OwnedSemaphorePermit,
    per_key: OwnedSemaphorePermit,
}

enum Liveness {
    Alive,
    Dead,
}

/// 非阻塞读一次：无积压为活；积压的 Ping 回 Pong 后继续读；
/// Close、EOF、读错误或空闲期出现的业务帧为死。
async fn probe_alive(ws: &mut WsStream) -> Liveness {
    loop {
        match ws.next().now_or_never() {
            None => return Liveness::Alive,
            Some(Some(Ok(Message::Ping(payload)))) => {
                if ws.send(Message::Pong(payload)).await.is_err() {
                    return Liveness::Dead;
                }
            }
            Some(Some(Ok(Message::Pong(_)))) => {}
            Some(Some(Ok(_) | Err(_)) | None) => return Liveness::Dead,
        }
    }
}

/// 借出的连接；默认 `broken: true`，仅完整收到匹配回复并调用 [`Self::mark_clean`] 后归还池。
pub struct WsConnGuard {
    stream: Option<WsStream>,
    pool: Arc<WsPoolInner>,
    key: PoolKey,
    _global: OwnedSemaphorePermit,
    _key: OwnedSemaphorePermit,
    broken: bool,
    reused: bool,
    discarded: usize,
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

    /// 标记连接已完成干净的请求-响应闭环，Drop 时允许归还空闲池。
    pub fn mark_clean(&mut self) {
        self.broken = false;
    }

    /// 标记坏连：Drop 时不归还池。
    pub fn mark_broken(&mut self) {
        self.broken = true;
    }

    /// 是否为从空闲池复用的连接（`false` 表示本次新握手）。
    #[must_use]
    pub fn reused(&self) -> bool {
        self.reused
    }

    /// 本次借出过程中因存活检查判死而丢弃的空闲连接数。
    #[must_use]
    pub fn stale_discarded(&self) -> usize {
        self.discarded
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

    #[tokio::test]
    async fn checkout_blocks_private_host_when_ssrf_disallows_private() {
        let pool = WsPool::new(WsPoolConfig::default());
        let ssrf = SsrfChecker::new();
        let key = PoolKey {
            base_url: "http://localhost:9999".into(),
            path: "/ws".into(),
            auth_fp: String::new(),
        };
        let res = pool
            .checkout(key, "ws://localhost:9999/ws", &[], &ssrf)
            .await;
        assert!(matches!(res, Err(ExecuteError::SsrfBlocked(_))));
    }
}
