use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

use mazbri_core::{ChannelEgressSpec, SupplierConfig};

/// Default max body bytes captured into stage summaries (64 KiB).
pub const DEFAULT_MAX_BODY_CAPTURE: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMode {
    /// Production bypass path (no detailed stages unless collect_trace).
    Production,
    /// Only fetch/inject auth token stages; no business upstream call.
    AuthOnly,
    /// Full business call with stages.
    Business,
    /// Render transforms only; no network.
    TransformPreview,
    /// Live run against published supplier config (same as business).
    Live,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteOptions {
    #[serde(default)]
    pub follow_redirects: bool,
    #[serde(default = "default_max_body")]
    pub max_body_capture_bytes: usize,
    #[serde(default = "default_true")]
    pub mask_secrets: bool,
    /// Collect stage traces even in Production mode.
    #[serde(default)]
    pub collect_trace: bool,
}

fn default_max_body() -> usize {
    DEFAULT_MAX_BODY_CAPTURE
}
fn default_true() -> bool {
    true
}

impl Default for ExecuteOptions {
    fn default() -> Self {
        Self {
            follow_redirects: false,
            max_body_capture_bytes: DEFAULT_MAX_BODY_CAPTURE,
            mask_secrets: true,
            collect_trace: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BodyKind {
    Json(String),
    Form(String),
    Xml(String),
    Raw { body: String, content_type: String },
}

impl BodyKind {
    pub fn as_str(&self) -> &str {
        match self {
            BodyKind::Json(s) | BodyKind::Form(s) | BodyKind::Xml(s) => s,
            BodyKind::Raw { body, .. } => body,
        }
    }

    pub fn into_string(self) -> String {
        match self {
            BodyKind::Json(s) | BodyKind::Form(s) | BodyKind::Xml(s) => s,
            BodyKind::Raw { body, .. } => body,
        }
    }

    pub fn is_json(&self) -> bool {
        matches!(self, BodyKind::Json(_))
    }

    pub fn content_type(&self) -> &str {
        match self {
            BodyKind::Json(_) => "application/json",
            BodyKind::Form(_) => "application/x-www-form-urlencoded",
            BodyKind::Xml(_) => "text/xml; charset=utf-8",
            BodyKind::Raw { content_type, .. } => content_type,
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.into_string().into_bytes()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundRequest {
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub query: Vec<(String, String)>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<BodyKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub final_url: String,
    pub method: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct ExecuteInput {
    pub supplier_code: String,
    pub base_url: String,
    /// Decrypted runtime config (may be None for plain passthrough).
    pub config: Option<Arc<SupplierConfig>>,
    pub request: OutboundRequest,
    pub timeout: Duration,
    pub mode: ExecuteMode,
    pub options: ExecuteOptions,
    /// Optional token cache key override (probe namespace). Defaults to supplier_code.
    pub cache_key: Option<String>,
}

/// Production / workbench invoke: tenant JSON object + channel egress spec.
#[derive(Debug, Clone)]
pub struct InvokeInput {
    pub supplier_id: String,
    pub supplier_code: String,
    pub channel_id: String,
    pub request_id: String,
    pub payload: serde_json::Value,
    pub spec: ChannelEgressSpec,
    pub config: Option<Arc<SupplierConfig>>,
    pub fallback_base_url: String,
    pub path_prefix: String,
    pub timeout: Duration,
    pub mode: ExecuteMode,
    pub options: ExecuteOptions,
    /// Decrypted credential map (supplier auth_flow.credentials).
    pub credentials: HashMap<String, String>,
    /// gRPC FileDescriptorSet / optional WSDL bytes.
    pub artifact_bytes: Option<Vec<u8>>,
    pub cache_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BizError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpstreamMeta {
    pub status: u16,
    pub url: String,
    pub adapter: String,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub stages: Vec<StageResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biz_error: Option<BizError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_meta: Option<UpstreamMeta>,
    #[serde(skip)]
    pub token: Option<String>,
    /// Raw outbound response (workbench assertions / previews).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<OutboundResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageSummary {
    pub method: Option<String>,
    pub url: Option<String>,
    pub headers_masked: Option<HashMap<String, String>>,
    pub body_preview: Option<String>,
    pub status: Option<u16>,
    pub extracted: Option<serde_json::Value>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    pub name: String,
    pub ok: bool,
    pub latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_summary: Option<StageSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_summary: Option<StageSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub stages: Vec<StageResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<OutboundResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Token used (raw) — only for internal inject; not serialized for API.
    #[serde(skip)]
    pub token: Option<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ExecuteError {
    #[error("CONFIG_INVALID: {0}")]
    ConfigInvalid(String),
    #[error("SSRF_BLOCKED: {0}")]
    SsrfBlocked(String),
    #[error("AUTH_TOKEN_FAILED: {0}")]
    AuthTokenFailed(String),
    #[error("TRANSFORM_FAILED: {0}")]
    TransformFailed(String),
    #[error("UPSTREAM_TRANSPORT: {0}")]
    UpstreamTransport(String),
    #[error("UPSTREAM_HTTP: {0}")]
    UpstreamHttp(String),
    #[error("INVALID_METHOD: {0}")]
    InvalidMethod(String),
    #[error("SPEC_INVALID: {0}")]
    SpecInvalid(String),
    #[error("ADAPTER_FAILED: {0}")]
    AdapterFailed(String),
    #[error("ARTIFACT_MISSING: {0}")]
    ArtifactMissing(String),
    #[error("{0}")]
    Other(String),
}

impl ExecuteError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ConfigInvalid(_) => "CONFIG_INVALID",
            Self::SsrfBlocked(_) => "SSRF_BLOCKED",
            Self::AuthTokenFailed(_) => "AUTH_TOKEN_FAILED",
            Self::TransformFailed(_) => "TRANSFORM_FAILED",
            Self::UpstreamTransport(_) => "UPSTREAM_TRANSPORT",
            Self::UpstreamHttp(_) => "UPSTREAM_HTTP",
            Self::InvalidMethod(_) => "INVALID_METHOD",
            Self::SpecInvalid(_) => "SPEC_INVALID",
            Self::AdapterFailed(_) => "ADAPTER_FAILED",
            Self::ArtifactMissing(_) => "ARTIFACT_MISSING",
            Self::Other(_) => "UPSTREAM_ERROR",
        }
    }
}
