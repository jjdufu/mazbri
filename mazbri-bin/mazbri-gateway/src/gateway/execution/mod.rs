use super::ProxyState;
use super::metrics::{
    API_PROXY_REQUESTS_TOTAL, GATEWAY_UPSTREAM_ATTEMPTS_TOTAL,
    GATEWAY_UPSTREAM_CONTRACT_ERRORS_TOTAL, RetryReason, TimeoutPhase, classify_proxy_result,
    record_internal_retries, record_retry, record_timeout_if_any, record_upstream_timeout,
};
use super::response::ProxyError;
use super::serializer::proxy_body_to_json_value;
use super::validation::{GatewayRequest, GatewayRequestBody};
use crate::auth::claims::MazbriClaims;
use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitResult};
use crate::interaction_store::enqueue_interaction_event;
use crate::ratelimit::{ChannelRateLimitDecision, ChannelRateLimitRequest};
use crate::route_source::{GatewayCandidateChannel, PreparedChannel};
use crate::upstream_contract::BYPASS_CONTRACT_ID;
use mazbri_core::interaction::UpstreamAttempt;
use mazbri_core::{AttemptStatus, InteractionEvent, TraceContext};
use mazbri_routing::{RouteMatchContext, SwitchCondition};
use mazbri_rpc::{
    FallbackRequest, ObservationRequest, RuntimeDecision, RuntimeDecisionRequest, StoreRequest,
};
use mazbri_upstream_contracts::shared as upstream_proto;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use uuid::Uuid;

pub(super) const CBPRO_FALLBACK_TTL_SECS: u64 = 300;

mod cbpro;
mod channel;
mod http_upstream;
mod types_and_entry;

pub(crate) use types_and_entry::*;

use cbpro::{
    ObservationRequestParts, build_fallback_cache_key, build_observation_request,
    enqueue_cbpro_observation, try_cbpro_fallback,
};
use channel::{
    ChannelAttempt, execute_channel_attempt, should_switch, should_switch_status,
    spawn_store_fallback_snapshot,
};
use http_upstream::{AttemptCtx, execute_http_upstream_request, is_retry_safe_request};
