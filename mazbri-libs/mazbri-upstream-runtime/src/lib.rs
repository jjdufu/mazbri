//! Shared transparent upstream execution kernel.
//!
//! Used by:
//! - `mazbri-bypass` production path
//! - `mazbri-mgmt` connector workbench probe runs
//!
//! **Single Runtime**: auth, adapters, SSRF checks and outbound HTTP live only
//! here — mgmt workbench/live runs and bypass all go through `RuntimeEngine::invoke`.

mod adapters;
mod assert;
mod auth;
mod auth_aws;
mod auth_digest;
mod auth_hawk;
mod auth_jwt;
pub mod canon;
pub mod crypto_pipeline;
mod engine;
#[cfg(test)]
mod engine_retry_tests;
mod funcs;
mod http_call;
mod mask;
mod retry_budget;
pub mod slots;
mod ssrf;
mod template;
mod token_cache;
mod types;
mod ws_pool;

pub use assert::{Assertion, AssertionResult, evaluate_assertions};
pub use crypto_pipeline::{
    default_slots_json_cipher_sign_iv, default_slots_sign_form, open_response,
    parts_from_json_object, pipeline_keys_from_credentials, seal_request, PipelineContext,
    PipelineError, PipelineKeys, RequestParts,
};
pub use engine::{RuntimeEngine, RuntimeOptions, resolve_timeout};
pub use ws_pool::{WsPool, WsPoolConfig};
pub use retry_budget::RetryBudget;
pub use mask::{mask_headers, mask_secret, mask_string_map, redact_body_preview};
pub use slots::{role as slot_role, SlotBags, SlotError};
pub use ssrf::{SsrfChecker, SsrfConfig, SsrfSafeResolver, is_always_blocked_ip, is_blocked_ip};
pub use template::{is_identity_response_template, render_body_template, render_response_template};
pub use token_cache::{CachedToken, TokenCache};
pub use types::{
    AuthRejectTrigger, AuthRejection, BizError, BodyKind, DEFAULT_MAX_BODY_CAPTURE, ExecuteError,
    ExecuteMode, ExecuteOptions, InvokeInput, InvokeResult, OutboundResponse, StageResult,
    StageSummary, UpstreamMeta,
};
