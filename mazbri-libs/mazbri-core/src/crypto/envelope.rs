use serde::{Deserialize, Serialize};

pub const TENANT_KEY_ENVELOPE_FORMAT: &str = "mazbri-tenant-key-envelope-v1";
pub const AES_256_GCM: &str = "AES-256-GCM";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TenantKeyEnvelope {
    pub format: String,
    pub wrapping_key_id: String,
    pub algorithm: String,
    pub iv: String,
    pub ciphertext: String,
    pub tag: String,
    pub aad: String,
}

impl TenantKeyEnvelope {
    pub fn new(
        wrapping_key_id: impl Into<String>,
        iv: impl Into<String>,
        ciphertext: impl Into<String>,
        tag: impl Into<String>,
        aad: impl Into<String>,
    ) -> Self {
        Self {
            format: TENANT_KEY_ENVELOPE_FORMAT.to_string(),
            wrapping_key_id: wrapping_key_id.into(),
            algorithm: AES_256_GCM.to_string(),
            iv: iv.into(),
            ciphertext: ciphertext.into(),
            tag: tag.into(),
            aad: aad.into(),
        }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), CryptoEnvelopeError> {
        if self.format != TENANT_KEY_ENVELOPE_FORMAT {
            return Err(CryptoEnvelopeError::UnsupportedFormat);
        }
        if self.algorithm != AES_256_GCM {
            return Err(CryptoEnvelopeError::UnsupportedAlgorithm);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CryptoEnvelopeError {
    #[error("unsupported tenant key envelope format")]
    UnsupportedFormat,
    #[error("unsupported tenant key envelope algorithm")]
    UnsupportedAlgorithm,
    #[error("invalid tenant key envelope json: {0}")]
    InvalidJson(#[from] serde_json::Error),
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn serialize_envelope(envelope: &TenantKeyEnvelope) -> Result<String, CryptoEnvelopeError> {
    envelope.validate()?;
    Ok(serde_json::to_string(envelope)?)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn parse_envelope(value: &str) -> Result<TenantKeyEnvelope, CryptoEnvelopeError> {
    let envelope: TenantKeyEnvelope = serde_json::from_str(value)?;
    envelope.validate()?;
    Ok(envelope)
}
