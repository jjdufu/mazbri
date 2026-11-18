//! Bypass HTTP proxy service — production path uses shared `mazbri-upstream-runtime`.

use crate::metrics::UpstreamMetricsRecorder;
use mazbri_core::num::SaturatingFrom;
use mazbri_core::trace_propagation::{remote_span_context, set_remote_parent};
use mazbri_core::{
    AccessHttpFields, AccessLogEvent, AccessNetworkFields, AccessOutcomeFields, AccessRouteFields,
    AccessTimingFields, AccessUpstreamMetaFields, ChannelEgressSpec, SupplierConfig, TraceContext,
    emit_access_log, hash_egress_spec_json, normalize_upstream_host, parse_and_validate_egress,
    classify_outcome, request_id_or_new,
};
use mazbri_upstream_contracts::UPSTREAM_AUTH_REJECTED_PREFIX;
use mazbri_upstream_contracts::shared as upstream_proto;
use mazbri_upstream_runtime::{
    pipeline_keys_from_credentials, ExecuteMode, ExecuteOptions, InvokeInput, RuntimeEngine,
    RuntimeOptions, SsrfChecker, StageTiming,
};
use moka::sync::Cache;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tonic::{Request, Response};
use tracing::Instrument;
use uuid::Uuid;

fn hash_config_json(config_json: &str) -> String {
    let digest = Sha256::digest(config_json.as_bytes());
    hex::encode(digest)
}

#[derive(Clone)]
struct CachedSupplierConfig {
    hash: String,
    config: Arc<SupplierConfig>,
}

#[derive(Clone)]
struct CachedEgressSpec {
    hash: String,
    spec: ChannelEgressSpec,
}

#[derive(Clone)]
pub struct MazbriBypassServiceImpl {
    engine: RuntimeEngine,
    service_name: Arc<str>,
    db: Arc<PgPool>,
    supplier_config_cache: Cache<String, CachedSupplierConfig>,
    egress_spec_cache: Cache<String, CachedEgressSpec>,
    artifact_cache: Cache<String, Arc<Vec<u8>>>,
    /// 出站 HTTP 客户端的 User-Agent（`upstream.user_agent`），记入访问日志。
    user_agent: Arc<str>,
}

/// bypass 侧配置缓存的空闲过期时间：命中即续期。网关只在首次或收到 `CONFIG_CACHE_MISS`
/// 后才发全量配置，按写入时间过期会让每个活跃渠道周期性地多一次未命中重发。
const CONFIG_CACHE_IDLE: Duration = Duration::from_secs(600);

