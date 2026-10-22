//! Core execute pipeline.
//!
//! New path: `invoke` — validate spec → ssrf → auth profile → adapter.invoke.
//! Legacy `execute` remains for supplier transform / probe compatibility tests.
use mazbri_core::num::SaturatingFrom;

use chrono::Utc;
use reqwest::Method;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mazbri_core::{AdapterKind, AuthFlowConfig};

use crate::adapters::{AdapterCtx, invoke_adapter, merge_request_headers};
use crate::auth::{apply_auth, token_cache_key};
use crate::mask::{mask_headers, redact_body_preview};
use crate::ssrf::SsrfChecker;
use crate::template::extract_by_path;
use crate::token_cache::{CachedToken, TokenCache};
use crate::transform::{
    apply_request_transform, apply_response_body_transform, apply_response_headers_transform,
    inject_token,
};
use crate::types::{
    ExecuteError, ExecuteInput, ExecuteMode, ExecuteResult, InvokeInput, InvokeResult,
    OutboundResponse, StageResult, StageSummary, UpstreamMeta,
};

#[derive(Debug, Clone)]
pub struct RuntimeLimits {
    pub ws_handshake_timeout: Duration,
    pub job_poll_min_interval: Duration,
}

struct InvokeFail<'a> {
    start: Instant,
    stages: Vec<StageResult>,
    want_trace: bool,
    stage_name: &'a str,
    err: ExecuteError,
    stage_start: Instant,
    adapter: &'a str,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            ws_handshake_timeout: Duration::from_secs(10),
            job_poll_min_interval: Duration::from_millis(200),
        }
    }
}

#[derive(Clone)]
pub struct RuntimeEngine {
    client: Arc<reqwest::Client>,
    ssrf: SsrfChecker,
    token_cache: Arc<TokenCache>,
    limits: RuntimeLimits,
}

impl RuntimeEngine {
    #[must_use]
    pub fn new(client: Arc<reqwest::Client>, ssrf: SsrfChecker) -> Self {
        Self {
            client,
            ssrf,
            token_cache: Arc::new(TokenCache::new()),
            limits: RuntimeLimits::default(),
        }
    }

    pub fn with_token_cache(
        client: Arc<reqwest::Client>,
        ssrf: SsrfChecker,
        token_cache: Arc<TokenCache>,
    ) -> Self {
        Self {
            client,
            ssrf,
            token_cache,
            limits: RuntimeLimits::default(),
        }
    }

    #[must_use]
    pub fn with_limits(mut self, limits: RuntimeLimits) -> Self {
        self.limits = limits;
        self
    }

    #[must_use]
    pub fn token_cache(&self) -> Arc<TokenCache> {
        self.token_cache.clone()
    }

    #[must_use]
    pub fn ssrf(&self) -> &SsrfChecker {
        &self.ssrf
    }

    #[must_use]
    pub fn client(&self) -> Arc<reqwest::Client> {
        self.client.clone()
    }

