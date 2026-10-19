//! Channel egress spec (`channels.config.egress`) — unique source of truth
//! for code-free outbound adapters.
//!
//! Gateway validates the object and forwards it to bypass; bypass / workbench
//! execute it through `mazbri-upstream-runtime`. Protocol work does **not**
//! live in the gateway.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use thiserror::Error;

/// Closed adapter enum (wire names are snake_case).
pub const ADAPTER_HTTP_JSON: &str = "http_json";
pub const ADAPTER_HTTP_ENVELOPE: &str = "http_envelope";
pub const ADAPTER_SIGN_FORM: &str = "sign_form";
pub const ADAPTER_JSONRPC: &str = "jsonrpc";
pub const ADAPTER_GRAPHQL: &str = "graphql";
pub const ADAPTER_GRPC: &str = "grpc";
pub const ADAPTER_SOAP_XML: &str = "soap_xml";
pub const ADAPTER_WS_UNARY: &str = "ws_unary";
pub const ADAPTER_JOB_POLL: &str = "job_poll";

pub const AUTH_NONE: &str = "none";
pub const AUTH_API_KEY: &str = "api_key";
pub const AUTH_OAUTH_CC_FORM: &str = "oauth_cc_form";
pub const AUTH_GETTOKEN: &str = "gettoken";
pub const AUTH_BASIC: &str = "basic";
pub const AUTH_SIGN_FORM: &str = "sign_form";
pub const AUTH_GRPC_METADATA: &str = "grpc_metadata";
pub const AUTH_BEARER: &str = "bearer";
pub const AUTH_JWT: &str = "jwt";
pub const AUTH_DIGEST: &str = "digest";
pub const AUTH_AWS_SIGV4: &str = "aws_sigv4";
pub const AUTH_HAWK: &str = "hawk";

pub const WRAP_METHOD_BIZ_CONTENT: &str = "method_biz_content";
pub const WRAP_NONE: &str = "none";

const KNOWN_ADAPTERS: &[&str] = &[
    ADAPTER_HTTP_JSON,
    ADAPTER_HTTP_ENVELOPE,
    ADAPTER_SIGN_FORM,
    ADAPTER_JSONRPC,
    ADAPTER_GRAPHQL,
    ADAPTER_GRPC,
    ADAPTER_SOAP_XML,
    ADAPTER_WS_UNARY,
    ADAPTER_JOB_POLL,
];

const KNOWN_AUTH: &[&str] = &[
    AUTH_NONE,
    AUTH_API_KEY,
    AUTH_OAUTH_CC_FORM,
    AUTH_GETTOKEN,
    AUTH_BASIC,
    AUTH_SIGN_FORM,
    AUTH_GRPC_METADATA,
    AUTH_BEARER,
    AUTH_JWT,
    AUTH_DIGEST,
    AUTH_AWS_SIGV4,
    AUTH_HAWK,
];

const KNOWN_WRAPS: &[&str] = &[WRAP_METHOD_BIZ_CONTENT, WRAP_NONE];

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ChannelEgressError {
    #[error("{0}")]
    Message(String),
}

impl ChannelEgressError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterKind {
    HttpJson,
    HttpEnvelope,
    SignForm,
    Jsonrpc,
    Graphql,
    Grpc,
    SoapXml,
    WsUnary,
    JobPoll,
}

