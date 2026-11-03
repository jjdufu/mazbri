//! HTTP 请求绑定：MAC 算法可经注册表配置（不再写死仅 hmac-sha256 实现路径）。

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use mazbri_crypto::id::AlgorithmId;
use mazbri_crypto::material::{KeyPurpose, KeyRef};
use mazbri_crypto::parse_algorithm_id;
use mazbri_crypto::registry::AlgorithmRegistry;
use mazbri_crypto::traits::{CryptoError, Mac};

/// 协议档：HMAC 输入格式 v1（非算法 id）。
pub const SIGNATURE_INPUT_HMAC_V1: &str = "hmac-sha256=v1";
/// 默认绑定 MAC 算法 id（kebab-case）。
pub const DEFAULT_BINDING_MAC_ALG: &str = "hmac-sha256";
/// Nonce 最小长度。
pub const NONCE_MIN_LEN: usize = 16;
/// Nonce 最大长度。
pub const NONCE_MAX_LEN: usize = 128;
/// 默认时钟偏移（秒）。
pub const DEFAULT_CLOCK_SKEW_SECONDS: i64 = 300;
/// 默认 nonce TTL（秒）。
pub const DEFAULT_NONCE_TTL_SECONDS: i32 = 300;

/// 请求绑定错误。
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RequestBindingError {
    /// 密钥长度 / 材料非法。
    #[error("invalid hmac key length")]
    InvalidKeyLength,
    /// Nonce 非法。
    #[error("invalid nonce")]
    InvalidNonce,
    /// 时间戳非法。
    #[error("invalid timestamp")]
    InvalidTimestamp,
    /// 超出允许时钟偏移。
    #[error("timestamp outside allowed skew")]
    TimestampSkew,
    /// 签名编码非法。
    #[error("invalid signature encoding")]
    InvalidSignatureEncoding,
    /// 签名不匹配。
    #[error("signature mismatch")]
    SignatureMismatch,
    /// 不支持的 signature-input 协议档。
    #[error("unsupported signature input")]
    UnsupportedSignatureInput,
    /// 不支持的 MAC 算法 id。
    #[error("unsupported mac algorithm")]
    UnsupportedMacAlgorithm,
}

#[must_use]
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

#[must_use]
pub fn path_and_query_from_parts(path: &str, query: Option<&str>) -> String {
    match query {
        Some(q) if !q.is_empty() => format!("{path}?{q}"),
        _ => path.to_owned(),
    }
}

/// # Errors
///
/// Nonce 长度或字符集非法时返回错误。
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

/// # Errors
///
/// 非整数时返回错误。
pub fn parse_unix_timestamp(value: &str) -> Result<i64, RequestBindingError> {
    value
        .parse::<i64>()
        .map_err(|_| RequestBindingError::InvalidTimestamp)
}

/// # Errors
///
/// 时间戳非正或超出偏移时返回错误。
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

/// # Errors
///
/// 非已知协议档时返回错误。
pub fn validate_signature_input(value: &str) -> Result<(), RequestBindingError> {
    if value == SIGNATURE_INPUT_HMAC_V1 {
        Ok(())
    } else {
        Err(RequestBindingError::UnsupportedSignatureInput)
    }
}

fn resolve_binding_mac(
    registry: &AlgorithmRegistry,
    mac_alg: AlgorithmId,
) -> Result<mazbri_crypto::MacAlgorithm, RequestBindingError> {
    registry
        .mac(mac_alg)
        .map_err(|_| RequestBindingError::UnsupportedMacAlgorithm)
}

/// 使用注册表 MAC 对绑定载荷签名，返回标准 base64。
///
/// # Errors
///
/// 算法未登记或密钥非法时返回错误。
pub fn sign_binding_mac(
    registry: &AlgorithmRegistry,
    mac_alg: AlgorithmId,
    key: &[u8],
    payload: &str,
) -> Result<String, RequestBindingError> {
    let mac = resolve_binding_mac(registry, mac_alg)?;
    let key_ref = KeyRef::raw(KeyPurpose::Mac, key.to_vec());
    let tag = mac.sign(&key_ref, payload.as_bytes()).map_err(|err| match err {
        CryptoError::InvalidKey { .. } => RequestBindingError::InvalidKeyLength,
        _ => RequestBindingError::UnsupportedMacAlgorithm,
    })?;
    Ok(STANDARD.encode(tag))
}

