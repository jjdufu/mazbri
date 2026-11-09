//! Core invoke pipeline: validate spec → ssrf → auth profile → adapter.invoke.
use mazbri_core::num::SaturatingFrom;

use std::sync::Arc;
use std::time::{Duration, Instant};

use mazbri_core::AdapterKind;
use mazbri_crypto::AlgorithmRegistry;

use crate::adapters::{
    AdapterCtx, AdapterOutcome, effective_invalidation, invoke_adapter, merge_request_headers,
};
use crate::crypto_pipeline::pipeline_keys_from_credentials;
use crate::auth::{AppliedAuth, apply_auth, token_cache_key};
use crate::mask::{mask_headers, redact_body_preview};
use crate::observe::{
    NoopObserver, RetryKind, RetryLog, RetryOutcome, RuntimeEvent, RuntimeObserver,
    token_fingerprint, warn_budget_exhausted,
};
use crate::retry_budget::{RetryBudget, RetryBudgets};
use crate::ssrf::SsrfChecker;
use crate::token_cache::TokenCache;
use crate::types::{
    AuthRejection, ExecuteError, ExecuteMode, InvokeInput, InvokeResult, RetryReason, RetrySkip,
    StageResult, StageSummary, UpstreamMeta,
};

/// 运行时参数（bypass `[upstream]` 与 mgmt `[connector_probe.runtime]` 同一套语义）。
///
/// 默认值与 bypass 配置的默认值一致；服务启动时从各自配置构造后经
/// [`RuntimeEngine::with_options`] 注入。
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeOptions {
    pub ws_handshake_timeout: Duration,
    /// 全局 `ws_unary` 并发上限（失败快，不排队）。
    pub ws_max_concurrent: u32,
    /// 每池键（`base_url` + path + auth 指纹）并发上限。
    pub ws_max_concurrent_per_key: u32,
    /// 每键空闲连接上限。
    pub ws_pool_max_idle_per_key: u32,
    /// 空闲连接 TTL；借出时丢弃过期连接。
    pub ws_pool_idle_ttl: Duration,
    /// 上游换票响应既没有配置的过期字段、也没有 `expires_in` 时的缓存时长。
    pub token_default_ttl: Duration,
    /// 令牌提前视为过期的余量。
    pub token_refresh_skew: Duration,
    /// 认证被拒后作废令牌、换票并重试一次。
    pub auth_retry_enabled: bool,
    /// 复用的 WS 连接在回复前断开时，换新连接重试一次。
    pub ws_stale_retry_enabled: bool,
    /// 借出空闲 WS 连接前做非阻塞存活检查。
    pub ws_liveness_check_on_checkout: bool,
    /// 重试预算：窗口内重试数 / 首次请求数的上限。
    pub retry_budget_ratio: f64,
    /// 低流量时窗口内保底允许的重试次数。
    pub retry_budget_min_per_window: u32,
    /// 重试预算的统计窗口。
    pub retry_budget_window: Duration,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            ws_handshake_timeout: Duration::from_secs(10),
            ws_max_concurrent: 32,
            ws_max_concurrent_per_key: 8,
            ws_pool_max_idle_per_key: 4,
            ws_pool_idle_ttl: Duration::from_secs(60),
            token_default_ttl: Duration::from_secs(3600),
            token_refresh_skew: Duration::from_secs(30),
            auth_retry_enabled: true,
            ws_stale_retry_enabled: true,
            ws_liveness_check_on_checkout: true,
            retry_budget_ratio: 0.2,
            retry_budget_min_per_window: 3,
            retry_budget_window: Duration::from_secs(10),
        }
    }
}

impl RuntimeOptions {
    /// 校验参数取值范围（服务配置加载时调用）。
    ///
    /// # Errors
    ///
    /// WS 限额或时长为 0、`token_refresh_skew` 不小于 `token_default_ttl`、
    /// `retry_budget_ratio` 不在 (0, 1] 或预算窗口为 0 时返回错误说明。
    pub fn validate(&self) -> Result<(), String> {
        if self.ws_handshake_timeout.is_zero() {
            return Err("ws_handshake_timeout_secs must be greater than 0".into());
        }
        if self.ws_max_concurrent == 0
            || self.ws_max_concurrent_per_key == 0
            || self.ws_pool_max_idle_per_key == 0
        {
            return Err("ws_max_concurrent / ws_max_concurrent_per_key / ws_pool_max_idle_per_key must be greater than 0".into());
        }
        if self.ws_pool_idle_ttl.is_zero() {
            return Err("ws_pool_idle_ttl_secs must be greater than 0".into());
        }
        if self.token_default_ttl.is_zero() {
            return Err("token_default_ttl_secs must be greater than 0".into());
        }
        if self.token_refresh_skew >= self.token_default_ttl {
            return Err("token_refresh_skew_secs must be less than token_default_ttl_secs".into());
        }
        if !(self.retry_budget_ratio > 0.0 && self.retry_budget_ratio <= 1.0) {
            return Err(format!(
                "retry_budget_ratio must be in (0, 1], got {}",
                self.retry_budget_ratio
            ));
        }
        if self.retry_budget_window.is_zero() {
            return Err("retry_budget_window_secs must be greater than 0".into());
        }
        Ok(())
    }