    /// Code-free invoke: tenant JSON + channel egress spec.
    pub async fn invoke(&self, input: InvokeInput) -> InvokeResult {
        let start = Instant::now();
        let want_trace =
            input.options.collect_trace || !matches!(input.mode, ExecuteMode::Production);
        let mut stages: Vec<StageResult> = Vec::new();
        let mask = input.options.mask_secrets;
        let max_cap = input.options.max_body_capture_bytes;

        let v_start = Instant::now();
        if let Err(e) = mazbri_core::validate_egress_spec(&input.spec) {
            return self.invoke_fail(InvokeFail {
                start,
                stages,
                want_trace,
                stage_name: "validate",
                err: ExecuteError::SpecInvalid(e.to_string()),
                stage_start: v_start,
                adapter: &input.spec.adapter,
            });
        }
        if !input.payload.is_object() {
            return self.invoke_fail(InvokeFail {
                start,
                stages,
                want_trace,
                stage_name: "validate",
                err: ExecuteError::SpecInvalid("payload_json 必须是 JSON 对象".into()),
                stage_start: v_start,
                adapter: &input.spec.adapter,
            });
        }
        let base_url = input
            .spec
            .resolved_base_url(&input.fallback_base_url)
            .trim()
            .to_string();
        if base_url.is_empty() {
            return self.invoke_fail(InvokeFail {
                start,
                stages,
                want_trace,
                stage_name: "validate",
                err: ExecuteError::ConfigInvalid("base_url is empty".into()),
                stage_start: v_start,
                adapter: &input.spec.adapter,
            });
        }
        if want_trace {
            stages.push(StageResult {
                name: "validate".into(),
                ok: true,
                latency_ms: u64::saturating_from(v_start.elapsed().as_millis()),
                request_summary: Some(StageSummary {
                    method: Some(input.spec.request_method().to_string()),
                    url: Some(base_url.clone()),
                    headers_masked: None,
                    body_preview: Some(redact_body_preview(
                        &input.payload.to_string(),
                        max_cap,
                        mask,
                    )),
                    status: None,
                    extracted: None,
                    detail: Some(format!("adapter={}", input.spec.adapter)),
                }),
                response_summary: None,
                error: None,
            });
        }

        let s_start = Instant::now();
        if let Err(e) = self.ssrf.validate_url(&base_url).await {
            return self.invoke_fail(InvokeFail {
                start,
                stages,
                want_trace,
                stage_name: "ssrf",
                err: ExecuteError::SsrfBlocked(e),
                stage_start: s_start,
                adapter: &input.spec.adapter,
            });
        }
        if let Some(ref auth) = input.spec.auth {
            if let Ok(p) = auth.profile_kind() {
                if p.uses_ticket() {
                    if let Some(url) = auth
                        .token_url
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        if let Err(e) = self.ssrf.validate_url(url).await {
                            return self.invoke_fail(InvokeFail {
                                start,
                                stages,
                                want_trace,
                                stage_name: "ssrf",
                                err: ExecuteError::SsrfBlocked(format!("token_url: {e}")),
                                stage_start: s_start,
                                adapter: &input.spec.adapter,
                            });
                        }
                    }
                }
            }
        }
        if want_trace {
            stages.push(StageResult {
                name: "ssrf".into(),
                ok: true,
                latency_ms: u64::saturating_from(s_start.elapsed().as_millis()),
                request_summary: None,
                response_summary: None,
                error: None,
            });
        }

        if matches!(input.mode, ExecuteMode::TransformPreview) {
            let wrapped = crate::adapters::wrap_by_name(&input.spec, &input.payload);
            let (preview, wrap_err) = match wrapped {
                Ok(v) => (v, None),
                Err(e) => (input.payload.clone(), Some(e)),
            };
            if want_trace {
                stages.push(StageResult {
                    name: "request_transform".into(),
                    ok: wrap_err.is_none(),
                    latency_ms: 0,
                    request_summary: Some(StageSummary {
                        method: Some(input.spec.request_method().to_string()),
                        url: Some(base_url),
                        headers_masked: None,
                        body_preview: Some(redact_body_preview(
                            &preview.to_string(),
                            max_cap,
                            mask,
                        )),
                        status: None,
                        extracted: None,
                        detail: Some("transform_preview (no network)".into()),
                    }),
                    response_summary: None,
                    error: wrap_err.as_ref().map(ToString::to_string),
                });
            }
            return InvokeResult {
                ok: wrap_err.is_none(),
                latency_ms: u64::saturating_from(start.elapsed().as_millis()),
                stages,
                payload: Some(preview),
                biz_error: None,
                error_code: wrap_err.as_ref().map(|e| e.code().to_string()),
                error_message: wrap_err.map(|e| e.to_string()),
                upstream_meta: None,
                token: None,
                response: None,
            };
        }

        let cache_key = input
            .cache_key
            .clone()
            .unwrap_or_else(|| token_cache_key(&input.supplier_id, &input.channel_id, &input.spec));
        let adapter_name = input.spec.adapter.clone();
        let mut attempt = 0u32;

