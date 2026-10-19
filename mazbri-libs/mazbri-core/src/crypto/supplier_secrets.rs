//! 供应商 `auth_flow.credentials` 落库加密（G-05）。
//!
//! - 密文格式：`enc:v1:` + base64(nonce[12] || ciphertext||tag)
//! - 密钥：环境变量 `MAZBRI_SUPPLIER_SECRETS_KEY`（64 hex 或标准 base64 32 字节）
//! - 未设置时：开发默认派生密钥（带 warn，仅便于自测）

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use rand::Rng;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use thiserror::Error;

/// 密文前缀（写入 JSON 字符串值）。
pub const ENCRYPTED_PREFIX: &str = "enc:v1:";

const NONCE_LEN: usize = 12;
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
pub fn supplier_secrets_key() -> Result<&'static [u8; 32], SupplierSecretsError> {
    static KEY: OnceLock<Result<[u8; 32], String>> = OnceLock::new();
    match KEY.get_or_init(load_key_once) {
        Ok(k) => Ok(k),
        Err(e) => Err(SupplierSecretsError::msg(e.clone())),
    }
}

fn load_key_once() -> Result<[u8; 32], String> {
    match std::env::var(ENV_KEY) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(format!("{ENV_KEY} is empty"));
            }
            parse_key_material(trimmed)
        }
        Err(_) => {
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
}

fn parse_key_material(s: &str) -> Result<[u8; 32], String> {
    if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        let bytes = hex::decode(s).map_err(|e| format!("invalid hex key: {e}"))?;
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }
    if let Ok(bytes) = B64.decode(s.as_bytes()) {
        if bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
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

pub fn encrypt_secret(plaintext: &str, key: &[u8; 32]) -> Result<String, SupplierSecretsError> {
    if is_encrypted_secret(plaintext) {
        return Ok(plaintext.to_string());
    }
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| SupplierSecretsError::msg("invalid supplier secrets key length"))?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| SupplierSecretsError::msg("encrypt credential failed"))?;
    let mut blob = Vec::with_capacity(NONCE_LEN + ct.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ct);
    Ok(format!("{ENCRYPTED_PREFIX}{}", B64.encode(blob)))
}

pub fn decrypt_secret(value: &str, key: &[u8; 32]) -> Result<String, SupplierSecretsError> {
    if !is_encrypted_secret(value) {
        return Ok(value.to_string());
    }
    let b64 = &value[ENCRYPTED_PREFIX.len()..];
    let blob = B64
        .decode(b64.as_bytes())
        .map_err(|e| SupplierSecretsError::msg(format!("invalid credential ciphertext: {e}")))?;
    if blob.len() < NONCE_LEN + 16 {
        return Err(SupplierSecretsError::msg("credential ciphertext too short"));
    }
    let (nonce_bytes, ct) = blob.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| SupplierSecretsError::msg("invalid supplier secrets key length"))?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct)
        .map_err(|_| SupplierSecretsError::msg("decrypt credential failed (wrong key?)"))?;
    String::from_utf8(pt).map_err(|_| SupplierSecretsError::msg("credential plaintext not utf-8"))
}

/// 加密 `auth_flow.credentials` 中尚未加密的值。
pub fn encrypt_credentials_map(
    creds: &mut std::collections::HashMap<String, String>,
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
pub fn decrypt_credentials_map(
    creds: &mut std::collections::HashMap<String, String>,
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
    fn roundtrip() {
        let key =
            parse_key_material("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
                .unwrap();
        let enc = encrypt_secret("s3cret", &key).unwrap();
        assert!(is_encrypted_secret(&enc));
        assert_eq!(decrypt_secret(&enc, &key).unwrap(), "s3cret");
        // 幂等
        assert_eq!(encrypt_secret(&enc, &key).unwrap(), enc);
    }
}
