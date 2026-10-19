use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub const SIGNATURE_INPUT_HMAC_V1: &str = "hmac-sha256=v1";
pub const NONCE_MIN_LEN: usize = 16;
pub const NONCE_MAX_LEN: usize = 128;
pub const DEFAULT_CLOCK_SKEW_SECONDS: i64 = 300;
pub const DEFAULT_NONCE_TTL_SECONDS: i32 = 300;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RequestBindingError {
    #[error("invalid hmac key length")]
    InvalidKeyLength,
    #[error("invalid nonce")]
    InvalidNonce,
    #[error("invalid timestamp")]
    InvalidTimestamp,
    #[error("timestamp outside allowed skew")]
    TimestampSkew,
    #[error("invalid signature encoding")]
    InvalidSignatureEncoding,
    #[error("signature mismatch")]
    SignatureMismatch,
    #[error("unsupported signature input")]
    UnsupportedSignatureInput,
}

pub fn canonical_http_binding(
    method: &str,
    path_and_query: &str,
    timestamp: i64,
    nonce: &str,
    body: &[u8],
) -> String {
    let mut payload = String::with_capacity(
        method.len() + path_and_query.len() + 20 + nonce.len() + body.len() + 4,
    );
    payload.push_str(method);
    payload.push('\n');
    payload.push_str(path_and_query);
    payload.push('\n');
    payload.push_str(&timestamp.to_string());
    payload.push('\n');
    payload.push_str(nonce);
    payload.push('\n');
    payload.push_str(&String::from_utf8_lossy(body));
    payload
}

pub fn path_and_query_from_parts(path: &str, query: Option<&str>) -> String {
    match query {
        Some(q) if !q.is_empty() => format!("{path}?{q}"),
        _ => path.to_string(),
    }
}

pub fn validate_binding_nonce(nonce: &str) -> Result<(), RequestBindingError> {
    if nonce.len() < NONCE_MIN_LEN || nonce.len() > NONCE_MAX_LEN {
        return Err(RequestBindingError::InvalidNonce);
    }
    if !nonce
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
    {
        return Err(RequestBindingError::InvalidNonce);
    }
    Ok(())
}

pub fn parse_unix_timestamp(value: &str) -> Result<i64, RequestBindingError> {
    value
        .parse::<i64>()
        .map_err(|_| RequestBindingError::InvalidTimestamp)
}

pub fn validate_unix_timestamp(
    timestamp: i64,
    now_unix: i64,
    clock_skew_seconds: i64,
) -> Result<(), RequestBindingError> {
    if timestamp <= 0 {
        return Err(RequestBindingError::InvalidTimestamp);
    }
    let skew = clock_skew_seconds.max(1);
    if (now_unix - timestamp).abs() > skew {
        return Err(RequestBindingError::TimestampSkew);
    }
    Ok(())
}

pub fn validate_signature_input(value: &str) -> Result<(), RequestBindingError> {
    if value == SIGNATURE_INPUT_HMAC_V1 {
        Ok(())
    } else {
        Err(RequestBindingError::UnsupportedSignatureInput)
    }
}

pub fn sign_hmac_sha256_base64(key: &[u8], payload: &str) -> Result<String, RequestBindingError> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key)
        .map_err(|_| RequestBindingError::InvalidKeyLength)?;
    mac.update(payload.as_bytes());
    Ok(STANDARD.encode(mac.finalize().into_bytes()))
}

pub fn verify_hmac_sha256_base64(
    key: &[u8],
    payload: &str,
    signature_b64: &str,
) -> Result<(), RequestBindingError> {
    let expected = sign_hmac_sha256_base64(key, payload)?;
    let expected_bytes = STANDARD
        .decode(expected.as_bytes())
        .map_err(|_| RequestBindingError::InvalidSignatureEncoding)?;
    let actual_bytes = STANDARD
        .decode(signature_b64.trim().as_bytes())
        .map_err(|_| RequestBindingError::InvalidSignatureEncoding)?;
    if expected_bytes.len() != actual_bytes.len() {
        return Err(RequestBindingError::SignatureMismatch);
    }
    let mut diff = 0u8;
    for (left, right) in expected_bytes.iter().zip(actual_bytes.iter()) {
        diff |= left ^ right;
    }
    if diff == 0 {
        Ok(())
    } else {
        Err(RequestBindingError::SignatureMismatch)
    }
}

pub fn generate_binding_nonce() -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let nonce: [u8; 16] = rand::random();
    URL_SAFE_NO_PAD.encode(nonce)
}

pub fn truncate_nonce_for_log(nonce: &str) -> String {
    const KEEP: usize = 8;
    if nonce.len() <= KEEP {
        nonce.to_string()
    } else {
        format!("{}…", &nonce[..KEEP])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_matches_scheme_vector() {
        let payload = canonical_http_binding(
            "POST",
            "/api/v1/gateway",
            1710000000,
            &"n".repeat(16),
            br#"{"product_id":"p1","path":"/x"}"#,
        );
        let nonce = "n".repeat(16);
        assert_eq!(
            payload,
            format!(
                "POST\n/api/v1/gateway\n1710000000\n{nonce}\n{{\"product_id\":\"p1\",\"path\":\"/x\"}}"
            )
        );
    }

    #[test]
    fn hmac_roundtrip_and_body_tamper() {
        let key = [7u8; 32];
        let nonce = "abcdefABCDEF12-_";
        let body = br#"{"product_id":"p1"}"#;
        let canonical = canonical_http_binding("POST", "/api/v1/gateway", 1710000000, nonce, body);
        let sig = sign_hmac_sha256_base64(&key, &canonical).unwrap();
        verify_hmac_sha256_base64(&key, &canonical, &sig).unwrap();
        let tampered = canonical_http_binding(
            "POST",
            "/api/v1/gateway",
            1710000000,
            nonce,
            br#"{"product_id":"p2"}"#,
        );
        assert_eq!(
            verify_hmac_sha256_base64(&key, &tampered, &sig),
            Err(RequestBindingError::SignatureMismatch)
        );
    }

    #[test]
    fn nonce_charset_and_length() {
        assert!(validate_binding_nonce("abcdefghijklmnop").is_ok());
        assert!(validate_binding_nonce("ab").is_err());
        assert!(validate_binding_nonce(&"a".repeat(129)).is_err());
        assert!(validate_binding_nonce("bad nonce spaces!!").is_err());
    }

    #[test]
    fn timestamp_skew_window() {
        validate_unix_timestamp(1000, 1000, 300).unwrap();
        validate_unix_timestamp(700, 1000, 300).unwrap();
        assert_eq!(
            validate_unix_timestamp(699, 1000, 300),
            Err(RequestBindingError::TimestampSkew)
        );
    }
}