    fn ws_pool_config(&self) -> crate::ws_pool::WsPoolConfig {
        crate::ws_pool::WsPoolConfig {
            handshake_timeout: self.ws_handshake_timeout,
            max_concurrent: self.ws_max_concurrent.max(1),
            max_concurrent_per_key: self.ws_max_concurrent_per_key.max(1),
            max_idle_per_key: self.ws_pool_max_idle_per_key.max(1),
            idle_ttl: self.ws_pool_idle_ttl,
            liveness_check_on_checkout: self.ws_liveness_check_on_checkout,
        }
    }

    fn retry_budgets(&self) -> RetryBudgets {
        RetryBudgets::new(
            self.retry_budget_ratio,
            self.retry_budget_min_per_window,
            self.retry_budget_window,
        )
    }
}

#[derive(Clone)]
pub struct RuntimeEngine {
    client: Arc<reqwest::Client>,
    ssrf: SsrfChecker,
    token_cache: Arc<TokenCache>,
    options: RuntimeOptions,
    ws_pool: Arc<crate::ws_pool::WsPool>,
    /// 按供应商分桶的重试预算（认证重试与 WS 陈旧连接重试共用同一上游的桶）。
    retry_budgets: Arc<RetryBudgets>,
    observer: Arc<dyn RuntimeObserver>,
}

impl RuntimeEngine {
    /// 以默认 [`RuntimeOptions`] 构造；服务启动时再经 [`Self::with_options`] 注入配置。
    #[must_use]
    pub fn new(client: Arc<reqwest::Client>, ssrf: SsrfChecker) -> Self {
        let options = RuntimeOptions::default();
        let ws_pool = Arc::new(crate::ws_pool::WsPool::new(options.ws_pool_config()));
        Self {
            client,
            ssrf,
            token_cache: Arc::new(TokenCache::new(options.token_refresh_skew)),
            retry_budgets: Arc::new(options.retry_budgets()),
            options,
            ws_pool,
            observer: Arc::new(NoopObserver),
        }
    }

    /// 注入内部事件观测者（服务据此记录指标）。
    #[must_use]
    pub fn with_observer(mut self, observer: Arc<dyn RuntimeObserver>) -> Self {
        self.observer = observer;
        self
    }

    /// 注入运行时参数：按新参数重建 WS 池、令牌缓存与重试预算（仅在启动时调用）。
    #[must_use]
    pub fn with_options(mut self, options: RuntimeOptions) -> Self {
        self.ws_pool = Arc::new(crate::ws_pool::WsPool::new(options.ws_pool_config()));
        self.token_cache = Arc::new(TokenCache::new(options.token_refresh_skew));
        self.retry_budgets = Arc::new(options.retry_budgets());
        self.options = options;
        self
    }

    #[must_use]
    pub fn options(&self) -> &RuntimeOptions {
        &self.options
    }

    #[must_use]
    pub fn ws_pool(&self) -> Arc<crate::ws_pool::WsPool> {
        self.ws_pool.clone()
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
        let retry_log = RetryLog::default();
        let mut result = self.invoke_inner(&input, &retry_log).await;
        result.retry_reasons = retry_log.take();
        result.internal_retries = u32::try_from(result.retry_reasons.len()).unwrap_or(u32::MAX);
        result
    }

