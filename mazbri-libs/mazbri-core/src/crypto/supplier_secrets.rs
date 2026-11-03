//! 供应商 `auth_flow.credentials` 落库加密（L2c）。
//!
//! - 密文格式：`enc:v2:<kebab-alg>:` + base64(nonce ‖ ciphertext‖tag)
//! - 算法经 `mazbri-crypto` 注册表解析；默认 `aes-256-gcm`
//! - 密钥：环境变量 `MAZBRI_SUPPLIER_SECRETS_KEY`（64 hex 或标准 base64 32 字节）
//! - 未设置时：开发默认派生密钥（带 warn，仅便于自测）
//! - **破坏性**：旧前缀密文一律拒绝；须重置凭据环境，无双读兼容

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use mazbri_crypto::aead::{AEAD_NONCE_LEN, AEAD_TAG_LEN};
use mazbri_crypto::id::AlgorithmId;
use mazbri_crypto::material::{KeyPurpose, KeyRef};
use mazbri_crypto::parse_algorithm_id;
use mazbri_crypto::registry::AlgorithmRegistry;
use mazbri_crypto::traits::AeadSeal;
use rand::Rng;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use thiserror::Error;

/// 落库密文前缀（其后为 `<alg>:<b64>`）。
pub const ENCRYPTED_PREFIX: &str = "enc:v2:";

/// 默认落库 AEAD（与 init-dbs wrapping 推荐默认一致）。
pub const DEFAULT_SECRET_ALGORITHM: &str = "aes-256-gcm";

const ENV_KEY: &str = "MAZBRI_SUPPLIER_SECRETS_KEY";
const DEV_SEED: &str = "mazbri-dev-supplier-secrets-key-v1-do-not-use-in-prod";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SupplierSecretsError {
    #[error("{0}")]
    Message(String),
}

impl SupplierSecretsError {
    fn msg(s: impl Into<String>) -> Self {
        Self::Message(s.into())
    }
}

/// 当前进程使用的 32 字节密钥（懒加载）。
///
/// # Errors
///
/// 密钥材料非法或环境变量为空时返回错误。
pub fn supplier_secrets_key() -> Result<&'static [u8; 32], SupplierSecretsError> {
    static KEY: OnceLock<Result<[u8; 32], String>> = OnceLock::new();
    match KEY.get_or_init(load_key_once) {
        Ok(k) => Ok(k),
        Err(e) => Err(SupplierSecretsError::msg(e.clone())),
    }
}

fn load_key_once() -> Result<[u8; 32], String> {
    if let Ok(raw) = std::env::var(ENV_KEY) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(format!("{ENV_KEY} is empty"));
        }
        parse_key_material(trimmed)
    } else {
        tracing::warn!(
            env = ENV_KEY,
            "MAZBRI_SUPPLIER_SECRETS_KEY not set; using development-derived key (not for production)"
        );
        let digest = Sha256::digest(DEV_SEED.as_bytes());
        let mut key = [0u8; 32];
        key.copy_from_slice(&digest);
        Ok(key)
    }
}

fn parse_key_material(s: &str) -> Result<[u8; 32], String> {
    if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        let bytes = hex::decode(s).map_err(|e| format!("invalid hex key: {e}"))?;
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }
    if let Ok(bytes) = B64.decode(s.as_bytes())
        && bytes.len() == 32
    {
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }
    // 任意口令：SHA-256 派生
    let digest = Sha256::digest(s.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    Ok(key)
}

#[must_use]
pub fn is_encrypted_secret(value: &str) -> bool {
    value.starts_with(ENCRYPTED_PREFIX)
}

fn reject_legacy_at_rest_prefix(value: &str) -> Result<(), SupplierSecretsError> {
    if value.starts_with("enc:") && !is_encrypted_secret(value) {
        return Err(SupplierSecretsError::msg(
            "unsupported at-rest secret format; re-encrypt credentials after destructive reset",
        ));
    }
    Ok(())
}

fn resolve_aead(
    registry: &AlgorithmRegistry,
    alg_raw: &str,
) -> Result<(AlgorithmId, mazbri_crypto::AeadAlgorithm), SupplierSecretsError> {
    let id = parse_algorithm_id(alg_raw).map_err(|_| {
        SupplierSecretsError::msg(format!("unsupported supplier-secret algorithm: {alg_raw}"))
    })?;
    let aead = registry.aead(id).map_err(|_| {
        SupplierSecretsError::msg(format!("algorithm is not a registered AEAD: {alg_raw}"))
    })?;
    Ok((id, aead))
}

/// 使用默认 `aes-256-gcm` 加密明文凭据。
///
/// # Errors
///
/// 密钥/算法非法或密封失败时返回错误；遇到旧前缀密文时拒绝（无双读）。
pub fn encrypt_secret(plaintext: &str, key: &[u8; 32]) -> Result<String, SupplierSecretsError> {
    encrypt_secret_with_alg(plaintext, key, AlgorithmId::Aes256Gcm)
}

