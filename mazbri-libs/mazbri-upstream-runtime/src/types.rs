use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

use mazbri_core::{ChannelEgressSpec, SupplierConfig};

use crate::crypto_pipeline::PipelineKeys;

/// Default max body bytes captured into stage summaries (64 KiB).
pub const DEFAULT_MAX_BODY_CAPTURE: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMode {
    /// Production bypass path (no detailed stages unless `collect_trace`).
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
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            BodyKind::Json(s) | BodyKind::Form(s) | BodyKind::Xml(s) => s,
            BodyKind::Raw { body, .. } => body,
        }
    }

    #[must_use]
    pub fn into_string(self) -> String {
        match self {
            BodyKind::Json(s) | BodyKind::Form(s) | BodyKind::Xml(s) => s,
            BodyKind::Raw { body, .. } => body,
        }
    }

    #[must_use]
    pub fn is_json(&self) -> bool {
        matches!(self, BodyKind::Json(_))
    }

    #[must_use]
    pub fn content_type(&self) -> &str {
        match self {
            BodyKind::Json(_) => "application/json",
            BodyKind::Form(_) => "application/x-www-form-urlencoded",
            BodyKind::Xml(_) => "text/xml; charset=utf-8",
            BodyKind::Raw { content_type, .. } => content_type,
        }
    }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.into_string().into_bytes()
    }
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
    pub timeout: Duration,
    pub mode: ExecuteMode,
    pub options: ExecuteOptions,
    /// Decrypted credential map (supplier `auth_flow.credentials`).
    pub credentials: HashMap<String, String>,
    /// 显式注入的流水线密钥（bypass / 探针）；`None` 时引擎从 `credentials` 推导。
    pub pipeline_keys: Option<PipelineKeys>,
    /// gRPC `FileDescriptorSet` / optional WSDL bytes.
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
    /// 本次调用内部发生的重试次数（认证换票、WS 换新连接）。
    #[serde(default)]
    pub internal_retries: u32,
    /// 每次内部重试的原因，顺序与发生顺序一致。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retry_reasons: Vec<RetryReason>,
    /// 判定为认证被拒但未重试时的原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_skipped: Option<RetrySkip>,
}

/// 内部重试原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryReason {
    AuthRejected(AuthRejectTrigger),
    StaleConnection,
}

impl RetryReason {
    /// 透传给网关 / 指标用的闭集标签，例如 `auth_rejected:jsonrpc_code`。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::AuthRejected(AuthRejectTrigger::HttpStatus(_)) => "auth_rejected:http_status",
            Self::AuthRejected(AuthRejectTrigger::JsonRpcCode(_)) => "auth_rejected:jsonrpc_code",
            Self::AuthRejected(AuthRejectTrigger::BodyMatch) => "auth_rejected:body_match",
            Self::AuthRejected(AuthRejectTrigger::WsClosedBeforeReply) => "auth_rejected:ws_close",
            Self::StaleConnection => "stale_connection",
        }
    }
}

/// 认证被拒却未重试的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrySkip {
    /// `auth_retry_enabled=false`。
    Disabled,
    /// 重试预算耗尽。
    Budget,
}

impl RetrySkip {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Budget => "budget",
        }
    }
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

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ExecuteError {
    #[error("CONFIG_INVALID: {0}")]
    ConfigInvalid(String),
    #[error("SSRF_BLOCKED: {0}")]
    SsrfBlocked(String),
    #[error("AUTH_TOKEN_FAILED: {0}")]
    AuthTokenFailed(String),
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
    /// 上游按认证失效规则拒绝了凭据（已换票重试仍被拒，或按配置不重试）。
    #[error("UPSTREAM_AUTH_REJECTED: {0}")]
    UpstreamAuthRejected(String),
    #[error("{0}")]
    Other(String),
}

impl ExecuteError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::ConfigInvalid(_) => "CONFIG_INVALID",
            Self::SsrfBlocked(_) => "SSRF_BLOCKED",
            Self::AuthTokenFailed(_) => "AUTH_TOKEN_FAILED",
            Self::UpstreamTransport(_) => "UPSTREAM_TRANSPORT",
            Self::UpstreamHttp(_) => "UPSTREAM_HTTP",
            Self::InvalidMethod(_) => "INVALID_METHOD",
            Self::SpecInvalid(_) => "SPEC_INVALID",
            Self::AdapterFailed(_) => "ADAPTER_FAILED",
            Self::ArtifactMissing(_) => "ARTIFACT_MISSING",
            Self::UpstreamAuthRejected(_) => "UPSTREAM_AUTH_REJECTED",
            Self::Other(_) => "UPSTREAM_ERROR",
        }
    }
}

/// 认证被拒的触发信号（来自上游认证失效规则）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthRejectTrigger {
    HttpStatus(u16),
    JsonRpcCode(i64),
    BodyMatch,
    WsClosedBeforeReply,
}

impl AuthRejectTrigger {
    /// 指标 / 日志用的闭集标签。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::HttpStatus(_) => "http_status",
            Self::JsonRpcCode(_) => "jsonrpc_code",
            Self::BodyMatch => "body_match",
            Self::WsClosedBeforeReply => "ws_close",
        }
    }
}

impl std::fmt::Display for AuthRejectTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HttpStatus(code) => write!(f, "http_status={code}"),
            Self::JsonRpcCode(code) => write!(f, "jsonrpc_code={code}"),
            Self::BodyMatch => f.write_str("body_match"),
            Self::WsClosedBeforeReply => f.write_str("ws_close"),
        }
    }
}

/// 认证被拒的证据：携带本次使用的令牌，供引擎比较后作废。
#[derive(Clone, PartialEq, Eq)]
pub struct AuthRejection {
    pub trigger: AuthRejectTrigger,
    pub token: String,
}

impl std::fmt::Debug for AuthRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthRejection")
            .field("trigger", &self.trigger)
            .field("token", &"<redacted>")
            .finish()
    }
}