    async fn invoke_inner(&self, input: &InvokeInput, retry_log: &RetryLog) -> InvokeResult {
        let mut trace = Trace::new(input);
        let base_url = match self.preflight(input, &mut trace).await {
            Ok(url) => url,
            Err(fail) => return trace.fail(fail),
        };
        if matches!(input.mode, ExecuteMode::TransformPreview) {
            return transform_preview(input, base_url, trace);
        }
        let cache_key = input
            .cache_key
            .clone()
            .unwrap_or_else(|| token_cache_key(&input.supplier_id, &input.channel_id, &input.spec));
        let budget = self.retry_budgets.for_upstream(&input.supplier_id);
        budget.record_request();
        let first = self
            .attempt(input, &base_url, &cache_key, None, &mut trace, retry_log)
            .await;
        let (rejection, mut first_result) = match first {
            AttemptOutcome::AuthRejected(rejection, result) => (rejection, result),
            AttemptOutcome::Done(result) => return *result,
            AttemptOutcome::Failed(fail) => return trace.fail(fail),
        };
        let kind = input.spec.adapter_kind().ok();
        self.note_rejection(input, kind, &rejection);
        if let Some(skip) = self.skip_auth_retry(input, kind, &budget) {
            // 不重试时与「重试后仍被拒」同一口径：UPSTREAM_AUTH_REJECTED（平台侧上游故障，不计费）。
            let message = format!(
                "upstream rejected credentials, retry skipped (trigger={}, reason={})",
                rejection.trigger,
                skip.label()
            );
            let mut result = trace.auth_rejected(message, *first_result);
            result.retry_skipped = Some(skip);
            return result;
        }
        // 认证被拒说明上游未执行业务，换票后重放是安全的；只重试一次，不递归。
        retry_log.push(RetryReason::AuthRejected(rejection.trigger));
        trace.stages = std::mem::take(&mut first_result.stages);
        trace.retry_round = true;
        let second = self
            .attempt(
                input,
                &base_url,
                &cache_key,
                Some(&rejection.token),
                &mut trace,
                retry_log,
            )
            .await;
        let (outcome, result) = match second {
            AttemptOutcome::AuthRejected(again, result) => {
                self.note_rejection(input, kind, &again);
                let message = format!(
                    "upstream rejected credentials after token refresh (trigger={})",
                    again.trigger
                );
                let result = trace.auth_rejected(message, *result);
                (RetryOutcome::RejectedAgain, result)
            }
            AttemptOutcome::Done(result) => (RetryOutcome::Ok, *result),
            AttemptOutcome::Failed(fail) => (RetryOutcome::Failed, trace.fail(fail)),
        };
        self.note_auth_retry(input, kind, outcome, None);
        result
    }

    /// 认证被拒：记事件，并打一条只含令牌指纹的日志。
    fn note_rejection(
        &self,
        input: &InvokeInput,
        kind: Option<AdapterKind>,
        rejection: &AuthRejection,
    ) {
        if let Some(adapter) = kind {
            self.observer.on_event(RuntimeEvent::AuthRejected {
                adapter,
                trigger: rejection.trigger,
            });
        }
        tracing::info!(
            target: mazbri_core::trace_spans::SPAN_EVENT_TARGET,
            event = "upstream.auth_rejected",
            channel_id = %input.channel_id,
            adapter = %input.spec.adapter,
            trigger = rejection.trigger.label(),
            token_fp = %token_fingerprint(&rejection.token),
            "upstream rejected credentials"
        );
    }

    /// 判定是否跳过认证重试（开关关闭或预算耗尽），跳过时记事件。
    fn skip_auth_retry(
        &self,
        input: &InvokeInput,
        kind: Option<AdapterKind>,
        budget: &RetryBudget,
    ) -> Option<RetrySkip> {
        let skip = if !self.options.auth_retry_enabled {
            RetrySkip::Disabled
        } else if budget.try_acquire() {
            return None;
        } else {
            self.observer.on_event(RuntimeEvent::BudgetExhausted {
                kind: RetryKind::Auth,
            });
            warn_budget_exhausted(&input.channel_id, RetryKind::Auth);
            RetrySkip::Budget
        };
        let outcome = match skip {
            RetrySkip::Disabled => RetryOutcome::SkippedDisabled,
            RetrySkip::Budget => RetryOutcome::SkippedBudget,
        };
        self.note_auth_retry(input, kind, outcome, Some(skip));
        Some(skip)
    }

