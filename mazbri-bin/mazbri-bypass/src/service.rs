//! Bypass HTTP proxy service — production path uses shared `mazbri-upstream-runtime`.

use crate::metrics::UpstreamMetricsRecorder;
use mazbri_core::num::SaturatingFrom;
use mazbri_core::trace_propagation::{remote_span_context, set_remote_parent};
use mazbri_core::{
    AccessHttpFields, AccessLogEvent, AccessNetworkFields, AccessOutcomeFields, AccessRouteFields,
    AccessTimingFields, AccessUpstreamMetaFields, ChannelEgressSpec, SupplierConfig, TraceContext,
    classify_outcome, emit_access_log, hash_egress_spec_json, normalize_upstream_host,
    parse_and_validate_egress, request_id_or_new,
};
use mazbri_upstream_contracts::UPSTREAM_AUTH_REJECTED_PREFIX;
use mazbri_upstream_contracts::mgmt_probe::{ProbeInvokeRequest, ProbeInvokeResponse};
use mazbri_upstream_contracts::shared as upstream_proto;
use mazbri_upstream_runtime::{
    DescriptorPool, ExecuteMode, ExecuteOptions, InvokeInput, PipelineKeys, RuntimeEngine,
    RuntimeOptions, SsrfChecker, StageResult, StageTiming, decode_grpc_descriptor_pool,
    pipeline_keys_from_credentials,
};
use moka::sync::Cache;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{Notify, mpsc};
use tonic::{Request, Response};
use tracing::Instrument;
use uuid::Uuid;

const ACCESS_LOG_QUEUE_CAPACITY: usize = 16_384;
static PENDING_BYPASS_LOGS: AtomicUsize = AtomicUsize::new(0);
static LOG_DRAIN_NOTIFY: Notify = Notify::const_new();

fn spawn_bypass_log_worker() -> Option<mpsc::Sender<BypassLogSnapshot>> {
    let handle = tokio::runtime::Handle::try_current().ok()?;
    let (tx, mut rx) = mpsc::channel::<BypassLogSnapshot>(ACCESS_LOG_QUEUE_CAPACITY);
    handle.spawn(async move {
        while let Some(snapshot) = rx.recv().await {
            snapshot.emit();
            if PENDING_BYPASS_LOGS.fetch_sub(1, Ordering::AcqRel) == 1 {
                LOG_DRAIN_NOTIFY.notify_waiters();
            }
        }
    });
    Some(tx)
}

pub async fn drain_pending_logs(timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let notified = LOG_DRAIN_NOTIFY.notified();
        if PENDING_BYPASS_LOGS.load(Ordering::Acquire) == 0 {
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        let _ = tokio::time::timeout(deadline.saturating_duration_since(now), notified).await;
    }
}

fn hash_config_json(config_json: &str) -> String {
    let digest = Sha256::digest(config_json.as_bytes());
    hex::encode(digest)
}

#[derive(Clone)]
struct CachedSupplierConfig {
    hash: String,
    config: Arc<SupplierConfig>,
    credentials: HashMap<String, String>,
    pipeline_keys: Arc<PipelineKeys>,
}

#[derive(Clone)]
struct CachedEgressSpec {
    hash: String,
    spec: Arc<ChannelEgressSpec>,
}

#[derive(Clone)]
struct CachedArtifact {
    bytes: Arc<[u8]>,
    descriptor_pool: Option<Arc<DescriptorPool>>,
}

#[derive(Clone)]
pub struct MazbriBypassServiceImpl {
    engine: RuntimeEngine,
    service_name: Arc<str>,
    db: Arc<PgPool>,
    supplier_config_cache: Cache<String, CachedSupplierConfig>,
    egress_spec_cache: Cache<String, CachedEgressSpec>,
    artifact_cache: Cache<String, CachedArtifact>,
    /// 出站 HTTP 客户端的 User-Agent（`upstream.user_agent`），记入访问日志。
    user_agent: Arc<str>,
    log_tx: Option<mpsc::Sender<BypassLogSnapshot>>,
}

/// bypass 侧配置缓存的空闲过期时间：命中即续期。网关只在首次或收到 `CONFIG_CACHE_MISS`
/// 后才发全量配置，按写入时间过期会让每个活跃渠道周期性地多一次未命中重发。
const CONFIG_CACHE_IDLE: Duration = Duration::from_secs(600);

/// bypass 返回给网关、要求带全量配置重发的错误前缀。
const CONFIG_CACHE_MISS_PREFIX: &str = "CONFIG_CACHE_MISS";

fn parse_cached_supplier_config(
    hash: String,
    json: &str,
) -> Result<CachedSupplierConfig, String> {
    let mut cfg: SupplierConfig = serde_json::from_str(json).map_err(|e| {
        UpstreamMetricsRecorder::record_config_cache("supplier", "invalid");
        format!("invalid supplier_config JSON: {e}")
    })?;
    cfg = cfg.decrypt_credentials_for_runtime().map_err(|e| {
        UpstreamMetricsRecorder::record_config_cache("supplier", "invalid");
        format!("decrypt supplier credentials failed: {e}")
    })?;
    let mut credentials = HashMap::new();
    if let Some(ref auth) = cfg.auth_flow {
        credentials.clone_from(&auth.credentials);
    }
    let pipeline_keys = Arc::new(pipeline_keys_from_credentials(&credentials));
    UpstreamMetricsRecorder::record_config_cache("supplier", "miss");
    Ok(CachedSupplierConfig {
        hash,
        config: Arc::new(cfg),
        credentials,
        pipeline_keys,
    })
}

