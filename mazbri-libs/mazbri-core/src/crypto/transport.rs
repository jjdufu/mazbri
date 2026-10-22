use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub const DATA_ALGORITHM: &str = "AES-256-GCM";
pub const SIGNATURE_ALGORITHM: &str = "HMAC-SHA256";
pub const TENANT_SECRET_KEY_BYTES: usize = 32;
pub const AES_GCM_IV_BYTES: usize = 12;
pub const AES_GCM_TAG_BYTES: usize = 16;
pub const NONCE_BYTES: usize = 16;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TransportEnvelope {
    pub encrypted: bool,
    pub key_version: String,
    pub algorithm: String,
    pub signature_algorithm: String,
    pub timestamp: i64,
    pub nonce: String,
    pub iv: String,
    pub ciphertext: String,
    pub tag: String,
    pub signature: String,
}

#[derive(Debug, Clone)]
pub struct EncryptionParts {
    pub iv: String,
    pub ciphertext: String,
    pub tag: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TransportCryptoError {
    #[error("invalid key length")]
    InvalidKeyLength,
    #[error("invalid iv")]
    InvalidIv,
    #[error("invalid ciphertext or tag")]
    InvalidCiphertext,
    #[error("encrypt failed")]
    EncryptFailed,
    #[error("decrypt failed")]
    DecryptFailed,
    #[error("signature mismatch")]
    SignatureMismatch,
    #[error("invalid hex signature")]
    InvalidSignature,
    #[error("unsupported algorithm")]
    UnsupportedAlgorithm,
}

#[must_use]
pub fn canonical_signature_payload(
    envelope: &TransportEnvelope,
    tenant_id: &str,
    http_path: &str,
) -> String {
    let timestamp = envelope.timestamp.to_string();
    let mut payload = String::with_capacity(
        tenant_id.len()
            + http_path.len()
            + envelope.key_version.len()
            + timestamp.len()
            + envelope.nonce.len()
            + envelope.iv.len()
            + envelope.ciphertext.len()
            + envelope.tag.len()
            + 7,
    );
    payload.push_str(tenant_id);
    payload.push('\n');
    payload.push_str(http_path);
    payload.push('\n');
    payload.push_str(&envelope.key_version);
    payload.push('\n');
    payload.push_str(&timestamp);
    payload.push('\n');
    payload.push_str(&envelope.nonce);
    payload.push('\n');
    payload.push_str(&envelope.iv);
    payload.push('\n');
    payload.push_str(&envelope.ciphertext);
    payload.push('\n');
    payload.push_str(&envelope.tag);
    payload
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn sign_payload(key: &[u8], payload: &str) -> Result<String, TransportCryptoError> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key)
        .map_err(|_| TransportCryptoError::InvalidKeyLength)?;
    mac.update(payload.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn verify_payload_signature(
    key: &[u8],
    payload: &str,
    signature_hex: &str,
) -> Result<(), TransportCryptoError> {
    let expected = sign_payload(key, payload)?;
    let expected_bytes =
        hex::decode(expected).map_err(|_| TransportCryptoError::InvalidSignature)?;
    let actual_bytes =
        hex::decode(signature_hex).map_err(|_| TransportCryptoError::InvalidSignature)?;
    if expected_bytes.len() != actual_bytes.len() {
        return Err(TransportCryptoError::SignatureMismatch);
    }
    let mut diff = 0u8;
    for (left, right) in expected_bytes.iter().zip(actual_bytes.iter()) {
        diff |= left ^ right;
    }
    if diff == 0 {
        Ok(())
    } else {
        Err(TransportCryptoError::SignatureMismatch)
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn encrypt_aes_256_gcm(
    key: &[u8],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<EncryptionParts, TransportCryptoError> {
    if key.len() != TENANT_SECRET_KEY_BYTES {
        return Err(TransportCryptoError::InvalidKeyLength);
    }
    let iv: [u8; AES_GCM_IV_BYTES] = rand::random();
    encrypt_aes_256_gcm_with_iv(key, &iv, plaintext, aad)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn encrypt_aes_256_gcm_with_iv(
    key: &[u8],
    iv: &[u8],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<EncryptionParts, TransportCryptoError> {
    if key.len() != TENANT_SECRET_KEY_BYTES {
        return Err(TransportCryptoError::InvalidKeyLength);
    }
    if iv.len() != AES_GCM_IV_BYTES {
        return Err(TransportCryptoError::InvalidIv);
    }
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| TransportCryptoError::InvalidKeyLength)?;
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(iv),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| TransportCryptoError::EncryptFailed)?;
    let split_at = encrypted
        .len()
        .checked_sub(AES_GCM_TAG_BYTES)
        .ok_or(TransportCryptoError::EncryptFailed)?;
    let (ciphertext, tag) = encrypted.split_at(split_at);
    Ok(EncryptionParts {
        iv: STANDARD.encode(iv),
        ciphertext: STANDARD.encode(ciphertext),
        tag: STANDARD.encode(tag),
    })
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn decrypt_aes_256_gcm(
    key: &[u8],
    iv_b64: &str,
    ciphertext_b64: &str,
    tag_b64: &str,
    aad: &[u8],
) -> Result<Vec<u8>, TransportCryptoError> {
    if key.len() != TENANT_SECRET_KEY_BYTES {
        return Err(TransportCryptoError::InvalidKeyLength);
    }
    let iv = STANDARD
        .decode(iv_b64)
        .map_err(|_| TransportCryptoError::InvalidIv)?;
    if iv.len() != AES_GCM_IV_BYTES {
        return Err(TransportCryptoError::InvalidIv);
    }
    let ciphertext = STANDARD
        .decode(ciphertext_b64)
        .map_err(|_| TransportCryptoError::InvalidCiphertext)?;
    let tag = STANDARD
        .decode(tag_b64)
        .map_err(|_| TransportCryptoError::InvalidCiphertext)?;
    if tag.len() != AES_GCM_TAG_BYTES {
        return Err(TransportCryptoError::InvalidCiphertext);
    }
    let mut combined = ciphertext;
    combined.extend_from_slice(&tag);
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| TransportCryptoError::InvalidKeyLength)?;
    cipher
        .decrypt(
            Nonce::from_slice(&iv),
            Payload {
                msg: &combined,
                aad,
            },
        )
        .map_err(|_| TransportCryptoError::DecryptFailed)
}

#[must_use]
pub fn generate_nonce_base64url() -> String {
    let nonce: [u8; NONCE_BYTES] = rand::random();
    URL_SAFE_NO_PAD.encode(nonce)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_transport_algorithms(
    envelope: &TransportEnvelope,
) -> Result<(), TransportCryptoError> {
    if !envelope.encrypted
        || envelope.algorithm != DATA_ALGORITHM
        || envelope.signature_algorithm != SIGNATURE_ALGORITHM
    {
        return Err(TransportCryptoError::UnsupportedAlgorithm);
    }
    Ok(())
}