    fn note_auth_retry(
        &self,
        input: &InvokeInput,
        kind: Option<AdapterKind>,
        result: RetryOutcome,
        skipped: Option<RetrySkip>,
    ) {
        if let Some(adapter) = kind {
            self.observer
                .on_event(RuntimeEvent::AuthRetry { adapter, result });
        }
        if skipped.is_none() {
            tracing::info!(
                target: mazbri_core::trace_spans::SPAN_EVENT_TARGET,
                event = "upstream.auth_retry",
                channel_id = %input.channel_id,
                adapter = %input.spec.adapter,
                result = result.label(),
                "upstream auth retry finished"
            );
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

    /// One auth + adapter round. `stale_token` 为上一轮被拒的令牌（重试轮）。
    async fn attempt(
        &self,
        input: &InvokeInput,
        base_url: &str,
        cache_key: &str,
        stale_token: Option<&str>,
        trace: &mut Trace,
        retry_log: &RetryLog,
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
            stale_token,
            token_default_ttl: self.options.token_default_ttl,
            request_id: &input.request_id,
        })
        .await
        {
            Ok(a) => a,
            Err(e) => return AttemptOutcome::Failed(StageFail::new("auth", a_start, e)),
        };
        if let Some(evicted) = applied.stale_evicted {
            self.observer.on_event(RuntimeEvent::TokenEvict { evicted });
        }
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
        let pipeline_keys = input.pipeline_keys.clone().unwrap_or_else(|| {
            pipeline_keys_from_credentials(&input.credentials)
        });
        let registry = AlgorithmRegistry::default_registry();
        let invalidation = effective_invalidation(input);
        let stale_budget = self.retry_budgets.for_upstream(&input.supplier_id);
        let ctx = AdapterCtx {
            client: &self.client,
            ssrf: &self.ssrf,
            input,
            base_url,
            auth: &applied,
            timeout: input.timeout,
            ws_pool: &self.ws_pool,
            pipeline_keys: &pipeline_keys,
            registry: &registry,
            invalidation: &invalidation,
            ws_stale_retry: self
                .options
                .ws_stale_retry_enabled
                .then_some(&*stale_budget),
            observer: &*self.observer,
            retry_log,
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
        trace.record(|| upstream_stage(input, &applied, &outcome, u_start));
        let rejection = outcome.auth_rejection;
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
        match rejection {
            Some(rejection) => AttemptOutcome::AuthRejected(rejection, Box::new(result)),
            None => AttemptOutcome::done(result),
        }
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
    /// 上游按认证失效规则拒绝了本轮令牌；附带本轮的真实结果，供不重试时原样返回。
    AuthRejected(AuthRejection, Box<InvokeResult>),
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
    /// 认证重试轮：阶段名加 `#retry` 后缀。
    retry_round: bool,
}

impl Trace {
    fn new(input: &InvokeInput) -> Self {
        Self {
            start: Instant::now(),
            stages: Vec::new(),
            enabled: input.options.collect_trace || !matches!(input.mode, ExecuteMode::Production),
            adapter: input.spec.adapter.clone(),
            retry_round: false,
        }
    }

    fn record(&mut self, stage: impl FnOnce() -> StageResult) {
        if self.enabled {
            let mut stage = stage();
            if self.retry_round {
                stage.name.push_str("#retry");
            }
            self.stages.push(stage);
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
            internal_retries: 0,
            retry_reasons: Vec::new(),
            retry_skipped: None,
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

    /// 认证被拒的最终结果（重试后仍被拒，或跳过重试）：保留最后一轮的上游元信息与阶段，
    /// 结果标记为 `UPSTREAM_AUTH_REJECTED`。
    fn auth_rejected(&self, message: String, last: InvokeResult) -> InvokeResult {
        let err = ExecuteError::UpstreamAuthRejected(message);
        InvokeResult {
            ok: false,
            latency_ms: elapsed_ms(self.start),
            payload: None,
            biz_error: None,
            error_code: Some(err.code().to_string()),
            error_message: Some(err.to_string()),
            ..last
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

#[cfg(test)]
mod options_tests {
    use super::*;

    #[test]
    fn default_options_are_valid() {
        assert!(RuntimeOptions::default().validate().is_ok());
    }

    #[test]
    fn invalid_options_rejected() {
        let cases: [fn(&mut RuntimeOptions); 8] = [
            |o| o.retry_budget_ratio = 0.0,
            |o| o.retry_budget_ratio = 1.5,
            |o| o.retry_budget_ratio = f64::NAN,
            |o| o.token_refresh_skew = o.token_default_ttl,
            |o| o.token_default_ttl = Duration::ZERO,
            |o| o.retry_budget_window = Duration::ZERO,
            |o| o.ws_max_concurrent = 0,
            |o| o.ws_pool_idle_ttl = Duration::ZERO,
        ];
        for (i, mutate) in cases.iter().enumerate() {
            let mut o = RuntimeOptions::default();
            mutate(&mut o);
            assert!(o.validate().is_err(), "case {i}");
        }
    }

    #[test]
    fn with_options_rebuilds_ws_pool() {
        let client = Arc::new(reqwest::Client::new());
        let opts = RuntimeOptions {
            ws_max_concurrent: 3,
            ws_pool_idle_ttl: Duration::from_secs(7),
            ..RuntimeOptions::default()
        };
        let engine = RuntimeEngine::new(client, SsrfChecker::with_allow_private(true))
            .with_options(opts.clone());
        assert_eq!(engine.options(), &opts);
        assert_eq!(engine.ws_pool().config().max_concurrent, 3);
        assert_eq!(engine.ws_pool().config().idle_ttl, Duration::from_secs(7));
    }
}