/// 校验绑定 MAC（标准 base64）。
///
/// # Errors
///
/// 编码非法或标签不匹配时返回错误。
pub fn verify_binding_mac(
    registry: &AlgorithmRegistry,
    mac_alg: AlgorithmId,
    key: &[u8],
    payload: &str,
    signature_b64: &str,
) -> Result<(), RequestBindingError> {
    let mac = resolve_binding_mac(registry, mac_alg)?;
    let key_ref = KeyRef::raw(KeyPurpose::Mac, key.to_vec());
    let actual = STANDARD
        .decode(signature_b64.trim().as_bytes())
        .map_err(|_| RequestBindingError::InvalidSignatureEncoding)?;
    mac.verify(&key_ref, payload.as_bytes(), &actual)
        .map_err(|err| match err {
            CryptoError::AuthenticationFailed => RequestBindingError::SignatureMismatch,
            CryptoError::InvalidKey { .. } => RequestBindingError::InvalidKeyLength,
            _ => RequestBindingError::UnsupportedMacAlgorithm,
        })
}

/// 解析 MAC 算法字符串（kebab-case）。
///
/// # Errors
///
/// 非闭集 id 时返回错误。
pub fn parse_binding_mac_alg(raw: &str) -> Result<AlgorithmId, RequestBindingError> {
    let id = parse_algorithm_id(raw).map_err(|_| RequestBindingError::UnsupportedMacAlgorithm)?;
    if id.kind() != mazbri_crypto::id::AlgorithmKind::Mac {
        return Err(RequestBindingError::UnsupportedMacAlgorithm);
    }
    Ok(id)
}

/// 默认 `hmac-sha256` 签名（base64）。
///
/// # Errors
///
/// 密钥非法时返回错误。
pub fn sign_hmac_sha256_base64(key: &[u8], payload: &str) -> Result<String, RequestBindingError> {
    let registry = AlgorithmRegistry::default_registry();
    sign_binding_mac(&registry, AlgorithmId::HmacSha256, key, payload)
}

/// 默认 `hmac-sha256` 验签（base64）。
///
/// # Errors
///
/// 编码非法或标签不匹配时返回错误。
pub fn verify_hmac_sha256_base64(
    key: &[u8],
    payload: &str,
    signature_b64: &str,
) -> Result<(), RequestBindingError> {
    let registry = AlgorithmRegistry::default_registry();
    verify_binding_mac(
        &registry,
        AlgorithmId::HmacSha256,
        key,
        payload,
        signature_b64,
    )
}

#[must_use]
pub fn generate_binding_nonce() -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let nonce: [u8; 16] = rand::random();
    URL_SAFE_NO_PAD.encode(nonce)
}

#[must_use]
pub fn truncate_nonce_for_log(nonce: &str) -> String {
    const KEEP: usize = 8;
    if nonce.len() <= KEEP {
        nonce.to_owned()
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
            1_710_000_000,
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
        let canonical =
            canonical_http_binding("POST", "/api/v1/gateway", 1_710_000_000, nonce, body);
        let sig = sign_hmac_sha256_base64(&key, &canonical).unwrap();
        verify_hmac_sha256_base64(&key, &canonical, &sig).unwrap();
        let tampered = canonical_http_binding(
            "POST",
            "/api/v1/gateway",
            1_710_000_000,
            nonce,
            br#"{"product_id":"p2"}"#,
        );
        assert_eq!(
            verify_hmac_sha256_base64(&key, &tampered, &sig),
            Err(RequestBindingError::SignatureMismatch)
        );
    }

    #[test]
    fn binding_mac_hmac_sm3_roundtrip() {
        let registry = AlgorithmRegistry::default_registry();
        let key = [9u8; 32];
        let payload = "POST\n/x\n1\nnonce-value-16ch\n{}";
        let sig = sign_binding_mac(&registry, AlgorithmId::HmacSm3, &key, payload).unwrap();
        verify_binding_mac(&registry, AlgorithmId::HmacSm3, &key, payload, &sig).unwrap();
    }

    #[test]
    fn parse_binding_mac_rejects_uppercase() {
        assert_eq!(
            parse_binding_mac_alg("HMAC-SHA256"),
            Err(RequestBindingError::UnsupportedMacAlgorithm)
        );
        assert_eq!(
            parse_binding_mac_alg("hmac-sha256").unwrap(),
            AlgorithmId::HmacSha256
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
