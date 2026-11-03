//! 租户密钥包装信封：算法字段走注册表 kebab-case（无硬编码 AES 唯一性）。

use mazbri_crypto::id::AlgorithmId;
use mazbri_crypto::parse_algorithm_id;
use mazbri_crypto::registry::AlgorithmRegistry;
use serde::{Deserialize, Serialize};

/// 租户密钥信封格式标识。
pub const TENANT_KEY_ENVELOPE_FORMAT: &str = "mazbri-tenant-key-envelope-v1";

/// 默认包装算法（推荐 AEAD）。
pub const DEFAULT_WRAPPING_ALGORITHM: &str = "aes-256-gcm";

/// 落库用的租户密钥包装信封。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TenantKeyEnvelope {
    /// 格式版本字符串。
    pub format: String,
    /// 包装密钥标识。
    pub wrapping_key_id: String,
    /// 包装算法（kebab-case registry id）。
    pub algorithm: String,
    /// IV（标准 base64）。
    pub iv: String,
    /// 密文（标准 base64）。
    pub ciphertext: String,
    /// AEAD tag（标准 base64）。
    pub tag: String,
    /// 附加认证数据（明文约定字符串）。
    pub aad: String,
}

impl TenantKeyEnvelope {
    /// 构造信封；`algorithm` 须为注册表 AEAD id。
    pub fn new(
        wrapping_key_id: impl Into<String>,
        algorithm: impl Into<String>,
        iv: impl Into<String>,
        ciphertext: impl Into<String>,
        tag: impl Into<String>,
        aad: impl Into<String>,
    ) -> Self {
        Self {
            format: TENANT_KEY_ENVELOPE_FORMAT.to_owned(),
            wrapping_key_id: wrapping_key_id.into(),
            algorithm: algorithm.into(),
            iv: iv.into(),
            ciphertext: ciphertext.into(),
            tag: tag.into(),
            aad: aad.into(),
        }
    }

    /// 使用默认 `aes-256-gcm` 包装算法构造。
    pub fn new_aes256_gcm(
        wrapping_key_id: impl Into<String>,
        iv: impl Into<String>,
        ciphertext: impl Into<String>,
        tag: impl Into<String>,
        aad: impl Into<String>,
    ) -> Self {
        Self::new(
            wrapping_key_id,
            AlgorithmId::Aes256Gcm.as_str(),
            iv,
            ciphertext,
            tag,
            aad,
        )
    }

    /// 校验格式，并以注册表确认 `algorithm` 为已登记 AEAD。
    ///
    /// # Errors
    ///
    /// 格式或算法不支持时返回 [`CryptoEnvelopeError`]。
    pub fn validate(&self, registry: &AlgorithmRegistry) -> Result<(), CryptoEnvelopeError> {
        if self.format != TENANT_KEY_ENVELOPE_FORMAT {
            return Err(CryptoEnvelopeError::UnsupportedFormat);
        }
        let id = parse_algorithm_id(&self.algorithm).map_err(|_| {
            CryptoEnvelopeError::UnsupportedAlgorithm
        })?;
        registry
            .aead(id)
            .map_err(|_| CryptoEnvelopeError::UnsupportedAlgorithm)?;
        Ok(())
    }
}

/// 信封编解码错误。
#[derive(Debug, thiserror::Error)]
pub enum CryptoEnvelopeError {
    /// 格式字段不识别。
    #[error("unsupported tenant key envelope format")]
    UnsupportedFormat,
    /// 算法非闭集 AEAD 或不在注册表。
    #[error("unsupported tenant key envelope algorithm")]
    UnsupportedAlgorithm,
    /// JSON 非法。
    #[error("invalid tenant key envelope json: {0}")]
    InvalidJson(#[from] serde_json::Error),
}

/// 序列化前校验信封。
///
/// # Errors
///
/// 校验或序列化失败时返回错误。
pub fn serialize_envelope(
    envelope: &TenantKeyEnvelope,
    registry: &AlgorithmRegistry,
) -> Result<String, CryptoEnvelopeError> {
    envelope.validate(registry)?;
    Ok(serde_json::to_string(envelope)?)
}

/// 反序列化并校验信封。
///
/// # Errors
///
/// JSON 或算法校验失败时返回错误。
pub fn parse_envelope(
    value: &str,
    registry: &AlgorithmRegistry,
) -> Result<TenantKeyEnvelope, CryptoEnvelopeError> {
    let envelope: TenantKeyEnvelope = serde_json::from_str(value)?;
    envelope.validate(registry)?;
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_uppercase_wrapping_alg() {
        let registry = AlgorithmRegistry::default_registry();
        let envelope = TenantKeyEnvelope::new(
            "env:KEY",
            "AES-256-GCM",
            "iv",
            "ct",
            "tag",
            "aad",
        );
        assert!(matches!(
            envelope.validate(&registry),
            Err(CryptoEnvelopeError::UnsupportedAlgorithm)
        ));
    }

    #[test]
    fn accepts_aes_256_gcm_kebab() {
        let registry = AlgorithmRegistry::default_registry();
        let envelope = TenantKeyEnvelope::new_aes256_gcm("env:KEY", "iv", "ct", "tag", "aad");
        envelope.validate(&registry).unwrap();
        assert_eq!(envelope.algorithm, DEFAULT_WRAPPING_ALGORITHM);
    }
}