fn parse_cached_egress_spec(hash: String, full: &str) -> Result<CachedEgressSpec, String> {
    let value: serde_json::Value = serde_json::from_str(full).map_err(|e| {
        UpstreamMetricsRecorder::record_config_cache("egress", "invalid");
        format!("invalid egress_spec JSON: {e}")
    })?;
    let mut spec = parse_and_validate_egress(&value).map_err(|e| {
        UpstreamMetricsRecorder::record_config_cache("egress", "invalid");
        e.to_string()
    })?;
    spec = spec.decrypt_secrets_for_runtime().map_err(|e| {
        UpstreamMetricsRecorder::record_config_cache("egress", "invalid");
        format!("decrypt egress secrets failed: {e}")
    })?;
    UpstreamMetricsRecorder::record_config_cache("egress", "miss");
    Ok(CachedEgressSpec {
        hash,
        spec: Arc::new(spec),
    })
}

impl MazbriBypassServiceImpl {
    pub fn new(
        client: Arc<reqwest::Client>,
        ssrf_checker: SsrfChecker,
        service_name: impl Into<Arc<str>>,
        db: Arc<PgPool>,
    ) -> Self {
        let engine = RuntimeEngine::new(client, ssrf_checker)
            .with_observer(Arc::new(crate::metrics::BypassRuntimeObserver));
        Self {
            engine,
            service_name: service_name.into(),
            db,
            supplier_config_cache: Cache::builder()
                .max_capacity(4_096)
                .time_to_idle(CONFIG_CACHE_IDLE)
                .build(),
            egress_spec_cache: Cache::builder()
                .max_capacity(4_096)
                .time_to_idle(CONFIG_CACHE_IDLE)
                .build(),
            artifact_cache: Cache::builder()
                .max_capacity(32)
                .time_to_live(Duration::from_secs(600))
                .build(),
            user_agent: Arc::from(""),
            log_tx: spawn_bypass_log_worker(),
        }
    }

    #[must_use]
    pub fn with_http_client_defaults(
        mut self,
        user_agent: &str,
        connect_timeout: Duration,
        timeout: Duration,
    ) -> Self {
        self.user_agent = Arc::from(user_agent);
        self.engine = self
            .engine
            .with_http_client_defaults(user_agent, connect_timeout, timeout);
        self
    }

    #[must_use]
    pub fn with_options(mut self, options: RuntimeOptions) -> Self {
        self.engine = self.engine.with_options(options);
        self
    }

    fn resolve_supplier_config(
        &self,
        hash: &str,
        full: &str,
    ) -> Result<Option<CachedSupplierConfig>, String> {
        let hash = hash.trim();
        let full = full.trim();
        if !hash.is_empty() {
            let lookup = if hash.bytes().all(|b| !b.is_ascii_uppercase()) {
                self.supplier_config_cache.get(hash)
            } else {
                self.supplier_config_cache.get(&hash.to_ascii_lowercase())
            };
            if let Some(cached) = lookup
                && cached.hash.eq_ignore_ascii_case(hash)
            {
                UpstreamMetricsRecorder::record_config_cache("supplier", "hit");
                return Ok(Some(cached));
            }
        }
        if !full.is_empty() {
            let computed = hash_config_json(full);
            if !hash.is_empty() && !computed.eq_ignore_ascii_case(hash) {
                UpstreamMetricsRecorder::record_config_cache("supplier", "invalid");
                return Err(
                    "CONFIG_INVALID: supplier_config_json sha256 does not match supplier_config_hash"
                        .to_string(),
                );
            }
            let entry = self
                .supplier_config_cache
                .try_get_with(computed.clone(), || {
                    parse_cached_supplier_config(computed, full)
                })
                .map_err(|e| e.as_ref().clone())?;
            return Ok(Some(entry));
        }
        if !hash.is_empty() {
            UpstreamMetricsRecorder::record_config_cache("supplier", "miss");
            return Err(
                "CONFIG_CACHE_MISS: supplier config hash not in bypass cache; resend full supplier_config_json"
                    .to_string(),
            );
        }
        Ok(None)
    }

