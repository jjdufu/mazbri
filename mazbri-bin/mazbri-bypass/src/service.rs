//! Bypass HTTP proxy service — production path uses shared `mazbri-upstream-runtime`.

use crate::metrics::UpstreamMetricsRecorder;
use mazbri_core::{
    AccessHttpFields, AccessLogEvent, AccessNetworkFields, AccessOutcomeFields, AccessRouteFields,
    AccessTimingFields, AccessUpstreamMetaFields, ChannelEgressSpec, SupplierConfig,
    emit_access_log, hash_egress_spec_json, normalize_upstream_host, parse_and_validate_egress,
    parse_trace_context, response_flags_for_error,
};
use mazbri_upstream_contracts::bypass_http_proxy::bypass_http_proxy_service_server::BypassHttpProxyService;
use mazbri_upstream_contracts::shared as upstream_proto;
use mazbri_upstream_runtime::{
    ExecuteMode, ExecuteOptions, InvokeInput, RuntimeEngine, RuntimeLimits, SsrfChecker,
    TokenCache,
};
use moka::sync::Cache;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tonic::{Request, Response, Status};
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
}

impl MazbriBypassServiceImpl {
    pub fn new(
        client: Arc<reqwest::Client>,
        ssrf_checker: SsrfChecker,
        service_name: impl Into<Arc<str>>,
        db: Arc<PgPool>,
    ) -> Self {
        let token_cache = Arc::new(TokenCache::new());
        let engine = RuntimeEngine::with_token_cache(client, ssrf_checker, token_cache);
        Self {
            engine,
            service_name: service_name.into(),
            db,
            supplier_config_cache: Cache::builder()
                .max_capacity(4_096)
                .time_to_live(Duration::from_secs(600))
                .build(),
            egress_spec_cache: Cache::builder()
                .max_capacity(4_096)
                .time_to_live(Duration::from_secs(600))
                .build(),
            artifact_cache: Cache::builder()
                .max_capacity(32)
                .time_to_live(Duration::from_secs(600))
                .build(),
        }
    }

    pub fn with_limits(mut self, limits: RuntimeLimits) -> Self {
        self.engine = self.engine.with_limits(limits);
        self
    }