impl AdapterKind {
    pub fn parse(s: &str) -> Result<Self, ChannelEgressError> {
        match s.trim() {
            ADAPTER_HTTP_JSON => Ok(Self::HttpJson),
            ADAPTER_HTTP_ENVELOPE => Ok(Self::HttpEnvelope),
            ADAPTER_SIGN_FORM => Ok(Self::SignForm),
            ADAPTER_JSONRPC => Ok(Self::Jsonrpc),
            ADAPTER_GRAPHQL => Ok(Self::Graphql),
            ADAPTER_GRPC => Ok(Self::Grpc),
            ADAPTER_SOAP_XML => Ok(Self::SoapXml),
            ADAPTER_WS_UNARY => Ok(Self::WsUnary),
            ADAPTER_JOB_POLL => Ok(Self::JobPoll),
            other => Err(ChannelEgressError::new(format!(
                "unknown egress.adapter '{other}': known=[{}]",
                KNOWN_ADAPTERS.join(", ")
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::HttpJson => ADAPTER_HTTP_JSON,
            Self::HttpEnvelope => ADAPTER_HTTP_ENVELOPE,
            Self::SignForm => ADAPTER_SIGN_FORM,
            Self::Jsonrpc => ADAPTER_JSONRPC,
            Self::Graphql => ADAPTER_GRAPHQL,
            Self::Grpc => ADAPTER_GRPC,
            Self::SoapXml => ADAPTER_SOAP_XML,
            Self::WsUnary => ADAPTER_WS_UNARY,
            Self::JobPoll => ADAPTER_JOB_POLL,
        }
    }

    pub fn known() -> &'static [&'static str] {
        KNOWN_ADAPTERS
    }

    pub fn requires_operation(self) -> bool {
        !matches!(self, Self::HttpJson)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthProfile {
    None,
    ApiKey,
    OauthCcForm,
    Gettoken,
    Basic,
    SignForm,
    GrpcMetadata,
    Bearer,
    Jwt,
    Digest,
    AwsSigV4,
    Hawk,
}

impl AuthProfile {
    pub fn parse(s: &str) -> Result<Self, ChannelEgressError> {
        match s.trim() {
            AUTH_NONE | "" => Ok(Self::None),
            AUTH_API_KEY => Ok(Self::ApiKey),
            AUTH_OAUTH_CC_FORM => Ok(Self::OauthCcForm),
            AUTH_GETTOKEN => Ok(Self::Gettoken),
            AUTH_BASIC => Ok(Self::Basic),
            AUTH_SIGN_FORM => Ok(Self::SignForm),
            AUTH_GRPC_METADATA => Ok(Self::GrpcMetadata),
            AUTH_BEARER => Ok(Self::Bearer),
            AUTH_JWT => Ok(Self::Jwt),
            AUTH_DIGEST => Ok(Self::Digest),
            AUTH_AWS_SIGV4 => Ok(Self::AwsSigV4),
            AUTH_HAWK => Ok(Self::Hawk),
            other => Err(ChannelEgressError::new(format!(
                "unknown egress.auth.profile '{other}': known=[{}]",
                KNOWN_AUTH.join(", ")
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => AUTH_NONE,
            Self::ApiKey => AUTH_API_KEY,
            Self::OauthCcForm => AUTH_OAUTH_CC_FORM,
            Self::Gettoken => AUTH_GETTOKEN,
            Self::Basic => AUTH_BASIC,
            Self::SignForm => AUTH_SIGN_FORM,
            Self::GrpcMetadata => AUTH_GRPC_METADATA,
            Self::Bearer => AUTH_BEARER,
            Self::Jwt => AUTH_JWT,
            Self::Digest => AUTH_DIGEST,
            Self::AwsSigV4 => AUTH_AWS_SIGV4,
            Self::Hawk => AUTH_HAWK,
        }
    }

    pub fn known() -> &'static [&'static str] {
        KNOWN_AUTH
    }

    pub fn is_none(self) -> bool {
        matches!(self, Self::None)
    }

    pub fn uses_ticket(self) -> bool {
        matches!(self, Self::OauthCcForm | Self::Gettoken)
    }

    pub fn uses_inject(self) -> bool {
        matches!(
            self,
            Self::ApiKey
                | Self::OauthCcForm
                | Self::Gettoken
                | Self::GrpcMetadata
                | Self::Bearer
                | Self::Jwt
        )
    }

    pub fn needs_http_challenge_or_sign(self) -> bool {
        matches!(self, Self::Digest | Self::AwsSigV4 | Self::Hawk)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AuthInjectSpec {
    #[serde(default = "default_inject_to")]
    pub to: String,
    #[serde(default)]
    pub key: String,
    #[serde(default = "default_inject_format")]
    pub format: String,
}

fn default_inject_to() -> String {
    "header".into()
}
fn default_inject_format() -> String {
    "Bearer {}".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AuthCredentialKeys {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AuthJwtSpec {
    /// HS256 | HS384 | HS512 | RS256 | RS384 | RS512. Empty = HS256.
    #[serde(default)]
    pub algorithm: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claims_tpl: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AuthDigestSpec {
    /// MD5 | SHA-256. Empty = MD5.
    #[serde(default)]
    pub algorithm: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realm: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AuthAwsSpec {
    pub region: String,
    pub service: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AuthHawkSpec {
    /// sha256 (default).
    #[serde(default)]
    pub algorithm: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AuthSpec {
    #[serde(default = "default_auth_profile")]
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_jsonpath: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_jsonpath: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_body_tpl: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inject: Option<AuthInjectSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_keys: Option<AuthCredentialKeys>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwt: Option<AuthJwtSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<AuthDigestSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aws: Option<AuthAwsSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hawk: Option<AuthHawkSpec>,
}

fn default_auth_profile() -> String {
    AUTH_NONE.into()
}

impl AuthSpec {
    pub fn profile_kind(&self) -> Result<AuthProfile, ChannelEgressError> {
        AuthProfile::parse(&self.profile)
    }
}

fn clear_ticket_fields(spec: &mut AuthSpec) {
    spec.token_path = None;
    spec.token_url = None;
    spec.token_method = None;
    spec.token_jsonpath = None;
    spec.expiry_jsonpath = None;
    spec.request_body_tpl = None;
}

fn clear_nested_specs(spec: &mut AuthSpec) {
    spec.jwt = None;
    spec.digest = None;
    spec.aws = None;
    spec.hawk = None;
}

/// Drop fields that the selected profile must not persist.
/// `None` means the caller should set `egress.auth = None`.
pub fn sanitize_auth_spec(mut spec: AuthSpec) -> Option<AuthSpec> {
    let profile = match AuthProfile::parse(&spec.profile) {
        Ok(p) => p,
        Err(_) => return Some(spec),
    };
    spec.profile = profile.as_str().to_string();
    match profile {
        AuthProfile::None => None,
        AuthProfile::Basic => {
            clear_ticket_fields(&mut spec);
            spec.inject = None;
            spec.credential_keys = None;
            clear_nested_specs(&mut spec);
            Some(spec)
        }
        AuthProfile::ApiKey | AuthProfile::Bearer => {
            clear_ticket_fields(&mut spec);
            spec.headers.clear();
            clear_nested_specs(&mut spec);
            Some(spec)
        }
        AuthProfile::OauthCcForm | AuthProfile::Gettoken => {
            spec.jwt = None;
            spec.digest = None;
            spec.aws = None;
            spec.hawk = None;
            Some(spec)
        }
        AuthProfile::SignForm => {
            clear_ticket_fields(&mut spec);
            spec.headers.clear();
            spec.inject = None;
            spec.credential_keys = None;
            clear_nested_specs(&mut spec);
            Some(spec)
        }
        AuthProfile::GrpcMetadata => {
            clear_ticket_fields(&mut spec);
            spec.headers.clear();
            spec.credential_keys = None;
            clear_nested_specs(&mut spec);
            let mut inject = spec.inject.unwrap_or_default();
            inject.to = "metadata".into();
            spec.inject = Some(inject);
            Some(spec)
        }
        AuthProfile::Jwt => {
            clear_ticket_fields(&mut spec);
            spec.headers.clear();
            spec.digest = None;
            spec.aws = None;
            spec.hawk = None;
            Some(spec)
        }
        AuthProfile::Digest => {
            clear_ticket_fields(&mut spec);
            spec.headers.clear();
            spec.inject = None;
            spec.credential_keys = None;
            spec.jwt = None;
            spec.aws = None;
            spec.hawk = None;
            Some(spec)
        }
        AuthProfile::AwsSigV4 => {
            clear_ticket_fields(&mut spec);
            spec.headers.clear();
            spec.inject = None;
            spec.credential_keys = None;
            spec.jwt = None;
            spec.digest = None;
            spec.hawk = None;
            Some(spec)
        }
        AuthProfile::Hawk => {
            clear_ticket_fields(&mut spec);
            spec.headers.clear();
            spec.inject = None;
            spec.credential_keys = None;
            spec.jwt = None;
            spec.digest = None;
            spec.aws = None;
            Some(spec)
        }
    }
}

pub fn sanitize_egress_auth(spec: &mut ChannelEgressSpec) {
    if let Some(auth) = spec.auth.take() {
        spec.auth = sanitize_auth_spec(auth);
    }
}

pub fn profile_is_undeclared(raw: Option<&str>) -> bool {
    matches!(raw.map(str::trim).unwrap_or(""), "" | AUTH_NONE)
}

/// L2 outbound request. Tenant L1 is always JSON (or none).
/// `content_type` overrides JSON MIME only; `body_encoding` is json|form|none.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EgressRequestSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<HashMap<String, String>>,
    /// JSON MIME override (`application/vnd.api+json`, …). Ignored for form/none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// `json` (default) | `form` | `none`. Absent = legacy http_json behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_encoding: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EgressOkSpec {
    pub path: String,
    #[serde(default)]
    pub eq: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EgressResponseSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unwrap: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<EgressOkSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GraphqlSpec {
    #[serde(default)]
    pub document: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables_map: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_map: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct JobPollSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit_path: Option<String>,
    #[serde(default)]
    pub query_path: String,
    #[serde(default)]
    pub job_id_jsonpath: String,
    #[serde(default)]
    pub status_jsonpath: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_jsonpath: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_jsonpath: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WsUnarySpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<String>,
    /// header / query / first_frame — closed set, combinable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_in: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SignFormSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SoapSpec {
    /// Upstream camelCase field → tenant snake_case (response reverse map).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_map: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_fields: Option<Vec<String>>,
}

/// Parsed `channels.config.egress` object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ChannelEgressSpec {
    pub adapter: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<EgressRequestSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<EgressResponseSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphql: Option<GraphqlSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_poll: Option<JobPollSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ws: Option<WsUnarySpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_form: Option<SignFormSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap: Option<SoapSpec>,
}

impl ChannelEgressSpec {
    pub fn adapter_kind(&self) -> Result<AdapterKind, ChannelEgressError> {
        AdapterKind::parse(&self.adapter)
    }

    pub fn wrap_name(&self) -> &str {
        self.request
            .as_ref()
            .and_then(|r| r.wrap.as_deref())
            .filter(|s| !s.is_empty())
            .unwrap_or(WRAP_METHOD_BIZ_CONTENT)
    }

    pub fn request_method(&self) -> &str {
        self.request
            .as_ref()
            .and_then(|r| r.method.as_deref())
            .filter(|s| !s.is_empty())
            .unwrap_or("POST")
    }

    pub fn request_path<'a>(&'a self, path_prefix_fallback: &'a str) -> &'a str {
        if let Some(p) = self
            .request
            .as_ref()
            .and_then(|r| r.path.as_deref())
            .filter(|s| !s.is_empty())
        {
            return p;
        }
        if path_prefix_fallback.is_empty() {
            "/"
        } else {
            path_prefix_fallback
        }
    }

    pub fn resolved_base_url<'a>(&'a self, supplier_upstream_url: &'a str) -> &'a str {
        self.base_url
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(supplier_upstream_url)
    }
}

/// SHA-256 hex of canonical JSON (stable enough for cache keys).
pub fn hash_egress_spec_json(json: &str) -> String {
    let digest = Sha256::digest(json.as_bytes());
    hex::encode(digest)
}

pub fn hash_egress_spec(spec: &ChannelEgressSpec) -> Result<String, ChannelEgressError> {
    let json = serde_json::to_string(spec)
        .map_err(|e| ChannelEgressError::new(format!("serialize egress: {e}")))?;
    Ok(hash_egress_spec_json(&json))
}

/// Parse and validate an `egress` object (not the whole `channels.config`).
pub fn parse_and_validate_egress(
    value: &serde_json::Value,
) -> Result<ChannelEgressSpec, ChannelEgressError> {
    if !value.is_object() {
        return Err(ChannelEgressError::new("egress 必须是 JSON 对象"));
    }
    let spec: ChannelEgressSpec = serde_json::from_value(value.clone())
        .map_err(|e| ChannelEgressError::new(format!("egress JSON 反序列化失败: {e}")))?;
    validate_egress_spec(&spec)?;
    Ok(spec)
}

/// Extract `channels.config.egress`. Missing egress is an error (caller may
/// treat empty config as “not a code-free channel”).
pub fn parse_channel_config_egress(
    channel_config: &serde_json::Value,
) -> Result<ChannelEgressSpec, ChannelEgressError> {
    let Some(egress) = channel_config.get("egress") else {
        return Err(ChannelEgressError::new(
            "channels.config.egress 缺失：免代码通道必须写入 adapter 规格，禁止再落空对象当 HTTP 透传",
        ));
    };
    parse_and_validate_egress(egress)
}

/// If `config.egress` is present, validate it. Absent egress is allowed
/// (microservice channels, incomplete drafts).
pub fn validate_channel_config_if_egress(
    channel_config: &serde_json::Value,
) -> Result<Option<ChannelEgressSpec>, ChannelEgressError> {
    if channel_config.is_null() {
        return Ok(None);
    }
    let Some(obj) = channel_config.as_object() else {
        return Err(ChannelEgressError::new("channels.config 必须是 JSON 对象"));
    };
    match obj.get("egress") {
        None => Ok(None),
        Some(v) => Ok(Some(parse_and_validate_egress(v)?)),
    }
}

fn is_allowed_http_method(method: &str) -> bool {
    let m = method.trim();
    if m.is_empty() || m.len() > 16 {
        return false;
    }
    m.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-')
}

pub fn validate_egress_spec(spec: &ChannelEgressSpec) -> Result<(), ChannelEgressError> {
    let adapter = spec.adapter_kind()?;
    if let Some(ref url) = spec.base_url {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            validate_outbound_url(trimmed, "egress.base_url")?;
        }
    }
    if let Some(ms) = spec.timeout_ms {
        if ms == 0 {
            return Err(ChannelEgressError::new("egress.timeout_ms 必须大于 0"));
        }
    }
    if let Some(ref req) = spec.request {
        if let Some(ref wrap) = req.wrap {
            if !wrap.is_empty() && !KNOWN_WRAPS.contains(&wrap.as_str()) {
                return Err(ChannelEgressError::new(format!(
                    "unknown egress.request.wrap '{wrap}': known=[{}]",
                    KNOWN_WRAPS.join(", ")
                )));
            }
        }
        if let Some(ref method) = req.method {
            let m = method.trim().to_ascii_uppercase();
            if !is_allowed_http_method(&m) {
                return Err(ChannelEgressError::new(format!(
                    "egress.request.method '{method}' 不受支持"
                )));
            }
        }
        if let Some(ref path) = req.path {
            if !path.is_empty() && !path.starts_with('/') && !path.starts_with("http") {
                return Err(ChannelEgressError::new(
                    "egress.request.path 必须以 / 开头（或为绝对 URL）",
                ));
            }
        }
        if let Some(ref enc) = req.body_encoding {
            let e = enc.trim();
            if !e.is_empty() {
                if !matches!(e, "json" | "form" | "none") {
                    return Err(ChannelEgressError::new(format!(
                        "unknown egress.request.body_encoding '{enc}': known=[json, form, none]"
                    )));
                }
                if e == "form" && adapter != AdapterKind::HttpJson {
                    return Err(ChannelEgressError::new(
                        "egress.request.body_encoding=form 仅支持 adapter=http_json",
                    ));
                }
                if e == "none"
                    && !matches!(adapter, AdapterKind::HttpJson | AdapterKind::HttpEnvelope)
                {
                    return Err(ChannelEgressError::new(format!(
                        "adapter {} requires json body",
                        adapter.as_str()
                    )));
                }
            }
        }
        if let Some(ref ct) = req.content_type {
            if ct.chars().any(char::is_control) {
                return Err(ChannelEgressError::new(
                    "egress.request.content_type 含非法控制字符",
                ));
            }
        }
    }
    if let Some(ref auth) = spec.auth {
        let profile = auth.profile_kind()?;
        if let Some(ref url) = auth.token_url {
            if !url.trim().is_empty() {
                validate_outbound_url(url.trim(), "egress.auth.token_url")?;
            }
        }
        if profile.uses_ticket() {
            let token_loc = auth
                .token_url
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .or(auth.token_path.as_deref().filter(|s| !s.trim().is_empty()));
            if token_loc.is_none() {
                return Err(ChannelEgressError::new(
                    "egress.auth 换票 profile 需要 token_url 或 token_path",
                ));
            }
        }
        if profile.needs_http_challenge_or_sign()
            && matches!(adapter, AdapterKind::Grpc | AdapterKind::WsUnary)
        {
            return Err(ChannelEgressError::new(format!(
                "egress.auth.profile={} 仅支持 HTTP 适配器",
                profile.as_str()
            )));
        }
        if profile == AuthProfile::AwsSigV4 {
            let aws = auth.aws.as_ref().ok_or_else(|| {
                ChannelEgressError::new(
                    "egress.auth.profile=aws_sigv4 需要 aws.region 与 aws.service",
                )
            })?;
            if aws.region.trim().is_empty() || aws.service.trim().is_empty() {
                return Err(ChannelEgressError::new(
                    "egress.auth.aws.region 与 aws.service 必填",
                ));
            }
        }
        if profile == AuthProfile::Jwt {
            let algo = auth
                .jwt
                .as_ref()
                .map(|j| j.algorithm.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or("HS256");
            if !matches!(
                algo,
                "HS256" | "HS384" | "HS512" | "RS256" | "RS384" | "RS512"
            ) {
                return Err(ChannelEgressError::new(format!(
                    "egress.auth.jwt.algorithm '{algo}' 不受支持"
                )));
            }
        }
        if profile == AuthProfile::Digest {
            let algo = auth
                .digest
                .as_ref()
                .map(|d| d.algorithm.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or("MD5");
            if !matches!(algo, "MD5" | "SHA-256" | "SHA256") {
                return Err(ChannelEgressError::new(format!(
                    "egress.auth.digest.algorithm '{algo}' 不受支持"
                )));
            }
        }
        if profile == AuthProfile::Hawk {
            let algo = auth
                .hawk
                .as_ref()
                .map(|h| h.algorithm.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or("sha256");
            if !algo.eq_ignore_ascii_case("sha256") {
                return Err(ChannelEgressError::new(format!(
                    "egress.auth.hawk.algorithm '{algo}' 仅支持 sha256"
                )));
            }
        }
        if let Some(ref inject) = auth.inject {
            let to = inject.to.trim();
            if !matches!(to, "header" | "query" | "metadata" | "first_frame" | "") {
                return Err(ChannelEgressError::new(format!(
                    "unknown egress.auth.inject.to '{to}'"
                )));
            }
        }
    }

    if adapter.requires_operation() {
        let op = spec.operation.as_deref().map(str::trim).unwrap_or("");
        if op.is_empty() {
            return Err(ChannelEgressError::new(format!(
                "egress.adapter={} 需要 egress.operation",
                adapter.as_str()
            )));
        }
    }

    match adapter {
        AdapterKind::Graphql => {
            let doc = spec
                .graphql
                .as_ref()
                .map(|g| g.document.trim())
                .unwrap_or("");
            if doc.is_empty() {
                return Err(ChannelEgressError::new(
                    "egress.adapter=graphql 需要 graphql.document",
                ));
            }
        }
        AdapterKind::Grpc => {
            let id = spec.artifact_id.as_deref().map(str::trim).unwrap_or("");
            if id.is_empty() {
                return Err(ChannelEgressError::new(
                    "egress.adapter=grpc 需要 egress.artifact_id（FileDescriptorSet）",
                ));
            }
        }
        AdapterKind::JobPoll => {
            let jp = spec.job_poll.as_ref().ok_or_else(|| {
                ChannelEgressError::new("egress.adapter=job_poll 需要 job_poll 对象")
            })?;
            if jp.query_path.trim().is_empty() {
                return Err(ChannelEgressError::new("job_poll.query_path 不能为空"));
            }
            if jp.job_id_jsonpath.trim().is_empty() {
                return Err(ChannelEgressError::new("job_poll.job_id_jsonpath 不能为空"));
            }
            if jp.status_jsonpath.trim().is_empty() {
                return Err(ChannelEgressError::new("job_poll.status_jsonpath 不能为空"));
            }
            if let Some(ms) = jp.poll_interval_ms {
                if ms == 0 {
                    return Err(ChannelEgressError::new(
                        "job_poll.poll_interval_ms 必须大于 0",
                    ));
                }
            }
        }
        AdapterKind::WsUnary => {
            if let Some(ref ws) = spec.ws {
                if let Some(ref auth_in) = ws.auth_in {
                    for loc in auth_in {
                        if !matches!(loc.as_str(), "header" | "query" | "first_frame") {
                            return Err(ChannelEgressError::new(format!(
                                "unknown ws.auth_in '{loc}'"
                            )));
                        }
                    }
                }
            }
        }
        AdapterKind::SignForm => {
            if let Some(ref sf) = spec.sign_form {
                if let Some(ref st) = sf.sign_type {
                    let t = st.trim();
                    if !t.is_empty() && !t.eq_ignore_ascii_case("HMAC-SHA256") {
                        return Err(ChannelEgressError::new(format!(
                            "sign_form.sign_type '{st}' 第一期仅支持 HMAC-SHA256"
                        )));
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_outbound_url(raw: &str, field: &str) -> Result<(), ChannelEgressError> {
    let ok_scheme = raw.starts_with("http://")
        || raw.starts_with("https://")
        || raw.starts_with("ws://")
        || raw.starts_with("wss://");
    if !ok_scheme {
        return Err(ChannelEgressError::new(format!(
            "{field} 必须是 http(s):// 或 ws(s):// URL"
        )));
    }
    let rest = raw
        .split_once("://")
        .map(|(_, r)| r)
        .unwrap_or("")
        .trim_start_matches('/');
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.split('@').next_back().unwrap_or(host);
    let host = host.split('%').next().unwrap_or(host);
    if host.is_empty() || host.starts_with(':') {
        return Err(ChannelEgressError::new(format!("{field} 缺少 host")));
    }
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(ChannelEgressError::new(format!("{field} 含非法空白字符")));
    }
    Ok(())
}

/// Default REST-shaped egress used when migrating a legacy channel that only
/// had `path_prefix` (M0 backfill).
pub fn default_http_json_egress(path: &str, method: &str) -> ChannelEgressSpec {
    ChannelEgressSpec {
        adapter: ADAPTER_HTTP_JSON.to_string(),
        request: Some(EgressRequestSpec {
            method: Some(method.to_string()),
            path: Some(if path.is_empty() {
                "/".into()
            } else {
                path.to_string()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Merge `egress` into an existing channel config object, preserving
/// `rate_limit_*` and other sibling keys.
pub fn upsert_egress_in_channel_config(
    existing: &serde_json::Value,
    egress: &ChannelEgressSpec,
) -> Result<serde_json::Value, ChannelEgressError> {
    let mut obj = match existing {
        serde_json::Value::Object(map) => map.clone(),
        serde_json::Value::Null => serde_json::Map::new(),
        _ => {
            return Err(ChannelEgressError::new(
                "channels.config 必须是 JSON 对象才能写入 egress",
            ));
        }
    };
    let egress_val = serde_json::to_value(egress)
        .map_err(|e| ChannelEgressError::new(format!("serialize egress: {e}")))?;
    obj.insert("egress".into(), egress_val);
    Ok(serde_json::Value::Object(obj))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_adapter() {
        let v = serde_json::json!({"adapter": "ftp"});
        let err = parse_and_validate_egress(&v).unwrap_err().to_string();
        assert!(err.contains("unknown egress.adapter"));
    }

    #[test]
    fn http_json_ok_without_operation() {
        let v = serde_json::json!({
            "adapter": "http_json",
            "request": {"method": "POST", "path": "/v1/search"}
        });
        let spec = parse_and_validate_egress(&v).unwrap();
        assert_eq!(spec.adapter_kind().unwrap(), AdapterKind::HttpJson);
    }

    #[test]
    fn envelope_requires_operation() {
        let v = serde_json::json!({"adapter": "http_envelope"});
        let err = parse_and_validate_egress(&v).unwrap_err().to_string();
        assert!(err.contains("operation"));
    }

    #[test]
    fn missing_egress_on_channel_config() {
        let cfg = serde_json::json!({"rate_limit_mode": "reject"});
        let err = parse_channel_config_egress(&cfg).unwrap_err().to_string();
        assert!(err.contains("egress"));
    }

    #[test]
    fn upsert_preserves_rate_limit() {
        let existing = serde_json::json!({"rate_limit_mode": "reject"});
        let spec = default_http_json_egress("/x", "POST");
        let merged = upsert_egress_in_channel_config(&existing, &spec).unwrap();
        assert_eq!(merged["rate_limit_mode"], "reject");
        assert_eq!(merged["egress"]["adapter"], "http_json");
    }

    #[test]
    fn rejects_bad_url() {
        let v = serde_json::json!({
            "adapter": "http_json",
            "base_url": "not-a-url"
        });
        let err = parse_and_validate_egress(&v).unwrap_err().to_string();
        assert!(err.contains("http"));
    }

    #[test]
    fn http_json_upstream_path_decoupled_from_tenant_prefix() {
        let v = serde_json::json!({
            "adapter": "http_json",
            "request": {"method": "POST", "path": "/v1/search"}
        });
        let spec = parse_and_validate_egress(&v).unwrap();
        assert_eq!(spec.request_path("/api/v1/gateway-product"), "/v1/search");
    }

    #[test]
    fn body_encoding_form_ok_on_http_json() {
        let v = serde_json::json!({
            "adapter": "http_json",
            "request": {"method": "POST", "path": "/", "body_encoding": "form"}
        });
        parse_and_validate_egress(&v).unwrap();
    }

    #[test]
    fn body_encoding_form_rejected_on_graphql() {
        let v = serde_json::json!({
            "adapter": "graphql",
            "operation": "Q",
            "graphql": {"document": "query Q { x }"},
            "request": {"body_encoding": "form"}
        });
        let err = parse_and_validate_egress(&v).unwrap_err().to_string();
        assert!(err.contains("form"));
    }

    #[test]
    fn body_encoding_none_rejected_on_sign_form() {
        let v = serde_json::json!({
            "adapter": "sign_form",
            "operation": "pay",
            "request": {"body_encoding": "none"}
        });
        let err = parse_and_validate_egress(&v).unwrap_err().to_string();
        assert!(err.contains("requires json body"));
    }

    #[test]
    fn content_type_rejects_crlf() {
        let v = serde_json::json!({
            "adapter": "http_json",
            "request": {"content_type": "application/json\r\nX-Inject: 1"}
        });
        let err = parse_and_validate_egress(&v).unwrap_err().to_string();
        assert!(err.contains("content_type"));
    }

    #[test]
    fn missing_body_encoding_still_valid() {
        let v = serde_json::json!({
            "adapter": "http_json",
            "request": {"method": "POST", "path": "/v1"}
        });
        let spec = parse_and_validate_egress(&v).unwrap();
        assert!(spec.request.as_ref().unwrap().body_encoding.is_none());
    }

    #[test]
    fn parses_new_auth_profiles() {
        for name in [
            AUTH_BEARER,
            AUTH_JWT,
            AUTH_DIGEST,
            AUTH_AWS_SIGV4,
            AUTH_HAWK,
        ] {
            assert_eq!(AuthProfile::parse(name).unwrap().as_str(), name);
        }
    }

    #[test]
    fn rejects_inject_to_body() {
        let v = serde_json::json!({
            "adapter": "http_json",
            "auth": {
                "profile": "api_key",
                "inject": {"to": "body", "key": "token", "format": "{}"}
            }
        });
        let err = parse_and_validate_egress(&v).unwrap_err().to_string();
        assert!(err.contains("inject.to"), "{err}");
    }

    #[test]
    fn sanitize_basic_drops_token_url() {
        let spec = AuthSpec {
            profile: AUTH_BASIC.into(),
            token_url: Some("https://idp.example/token".into()),
            inject: Some(AuthInjectSpec::default()),
            ..Default::default()
        };
        let out = sanitize_auth_spec(spec).expect("basic keeps auth");
        assert_eq!(out.profile, AUTH_BASIC);
        assert!(out.token_url.is_none());
        assert!(out.inject.is_none());
    }

    #[test]
    fn sanitize_none_removes_auth() {
        let spec = AuthSpec {
            profile: AUTH_NONE.into(),
            token_url: Some("https://idp.example/token".into()),
            ..Default::default()
        };
        assert!(sanitize_auth_spec(spec).is_none());
    }

    #[test]
    fn digest_rejected_on_grpc() {
        let v = serde_json::json!({
            "adapter": "grpc",
            "operation": "pkg.Svc/M",
            "artifact_id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
            "auth": {"profile": "digest"}
        });
        let err = parse_and_validate_egress(&v).unwrap_err().to_string();
        assert!(err.contains("HTTP"), "{err}");
    }

    #[test]
    fn aws_requires_region_and_service() {
        let v = serde_json::json!({
            "adapter": "http_json",
            "auth": {"profile": "aws_sigv4"}
        });
        let err = parse_and_validate_egress(&v).unwrap_err().to_string();
        assert!(err.contains("aws"), "{err}");
    }
}
