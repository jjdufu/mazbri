use super::ProxyState;
use super::metrics::{
    API_PROXY_REQUESTS_TOTAL, GATEWAY_UPSTREAM_ATTEMPTS_TOTAL,
    GATEWAY_UPSTREAM_CONTRACT_ERRORS_TOTAL, classify_proxy_result,
};
use super::response::ProxyError;
use super::validation::{GatewayRequest, GatewayRequestBody};
use crate::auth::claims::MazbriClaims;
use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitResult};
use crate::interaction_store::persist_interaction_event;
use crate::ratelimit::{ChannelRateLimitDecision, ChannelRateLimitRequest};
use crate::upstream_contract::{BYPASS_CONTRACT_ID, grpc_path_for_contract};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use mazbri_core::interaction::UpstreamAttempt;
use mazbri_core::{AttemptStatus, InteractionEvent, TraceContext};
use mazbri_routing::{CandidateChannel, RouteMatchContext, SwitchCondition};
use mazbri_rpc::{
    FallbackRequest, ObservationRequest, RuntimeDecision, RuntimeDecisionRequest, StoreRequest,
};
use mazbri_upstream_contracts::shared as upstream_proto;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use uuid::Uuid;
pub(super) const CBPRO_FALLBACK_TTL_SECS: u64 = 300;

mod cbpro;
mod channel;
mod http_upstream;
mod types_and_entry;

// Re-export public(crate) API used by proxy handler / other gateway modules.
pub(crate) use types_and_entry::*;
// Keep sibling helpers in this module's namespace for `use super::*` in children.
use cbpro::{build_fallback_cache_key, enqueue_cbpro_observation, build_observation_request, ObservationRequestParts, try_cbpro_fallback};
use channel::{resolve_mode_and_contract_hint, spawn_persist_event_best_effort, execute_channel_attempt, should_switch_status, store_fallback_snapshot, should_switch};
use http_upstream::{execute_http_upstream_request, proxy_body_to_json_value};