    fn resolve_supplier_config(
        &self,
        hash: &str,
        full: &str,
    ) -> Result<Option<Arc<SupplierConfig>>, String> {
        let hash = if hash.is_empty() { None } else { Some(hash.to_string()) };
        let full = if full.is_empty() { None } else { Some(full.to_string()) };

        if let Some(ref h) = hash {
            if let Some(cached) = self.supplier_config_cache.get(h) {
                if full.is_none() || cached.hash == *h {
                    UpstreamMetricsRecorder::record_config_cache("hit");
                    return Ok(Some(cached.config));
                }
            }
        }

        if let Some(ref json) = full {
            let mut cfg: SupplierConfig = serde_json::from_str(json).map_err(|e| {
                UpstreamMetricsRecorder::record_config_cache("invalid");
                format!("invalid supplier_config JSON: {e}")
            })?;
            cfg = cfg.decrypt_credentials_for_runtime().map_err(|e| {
                UpstreamMetricsRecorder::record_config_cache("invalid");
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
            UpstreamMetricsRecorder::record_config_cache("miss");
            return Ok(Some(arc));
        }

        if hash.is_some() {
            UpstreamMetricsRecorder::record_config_cache("miss");
            return Err(
                "CONFIG_CACHE_MISS: supplier config hash not in bypass cache; resend full supplier_config_json"
                    .to_string(),
            );
        }
        Ok(None)
    }

    fn resolve_egress_spec(
        &self,
        hash: &str,
        full: &str,
    ) -> Result<ChannelEgressSpec, String> {
        let hash = hash.trim();
        let full = full.trim();
        if !hash.is_empty() {
            if let Some(cached) = self.egress_spec_cache.get(hash) {
                if full.is_empty() || cached.hash == hash {
                    return Ok(cached.spec);
                }
            }
        }
        if full.is_empty() {
            if hash.is_empty() {
                return Err("SPEC_INVALID: missing egress_spec_json".into());
            }
            return Err(
                "CONFIG_CACHE_MISS: egress spec hash not in bypass cache; resend egress_spec_json"
                    .into(),
            );
        }
        let value: serde_json::Value = serde_json::from_str(full)
            .map_err(|e| format!("invalid egress_spec JSON: {e}"))?;
        let spec = parse_and_validate_egress(&value).map_err(|e| e.to_string())?;
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
        let uuid = Uuid::parse_str(id).map_err(|_| {
            format!("ARTIFACT_INVALID: artifact_id '{id}' is not a UUID")
        })?;
        let row: Option<(Vec<u8>,)> = sqlx::query_as(
            "SELECT content FROM upstream_artifacts WHERE id = $1",
        )
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

#[tonic::async_trait]
impl BypassHttpProxyService for MazbriBypassServiceImpl {
    async fn execute(
        &self,
        request: Request<upstream_proto::InvokeRequest>,
    ) -> Result<Response<upstream_proto::InvokeResponse>, Status> {
        let req = request.into_inner();
        let start_time = Instant::now();
        let response = self.execute_once(&req).await;
        emit_bypass_access(self.service_name.as_ref(), &req, start_time, &response);
        Ok(Response::new(response))
    }

    async fn health(
        &self,
        _request: Request<upstream_proto::HealthRequest>,
    ) -> Result<Response<upstream_proto::HealthResponse>, Status> {
        Ok(Response::new(upstream_proto::HealthResponse {
            status: "ok".to_string(),
            service: self.service_name.as_ref().to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        }))
    }
}

impl MazbriBypassServiceImpl {
    async fn execute_once(&self, req: &upstream_proto::InvokeRequest) -> upstream_proto::InvokeResponse {
        let start_time = Instant::now();
        let spec = match self.resolve_egress_spec(&req.egress_spec_hash, &req.egress_spec_json) {
            Ok(s) => s,
            Err(error) => {
                return invoke_error(error);
            }
        };
        let config = match self
            .resolve_supplier_config(&req.supplier_config_hash, &req.supplier_config_json)
        {
            Ok(cfg) => cfg,
            Err(error) => return invoke_error(error),
        };

        let payload: serde_json::Value = if req.payload_json.trim().is_empty() {
            serde_json::json!({})
        } else {
            match serde_json::from_str(&req.payload_json) {
                Ok(v) => v,
                Err(e) => return invoke_error(format!("invalid payload_json: {e}")),
            }
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
        if let Some(ref cfg) = config {
            if let Some(ref auth) = cfg.auth_flow {
                credentials = auth.credentials.clone();
            }
        }

        let fallback = if req.fallback_base_url.is_empty() {
            spec.base_url.clone().unwrap_or_default()
        } else {
            req.fallback_base_url.clone()
        };

        let artifact_bytes = match spec.artifact_id.as_deref().map(str::trim) {
            Some(id) if !id.is_empty() => match self.load_artifact_bytes(id).await {
                Ok(bytes) => bytes,
                Err(error) => return invoke_error(error),
            },
            _ => None,
        };

        let input = InvokeInput {
            supplier_id: req.supplier_id.clone(),
            supplier_code: req.supplier_code.clone(),
            channel_id: req.channel_id.clone(),
            request_id: req.request_id.clone(),
            payload,
            spec: spec.clone(),
            config,
            fallback_base_url: fallback,
            path_prefix: req.path_prefix.clone(),
            timeout,
            mode: ExecuteMode::Production,
            options: ExecuteOptions {
                follow_redirects: false,
                max_body_capture_bytes: 0,
                mask_secrets: false,
                collect_trace: false,
            },
            credentials,
            artifact_bytes,
            cache_key: None,
        };

        let result = self.engine.invoke(input).await;
        let duration = start_time.elapsed().as_secs_f64();
        let adapter = spec.adapter.as_str();

        if let Some(ref code) = result.error_code {
            let err_msg = result
                .error_message
                .clone()
                .unwrap_or_else(|| "upstream invoke failed".into());
            if code == "SSRF_BLOCKED" || err_msg.contains("SSRF") {
                UpstreamMetricsRecorder::record_ssrf(&err_msg);
                UpstreamMetricsRecorder::record_failure(adapter, "ssrf");
            } else if code == "AUTH_TOKEN_FAILED" {
                UpstreamMetricsRecorder::record_token_fetch("error");
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
            } else if code == "CONFIG_CACHE_MISS" || err_msg.contains("CONFIG_CACHE") {
                err_msg
            } else {
                format!("Transparent Bypass Outbound Error: {err_msg}")
            };
            return invoke_error(error);
        }

        let metric_result = if result.ok { "ok" } else { "biz" };
        let http_status = result
            .upstream_meta
            .as_ref()
            .map(|m| m.status)
            .unwrap_or(200);
        let status_code =
            reqwest::StatusCode::from_u16(http_status).unwrap_or(reqwest::StatusCode::OK);
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
                adapter: spec.adapter.clone(),
                latency_ms: result.latency_ms,
                ..Default::default()
            });

        upstream_proto::InvokeResponse {
            success: result.error_code.is_none(),
            error: String::new(),
            biz_error,
            payload_json,
            upstream_meta: Some(upstream_proto::InvokeUpstreamMeta {
                status: u32::from(meta.status),
                url: meta.url,
                adapter: meta.adapter,
                latency_ms: meta.latency_ms,
            }),
        }
    }
}

fn invoke_error(error: String) -> upstream_proto::InvokeResponse {
    upstream_proto::InvokeResponse {
        success: false,
        error,
        biz_error: None,
        payload_json: String::new(),
        upstream_meta: None,
    }
}

fn emit_bypass_access(
    service_name: &str,
    req: &upstream_proto::InvokeRequest,
    start_time: Instant,
    response: &upstream_proto::InvokeResponse,
) {
    let duration_ms = start_time.elapsed().as_millis() as u64;
    let mut headers = HashMap::new();
    if let Some(ref t) = req.trace {
        if !t.request_id.is_empty() {
            headers.insert("x-request-id".into(), t.request_id.clone());
        }
        if !t.trace_id.is_empty() {
            headers.insert("x-trace-id".into(), t.trace_id.clone());
        }
        if !t.span_id.is_empty() {
            headers.insert("x-span-id".into(), t.span_id.clone());
        }
    }
    let trace_context = parse_trace_context(&headers);
    let adapter = response
        .upstream_meta
        .as_ref()
        .map(|m| m.adapter.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("http_json");
    let (status_code, upstream_url) = if let Some(ref meta) = response.upstream_meta {
        (meta.status as u16, meta.url.clone())
    } else {
        (
            if response.success { 200 } else { 502 },
            req.fallback_base_url.clone(),
        )
    };
    let is_success = response.success && response.biz_error.is_none();
    let (error_code, error_class) = if is_success {
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
    };
    let error_stage = if is_success {
        String::new()
    } else {
        "upstream".to_string()
    };
    let response_flags = response_flags_for_error(&error_code, &error_stage, status_code);
    let upstream_host = normalize_upstream_host(&upstream_url);
    let response_bytes = response.payload_json.len() as u64;

    let event = AccessLogEvent::builder(service_name.to_string())
        .message(format!("request_completed mode={adapter}"))
        .trace_context(&trace_context)
        .route(AccessRouteFields {
            tenant_id: Some(req.tenant_id.clone()).filter(|s| !s.is_empty()),
            tenant_code: None,
            product_id: Some(req.product_id.clone()).filter(|s| !s.is_empty()),
            supplier_id: Some(req.supplier_id.clone()).filter(|s| !s.is_empty()),
            supplier_code: Some(req.supplier_code.clone()).filter(|s| !s.is_empty()),
            channel_id: Some(req.channel_id.clone()).filter(|s| !s.is_empty()),
            channel_code: None,
            attempt_order: Some(1),
        })
        .http(AccessHttpFields {
            method: "INVOKE".into(),
            protocol: "HTTP/2".to_string(),
            host_header: None,
            path: req.path_prefix.clone(),
            route_template: req.path_prefix.clone(),
            status_code,
            request_bytes: req.payload_json.len() as u64,
            response_bytes,
            user_agent: None,
        })
        .timing(AccessTimingFields {
            duration_ms,
            upstream_service_time_ms: Some(duration_ms),
            dependency_wait_ms: 0,
            internal_ms: 0,
            post_response_ms: 0,
        })
        .network(AccessNetworkFields {
            upstream_host,
            upstream_status_code: Some(status_code),
            client_ip: None,
            xff: None,
            cache_hit: false,
            fallback_used: false,
            retry_count: 0,
        })
        .upstream_meta(AccessUpstreamMetaFields {
            upstream_mode: adapter.to_string(),
            contract_id: "BYPASS-HTTP-PROXY".into(),
        })
        .outcome(AccessOutcomeFields {
            error_code,
            error_stage,
            response_flags,
            error_class,
        })
        .build();
    emit_access_log("mazbri-bypass.access", event);
}
