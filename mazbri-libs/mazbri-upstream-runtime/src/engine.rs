//! Core invoke pipeline: validate spec → ssrf → auth profile → adapter.invoke.
use mazbri_core::num::SaturatingFrom;

use std::sync::Arc;
use std::time::{Duration, Instant};

use mazbri_core::AdapterKind;

use crate::adapters::{AdapterCtx, AdapterOutcome, invoke_adapter, merge_request_headers};
use crate::auth::{AppliedAuth, apply_auth, token_cache_key};
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
        let mut trace = Trace::new(&input);
        let base_url = match self.preflight(&input, &mut trace).await {
            Ok(url) => url,
            Err(fail) => return trace.fail(fail),
        };
        if matches!(input.mode, ExecuteMode::TransformPreview) {
            return transform_preview(&input, base_url, trace);
        }
        let cache_key = input
            .cache_key
            .clone()
            .unwrap_or_else(|| token_cache_key(&input.supplier_id, &input.channel_id, &input.spec));
        let first = self
            .attempt(&input, &base_url, &cache_key, false, &mut trace)
            .await;
        let outcome = match first {
            AttemptOutcome::RetryUnauthorized => {
                self.attempt(&input, &base_url, &cache_key, true, &mut trace)
                    .await
            }
            other => other,
        };
        match outcome {
            AttemptOutcome::Done(result) => *result,
            AttemptOutcome::Failed(fail) => trace.fail(fail),
            AttemptOutcome::RetryUnauthorized => trace.retries_exhausted(),
        }
    }

    /// validate + ssrf stages; returns the resolved base URL.
    async fn preflight(&self, input: &InvokeInput, trace: &mut Trace) -> Result<String, StageFail> {
        let v_start = Instant::now();
        let validate_fail = |err| StageFail::new("validate", v_start, err);
        mazbri_core::validate_egress_spec(&input.spec)
            .map_err(|e| validate_fail(ExecuteError::SpecInvalid(e.to_string())))?;
        if !input.payload.is_object() {
            return Err(validate_fail(ExecuteError::SpecInvalid(
                "payload_json 必须是 JSON 对象".into(),
            )));
        }
        let base_url = input.spec.base_url.trim().to_string();
        let max_cap = input.options.max_body_capture_bytes;
        let mask = input.options.mask_secrets;
        trace.record(|| StageResult {
            name: "validate".into(),
            ok: true,
            latency_ms: elapsed_ms(v_start),
            request_summary: Some(StageSummary {
                method: Some(input.spec.request.method.clone()),
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

        let s_start = Instant::now();
        self.ssrf
            .validate_url(&base_url)
            .await
            .map_err(|e| StageFail::new("ssrf", s_start, ExecuteError::SsrfBlocked(e)))?;
        if let Some(url) = ticket_token_url(input) {
            self.ssrf.validate_url(url).await.map_err(|e| {
                StageFail::new(
                    "ssrf",
                    s_start,
                    ExecuteError::SsrfBlocked(format!("token_url: {e}")),
                )
            })?;
        }
        trace.record(|| StageResult {
            name: "ssrf".into(),
            ok: true,
            latency_ms: elapsed_ms(s_start),
            request_summary: None,
            response_summary: None,
            error: None,
        });
        Ok(base_url)
    }

    /// One auth + adapter round. `evict_first` marks the retry after a 401.
    async fn attempt(
        &self,
        input: &InvokeInput,
        base_url: &str,
        cache_key: &str,
        evict_first: bool,
        trace: &mut Trace,
    ) -> AttemptOutcome {
        let a_start = Instant::now();
        let applied = match apply_auth(crate::auth::AuthApply {
            client: &self.client,
            ssrf: &self.ssrf,
            token_cache: &self.token_cache,
            spec: &input.spec,
            base_url,
            credentials: &input.credentials,
            cache_key,
            evict_first,
            request_id: &input.request_id,
        })
        .await
        {
            Ok(a) => a,
            Err(e) => return AttemptOutcome::Failed(StageFail::new("auth", a_start, e)),
        };
        trace.record(|| auth_stage(input, &applied, a_start));

        let kind = match input.spec.adapter_kind() {
            Ok(k) => k,
            Err(e) => {
                return AttemptOutcome::Failed(StageFail::new(
                    "adapter",
                    Instant::now(),
                    ExecuteError::SpecInvalid(e.to_string()),
                ));
            }
        };
        let auth_only = matches!(input.mode, ExecuteMode::AuthOnly);
        if auth_only && matches!(kind, AdapterKind::Grpc | AdapterKind::WsUnary) {
            return AttemptOutcome::done(trace.result(ResultParts {
                token: applied.token,
                ..ResultParts::ok()
            }));
        }
        let u_start = Instant::now();
        let ctx = AdapterCtx {
            client: &self.client,
            ssrf: &self.ssrf,
            input,
            base_url,
            auth: &applied,
            timeout: input.timeout,
            ws_handshake_timeout: self.limits.ws_handshake_timeout,
            job_poll_min_interval: self.limits.job_poll_min_interval,
        };
        let outcome = match invoke_adapter(&ctx, kind).await {
            Ok(o) => o,
            Err(e) => return AttemptOutcome::Failed(StageFail::new("upstream", u_start, e)),
        };
        if auth_only {
            let meta = trace.upstream_meta(&outcome.response);
            return AttemptOutcome::done(trace.result(ResultParts {
                upstream_meta: Some(meta),
                token: applied.token,
                response: Some(outcome.response),
                ..ResultParts::ok()
            }));
        }
        if outcome.response.status == 401 && applied.token.is_some() && !evict_first {
            return AttemptOutcome::RetryUnauthorized;
        }
        trace.record(|| upstream_stage(input, &applied, &outcome, u_start));
        let meta = trace.upstream_meta(&outcome.response);
        let latency_ms = meta.latency_ms;
        let mut result = trace.result(ResultParts {
            ok: outcome.biz_error.is_none(),
            payload: Some(outcome.payload),
            biz_error: outcome.biz_error,
            upstream_meta: Some(meta),
            token: applied.token,
            response: Some(outcome.response),
            ..ResultParts::ok()
        });
        result.latency_ms = latency_ms;
        AttemptOutcome::done(result)
    }
}

/// `token_url` of a ticket-style auth profile (must pass SSRF too).
fn ticket_token_url(input: &InvokeInput) -> Option<&str> {
    let auth = input.spec.auth.as_ref()?;
    if !auth
        .profile_kind()
        .is_ok_and(mazbri_core::AuthProfile::uses_ticket)
    {
        return None;
    }
    auth.token_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// `TransformPreview`: render the adapter wrap, no network.
fn transform_preview(input: &InvokeInput, base_url: String, mut trace: Trace) -> InvokeResult {
    let (preview, wrap_err) = match crate::adapters::wrap_by_name(&input.spec, &input.payload) {
        Ok(v) => (v, None),
        Err(e) => (input.payload.clone(), Some(e)),
    };
    let max_cap = input.options.max_body_capture_bytes;
    let mask = input.options.mask_secrets;
    trace.record(|| StageResult {
        name: "request_transform".into(),
        ok: wrap_err.is_none(),
        latency_ms: 0,
        request_summary: Some(StageSummary {
            method: Some(input.spec.request.method.clone()),
            url: Some(base_url),
            headers_masked: None,
            body_preview: Some(redact_body_preview(&preview.to_string(), max_cap, mask)),
            status: None,
            extracted: None,
            detail: Some("transform_preview (no network)".into()),
        }),
        response_summary: None,
        error: wrap_err.as_ref().map(ToString::to_string),
    });
    trace.result(ResultParts {
        ok: wrap_err.is_none(),
        payload: Some(preview),
        error_code: wrap_err.as_ref().map(|e| e.code().to_string()),
        error_message: wrap_err.map(|e| e.to_string()),
        ..ResultParts::ok()
    })
}

fn auth_stage(input: &InvokeInput, applied: &AppliedAuth, started: Instant) -> StageResult {
    StageResult {
        name: "auth".into(),
        ok: true,
        latency_ms: elapsed_ms(started),
        request_summary: Some(StageSummary {
            method: None,
            url: None,
            headers_masked: Some(mask_headers(&applied.headers, input.options.mask_secrets)),
            body_preview: None,
            status: None,
            extracted: Some(serde_json::json!({
                "token_present": applied.token.is_some()
            })),
            detail: input.spec.auth.as_ref().map(|a| a.profile.clone()),
        }),
        response_summary: None,
        error: None,
    }
}

fn upstream_stage(
    input: &InvokeInput,
    applied: &AppliedAuth,
    outcome: &AdapterOutcome,
    started: Instant,
) -> StageResult {
    let mask = input.options.mask_secrets;
    let response = &outcome.response;
    StageResult {
        name: "upstream".into(),
        ok: outcome.biz_error.is_none() && (200..300).contains(&response.status),
        latency_ms: elapsed_ms(started),
        request_summary: Some(StageSummary {
            method: Some(response.method.clone()),
            url: Some(response.final_url.clone()),
            headers_masked: Some(mask_headers(
                &merge_request_headers(applied, &input.spec),
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
            headers_masked: Some(mask_headers(&response.headers, mask)),
            body_preview: Some(redact_body_preview(
                &String::from_utf8_lossy(&response.body),
                input.options.max_body_capture_bytes,
                mask,
            )),
            status: Some(response.status),
            extracted: Some(outcome.payload.clone()),
            detail: None,
        }),
        error: outcome.biz_error.as_ref().map(|b| b.message.clone()),
    }
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::saturating_from(since.elapsed().as_millis())
}

/// A failed stage: name, when it started, and the error.
struct StageFail {
    stage: &'static str,
    started: Instant,
    err: ExecuteError,
}

impl StageFail {
    fn new(stage: &'static str, started: Instant, err: ExecuteError) -> Self {
        Self {
            stage,
            started,
            err,
        }
    }
}

/// Result of one auth + adapter round.
enum AttemptOutcome {
    Done(Box<InvokeResult>),
    Failed(StageFail),
    /// Upstream answered 401 with a cached token: evict and retry once.
    RetryUnauthorized,
}

impl AttemptOutcome {
    fn done(result: InvokeResult) -> Self {
        Self::Done(Box::new(result))
    }
}

/// Non-stage fields of an [`InvokeResult`].
struct ResultParts {
    ok: bool,
    payload: Option<serde_json::Value>,
    biz_error: Option<crate::types::BizError>,
    error_code: Option<String>,
    error_message: Option<String>,
    upstream_meta: Option<UpstreamMeta>,
    token: Option<String>,
    response: Option<crate::types::OutboundResponse>,
}

impl ResultParts {
    fn ok() -> Self {
        Self {
            ok: true,
            payload: None,
            biz_error: None,
            error_code: None,
            error_message: None,
            upstream_meta: None,
            token: None,
            response: None,
        }
    }
}

/// Stage collector for one invoke: records only when tracing is on.
struct Trace {
    start: Instant,
    stages: Vec<StageResult>,
    enabled: bool,
    adapter: String,
}

impl Trace {
    fn new(input: &InvokeInput) -> Self {
        Self {
            start: Instant::now(),
            stages: Vec::new(),
            enabled: input.options.collect_trace || !matches!(input.mode, ExecuteMode::Production),
            adapter: input.spec.adapter.clone(),
        }
    }

    fn record(&mut self, stage: impl FnOnce() -> StageResult) {
        if self.enabled {
            self.stages.push(stage());
        }
    }

    fn upstream_meta(&self, response: &crate::types::OutboundResponse) -> UpstreamMeta {
        UpstreamMeta {
            status: response.status,
            url: response.final_url.clone(),
            adapter: self.adapter.clone(),
            latency_ms: elapsed_ms(self.start),
        }
    }

    fn result(&mut self, parts: ResultParts) -> InvokeResult {
        InvokeResult {
            ok: parts.ok,
            latency_ms: elapsed_ms(self.start),
            stages: std::mem::take(&mut self.stages),
            payload: parts.payload,
            biz_error: parts.biz_error,
            error_code: parts.error_code,
            error_message: parts.error_message,
            upstream_meta: parts.upstream_meta,
            token: parts.token,
            response: parts.response,
        }
    }

    fn fail(mut self, fail: StageFail) -> InvokeResult {
        let StageFail {
            stage,
            started,
            err,
        } = fail;
        self.record(|| StageResult {
            name: stage.into(),
            ok: false,
            latency_ms: elapsed_ms(started),
            request_summary: None,
            response_summary: None,
            error: Some(err.to_string()),
        });
        let upstream_meta = Some(UpstreamMeta {
            adapter: self.adapter.clone(),
            ..Default::default()
        });
        self.result(ResultParts {
            ok: false,
            error_code: Some(err.code().to_string()),
            error_message: Some(err.to_string()),
            upstream_meta,
            ..ResultParts::ok()
        })
    }

    fn retries_exhausted(mut self) -> InvokeResult {
        let upstream_meta = Some(UpstreamMeta {
            adapter: self.adapter.clone(),
            ..Default::default()
        });
        self.result(ResultParts {
            ok: false,
            error_code: Some("AUTH_TOKEN_FAILED".into()),
            error_message: Some("Outbound request failed after 401 retries".into()),
            upstream_meta,
            ..ResultParts::ok()
        })
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
