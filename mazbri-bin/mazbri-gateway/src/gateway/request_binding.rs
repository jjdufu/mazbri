use super::response::gateway_error_response;
use crate::crypto::TenantCryptoState;
use crate::gateway::metrics::{GATEWAY_JWT_KID_TOTAL, GATEWAY_REQUEST_BINDING_TOTAL};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use mazbri_core::crypto::request_binding::{
    canonical_http_binding, parse_unix_timestamp, truncate_nonce_for_log, validate_binding_nonce,
    validate_signature_input, validate_unix_timestamp, verify_hmac_sha256_base64,
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct GatewayHttpPath(pub String);

pub const HEADER_TIMESTAMP: &str = "x-mazbri-timestamp";
pub const HEADER_NONCE: &str = "x-mazbri-nonce";
pub const HEADER_SIGNATURE: &str = "x-mazbri-signature";
pub const HEADER_SIGNATURE_INPUT: &str = "x-mazbri-signature-input";
pub const HEADER_KEY_ID: &str = "x-mazbri-key-id";

#[derive(Debug, Clone)]
pub struct RequestBindingOutcome {
    pub auth_mode: &'static str,
    pub kid: Option<String>,
    pub nonce_trunc: Option<String>,
    pub sig_ok: Option<bool>,
}

#[derive(Debug)]
pub struct RequestBindingError {
    pub code: &'static str,
    pub message: &'static str,
}

impl RequestBindingError {
    fn missing() -> Self {
        Self {
            code: "40110",
            message: "Request binding headers are required",
        }
    }
    fn timestamp() -> Self {
        Self {
            code: "40111",
            message: "Request timestamp is outside the allowed skew",
        }
    }
    fn nonce() -> Self {
        Self {
            code: "40112",
            message: "Request nonce was already used",
        }
    }
    fn signature() -> Self {
        Self {
            code: "40113",
            message: "Request signature mismatch",
        }
    }
    fn kid() -> Self {
        Self {
            code: "40114",
            message: "Request binding key id is unknown or inactive",
        }
    }
}

impl IntoResponse for RequestBindingError {
    fn into_response(self) -> Response {
        GATEWAY_REQUEST_BINDING_TOTAL
            .with_label_values(&[self.code])
            .inc();
        let mut response =
            gateway_error_response(StatusCode::UNAUTHORIZED, self.code, self.message, None);
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            header::HeaderValue::from_static("Bearer realm=\"mazbri\", error=\"invalid_token\""),
        );
        response
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &'static str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn any_binding_header(headers: &HeaderMap) -> bool {
    [
        HEADER_TIMESTAMP,
        HEADER_NONCE,
        HEADER_SIGNATURE,
        HEADER_SIGNATURE_INPUT,
        HEADER_KEY_ID,
    ]
    .iter()
    .any(|name| headers.contains_key(*name))
}

pub async fn enforce_request_binding(
    crypto: &TenantCryptoState,
    tenant_id: Uuid,
    require: bool,
    nonce_prefix: &str,
    method: &str,
    path_and_query: &str,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<RequestBindingOutcome, RequestBindingError> {
    let profile = crypto
        .profile(tenant_id)
        .await
        .map_err(|_| RequestBindingError::kid())?;
    let required = require || profile.require_request_binding;
    if required && profile.signature_version != "hmac-v1" {
        return Err(RequestBindingError::signature());
    }
    if !required && !any_binding_header(headers) {
        GATEWAY_REQUEST_BINDING_TOTAL
            .with_label_values(&["skipped"])
            .inc();
        return Ok(RequestBindingOutcome {
            auth_mode: "bearer",
            kid: None,
            nonce_trunc: None,
            sig_ok: None,
        });
    }
    let timestamp_raw = header_str(headers, HEADER_TIMESTAMP);
    let nonce = header_str(headers, HEADER_NONCE);
    let signature = header_str(headers, HEADER_SIGNATURE);
    let signature_input = header_str(headers, HEADER_SIGNATURE_INPUT);
    let kid = header_str(headers, HEADER_KEY_ID);
    if timestamp_raw.is_none()
        || nonce.is_none()
        || signature.is_none()
        || signature_input.is_none()
        || kid.is_none()
    {
        return Err(RequestBindingError::missing());
    }
    let timestamp_raw = timestamp_raw.expect("checked");
    let nonce = nonce.expect("checked");
    let signature = signature.expect("checked");
    let signature_input = signature_input.expect("checked");
    let kid = kid.expect("checked");
    if validate_signature_input(signature_input).is_err() {
        return Err(RequestBindingError::signature());
    }
    if validate_binding_nonce(nonce).is_err() {
        return Err(RequestBindingError::signature());
    }
    let timestamp =
        parse_unix_timestamp(timestamp_raw).map_err(|_| RequestBindingError::timestamp())?;
    let now = chrono::Utc::now().timestamp();
    let skew = i64::from(profile.clock_skew_seconds);
    if validate_unix_timestamp(timestamp, now, skew).is_err() {
        return Err(RequestBindingError::timestamp());
    }
    let hmac_key = crypto
        .request_hmac_key(tenant_id, kid)
        .await
        .map_err(|_| RequestBindingError::kid())?
        .ok_or_else(RequestBindingError::kid)?;
    let canonical = canonical_http_binding(method, path_and_query, timestamp, nonce, body);
    if verify_hmac_sha256_base64(&hmac_key, &canonical, signature).is_err() {
        return Err(RequestBindingError::signature());
    }
    let inserted = crypto
        .consume_binding_nonce(nonce_prefix, tenant_id, nonce, profile.nonce_ttl_seconds)
        .await
        .map_err(|_| RequestBindingError::nonce())?;
    if !inserted {
        return Err(RequestBindingError::nonce());
    }
    GATEWAY_REQUEST_BINDING_TOTAL
        .with_label_values(&["ok"])
        .inc();
    Ok(RequestBindingOutcome {
        auth_mode: "bearer+hmac",
        kid: Some(kid.to_string()),
        nonce_trunc: Some(truncate_nonce_for_log(nonce)),
        sig_ok: Some(true),
    })
}

pub fn record_jwt_kid(kid: &str, result: &'static str) {
    GATEWAY_JWT_KID_TOTAL
        .with_label_values(&[kid, result])
        .inc();
}