/// bypass 返回给网关、要求带全量配置重发的错误前缀。
const CONFIG_CACHE_MISS_PREFIX: &str = "CONFIG_CACHE_MISS";

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
        }
    }

    #[must_use]
    pub fn with_user_agent(mut self, user_agent: &str) -> Self {
        self.user_agent = Arc::from(user_agent);
        self
    }

    pub fn with_options(mut self, options: RuntimeOptions) -> Self {
        self.engine = self.engine.with_options(options);
        self
    }

    fn resolve_supplier_config(
        &self,
        hash: &str,
        full: &str,
    ) -> Result<Option<Arc<SupplierConfig>>, String> {
        let hash = if hash.is_empty() {
            None
        } else {
            Some(hash.to_string())
        };
        let full = if full.is_empty() {
            None
        } else {
            Some(full.to_string())
        };

        if let Some(ref h) = hash
            && let Some(cached) = self.supplier_config_cache.get(h)
            && (full.is_none() || cached.hash == *h)
        {
            UpstreamMetricsRecorder::record_config_cache("supplier", "hit");
            return Ok(Some(cached.config));
        }

        if let Some(ref json) = full {
            let mut cfg: SupplierConfig = serde_json::from_str(json).map_err(|e| {
                UpstreamMetricsRecorder::record_config_cache("supplier", "invalid");
                format!("invalid supplier_config JSON: {e}")
            })?;
            cfg = cfg.decrypt_credentials_for_runtime().map_err(|e| {
                UpstreamMetricsRecorder::record_config_cache("supplier", "invalid");
                format!("decrypt supplier credentials failed: {e}")
            })?;
            let h = hash.unwrap_or_else(|| hash_config_json(json));
            let arc = Arc::new(cfg);
            self.supplier_config_cache.insert(
                h.clone(),
                CachedSupplierConfig {
                    hash: h,
                    config: arc.clone(),
                },
            );
            UpstreamMetricsRecorder::record_config_cache("supplier", "miss");
            return Ok(Some(arc));
        }

        if hash.is_some() {
            UpstreamMetricsRecorder::record_config_cache("supplier", "miss");
            return Err(
                "CONFIG_CACHE_MISS: supplier config hash not in bypass cache; resend full supplier_config_json"
                    .to_string(),
            );
        }
        Ok(None)
    }

    fn resolve_egress_spec(&self, hash: &str, full: &str) -> Result<ChannelEgressSpec, String> {
        let hash = hash.trim();
        let full = full.trim();
        if !hash.is_empty()
            && let Some(cached) = self.egress_spec_cache.get(hash)
            && (full.is_empty() || cached.hash == hash)
        {
            UpstreamMetricsRecorder::record_config_cache("egress", "hit");
            return Ok(cached.spec);
        }
        if full.is_empty() {
            if hash.is_empty() {
                return Err("SPEC_INVALID: missing egress_spec_json".into());
            }
            UpstreamMetricsRecorder::record_config_cache("egress", "miss");
            return Err(
                "CONFIG_CACHE_MISS: egress spec hash not in bypass cache; resend egress_spec_json"
                    .into(),
            );
        }
        let value: serde_json::Value = serde_json::from_str(full).map_err(|e| {
            UpstreamMetricsRecorder::record_config_cache("egress", "invalid");
            format!("invalid egress_spec JSON: {e}")
        })?;
        let spec = parse_and_validate_egress(&value).map_err(|e| {
            UpstreamMetricsRecorder::record_config_cache("egress", "invalid");
            e.to_string()
        })?;
        UpstreamMetricsRecorder::record_config_cache("egress", "miss");
        let h = if hash.is_empty() {
            hash_egress_spec_json(full)
        } else {
            hash.to_string()
        };
        self.egress_spec_cache.insert(
            h.clone(),
            CachedEgressSpec {
                hash: h,
                spec: spec.clone(),
            },
        );
        Ok(spec)
    }

    async fn load_artifact_bytes(&self, artifact_id: &str) -> Result<Option<Vec<u8>>, String> {
        let id = artifact_id.trim();
        if id.is_empty() {
            return Ok(None);
        }
        if let Some(cached) = self.artifact_cache.get(id) {
            return Ok(Some(cached.as_ref().clone()));
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
                let arc = Arc::new(bytes);
                self.artifact_cache.insert(id.to_string(), arc.clone());
                Ok(Some(arc.as_ref().clone()))
            }
            None => Err(format!(
                "ARTIFACT_MISSING: upstream_artifacts id={id} not found"
            )),
        }
    }
}

impl MazbriBypassServiceImpl {
    /// `BypassHttpProxyService/Execute`（gRPC 入口）。
    pub async fn execute(
        &self,
        request: Request<upstream_proto::InvokeRequest>,
    ) -> Response<upstream_proto::InvokeResponse> {
        Response::new(self.execute_invoke(request.into_inner()).await)
    }