    fn resolve_egress_spec(&self, hash: &str, full: &str) -> Result<Arc<ChannelEgressSpec>, String> {
        let hash = hash.trim();
        let full = full.trim();
        if !hash.is_empty() {
            let lookup = if hash.bytes().all(|b| !b.is_ascii_uppercase()) {
                self.egress_spec_cache.get(hash)
            } else {
                self.egress_spec_cache.get(&hash.to_ascii_lowercase())
            };
            if let Some(cached) = lookup
                && cached.hash.eq_ignore_ascii_case(hash)
            {
                UpstreamMetricsRecorder::record_config_cache("egress", "hit");
                return Ok(cached.spec);
            }
        }
        if !full.is_empty() {
            let computed = hash_egress_spec_json(full);
            if !hash.is_empty() && !computed.eq_ignore_ascii_case(hash) {
                UpstreamMetricsRecorder::record_config_cache("egress", "invalid");
                return Err(
                    "SPEC_INVALID: egress_spec_json sha256 does not match egress_spec_hash".into(),
                );
            }
            return self
                .egress_spec_cache
                .try_get_with(computed.clone(), || parse_cached_egress_spec(computed, full))
                .map(|c| c.spec)
                .map_err(|e| e.as_ref().clone());
        }
        if hash.is_empty() {
            return Err("SPEC_INVALID: missing egress_spec_json".into());
        }
        UpstreamMetricsRecorder::record_config_cache("egress", "miss");
        Err("CONFIG_CACHE_MISS: egress spec hash not in bypass cache; resend egress_spec_json".into())
    }

    async fn load_artifact(&self, artifact_id: &str) -> Result<Option<CachedArtifact>, String> {
        let id = artifact_id.trim();
        if id.is_empty() {
            return Ok(None);
        }
        if let Some(cached) = self.artifact_cache.get(id) {
            return Ok(Some(cached));
        }
        let uuid = Uuid::parse_str(id)
            .map_err(|_| format!("ARTIFACT_INVALID: artifact_id '{id}' is not a UUID"))?;
        let row: Option<(Vec<u8>,)> =
            sqlx::query_as("SELECT content FROM upstream_artifacts WHERE id = $1")
                .bind(uuid)
                .fetch_optional(self.db.as_ref())
                .await
                .map_err(|e| format!("ARTIFACT_LOAD_FAILED: {e}"))?;
        match row {
            Some((bytes,)) => {
                let bytes: Arc<[u8]> = Arc::from(bytes);
                let descriptor_pool = decode_grpc_descriptor_pool(&bytes).ok();
                let cached = CachedArtifact {
                    bytes,
                    descriptor_pool,
                };
                self.artifact_cache.insert(id.to_string(), cached.clone());
                Ok(Some(cached))
            }
            None => Err(format!(
                "ARTIFACT_MISSING: upstream_artifacts id={id} not found"
            )),
        }
    }
}

struct OnceOutcome {
    response: upstream_proto::InvokeResponse,
    stage_timings: Vec<StageTiming>,
    trace_stages: Vec<StageResult>,
    response_headers: BTreeMap<String, String>,
}

impl MazbriBypassServiceImpl {
    /// `BypassHttpProxyService/Execute`（gRPC 入口）。
    pub async fn execute(
        &self,
        request: Request<upstream_proto::InvokeRequest>,
    ) -> Response<upstream_proto::InvokeResponse> {
        Response::new(self.execute_invoke(request.into_inner()).await)
    }

    /// 生产 gRPC 数据面 invoke 执行面（不采集报文快照与脱敏流水线，保持零额外开销）。
    pub async fn execute_invoke(
        &self,
        req: upstream_proto::InvokeRequest,
    ) -> upstream_proto::InvokeResponse {
        let options = ExecuteOptions {
            max_body_capture_bytes: 0,
            mask_secrets: false,
            collect_trace: false,
        };
        let (outcome, _) = self
            .run_traced_invoke(req, ExecuteMode::Production, options)
            .await;
        outcome.response
    }

    /// 管理探针 HTTP 执行面：复用同一连接池与适配器内核，同时回传脱敏流水线阶段、响应头与 `trace_id`。
    pub async fn execute_probe_invoke(&self, probe_req: ProbeInvokeRequest) -> ProbeInvokeResponse {
        let req: upstream_proto::InvokeRequest = probe_req.into();
        let options = ExecuteOptions {
            max_body_capture_bytes: 64 * 1024,
            mask_secrets: true,
            collect_trace: true,
        };
        let (outcome, trace_id) = self
            .run_traced_invoke(req, ExecuteMode::Business, options)
            .await;
        let mut probe_resp = ProbeInvokeResponse::from(outcome.response);
        probe_resp.response_headers = outcome.response_headers;
        if !outcome.trace_stages.is_empty() {
            probe_resp.stages_json =
                serde_json::to_string(&outcome.trace_stages).unwrap_or_default();
        }
        probe_resp.trace_id = trace_id;
        probe_resp
    }

    async fn run_traced_invoke(
        &self,
        req: upstream_proto::InvokeRequest,
        mode: ExecuteMode,
        options: ExecuteOptions,
    ) -> (OnceOutcome, String) {
        let start_time = Instant::now();
        let span = tracing::info_span!(
            "BypassHttpProxyService.Execute",
            otel.kind = "server",
            request_id = %req.request_id,
            channel_id = %req.channel_id
        );
        let trace_context = bypass_trace_context(&req, &span);
        let (mut outcome, spec) = async {
            match self.resolve_egress_spec(&req.egress_spec_hash, &req.egress_spec_json) {
                Ok(spec) => {
                    let out = self.execute_once(&req, &spec, mode, options).await;
                    (out, Some(spec))
                }
                Err(error) => (OnceOutcome::error(error), None),
            }
        }
        .instrument(span)
        .await;
        let trace_id = trace_context.trace_id.to_string();
        if outcome.response.error.starts_with(CONFIG_CACHE_MISS_PREFIX) {
            emit_config_cache_miss(&req, &trace_context, &outcome.response.error);
            return (outcome, trace_id);
        }
        let duration_ms = mazbri_core::num::millis_u64(start_time.elapsed());
        let snapshot = self.capture_log_snapshot(
            req,
            trace_context,
            spec.as_deref(),
            &outcome.response,
            std::mem::take(&mut outcome.stage_timings),
            duration_ms,
        );
        self.enqueue_log_snapshot(snapshot);
        (outcome, trace_id)
    }