/// 指定注册表 AEAD id 加密明文凭据。
///
/// # Errors
///
/// 密钥长度与算法不匹配、密封失败、或输入已是不受支持的 `enc:` 前缀时返回错误。
pub fn encrypt_secret_with_alg(
    plaintext: &str,
    key: &[u8; 32],
    alg: AlgorithmId,
) -> Result<String, SupplierSecretsError> {
    if is_encrypted_secret(plaintext) {
        return Ok(plaintext.to_string());
    }
    reject_legacy_at_rest_prefix(plaintext)?;

    let registry = AlgorithmRegistry::default_registry();
    let (_, aead) = resolve_aead(&registry, alg.as_str())?;
    let key_ref = KeyRef::raw(KeyPurpose::KeyWrap, key.to_vec());
    let mut nonce = [0u8; AEAD_NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);
    let packed = aead
        .seal(&key_ref, &nonce, b"", plaintext.as_bytes())
        .map_err(|e| SupplierSecretsError::msg(format!("encrypt credential failed: {e}")))?;
    if packed.len() < AEAD_TAG_LEN {
        return Err(SupplierSecretsError::msg("encrypt credential produced short ciphertext"));
    }
    let mut blob = Vec::with_capacity(AEAD_NONCE_LEN + packed.len());
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&packed);
    Ok(format!(
        "{ENCRYPTED_PREFIX}{}:{}",
        alg.as_str(),
        B64.encode(blob)
    ))
}

/// 解密落库凭据；非 `enc:v2:` 前缀时原样返回（明文兼容）。
///
/// # Errors
///
/// 旧前缀、算法未登记、密文损坏或 AEAD 认证失败时返回错误。
pub fn decrypt_secret(value: &str, key: &[u8; 32]) -> Result<String, SupplierSecretsError> {
    if !is_encrypted_secret(value) {
        reject_legacy_at_rest_prefix(value)?;
        return Ok(value.to_string());
    }
    let rest = &value[ENCRYPTED_PREFIX.len()..];
    let (alg_raw, b64) = rest.split_once(':').ok_or_else(|| {
        SupplierSecretsError::msg("invalid enc:v2 credential format (missing alg)")
    })?;
    if alg_raw.is_empty() || b64.is_empty() {
        return Err(SupplierSecretsError::msg(
            "invalid enc:v2 credential format (empty alg or blob)",
        ));
    }
    let registry = AlgorithmRegistry::default_registry();
    let (_, aead) = resolve_aead(&registry, alg_raw)?;
    let blob = B64
        .decode(b64.as_bytes())
        .map_err(|e| SupplierSecretsError::msg(format!("invalid credential ciphertext: {e}")))?;
    if blob.len() < AEAD_NONCE_LEN + AEAD_TAG_LEN {
        return Err(SupplierSecretsError::msg("credential ciphertext too short"));
    }
    let (nonce, ct) = blob.split_at(AEAD_NONCE_LEN);
    let key_ref = KeyRef::raw(KeyPurpose::KeyWrap, key.to_vec());
    let pt = aead
        .open(&key_ref, nonce, b"", ct)
        .map_err(|_| SupplierSecretsError::msg("decrypt credential failed (wrong key or alg?)"))?;
    String::from_utf8(pt).map_err(|_| SupplierSecretsError::msg("credential plaintext not utf-8"))
}

/// 加密 `auth_flow.credentials` 中尚未加密的值。
///
/// # Errors
///
/// 任一字段加密失败时返回错误。
pub fn encrypt_credentials_map<S: std::hash::BuildHasher>(
    creds: &mut std::collections::HashMap<String, String, S>,
    key: &[u8; 32],
) -> Result<(), SupplierSecretsError> {
    for v in creds.values_mut() {
        if v.is_empty() || is_encrypted_secret(v) {
            continue;
        }
        *v = encrypt_secret(v, key)?;
    }
    Ok(())
}

/// 解密 `auth_flow.credentials`（数据面出站前）。
///
/// # Errors
///
/// 任一字段解密失败时返回错误。
pub fn decrypt_credentials_map<S: std::hash::BuildHasher>(
    creds: &mut std::collections::HashMap<String, String, S>,
    key: &[u8; 32],
) -> Result<(), SupplierSecretsError> {
    for v in creds.values_mut() {
        *v = decrypt_secret(v, key)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_default_aes256_gcm() {
        let key =
            parse_key_material("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
                .unwrap();
        let enc = encrypt_secret("s3cret", &key).unwrap();
        assert!(is_encrypted_secret(&enc));
        assert!(enc.starts_with(&format!("{ENCRYPTED_PREFIX}{DEFAULT_SECRET_ALGORITHM}:")));
        assert_eq!(decrypt_secret(&enc, &key).unwrap(), "s3cret");
        // 幂等
        assert_eq!(encrypt_secret(&enc, &key).unwrap(), enc);
    }

    #[test]
    fn roundtrip_chacha20() {
        let key =
            parse_key_material("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
                .unwrap();
        let enc = encrypt_secret_with_alg("hello", &key, AlgorithmId::Chacha20Poly1305).unwrap();
        assert!(enc.starts_with("enc:v2:chacha20-poly1305:"));
        assert_eq!(decrypt_secret(&enc, &key).unwrap(), "hello");
    }

    #[test]
    fn rejects_unsupported_at_rest_prefix() {
        let key =
            parse_key_material("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
                .unwrap();
        let err = decrypt_secret("enc:legacy:abc", &key).unwrap_err();
        assert!(err.to_string().contains("unsupported at-rest secret format"));
        let err = encrypt_secret("enc:legacy:abc", &key).unwrap_err();
        assert!(err.to_string().contains("unsupported at-rest secret format"));
    }

    #[test]
    fn self_describes_alg_in_ciphertext() {
        let key =
            parse_key_material("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
                .unwrap();
        let enc = encrypt_secret("x", &key).unwrap();
        let rest = enc.strip_prefix(ENCRYPTED_PREFIX).unwrap();
        let (alg, _) = rest.split_once(':').unwrap();
        assert_eq!(alg, "aes-256-gcm");
        let registry = AlgorithmRegistry::default_registry();
        registry.aead(parse_algorithm_id(alg).unwrap()).unwrap();
    }
}
