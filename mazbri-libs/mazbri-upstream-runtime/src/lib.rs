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
mod engine;
mod funcs;
mod http_call;
mod mask;
mod ssrf;
mod template;
mod token_cache;
mod types;

pub use assert::{Assertion, AssertionResult, evaluate_assertions};
pub use engine::{RuntimeEngine, RuntimeLimits, resolve_timeout};
pub use mask::{mask_headers, mask_secret, mask_string_map, redact_body_preview};
pub use ssrf::{SsrfChecker, SsrfConfig, SsrfSafeResolver, is_always_blocked_ip, is_blocked_ip};
pub use template::{
    extract_by_path, is_identity_response_template, render_body_template, render_response_template,
};
pub use token_cache::{CachedToken, TokenCache};
pub use types::{
    BizError, BodyKind, DEFAULT_MAX_BODY_CAPTURE, ExecuteError, ExecuteMode, ExecuteOptions,
    InvokeInput, InvokeResult, OutboundResponse, StageResult, StageSummary, UpstreamMeta,
};