    /// 与 gRPC Execute / mgmt HTTP 探针共用的 invoke 执行面。
    pub async fn execute_invoke(
        &self,
        req: upstream_proto::InvokeRequest,
    ) -> upstream_proto::InvokeResponse {
        let start_time = Instant::now();
        let span = tracing::info_span!(
            "BypassHttpProxyService.Execute",
            otel.kind = "server",
            request_id = %req.request_id,
            channel_id = %req.channel_id
        );
        let trace_context = bypass_trace_context(&req, &span);
        let (response, spec, stages) = async {
            match self.resolve_egress_spec(&req.egress_spec_hash, &req.egress_spec_json) {
                Ok(spec) => {
                    let (response, stages) = self.execute_once(&req, &spec).await;
                    (response, Some(spec), stages)
                }
                Err(error) => (invoke_error(error), None, Vec::new()),
            }
        }
        .instrument(span)
        .await;
        if response.error.starts_with(CONFIG_CACHE_MISS_PREFIX) {
            // 控制面配置未命中：上游没被调用，网关会带全量配置重发并在自己的访问行里计一次重试。
            // 只记内部事件，不写 502 访问行（对标 Envoy xDS 未命中不产生 access log）。
            emit_config_cache_miss(&req, &trace_context, &response.error);
            return response;
        }
        let ctx = AccessCtx {
            service_name: self.service_name.as_ref(),
            user_agent: self.user_agent.as_ref(),
            req: &req,
            trace_context: &trace_context,
            spec: spec.as_ref(),
            start_time,
        };
        emit_bypass_access(&ctx, &response);
        emit_bypass_stages(&ctx, &response, &stages);
        response
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
        spec: &ChannelEgressSpec,
    ) -> (upstream_proto::InvokeResponse, Vec<StageTiming>) {
        let start_time = Instant::now();
        let config = match self
            .resolve_supplier_config(&req.supplier_config_hash, &req.supplier_config_json)
        {
            Ok(cfg) => cfg,
            Err(error) => return (invoke_error(error), Vec::new()),
        };

        let payload = match parse_payload(&req.payload_json) {
            Ok(v) => v,
            Err(error) => return (invoke_error(error), Vec::new()),
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

        let mut credentials = HashMap::new();
        if let Some(ref cfg) = config
            && let Some(ref auth) = cfg.auth_flow
        {
            credentials = auth.credentials.clone();
        }

        let artifact_bytes = match spec.artifact_id.as_deref().map(str::trim) {
            Some(id) if !id.is_empty() => match self.load_artifact_bytes(id).await {
                Ok(bytes) => bytes,
                Err(error) => return (invoke_error(error), Vec::new()),
            },
            _ => None,
        };

        // 密钥仅经 PipelineKeys / KeyRef 注入 runtime；禁止明文日志。
        let pipeline_keys = Some(pipeline_keys_from_credentials(&credentials));
        let input = InvokeInput {
            supplier_id: req.supplier_id.clone(),
            supplier_code: req.supplier_code.clone(),
            channel_id: req.channel_id.clone(),
            request_id: req.request_id.clone(),
            payload,
            spec: spec.clone(),
            config,
            timeout,
            mode: ExecuteMode::Production,
            options: ExecuteOptions {
                follow_redirects: false,
                max_body_capture_bytes: 0,
                mask_secrets: false,
                collect_trace: false,
            },
            credentials,
            pipeline_keys,
            artifact_bytes,
            cache_key: None,
        };

        let mut result = {
            let span = tracing::info_span!(
                "upstream.http",
                channel_id = %req.channel_id,
                adapter = %spec.adapter,
            );
            self.engine.invoke(input).instrument(span).await
        };
        let duration = start_time.elapsed().as_secs_f64();
        let adapter = spec.adapter.as_str();
        let retries = InternalRetries::of(&result);
        let mut stages = std::mem::take(&mut result.stage_timings);
        let decode_start = Instant::now();

        let response = if let Some(ref code) = result.error_code {
            let err_msg = result
                .error_message
                .clone()
                .unwrap_or_else(|| "upstream invoke failed".into());
            let mut failed = invoke_failure(code, err_msg, adapter, duration);
            // 上游确实回了响应（如鉴权被拒）时保留真实状态与流水号；没发出去则不带 meta。
            failed.upstream_meta = result
                .upstream_meta
                .clone()
                .filter(|m| m.status > 0)
                .map(proto_upstream_meta);
            failed
        } else {
            invoke_success(result, &spec.adapter, duration)
        };
        stages.push(StageTiming {
            name: "response_decode".into(),
            ms: mazbri_core::num::millis_u64(decode_start.elapsed()),
            ok: response.success,
        });
        (retries.attach(response), stages)
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

    let payload_json = result
        .payload
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok())
        .unwrap_or_default();
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

fn proto_upstream_meta(meta: mazbri_upstream_runtime::UpstreamMeta) -> upstream_proto::InvokeUpstreamMeta {
    upstream_proto::InvokeUpstreamMeta {
        status: u32::from(meta.status),
        url: meta.url,
        adapter: meta.adapter,
        upstream_wire_ms: meta.latency_ms,
        upstream_request_id: meta.request_id,
        method: meta.method,
        path: meta.path,
        protocol: meta.protocol,
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
    response: &upstream_proto::InvokeResponse,
    is_success: bool,
) -> (String, String) {
    if is_success {
        (String::new(), "ok".to_string())
    } else if let Some(ref biz) = response.biz_error {
        (biz.code.clone(), "biz".to_string())
    } else if response.error.contains("SSRF") {
        ("SSRF_BLOCKED".to_string(), "ssrf".to_string())
    } else if response.error.contains("Token") || response.error.contains("Auth") {
        ("AUTH_ERROR".to_string(), "auth_token".to_string())
    } else if response.error.contains("CONFIG_CACHE") || response.error.contains("SPEC_INVALID") {
        ("CONFIG_ERROR".to_string(), "config".to_string())
    } else if response.error.contains("timeout") || response.error.contains("Timeout") {
        ("UPSTREAM_TIMEOUT".to_string(), "timeout".to_string())
    } else {
        ("UPSTREAM_ERROR".to_string(), "transport".to_string())
    }
}

/// 访问行的请求级上下文（与本次调用结果无关的部分）。
struct AccessCtx<'a> {
    service_name: &'a str,
    user_agent: &'a str,
    req: &'a upstream_proto::InvokeRequest,
    trace_context: &'a TraceContext,
    spec: Option<&'a ChannelEgressSpec>,
    start_time: Instant,
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

/// bypass 阶段日志（target `bypass.stage`，与网关阶段日志同表）：每次真正执行上游调用写
/// `egress_build`、`upstream_auth`、`upstream_wire`、`response_decode` 各一行（认证重试轮另有
/// `#retry` 行），维度与访问行一致，可按 `request_id` / `trace_id` 和网关阶段串起来。
fn emit_bypass_stages(
    ctx: &AccessCtx<'_>,
    response: &upstream_proto::InvokeResponse,
    stages: &[StageTiming],
) {
    let req = ctx.req;
    let adapter = ctx.spec.map_or("", |s| s.adapter.as_str());
    for stage in stages {
        tracing::info!(
            target: "bypass.stage",
            service = %ctx.service_name,
            stage_name = %stage.name,
            stage_ms = stage.ms,
            request_id = %ctx.trace_context.request_id,
            trace_id = %ctx.trace_context.trace_id,
            span_id = %ctx.trace_context.span_id,
            tenant_id = %req.tenant_id,
            tenant_code = %req.tenant_code,
            product_id = %req.product_id,
            supplier_id = %req.supplier_id,
            supplier_code = %req.supplier_code,
            channel_id = %req.channel_id,
            channel_code = %req.channel_code,
            attempt_order = req.attempt_order,
            upstream_mode = %adapter,
            result = if stage.ok { "ok" } else { "error" },
            error_stage = if stage.ok { "" } else { stage.name.as_str() },
            error_code = if stage.ok { "" } else { response.error.split(':').next().unwrap_or_default() },
            message = "Bypass stage completed"
        );
    }
}

fn emit_bypass_access(ctx: &AccessCtx<'_>, response: &upstream_proto::InvokeResponse) {
    let req = ctx.req;
    let duration_ms = mazbri_core::num::millis_u64(ctx.start_time.elapsed());
    let route_template = ctx.spec.map_or_default(|s| s.request.path.clone());
    let meta = response.upstream_meta.as_ref();
    let adapter = meta
        .map(|m| m.adapter.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("http_json");
    // 上游真正回了响应才有状态码；没发到上游的失败记 upstream_status_code=0、upstream_host 空。
    let upstream_status = meta.map_or(0, |m| u16::saturating_from(m.status));
    let status_code = match upstream_status {
        0 if response.success => 200,
        0 => 502,
        s => s,
    };
    let is_success = response.success && response.biz_error.is_none();
    let (error_code, error_class) = classify_bypass_outcome(response, is_success);
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

    let event = AccessLogEvent::builder(ctx.service_name.to_string())
        .message(format!("request_completed mode={adapter}"))
        .trace_context(ctx.trace_context)
        .route(AccessRouteFields {
            tenant_id: non_empty(&req.tenant_id),
            tenant_code: non_empty(&req.tenant_code),
            product_id: non_empty(&req.product_id),
            supplier_id: non_empty(&req.supplier_id),
            supplier_code: non_empty(&req.supplier_code),
            channel_id: non_empty(&req.channel_id),
            channel_code: non_empty(&req.channel_code),
            attempt_order: Some(req.attempt_order).filter(|n| *n > 0),
            api_key_id: None,
        })
        .http(AccessHttpFields {
            // 记真实出站值（对标 Kong `upstream_uri`、Apigee `target_url`），不是 gRPC hop 的值。
            method: outbound(|m| m.method.as_str())
                .or_else(|| ctx.spec.map(|s| s.request.method.to_ascii_uppercase()))
                .unwrap_or_default(),
            protocol: outbound(|m| m.protocol.as_str()).unwrap_or_default(),
            host_header: None,
            path: outbound(|m| m.path.as_str()).unwrap_or_else(|| route_template.clone()),
            route_template,
            status_code,
            request_bytes: req.payload_json.len() as u64,
            response_bytes: response.payload_json.len() as u64,
            user_agent: non_empty(ctx.user_agent),
        })
        .timing(AccessTimingFields {
            duration_ms,
            // Pure upstream wire time (not full Execute).
            upstream_service_time_ms: Some(upstream_wire_ms),
            dependency_wait_ms: 0,
            internal_ms: duration_ms.saturating_sub(upstream_wire_ms),
            post_response_ms: 0,
        })
        .network(AccessNetworkFields {
            upstream_host,
            upstream_status_code: Some(upstream_status),
            client_ip: None,
            xff: None,
            cache_hit: false,
            fallback_used: false,
            retry_reasons: response.retry_reasons.clone(),
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
    use mazbri_upstream_runtime::{AuthRejectTrigger, InvokeResult, RetryReason};

    fn result(ok: bool) -> InvokeResult {
        InvokeResult {
            ok,
            latency_ms: 1,
            stages: Vec::new(),
            payload: None,
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
}