    fn enqueue_log_snapshot(&self, snapshot: BypassLogSnapshot) {
        let Some(ref tx) = self.log_tx else {
            snapshot.emit();
            return;
        };
        PENDING_BYPASS_LOGS.fetch_add(1, Ordering::AcqRel);
        if let Err(err) = tx.try_send(snapshot) {
            let snapshot = match err {
                mpsc::error::TrySendError::Full(s) | mpsc::error::TrySendError::Closed(s) => s,
            };
            if PENDING_BYPASS_LOGS.fetch_sub(1, Ordering::AcqRel) == 1 {
                LOG_DRAIN_NOTIFY.notify_waiters();
            }
            snapshot.emit();
        }
    }

    fn capture_log_snapshot(
        &self,
        req: upstream_proto::InvokeRequest,
        trace_context: TraceContext,
        spec: Option<&ChannelEgressSpec>,
        response: &upstream_proto::InvokeResponse,
        stage_timings: Vec<StageTiming>,
        duration_ms: u64,
    ) -> BypassLogSnapshot {
        let request_bytes = req.payload_json.len() as u64;
        BypassLogSnapshot {
            service_name: Arc::clone(&self.service_name),
            user_agent: Arc::clone(&self.user_agent),
            trace_context,
            tenant_id: req.tenant_id,
            tenant_code: req.tenant_code,
            product_id: req.product_id,
            supplier_id: req.supplier_id,
            supplier_code: req.supplier_code,
            channel_id: req.channel_id,
            channel_code: req.channel_code,
            attempt_order: req.attempt_order,
            request_bytes,
            response_bytes: response.payload_json.len() as u64,
            spec_adapter: spec.map_or_default(|s| s.adapter.clone()),
            spec_method_upper: spec.map(|s| s.request.method.to_ascii_uppercase()),
            route_template: spec.map_or_default(|s| s.request.path.clone()),
            response_success: response.success,
            response_error: response.error.clone(),
            biz_error_code: response.biz_error.as_ref().map(|b| b.code.clone()),
            upstream_meta: response.upstream_meta.clone(),
            retry_reasons: response.retry_reasons.clone(),
            stage_timings,
            duration_ms,
        }
    }

