use super::{debug, info, ProxyState, GatewayRequest, MazbriClaims, UpstreamResponse, Instant, FallbackRequest, Digest, FallbackSnapshot, Duration, ProxyBody, RuntimeDecision, RuntimeDecisionRequest, ObservationRequest, RngExt, BTreeMap, GatewayRequestBody};

pub(super) async fn try_cbpro_fallback(
    state: &ProxyState,
    req: &GatewayRequest,
    claims: &MazbriClaims,
    request_id: &str,
    failure_kind: &str,
) -> Option<UpstreamResponse> {
    let cbpro_client = state.cbpro_client.as_ref()?;
    let cache_key = build_fallback_cache_key(req);
    let lookup_started_at = Instant::now();
    let fallback = cbpro_client
        .get_fallback(FallbackRequest {
            product_id: req.product_id.clone(),
            tenant_id: claims.tenant_id.clone(),
            cache_key: cache_key.clone(),
            path: req.path.clone(),
            body: request_body_as_json(req.body.as_ref()),
        })
        .await;
    if !fallback.available {
        enqueue_cbpro_observation(
            state,
            build_observation_request(ObservationRequestParts {
                event_type: "cbpro_cache_miss",
                request_id,
                tenant_id: &claims.tenant_id,
                product_id: &req.product_id,
                method: &req.method,
                path: &req.path,
                cache_key: Some(cache_key.clone()),
                status: None,
                service_total_ms: None,
                response: None,
            }),
        );
        debug!(
            request_id = % request_id, product_id = % req.product_id, tenant_id = %
            claims.tenant_id, cache_key = % cache_key, failure_kind = failure_kind,
            "CBPro fallback not available"
        );
        return None;
    }
    let snapshot = fallback
        .data
        .and_then(|data| serde_json::from_value::<FallbackSnapshot>(data).ok())?;
    let runtime_decision = resolve_runtime_decision(state, req, claims, &cache_key).await;
    let simulated_sleep_ms =
        compute_simulated_sleep_ms(&runtime_decision, lookup_started_at.elapsed());
    if simulated_sleep_ms > 0 {
        tokio::time::sleep(Duration::from_millis(simulated_sleep_ms)).await;
    }
    enqueue_cbpro_observation(
        state,
        build_observation_request(ObservationRequestParts {
            event_type: "cbpro_cache_hit",
            request_id,
            tenant_id: &claims.tenant_id,
            product_id: &req.product_id,
            method: &req.method,
            path: &req.path,
            cache_key: Some(cache_key.clone()),
            status: Some(snapshot.status),
            service_total_ms: Some(mazbri_core::num::millis_u64(lookup_started_at.elapsed())),
            response: Some(&snapshot.data),
        }),
    );
    info!(
        request_id = % request_id, product_id = % req.product_id, tenant_id = % claims
        .tenant_id, source = % fallback.source, failure_kind = failure_kind,
        "Served response from CBPro fallback snapshot"
    );
    let body_bytes_len = serde_json::to_vec(&snapshot.data)
        .map_or(0, |bytes| bytes.len() as u64);
    Some(UpstreamResponse {
        status: snapshot.status,
        body: ProxyBody::JsonValue(snapshot.data),
        error: String::new(),
        content_type: Some("application/json; charset=utf-8".to_string()),
        body_bytes_len,
    })
}
pub(super) async fn resolve_runtime_decision(
    state: &ProxyState,
    req: &GatewayRequest,
    claims: &MazbriClaims,
    cache_key: &str,
) -> RuntimeDecision {
    let runtime_key = build_runtime_decision_cache_key(req, &claims.tenant_id, cache_key);
    if let Some(decision) = state.cbpro_runtime_decision_cache.get(&runtime_key) {
        return decision.clone();
    }
    if let Some(client) = state.cbpro_ext_client.as_ref() {
        let response = client
            .get_runtime_decision(RuntimeDecisionRequest {
                tenant_id: claims.tenant_id.clone(),
                product_id: req.product_id.clone(),
                method: req.method.clone(),
                path: req.path.clone(),
                cache_key: cache_key.to_string(),
                policy_version: None,
            })
            .await;
        if response.available
            && let Some(decision) = response.decision
        {
            state
                .cbpro_runtime_decision_cache
                .insert(runtime_key, decision.clone());
            return decision;
        }
    }
    let decision = default_runtime_decision(req, claims, cache_key);
    state
        .cbpro_runtime_decision_cache
        .insert(runtime_key, decision.clone());
    decision
}
pub(super) fn build_runtime_decision_cache_key(
    req: &GatewayRequest,
    tenant_id: &str,
    cache_key: &str,
) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(tenant_id.as_bytes());
    hasher.update(b"|");
    hasher.update(req.product_id.as_bytes());
    hasher.update(b"|");
    hasher.update(req.method.as_bytes());
    hasher.update(b"|");
    hasher.update(req.path.as_bytes());
    hasher.update(b"|");
    hasher.update(cache_key.as_bytes());
    hex::encode(hasher.finalize())
}
pub(super) fn default_runtime_decision(
    req: &GatewayRequest,
    claims: &MazbriClaims,
    cache_key: &str,
) -> RuntimeDecision {
    let now = chrono::Utc::now().timestamp_millis();
    RuntimeDecision {
        cache_key: cache_key.to_string(),
        tenant_id: claims.tenant_id.clone(),
        product_id: req.product_id.clone(),
        method: req.method.clone(),
        path: req.path.clone(),
        mode: "fallback_only".to_string(),
        policy_version: 0,
        latency_baseline_ms: 120,
        latency_p50_ms: 120,
        latency_p75_ms: 132,
        latency_p95_ms: 150,
        jitter_min_ms: 0,
        jitter_max_ms: 20,
        max_simulated_delay_ms: 1_000,
        current_ttl_secs: 300,
        validate_due: false,
        hot_level: 0,
        last_validation_status: None,
        valid_until_unix_ms: now + 3_000,
        generated_at_unix_ms: now,
    }
}
pub(super) struct ObservationRequestParts<'a> {
    pub(crate) event_type: &'a str,
    pub(crate) request_id: &'a str,
    pub(crate) tenant_id: &'a str,
    pub(crate) product_id: &'a str,
    pub(crate) method: &'a str,
    pub(crate) path: &'a str,
    pub(crate) cache_key: Option<String>,
    pub(crate) status: Option<u16>,
    pub(crate) service_total_ms: Option<u64>,
    pub(crate) response: Option<&'a serde_json::Value>,
}
pub(super) fn build_observation_request(parts: ObservationRequestParts<'_>) -> ObservationRequest {
    let (response_hash, response_size_bytes) = parts.response.map_or((None, None), |value| {
        serde_json::to_vec(value)
            .ok()
            .map_or((None, None), |bytes| {
                (
                    Some(hex::encode(sha2::Sha256::digest(&bytes))),
                    Some(bytes.len() as u64),
                )
            })
    });
    ObservationRequest {
        event_type: parts.event_type.to_string(),
        request_id: parts.request_id.to_string(),
        tenant_id: parts.tenant_id.to_string(),
        product_id: parts.product_id.to_string(),
        method: parts.method.to_string(),
        path: parts.path.to_string(),
        cache_key: parts.cache_key,
        status: parts.status,
        service_total_ms: parts.service_total_ms,
        response_hash,
        response_size_bytes,
        occurred_at_unix_ms: chrono::Utc::now().timestamp_millis(),
    }
}
pub(super) fn compute_simulated_sleep_ms(
    decision: &RuntimeDecision,
    actual_elapsed: Duration,
) -> u64 {
    let jitter = if decision.jitter_max_ms > decision.jitter_min_ms {
        rand::rng().random_range(decision.jitter_min_ms..=decision.jitter_max_ms)
    } else {
        decision.jitter_min_ms
    };
    let target_delay_ms = decision.latency_baseline_ms.saturating_add(jitter);
    target_delay_ms
        .saturating_sub(mazbri_core::num::millis_u64(actual_elapsed))
        .min(decision.max_simulated_delay_ms)
}
pub(super) fn enqueue_cbpro_observation(state: &ProxyState, observation: ObservationRequest) {
    if let Some(tx) = state.cbpro_observation_tx.as_ref() {
        if let Err(error) = tx.try_send(observation) {
            debug!(error = % error, "CBPro observation queue full, dropping event");
        }
    }
}
pub(super) fn build_fallback_cache_key(req: &GatewayRequest) -> String {
    let query = req
        .query
        .as_ref()
        .map(|query| {
            query
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let body = request_body_as_json(req.body.as_ref()).unwrap_or(serde_json::Value::Null);
    serde_json::json!(
        { "method" : req.method, "path" : req.path, "query" : query, "body" : body, }
    )
    .to_string()
}
pub(super) fn request_body_as_json(body: Option<&GatewayRequestBody>) -> Option<serde_json::Value> {
    match body {
        Some(GatewayRequestBody::Json(value)) => Some(value.clone()),
        Some(GatewayRequestBody::Raw(value)) => Some(serde_json::Value::String(value.clone())),
        Some(GatewayRequestBody::Empty) | None => None,
    }
}