        loop {
            if attempt >= 2 {
                return InvokeResult {
                    ok: false,
                    latency_ms: u64::saturating_from(start.elapsed().as_millis()),
                    stages,
                    payload: None,
                    biz_error: None,
                    error_code: Some("AUTH_TOKEN_FAILED".into()),
                    error_message: Some("Outbound request failed after 401 retries".into()),
                    upstream_meta: Some(UpstreamMeta {
                        adapter: adapter_name,
                        ..Default::default()
                    }),
                    token: None,
                    response: None,
                };
            }

            let a_start = Instant::now();
            let applied = match apply_auth(crate::auth::AuthApply {
                client: &self.client,
                ssrf: &self.ssrf,
                token_cache: &self.token_cache,
                spec: &input.spec,
                base_url: &base_url,
                credentials: &input.credentials,
                cache_key: &cache_key,
                evict_first: attempt > 0,
                request_id: &input.request_id,
            })
            .await
            {
                Ok(a) => a,
                Err(e) => {
                    return self.invoke_fail(InvokeFail {
                        start,
                        stages,
                        want_trace,
                        stage_name: "auth",
                        err: e,
                        stage_start: a_start,
                        adapter: &adapter_name,
                    });
                }
            };
            if want_trace {
                stages.push(StageResult {
                    name: "auth".into(),
                    ok: true,
                    latency_ms: u64::saturating_from(a_start.elapsed().as_millis()),
                    request_summary: Some(StageSummary {
                        method: None,
                        url: None,
                        headers_masked: Some(mask_headers(&applied.headers, mask)),
                        body_preview: None,
                        status: None,
                        extracted: Some(serde_json::json!({
                            "token_present": applied.token.is_some()
                        })),
                        detail: input.spec.auth.as_ref().map(|a| a.profile.clone()),
                    }),
                    response_summary: None,
                    error: None,
                });
            }

            let kind = match input.spec.adapter_kind() {
                Ok(k) => k,
                Err(e) => {
                    return self.invoke_fail(InvokeFail {
                        start,
                        stages,
                        want_trace,
                        stage_name: "adapter",
                        err: ExecuteError::SpecInvalid(e.to_string()),
                        stage_start: Instant::now(),
                        adapter: &adapter_name,
                    });
                }
            };
            let u_start = Instant::now();
            let ctx = AdapterCtx {
                client: &self.client,
                ssrf: &self.ssrf,
                input: &input,
                base_url: &base_url,
                auth: &applied,
                timeout: input.timeout,
                ws_handshake_timeout: self.limits.ws_handshake_timeout,
                job_poll_min_interval: self.limits.job_poll_min_interval,
            };
            if matches!(input.mode, ExecuteMode::AuthOnly)
                && matches!(kind, AdapterKind::Grpc | AdapterKind::WsUnary)
            {
                return InvokeResult {
                    ok: true,
                    latency_ms: u64::saturating_from(start.elapsed().as_millis()),
                    stages,
                    payload: None,
                    biz_error: None,
                    error_code: None,
                    error_message: None,
                    upstream_meta: None,
                    token: applied.token,
                    response: None,
                };
            }
            match invoke_adapter(&ctx, kind).await {
                Ok(outcome) => {
                    if matches!(input.mode, ExecuteMode::AuthOnly) {
                        return InvokeResult {
                            ok: true,
                            latency_ms: u64::saturating_from(start.elapsed().as_millis()),
                            stages,
                            payload: None,
                            biz_error: None,
                            error_code: None,
                            error_message: None,
                            upstream_meta: Some(UpstreamMeta {
                                status: outcome.response.status,
                                url: outcome.response.final_url.clone(),
                                adapter: adapter_name,
                                latency_ms: u64::saturating_from(start.elapsed().as_millis()),
                            }),
                            token: applied.token,
                            response: Some(outcome.response),
                        };
                    }
                    if outcome.response.status == 401 && applied.token.is_some() && attempt == 0 {
                        attempt += 1;
                        continue;
                    }
                    if want_trace {
                        stages.push(StageResult {
                            name: "upstream".into(),
                            ok: outcome.biz_error.is_none()
                                && (200..300).contains(&outcome.response.status),
                            latency_ms: u64::saturating_from(u_start.elapsed().as_millis()),
                            request_summary: Some(StageSummary {
                                method: Some(outcome.response.method.clone()),
                                url: Some(outcome.response.final_url.clone()),
                                headers_masked: Some(mask_headers(
                                    &merge_request_headers(&applied, &input.spec),
                                    mask,
                                )),
                                body_preview: None,
                                status: None,
                                extracted: None,
                                detail: None,
                            }),
                            response_summary: Some(StageSummary {
                                method: None,
                                url: None,
                                headers_masked: Some(mask_headers(&outcome.response.headers, mask)),
                                body_preview: Some(redact_body_preview(
                                    &String::from_utf8_lossy(&outcome.response.body),
                                    max_cap,
                                    mask,
                                )),
                                status: Some(outcome.response.status),
                                extracted: Some(outcome.payload.clone()),
                                detail: None,
                            }),
                            error: outcome.biz_error.as_ref().map(|b| b.message.clone()),
                        });
                    }
                    let meta = UpstreamMeta {
                        status: outcome.response.status,
                        url: outcome.response.final_url.clone(),
                        adapter: adapter_name,
                        latency_ms: u64::saturating_from(start.elapsed().as_millis()),
                    };
                    return InvokeResult {
                        ok: outcome.biz_error.is_none(),
                        latency_ms: meta.latency_ms,
                        stages,
                        payload: Some(outcome.payload),
                        biz_error: outcome.biz_error,
                        error_code: None,
                        error_message: None,
                        upstream_meta: Some(meta),
                        token: applied.token,
                        response: Some(outcome.response),
                    };
                }
                Err(e) => {
                    return self.invoke_fail(InvokeFail {
                        start,
                        stages,
                        want_trace,
                        stage_name: "upstream",
                        err: e,
                        stage_start: u_start,
                        adapter: &adapter_name,
                    });
                }
            }
        }
    }

    fn invoke_fail(&self, fail: InvokeFail<'_>) -> InvokeResult {
        let InvokeFail {
            start,
            mut stages,
            want_trace,
            stage_name,
            err,
            stage_start,
            adapter,
        } = fail;
        if want_trace {
            stages.push(StageResult {
                name: stage_name.into(),
                ok: false,
                latency_ms: u64::saturating_from(stage_start.elapsed().as_millis()),
                request_summary: None,
                response_summary: None,
                error: Some(err.to_string()),
            });
        }
        InvokeResult {
            ok: false,
            latency_ms: u64::saturating_from(start.elapsed().as_millis()),
            stages,
            payload: None,
            biz_error: None,
            error_code: Some(err.code().to_string()),
            error_message: Some(err.to_string()),
            upstream_meta: Some(UpstreamMeta {
                adapter: adapter.to_string(),
                ..Default::default()
            }),
            token: None,
            response: None,
        }
    }

    /// Full pipeline used by leftover transform-based tests.
    pub async fn execute(&self, input: ExecuteInput) -> ExecuteResult {
        let start = Instant::now();
        let want_trace =
            input.options.collect_trace || !matches!(input.mode, ExecuteMode::Production);
        let mut stages: Vec<StageResult> = Vec::new();
        let mask = input.options.mask_secrets;
        let max_cap = input.options.max_body_capture_bytes;

        // Stage: validate
        let v_start = Instant::now();
        if input.base_url.trim().is_empty() {
            return self.fail(
                start,
                stages,
                want_trace,
                "validate",
                ExecuteError::ConfigInvalid("base_url is empty".into()),
                v_start,
            );
        }
        if want_trace {
            stages.push(StageResult {
                name: "validate".into(),
                ok: true,
                latency_ms: u64::saturating_from(v_start.elapsed().as_millis()),
                request_summary: Some(StageSummary {
                    method: Some(input.request.method.clone()),
                    url: Some(input.base_url.clone()),
                    headers_masked: None,
                    body_preview: None,
                    status: None,
                    extracted: None,
                    detail: Some(format!("mode={:?}", input.mode)),
                }),
                response_summary: None,
                error: None,
            });
        }

        // Stage: ssrf (base + token_url)
        let s_start = Instant::now();
        if let Err(e) = self.ssrf.validate_url(&input.base_url).await {
            // transparent http(s) only — microservice host:port may not parse as URL
            if input.base_url.starts_with("http://") || input.base_url.starts_with("https://") {
                return self.fail(
                    start,
                    stages,
                    want_trace,
                    "ssrf",
                    ExecuteError::SsrfBlocked(e),
                    s_start,
                );
            }
        }
        if let Some(ref cfg) = input.config {
            if let Some(ref auth) = cfg.auth_flow {
                if auth.enabled {
                    match auth.profile_kind() {
                        Ok(p) if p.uses_ticket() && !auth.token_url.trim().is_empty() => {
                            if let Err(e) = self.ssrf.validate_url(&auth.token_url).await {
                                return self.fail(
                                    start,
                                    stages,
                                    want_trace,
                                    "ssrf",
                                    ExecuteError::SsrfBlocked(format!("token_url: {e}")),
                                    s_start,
                                );
                            }
                        }
                        Err(e) => {
                            return self.fail(
                                start,
                                stages,
                                want_trace,
                                "ssrf",
                                ExecuteError::SpecInvalid(e.to_string()),
                                s_start,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
        if want_trace {
            stages.push(StageResult {
                name: "ssrf".into(),
                ok: true,
                latency_ms: u64::saturating_from(s_start.elapsed().as_millis()),
                request_summary: None,
                response_summary: None,
                error: None,
            });
        }

        if matches!(input.mode, ExecuteMode::TransformPreview) {
            return self.execute_transform_preview(input, start, stages, mask, max_cap);
        }

        let cache_key = input
            .cache_key
            .clone()
            .unwrap_or_else(|| input.supplier_code.clone());

        let mut attempt = 0u32;
        let mut last_error = String::new();

        loop {
            if attempt >= 2 {
                return ExecuteResult {
                    ok: false,
                    latency_ms: u64::saturating_from(start.elapsed().as_millis()),
                    stages,
                    response: None,
                    error_code: Some("AUTH_TOKEN_FAILED".into()),
                    error_message: Some(format!(
                        "Outbound request failed after 401 retries: {last_error}"
                    )),
                    token: None,
                };
            }

            if attempt > 0 {
                self.token_cache.evict_token(&cache_key).await;
            }

            // Stage: auth
            let mut token: Option<String> = None;
            let a_start = Instant::now();
            if let Some(ref cfg) = input.config {
                if let Some(ref auth) = cfg.auth_flow {
                    let fetch = match auth.profile_kind() {
                        Err(e) if auth.enabled => {
                            return self.fail(
                                start,
                                stages,
                                want_trace,
                                "auth",
                                ExecuteError::SpecInvalid(e.to_string()),
                                a_start,
                            );
                        }
                        Ok(p) if auth.enabled && p.uses_ticket() => true,
                        _ => false,
                    };
                    if fetch {
                        match self.fetch_token(&cache_key, auth).await {
                            Ok(t) => {
                                if want_trace {
                                    stages.push(StageResult {
                                        name: "auth".into(),
                                        ok: true,
                                        latency_ms: u64::saturating_from(
                                            a_start.elapsed().as_millis(),
                                        ),
                                        request_summary: Some(StageSummary {
                                            method: Some(auth.method.clone()),
                                            url: Some(auth.token_url.clone()),
                                            headers_masked: Some(mask_headers(&auth.headers, mask)),
                                            body_preview: Some(redact_body_preview(
                                                &auth.request_body_tpl,
                                                max_cap,
                                                mask,
                                            )),
                                            status: None,
                                            extracted: Some(serde_json::json!({
                                                "token_present": true
                                            })),
                                            detail: None,
                                        }),
                                        response_summary: None,
                                        error: None,
                                    });
                                }
                                token = Some(t);
                            }
                            Err(e) => {
                                return self.fail(
                                    start,
                                    stages,
                                    want_trace,
                                    "auth",
                                    ExecuteError::AuthTokenFailed(e),
                                    a_start,
                                );
                            }
                        }
                    } else if want_trace {
                        stages.push(StageResult {
                            name: "auth".into(),
                            ok: true,
                            latency_ms: u64::saturating_from(a_start.elapsed().as_millis()),
                            request_summary: None,
                            response_summary: Some(StageSummary {
                                method: None,
                                url: None,
                                headers_masked: None,
                                body_preview: None,
                                status: None,
                                extracted: Some(serde_json::json!({"skipped": true})),
                                detail: Some("auth_flow disabled".into()),
                            }),
                            error: None,
                        });
                    }
                } else if want_trace {
                    stages.push(StageResult {
                        name: "auth".into(),
                        ok: true,
                        latency_ms: u64::saturating_from(a_start.elapsed().as_millis()),
                        request_summary: None,
                        response_summary: Some(StageSummary {
                            method: None,
                            url: None,
                            headers_masked: None,
                            body_preview: None,
                            status: None,
                            extracted: Some(serde_json::json!({"skipped": true})),
                            detail: Some("no auth_flow".into()),
                        }),
                        error: None,
                    });
                }
            }

            if matches!(input.mode, ExecuteMode::AuthOnly) {
                return ExecuteResult {
                    ok: token.is_some()
                        || input
                            .config
                            .as_ref()
                            .and_then(|c| c.auth_flow.as_ref())
                            .is_none_or(|a| !a.enabled),
                    latency_ms: u64::saturating_from(start.elapsed().as_millis()),
                    stages,
                    response: None,
                    error_code: None,
                    error_message: None,
                    token,
                };
            }

            // Stage: request_transform + inject
            let t_start = Instant::now();
            let original_path = input.request.path.clone();
            let mut req_path = input.request.path.clone();
            let mut outbound_headers = input.request.headers.clone();
            let mut outbound_query = input.request.query.clone();
            let mut req_body = input.request.body.clone();

            if let Some(ref cfg) = input.config {
                if let Some(ref trans) = cfg.transform {
                    apply_request_transform(
                        trans,
                        &mut req_path,
                        &mut outbound_headers,
                        &mut req_body,
                        &original_path,
                    );
                }
            }

            if let (Some(tok), Some(cfg)) = (token.as_ref(), input.config.as_ref()) {
                if let Some(ref auth) = cfg.auth_flow {
                    inject_token(
                        auth,
                        tok,
                        &mut outbound_headers,
                        &mut outbound_query,
                        &mut req_body,
                    );
                }
            }

            let target_url = build_target_url(&input.base_url, &req_path, &outbound_query);

            if want_trace {
                stages.push(StageResult {
                    name: "request_transform".into(),
                    ok: true,
                    latency_ms: u64::saturating_from(t_start.elapsed().as_millis()),
                    request_summary: Some(StageSummary {
                        method: Some(input.request.method.clone()),
                        url: Some(target_url.clone()),
                        headers_masked: Some(mask_headers(&outbound_headers, mask)),
                        body_preview: req_body
                            .as_ref()
                            .map(|b| redact_body_preview(b.as_str(), max_cap, mask)),
                        status: None,
                        extracted: None,
                        detail: Some(format!("path={req_path}")),
                    }),
                    response_summary: None,
                    error: None,
                });
            }

            // Final URL SSRF
            if let Err(e) = self.ssrf.validate_url(&target_url).await {
                return self.fail(
                    start,
                    stages,
                    want_trace,
                    "upstream",
                    ExecuteError::SsrfBlocked(e),
                    Instant::now(),
                );
            }

            let method_type = match Method::from_bytes(input.request.method.as_bytes()) {
                Ok(m) => m,
                Err(e) => {
                    return self.fail(
                        start,
                        stages,
                        want_trace,
                        "upstream",
                        ExecuteError::InvalidMethod(format!(
                            "Invalid HTTP method '{}': {}",
                            input.request.method, e
                        )),
                        Instant::now(),
                    );
                }
            };

            let mut header_map = HeaderMap::new();
            for (k, v) in &outbound_headers {
                if k.to_lowercase().starts_with("x-mazbri-") {
                    continue;
                }
                if let (Ok(hn), Ok(hv)) = (
                    HeaderName::from_bytes(k.as_bytes()),
                    HeaderValue::from_str(v),
                ) {
                    header_map.insert(hn, hv);
                }
            }

            if let Ok(parsed_url) = url::Url::parse(&input.base_url) {
                if let Some(host) = parsed_url.host_str() {
                    let host_header_val = if let Some(port) = parsed_url.port() {
                        format!("{host}:{port}")
                    } else {
                        host.to_string()
                    };
                    if let (Ok(hn), Ok(hv)) = (
                        HeaderName::from_bytes(b"host"),
                        HeaderValue::from_str(&host_header_val),
                    ) {
                        header_map.insert(hn, hv);
                    }
                }
            }

            let mut builder = self
                .client
                .request(method_type.clone(), &target_url)
                .headers(header_map)
                .timeout(input.timeout);

            if let Some(body) = req_body {
                let ct = body.content_type().to_string();
                builder = builder.header("content-type", ct).body(body.into_bytes());
            }

            let u_start = Instant::now();
            let response = builder.send().await;
            match response {
                Ok(resp) => {
                    let status_code = resp.status();

                    if status_code == reqwest::StatusCode::UNAUTHORIZED
                        && token.is_some()
                        && attempt == 0
                    {
                        attempt += 1;
                        last_error = "401 Unauthorized".to_string();
                        continue;
                    }

                    let mut resp_headers = HashMap::new();
                    for (key, val) in resp.headers() {
                        if let Ok(str_val) = val.to_str() {
                            resp_headers.insert(key.to_string(), str_val.to_string());
                        }
                    }

                    let body_bytes = resp.bytes().await.unwrap_or_default().to_vec();

                    if want_trace {
                        stages.push(StageResult {
                            name: "upstream".into(),
                            ok: status_code.is_success(),
                            latency_ms: u64::saturating_from(u_start.elapsed().as_millis()),
                            request_summary: Some(StageSummary {
                                method: Some(input.request.method.clone()),
                                url: Some(target_url.clone()),
                                headers_masked: Some(mask_headers(&outbound_headers, mask)),
                                body_preview: None,
                                status: None,
                                extracted: None,
                                detail: None,
                            }),
                            response_summary: Some(StageSummary {
                                method: None,
                                url: None,
                                headers_masked: Some(mask_headers(&resp_headers, mask)),
                                body_preview: Some(redact_body_preview(
                                    &String::from_utf8_lossy(&body_bytes),
                                    max_cap,
                                    mask,
                                )),
                                status: Some(status_code.as_u16()),
                                extracted: None,
                                detail: None,
                            }),
                            error: if status_code.is_success() {
                                None
                            } else {
                                Some(format!("HTTP {}", status_code.as_u16()))
                            },
                        });
                    }

                    // response transform
                    let rt_start = Instant::now();
                    if let Some(ref cfg) = input.config {
                        if let Some(ref trans) = cfg.transform {
                            apply_response_headers_transform(trans, &mut resp_headers);
                        }
                    }

                    // propagate tracing headers from inbound
                    for (key, val) in &input.request.headers {
                        let k_lower = key.to_lowercase();
                        if k_lower.starts_with("x-b3-")
                            || k_lower == "x-request-id"
                            || k_lower == "x-trace-id"
                            || k_lower == "x-span-id"
                            || k_lower == "x-mazbri-tenant-id"
                            || k_lower == "x-mazbri-product-id"
                        {
                            resp_headers.insert(key.clone(), val.clone());
                        }
                    }

                    let final_body = if let Some(ref cfg) = input.config {
                        if let Some(ref trans) = cfg.transform {
                            apply_response_body_transform(trans, body_bytes, status_code.as_u16())
                        } else {
                            body_bytes
                        }
                    } else {
                        body_bytes
                    };

                    if want_trace {
                        stages.push(StageResult {
                            name: "response_transform".into(),
                            ok: true,
                            latency_ms: u64::saturating_from(rt_start.elapsed().as_millis()),
                            request_summary: None,
                            response_summary: Some(StageSummary {
                                method: None,
                                url: None,
                                headers_masked: Some(mask_headers(&resp_headers, mask)),
                                body_preview: Some(redact_body_preview(
                                    &String::from_utf8_lossy(&final_body),
                                    max_cap,
                                    mask,
                                )),
                                status: Some(status_code.as_u16()),
                                extracted: None,
                                detail: None,
                            }),
                            error: None,
                        });
                    }

                    let outbound = OutboundResponse {
                        status: status_code.as_u16(),
                        headers: resp_headers,
                        body: final_body,
                        final_url: target_url,
                        method: input.request.method.clone(),
                        path: original_path,
                    };

                    return ExecuteResult {
                        ok: true,
                        latency_ms: u64::saturating_from(start.elapsed().as_millis()),
                        stages,
                        response: Some(outbound),
                        error_code: None,
                        error_message: None,
                        token,
                    };
                }
                Err(e) => {
                    let err = if e.is_timeout() {
                        ExecuteError::UpstreamTransport(format!("timeout: {e}"))
                    } else {
                        ExecuteError::UpstreamTransport(e.to_string())
                    };
                    return self.fail(start, stages, want_trace, "upstream", err, u_start);
                }
            }
        }
    }

    async fn fetch_token(&self, cache_key: &str, auth: &AuthFlowConfig) -> Result<String, String> {
        let client = self.client.clone();
        let ssrf = self.ssrf.clone();
        let auth_clone = auth.clone();

        self.token_cache
            .get_token(cache_key, || async move {
                if let Err(e) = ssrf.validate_url(&auth_clone.token_url).await {
                    return Err(format!("Auth SSRF Violation: {e}"));
                }
                let mut body_rendered = auth_clone.request_body_tpl.clone();
                for (k, v) in &auth_clone.credentials {
                    let placeholder = format!("{{{{{k}}}}}");
                    body_rendered = body_rendered.replace(&placeholder, v);
                }
                let method = if auth_clone.method.to_uppercase() == "GET" {
                    Method::GET
                } else {
                    Method::POST
                };
                let mut builder = client.request(method, &auth_clone.token_url);
                for (k, v) in &auth_clone.headers {
                    if let (Ok(hn), Ok(hv)) = (
                        HeaderName::from_bytes(k.as_bytes()),
                        HeaderValue::from_str(v),
                    ) {
                        builder = builder.header(hn, hv);
                    }
                }
                if auth_clone.method.to_uppercase() != "GET" && !body_rendered.is_empty() {
                    if !auth_clone
                        .headers
                        .keys()
                        .any(|k| k.eq_ignore_ascii_case("content-type"))
                    {
                        builder = builder.header("content-type", "application/json");
                    }
                    builder = builder.body(body_rendered);
                }
                let resp = builder
                    .send()
                    .await
                    .map_err(|e| format!("Token request failed: {e}"))?;
                let status = resp.status();
                if !status.is_success() {
                    return Err(format!("Token request returned status code {status}"));
                }
                let json = resp
                    .json::<serde_json::Value>()
                    .await
                    .map_err(|e| format!("Failed to parse token response as JSON: {e}"))?;
                let t_val =
                    extract_by_path(&json, &auth_clone.token_jsonpath).ok_or_else(|| {
                        format!("Token not found at path: {}", auth_clone.token_jsonpath)
                    })?;
                let t_str = t_val
                    .as_str()
                    .ok_or_else(|| "Token is not a string".to_string())?
                    .to_string();
                let mut expires_in_sec = 3600u64;
                if let Some(expiry_val) = extract_by_path(&json, &auth_clone.expiry_jsonpath) {
                    if let Some(n) = expiry_val.as_u64() {
                        expires_in_sec = n;
                    } else if let Some(s) = expiry_val.as_str() {
                        if let Ok(n) = s.parse::<u64>() {
                            expires_in_sec = n;
                        }
                    }
                }
                let expires_at =
                    Utc::now() + chrono::Duration::seconds(i64::saturating_from(expires_in_sec));
                Ok(CachedToken {
                    token: t_str,
                    expires_at,
                })
            })
            .await
    }

    fn execute_transform_preview(
        &self,
        input: ExecuteInput,
        start: Instant,
        mut stages: Vec<StageResult>,
        mask: bool,
        max_cap: usize,
    ) -> ExecuteResult {
        let original_path = input.request.path.clone();
        let mut req_path = input.request.path.clone();
        let mut outbound_headers = input.request.headers.clone();
        let mut outbound_query = input.request.query.clone();
        let mut req_body = input.request.body.clone();

        if let Some(ref cfg) = input.config {
            if let Some(ref trans) = cfg.transform {
                apply_request_transform(
                    trans,
                    &mut req_path,
                    &mut outbound_headers,
                    &mut req_body,
                    &original_path,
                );
            }
            // preview inject with placeholder token
            if let Some(ref auth) = cfg.auth_flow {
                if auth.enabled {
                    inject_token(
                        auth,
                        "PREVIEW_TOKEN",
                        &mut outbound_headers,
                        &mut outbound_query,
                        &mut req_body,
                    );
                }
            }
        }

        let target_url = build_target_url(&input.base_url, &req_path, &outbound_query);
        stages.push(StageResult {
            name: "request_transform".into(),
            ok: true,
            latency_ms: u64::saturating_from(start.elapsed().as_millis()),
            request_summary: Some(StageSummary {
                method: Some(input.request.method.clone()),
                url: Some(target_url),
                headers_masked: Some(mask_headers(&outbound_headers, mask)),
                body_preview: req_body
                    .as_ref()
                    .map(|b| redact_body_preview(b.as_str(), max_cap, mask)),
                status: None,
                extracted: None,
                detail: Some("transform_preview (no network)".into()),
            }),
            response_summary: None,
            error: None,
        });

        ExecuteResult {
            ok: true,
            latency_ms: u64::saturating_from(start.elapsed().as_millis()),
            stages,
            response: None,
            error_code: None,
            error_message: None,
            token: None,
        }
    }

    fn fail(
        &self,
        start: Instant,
        mut stages: Vec<StageResult>,
        want_trace: bool,
        stage_name: &str,
        err: ExecuteError,
        stage_start: Instant,
    ) -> ExecuteResult {
        if want_trace {
            stages.push(StageResult {
                name: stage_name.into(),
                ok: false,
                latency_ms: u64::saturating_from(stage_start.elapsed().as_millis()),
                request_summary: None,
                response_summary: None,
                error: Some(err.to_string()),
            });
        }
        ExecuteResult {
            ok: false,
            latency_ms: u64::saturating_from(start.elapsed().as_millis()),
            stages,
            response: None,
            error_code: Some(err.code().to_string()),
            error_message: Some(err.to_string()),
            token: None,
        }
    }
}

fn build_target_url(base_url: &str, path: &str, query: &[(String, String)]) -> String {
    let mut target_url = format!("{}{}", base_url.trim_end_matches('/'), path);
    if !query.is_empty() {
        let mut parts = Vec::new();
        for (k, v) in query {
            if let Ok(encoded_k) = serde_urlencoded::to_string(k) {
                if let Ok(encoded_v) = serde_urlencoded::to_string(v) {
                    parts.push(format!("{encoded_k}={encoded_v}"));
                }
            }
        }
        if !parts.is_empty() {
            target_url.push('?');
            target_url.push_str(&parts.join("&"));
        }
    }
    target_url
}

/// Resolve timeout: explicit override, else `config.total_ms`, else 15s.
#[must_use]
pub fn resolve_timeout(override_secs: u64, config_total_ms: Option<u64>) -> Duration {
    if override_secs > 0 {
        Duration::from_secs(override_secs)
    } else if let Some(ms) = config_total_ms {
        Duration::from_millis(ms.max(1))
    } else {
        Duration::from_secs(15)
    }
}