    /// `BypassHttpProxyService/Health`.
    #[must_use]
    pub fn health(&self) -> Response<upstream_proto::HealthResponse> {
        Response::new(upstream_proto::HealthResponse {
            status: "ok".to_string(),
            service: self.service_name.as_ref().to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    /// 执行一次上游调用，返回结果与各阶段耗时（运行时三段 + 本地 `response_decode`）。
    async fn execute_once(
        &self,
        req: &upstream_proto::InvokeRequest,
        spec: &Arc<ChannelEgressSpec>,
        mode: ExecuteMode,
        options: ExecuteOptions,
    ) -> OnceOutcome {
        let start_time = Instant::now();
        let cached_supplier = match self
            .resolve_supplier_config(&req.supplier_config_hash, &req.supplier_config_json)
        {
            Ok(cfg) => cfg,
            Err(error) => return OnceOutcome::error(error),
        };
        let payload = match parse_payload(&req.payload_json) {
            Ok(v) => v,
            Err(error) => return OnceOutcome::error(error),
        };
        let artifact = match spec.artifact_id.as_deref().map(str::trim) {
            Some(id) if !id.is_empty() => match self.load_artifact(id).await {
                Ok(art) => art,
                Err(error) => return OnceOutcome::error(error),
            },
            _ => None,
        };
        let collect_probe_meta = options.collect_trace;
        let input = build_invoke_input(
            req,
            spec,
            cached_supplier,
            payload,
            artifact,
            mode,
            options,
        );
        let mut result = {
            let span = tracing::info_span!(
                "upstream.http",
                channel_id = %req.channel_id,
                adapter = %spec.adapter,
            );
            self.engine.invoke(input).instrument(span).await
        };
        let duration = start_time.elapsed().as_secs_f64();
        let retries = InternalRetries::of(&result);
        let mut stage_timings = std::mem::take(&mut result.stage_timings);
        let trace_stages = if collect_probe_meta {
            std::mem::take(&mut result.stages)
        } else {
            Vec::new()
        };
        let response_headers = if collect_probe_meta {
            result.response.as_ref().map_or_default(|r| {
                r.headers
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
        } else {
            BTreeMap::new()
        };
        let decode_start = Instant::now();
        let response = finalize_invoke_response(result, &spec.adapter, duration);
        stage_timings.push(StageTiming {
            name: "response_decode".into(),
            ms: mazbri_core::num::millis_u64(decode_start.elapsed()),
            ok: response.success,
        });
        OnceOutcome {
            response: retries.attach(response),
            stage_timings,
            trace_stages,
            response_headers,
        }
    }
}

impl OnceOutcome {
    fn error(error: String) -> Self {
        Self {
            response: invoke_error(error),
            stage_timings: Vec::new(),
            trace_stages: Vec::new(),
            response_headers: BTreeMap::new(),
        }
    }
}

fn build_invoke_input(
    req: &upstream_proto::InvokeRequest,
    spec: &Arc<ChannelEgressSpec>,
    cached_supplier: Option<CachedSupplierConfig>,
    payload: serde_json::Value,
    artifact: Option<CachedArtifact>,
    mode: ExecuteMode,
    options: ExecuteOptions,
) -> InvokeInput {
    let (config, credentials, pipeline_keys) = match cached_supplier {
        Some(c) => (Some(c.config), c.credentials, Some(c.pipeline_keys)),
        None => (None, HashMap::new(), None),
    };
    let timeout = if req.timeout_ms > 0 {
        Duration::from_millis(req.timeout_ms)
    } else {
        mazbri_upstream_runtime::resolve_timeout(
            0,
            spec.timeout_ms.or_else(|| {
                config
                    .as_ref()
                    .and_then(|c| c.timeouts.as_ref())
                    .and_then(|t| t.total_ms)
            }),
        )
    };
    let (artifact_bytes, descriptor_pool) = match artifact {
        Some(a) => (Some(a.bytes), a.descriptor_pool),
        None => (None, None),
    };
    InvokeInput {
        supplier_id: req.supplier_id.clone(),
        supplier_code: req.supplier_code.clone(),
        channel_id: req.channel_id.clone(),
        request_id: req.request_id.clone(),
        payload,
        spec: Arc::clone(spec),
        spec_validated: true,
        config,
        timeout,
        mode,
        options,
        credentials,
        pipeline_keys,
        artifact_bytes,
        descriptor_pool,
        cache_key: None,
    }
}

fn finalize_invoke_response(
    result: mazbri_upstream_runtime::InvokeResult,
    adapter: &str,
    duration: f64,
) -> upstream_proto::InvokeResponse {
    if let Some(ref code) = result.error_code {
        let err_msg = result
            .error_message
            .clone()
            .unwrap_or_else(|| "upstream invoke failed".into());
        let mut failed = invoke_failure(code, err_msg, adapter, duration);
        failed.upstream_meta = result
            .upstream_meta
            .clone()
            .filter(|m| m.status > 0)
            .map(proto_upstream_meta);
        failed
    } else {
        invoke_success(result, adapter, duration)
    }
}

/// 运行时内部重试，成功与失败路径都透传给网关（计入访问日志 `retry_count`）。
struct InternalRetries {
    count: u32,
    reasons: Vec<String>,
}

impl InternalRetries {
    fn of(result: &mazbri_upstream_runtime::InvokeResult) -> Self {
        Self {
            count: result.internal_retries,
            reasons: result
                .retry_reasons
                .iter()
                .map(|r| r.label().to_string())
                .collect(),
        }
    }

    fn attach(
        self,
        mut response: upstream_proto::InvokeResponse,
    ) -> upstream_proto::InvokeResponse {
        response.internal_retries = self.count;
        response.retry_reasons = self.reasons;
        response
    }
}

fn invoke_failure(
    code: &str,
    err_msg: String,
    adapter: &str,
    duration: f64,
) -> upstream_proto::InvokeResponse {
    if code == "SSRF_BLOCKED" || err_msg.contains("SSRF") {
        UpstreamMetricsRecorder::record_ssrf(&err_msg);
        UpstreamMetricsRecorder::record_failure(adapter, "ssrf");
    } else if code == "AUTH_TOKEN_FAILED" {
        UpstreamMetricsRecorder::record_token_fetch("error");
    } else if code == "UPSTREAM_AUTH_REJECTED" {
        UpstreamMetricsRecorder::observe_duration(adapter, "auth_rejected", duration);
    } else if err_msg.contains("timeout") || err_msg.contains("Timeout") {
        UpstreamMetricsRecorder::record_timeout(adapter);
        UpstreamMetricsRecorder::observe_duration(adapter, "timeout", duration);
    } else {
        UpstreamMetricsRecorder::record_transport_error(adapter);
        UpstreamMetricsRecorder::observe_duration(adapter, "transport", duration);
    }
    let error = if code == "SSRF_BLOCKED" {
        format!("SSRF Violation: {err_msg}")
    } else if code == "AUTH_TOKEN_FAILED" {
        format!("Dynamic Auth Token Retrieval Error: {err_msg}")
    } else if code == "UPSTREAM_AUTH_REJECTED" {
        format!("{UPSTREAM_AUTH_REJECTED_PREFIX}{err_msg}")
    } else if code == "CONFIG_CACHE_MISS" || err_msg.contains("CONFIG_CACHE") {
        err_msg
    } else {
        format!("Transparent Bypass Outbound Error: {err_msg}")
    };
    invoke_error(error)
}

fn invoke_success(
    result: mazbri_upstream_runtime::InvokeResult,
    spec_adapter: &str,
    duration: f64,
) -> upstream_proto::InvokeResponse {
    let adapter = spec_adapter;
    let metric_result = if result.ok { "ok" } else { "biz" };
    let http_status = result.upstream_meta.as_ref().map_or(200, |m| m.status);
    let status_code = reqwest::StatusCode::from_u16(http_status).unwrap_or(reqwest::StatusCode::OK);
    UpstreamMetricsRecorder::record_response(adapter, status_code, metric_result);
    UpstreamMetricsRecorder::observe_duration(adapter, metric_result, duration);
    if result.token.is_some() {
        UpstreamMetricsRecorder::record_token_fetch("ok");
    }

    let payload_json = result.raw_payload_json.unwrap_or_else(|| {
        result
            .payload
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok())
            .unwrap_or_default()
    });
    let biz_error = result.biz_error.map(|b| upstream_proto::InvokeBizError {
        code: b.code,
        message: b.message,
        details_json: b
            .details
            .and_then(|d| serde_json::to_string(&d).ok())
            .unwrap_or_default(),
    });
    let meta = result
        .upstream_meta
        .unwrap_or(mazbri_upstream_runtime::UpstreamMeta {
            adapter: spec_adapter.to_string(),
            latency_ms: result.latency_ms,
            latency_us: result.latency_ms.saturating_mul(1_000),
            ..Default::default()
        });

    upstream_proto::InvokeResponse {
        success: result.error_code.is_none(),
        error: String::new(),
        biz_error,
        payload_json,
        upstream_meta: Some(proto_upstream_meta(meta)),
        internal_retries: 0,
        retry_reasons: Vec::new(),
    }
}

/// 解析调用载荷；空串视为 `{}`。
fn parse_payload(payload_json: &str) -> Result<serde_json::Value, String> {
    if payload_json.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(payload_json).map_err(|e| format!("invalid payload_json: {e}"))
}

fn proto_upstream_meta(
    meta: mazbri_upstream_runtime::UpstreamMeta,
) -> upstream_proto::InvokeUpstreamMeta {
    let upstream_wire_us = if meta.latency_us > 0 {
        meta.latency_us
    } else {
        meta.latency_ms.saturating_mul(1_000)
    };
    upstream_proto::InvokeUpstreamMeta {
        status: u32::from(meta.status),
        url: meta.url,
        adapter: meta.adapter,
        upstream_wire_ms: meta.latency_ms,
        upstream_request_id: meta.request_id,
        method: meta.method,
        path: meta.path,
        protocol: meta.protocol,
        upstream_wire_us,
    }
}

fn invoke_error(error: String) -> upstream_proto::InvokeResponse {
    upstream_proto::InvokeResponse {
        success: false,
        error,
        biz_error: None,
        payload_json: String::new(),
        upstream_meta: None,
        internal_retries: 0,
        retry_reasons: Vec::new(),
    }
}

/// 以网关传来的 trace 为父节点挂接 bypass 的服务端 span，并据此生成访问日志的链路 ID。
fn bypass_trace_context(req: &upstream_proto::InvokeRequest, span: &tracing::Span) -> TraceContext {
    let trace = req.trace.as_ref();
    let parent = trace.and_then(|t| remote_span_context(&t.trace_id, &t.span_id, t.sampled));
    let parent_span_id: Arc<str> = parent
        .as_ref()
        .map_or_else(|| Arc::from(""), |p| p.span_id().to_string().into());
    if let Some(parent) = parent {
        set_remote_parent(span, parent);
    }
    let request_id = trace
        .map(|t| t.request_id.as_str())
        .filter(|id| !id.is_empty())
        .unwrap_or(req.request_id.as_str());
    TraceContext::for_span(request_id_or_new(request_id), span, parent_span_id)
}

/// `(error_code, error_class)` for the access log.
fn classify_bypass_outcome(
    error: &str,
    biz_code: Option<&str>,
    is_success: bool,
) -> (String, String) {
    if is_success {
        (String::new(), "ok".to_string())
    } else if let Some(code) = biz_code {
        (code.to_string(), "biz".to_string())
    } else if error.contains("SSRF") {
        ("SSRF_BLOCKED".to_string(), "ssrf".to_string())
    } else if error.contains("Token") || error.contains("Auth") {
        ("AUTH_ERROR".to_string(), "auth_token".to_string())
    } else if error.contains("CONFIG_CACHE") || error.contains("SPEC_INVALID") {
        ("CONFIG_ERROR".to_string(), "config".to_string())
    } else if error.contains("timeout") || error.contains("Timeout") {
        ("UPSTREAM_TIMEOUT".to_string(), "timeout".to_string())
    } else {
        ("UPSTREAM_ERROR".to_string(), "transport".to_string())
    }
}

/// 异步访问与阶段日志的轻量快照：不克隆请求载荷、全量配置或响应大报文。
struct BypassLogSnapshot {
    service_name: Arc<str>,
    user_agent: Arc<str>,
    trace_context: TraceContext,
    tenant_id: String,
    tenant_code: String,
    product_id: String,
    supplier_id: String,
    supplier_code: String,
    channel_id: String,
    channel_code: String,
    attempt_order: u32,
    request_bytes: u64,
    response_bytes: u64,
    spec_adapter: String,
    spec_method_upper: Option<String>,
    route_template: String,
    response_success: bool,
    response_error: String,
    biz_error_code: Option<String>,
    upstream_meta: Option<upstream_proto::InvokeUpstreamMeta>,
    retry_reasons: Vec<String>,
    stage_timings: Vec<StageTiming>,
    duration_ms: u64,
}

impl BypassLogSnapshot {
    fn emit(&self) {
        self.emit_access();
        self.emit_stages();
    }

    fn emit_stages(&self) {
        let error_code_head = self.response_error.split(':').next().unwrap_or_default();
        for stage in &self.stage_timings {
            tracing::info!(
                target: "bypass.stage",
                service = %self.service_name,
                stage_name = %stage.name,
                stage_ms = stage.ms,
                request_id = %self.trace_context.request_id,
                trace_id = %self.trace_context.trace_id,
                span_id = %self.trace_context.span_id,
                tenant_id = %self.tenant_id,
                tenant_code = %self.tenant_code,
                product_id = %self.product_id,
                supplier_id = %self.supplier_id,
                supplier_code = %self.supplier_code,
                channel_id = %self.channel_id,
                channel_code = %self.channel_code,
                attempt_order = self.attempt_order,
                upstream_mode = %self.spec_adapter,
                result = if stage.ok { "ok" } else { "error" },
                error_stage = if stage.ok { "" } else { stage.name.as_str() },
                error_code = if stage.ok { "" } else { error_code_head },
                message = "Bypass stage completed"
            );
        }
    }

    fn emit_access(&self) {
        let meta = self.upstream_meta.as_ref();
        let adapter = meta
            .map(|m| m.adapter.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("http_json");
        let upstream_status = meta.map_or(0, |m| u16::saturating_from(m.status));
        let status_code = match upstream_status {
            0 if self.response_success => 200,
            0 => 502,
            s => s,
        };
        let is_success = self.response_success && self.biz_error_code.is_none();
        let (error_code, error_class) = classify_bypass_outcome(
            &self.response_error,
            self.biz_error_code.as_deref(),
            is_success,
        );
        let error_stage = if is_success {
            String::new()
        } else {
            "upstream".to_string()
        };
        let outcome = classify_outcome(&error_code, &error_stage, status_code);
        let upstream_host = meta
            .filter(|_| upstream_status > 0)
            .and_then(|m| normalize_upstream_host(&m.url));
        let outbound = |f: fn(&upstream_proto::InvokeUpstreamMeta) -> &str| {
            meta.map(f).filter(|s| !s.is_empty()).map(str::to_string)
        };
        let upstream_wire_ms = meta.map_or(0, |m| m.upstream_wire_ms);

        let event = AccessLogEvent::builder(self.service_name.as_ref().to_string())
            .message(format!("request_completed mode={adapter}"))
            .trace_context(&self.trace_context)
            .route(AccessRouteFields {
                tenant_id: non_empty(&self.tenant_id),
                tenant_code: non_empty(&self.tenant_code),
                product_id: non_empty(&self.product_id),
                supplier_id: non_empty(&self.supplier_id),
                supplier_code: non_empty(&self.supplier_code),
                channel_id: non_empty(&self.channel_id),
                channel_code: non_empty(&self.channel_code),
                attempt_order: Some(self.attempt_order).filter(|n| *n > 0),
                api_key_id: None,
            })
            .http(AccessHttpFields {
                method: outbound(|m| m.method.as_str())
                    .or_else(|| self.spec_method_upper.clone())
                    .unwrap_or_default(),
                protocol: outbound(|m| m.protocol.as_str()).unwrap_or_default(),
                host_header: None,
                path: outbound(|m| m.path.as_str())
                    .unwrap_or_else(|| self.route_template.clone()),
                route_template: self.route_template.clone(),
                status_code,
                request_bytes: self.request_bytes,
                response_bytes: self.response_bytes,
                user_agent: non_empty(&self.user_agent),
            })
            .timing(AccessTimingFields {
                duration_ms: self.duration_ms,
                upstream_service_time_ms: Some(upstream_wire_ms),
                dependency_wait_ms: 0,
                internal_ms: self.duration_ms.saturating_sub(upstream_wire_ms),
                post_response_ms: 0,
            })
            .network(AccessNetworkFields {
                upstream_host,
                upstream_status_code: Some(upstream_status),
                client_ip: None,
                xff: None,
                cache_hit: false,
                fallback_used: false,
                retry_reasons: self.retry_reasons.clone(),
                upstream_request_id: outbound(|m| m.upstream_request_id.as_str()),
            })
            .upstream_meta(AccessUpstreamMetaFields {
                upstream_mode: adapter.to_string(),
                contract_id: "BYPASS-HTTP-PROXY".into(),
            })
            .outcome(AccessOutcomeFields {
                error_code,
                error_stage,
                response_flags: outcome.response_flags.to_string(),
                error_source: outcome.error_source.as_str().to_string(),
                error_class,
            })
            .build();
        emit_access_log("mazbri-bypass.access", &event);
    }
}

fn non_empty(value: &str) -> Option<String> {
    Some(value.to_string()).filter(|s| !s.is_empty())
}

/// `CONFIG_CACHE_MISS` 只记内部事件（`SPAN_EVENT_TARGET`），不进访问日志。
fn emit_config_cache_miss(
    req: &upstream_proto::InvokeRequest,
    trace_context: &TraceContext,
    error: &str,
) {
    let kind = if error.contains("egress spec") {
        "egress"
    } else {
        "supplier"
    };
    tracing::info!(
        target: mazbri_core::trace_spans::SPAN_EVENT_TARGET,
        event = "config_cache_miss",
        kind,
        request_id = %trace_context.request_id,
        trace_id = %trace_context.trace_id,
        channel_id = %req.channel_id,
        supplier_code = %req.supplier_code,
        "bypass.config_cache_miss"
    );
}

#[cfg(test)]
mod pipeline_key_tests {
    use mazbri_upstream_runtime::pipeline_keys_from_credentials;
    use std::collections::HashMap;

    #[test]
    fn bypass_injects_pipeline_keys_from_credentials() {
        let mut credentials = HashMap::new();
        credentials.insert("client_secret".into(), "unit-test-secret".into());
        credentials.insert(
            "crypto_data_key".into(),
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".into(),
        );
        let keys = pipeline_keys_from_credentials(&credentials);
        assert!(keys.mac.is_some());
        assert!(keys.data.is_some());
        // 确保 Debug 不泄露密钥材料
        let dbg = format!("{keys:?}");
        assert!(!dbg.contains("unit-test-secret"));
        assert!(!dbg.contains("00112233445566778899aabbccddeeff"));
    }
}

#[cfg(test)]
mod retry_passthrough_tests {
    use super::*;
    use mazbri_upstream_runtime::{AuthRejectTrigger, InvokeResult, RetryReason, UpstreamMeta};

    fn result(ok: bool) -> InvokeResult {
        InvokeResult {
            ok,
            latency_ms: 1,
            stages: Vec::new(),
            payload: None,
            raw_payload_json: None,
            biz_error: None,
            error_code: (!ok).then(|| "UPSTREAM_AUTH_REJECTED".into()),
            error_message: None,
            upstream_meta: None,
            token: None,
            response: None,
            internal_retries: 1,
            retry_reasons: vec![RetryReason::AuthRejected(AuthRejectTrigger::JsonRpcCode(
                -32001,
            ))],
            retry_skipped: None,
            stage_timings: Vec::new(),
        }
    }

    #[test]
    fn success_path_carries_internal_retries() {
        let r = result(true);
        let retries = InternalRetries::of(&r);
        let resp = retries.attach(invoke_success(r, "jsonrpc", 0.01));
        assert!(resp.success);
        assert_eq!(resp.internal_retries, 1);
        assert_eq!(resp.retry_reasons, vec!["auth_rejected:jsonrpc_code"]);
    }

    #[test]
    fn failure_path_carries_internal_retries() {
        let r = result(false);
        let resp = InternalRetries::of(&r).attach(invoke_failure(
            "UPSTREAM_AUTH_REJECTED",
            "upstream rejected credentials".into(),
            "jsonrpc",
            0.01,
        ));
        assert!(!resp.success);
        assert!(
            resp.error.starts_with(UPSTREAM_AUTH_REJECTED_PREFIX),
            "{}",
            resp.error
        );
        assert_eq!(resp.internal_retries, 1);
        assert_eq!(resp.retry_reasons, vec!["auth_rejected:jsonrpc_code"]);
    }

    #[test]
    fn invoke_success_prefers_raw_payload_json_and_preserves_wire_us() {
        let mut r = result(true);
        r.raw_payload_json = Some(r#"{"fast":true}"#.into());
        r.upstream_meta = Some(UpstreamMeta {
            status: 200,
            adapter: "http_json".into(),
            latency_ms: 0,
            latency_us: 450,
            ..Default::default()
        });
        let resp = invoke_success(r, "http_json", 0.001);
        assert_eq!(resp.payload_json, r#"{"fast":true}"#);
        let meta = resp.upstream_meta.expect("meta");
        assert_eq!(meta.upstream_wire_ms, 0);
        assert_eq!(meta.upstream_wire_us, 450);
    }

    #[tokio::test]
    async fn config_cache_hits_by_hash_without_recomputing_and_rejects_initial_mismatch() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@127.0.0.1:5432/mazbri")
            .expect("lazy pool");
        let client = Arc::new(reqwest::Client::new());
        let svc = MazbriBypassServiceImpl::new(
            client,
            SsrfChecker::with_allow_private(true),
            "mazbri-bypass",
            Arc::new(pool),
        );

        let supplier_json = r#"{"timeouts":{"total_ms":5000}}"#;
        let valid_hash = hash_config_json(supplier_json);
        assert!(
            svc.resolve_supplier_config("deadbeef", supplier_json)
                .is_err(),
            "mismatched hash on uncached insert must fail"
        );
        let first = svc
            .resolve_supplier_config(&valid_hash, supplier_json)
            .expect("valid insert")
            .expect("some config");
        let second = svc
            .resolve_supplier_config(&valid_hash.to_ascii_uppercase(), supplier_json)
            .expect("cached hit")
            .expect("some config");
        assert!(Arc::ptr_eq(&first.config, &second.config));

        drain_pending_logs(Duration::from_millis(50)).await;
    }
}
