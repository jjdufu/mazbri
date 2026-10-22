//! Core invoke pipeline: validate spec → ssrf → auth profile → adapter.invoke.
use mazbri_core::num::SaturatingFrom;

use std::sync::Arc;
use std::time::{Duration, Instant};

use mazbri_core::AdapterKind;

use crate::adapters::{AdapterCtx, invoke_adapter, merge_request_headers};
use crate::auth::{apply_auth, token_cache_key};
use crate::mask::{mask_headers, redact_body_preview};
use crate::ssrf::SsrfChecker;
use crate::token_cache::TokenCache;
use crate::types::{
    ExecuteError, ExecuteMode, InvokeInput, InvokeResult, StageResult, StageSummary, UpstreamMeta,
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
            return Self::invoke_fail(InvokeFail {
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
            return Self::invoke_fail(InvokeFail {
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
            return Self::invoke_fail(InvokeFail {
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
            return Self::invoke_fail(InvokeFail {
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
                            return Self::invoke_fail(InvokeFail {
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
                    return Self::invoke_fail(InvokeFail {
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
                    return Self::invoke_fail(InvokeFail {
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
                    return Self::invoke_fail(InvokeFail {
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

    fn invoke_fail(fail: InvokeFail<'_>) -> InvokeResult {
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
