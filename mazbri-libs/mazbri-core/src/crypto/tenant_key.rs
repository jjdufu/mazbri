//! 租户传输密钥生成 / 包装 / 摘要：经 `mazbri-crypto` + Profile 默认套件。

use super::envelope::{
    DEFAULT_WRAPPING_ALGORITHM, TenantKeyEnvelope, parse_envelope, serialize_envelope,
};
use super::supplier_secrets::RootMaterialKeyring;
use super::transport::{
    TENANT_SECRET_KEY_BYTES, TransportCipher, decrypt_with_alg, default_data_alg, default_suite,
    encrypt_with_alg, sign_payload,
};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use mazbri_crypto::id::AlgorithmId;
use mazbri_crypto::registry::AlgorithmRegistry;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// 商业根保护包装密钥 id（环境变量引用形态）。
pub const COMMERCIAL_ROOT_PROTECTION_WRAPPING_KEY_ID: &str =
    "env:MAZBRI_COMMERCIAL_ROOT_PROTECTION_MASTER_KEY";
/// 主包装密钥环境变量名。
pub const COMMERCIAL_ROOT_PROTECTION_MASTER_KEY_ENV: &str =
    "MAZBRI_COMMERCIAL_ROOT_PROTECTION_MASTER_KEY";
/// 完整性 HMAC 密钥环境变量名。
pub const COMMERCIAL_ROOT_PROTECTION_INTEGRITY_KEY_ENV: &str =
    "MAZBRI_COMMERCIAL_ROOT_PROTECTION_INTEGRITY_KEY";

/// 默认完整性 MAC（与默认套件配套的外层完整性习惯对齐）。
const DEFAULT_INTEGRITY_MAC: AlgorithmId = AlgorithmId::HmacSha256;

/// 租户密钥错误。
#[derive(Debug, thiserror::Error)]
pub enum TenantKeyError {
    /// 主密钥未配置。
    #[error("tenant crypto master key is not configured")]
    MasterKeyMissing,
    /// 完整性密钥未配置。
    #[error("tenant crypto integrity key is not configured")]
    IntegrityKeyMissing,
    /// 密钥长度非法。
    #[error("tenant crypto key material must be 32 bytes")]
    InvalidKeyLength,
    /// 根材料请求失败。
    #[error("tenant root material request failed: {0}")]
    RootMaterialRequestFailed(String),
    /// 信封错误。
    #[error("tenant key envelope error: {0}")]
    Envelope(#[from] super::envelope::CryptoEnvelopeError),
    /// 传输密码学错误。
    #[error("tenant transport crypto error: {0}")]
    Transport(#[from] super::transport::TransportCryptoError),
    /// 编码非法。
    #[error("tenant key encoding is invalid")]
    InvalidEncoding,
}

/// 商业根保护材料包。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootProtectionMaterialBundle {
    /// 密钥版本号（如 `v1`、`v2`）。
    pub current_key_version: String,
    /// 包装密钥 id。
    pub wrapping_key_id: String,
    /// 主密钥材料（Base64 32 字节）。
    pub master_key: String,
    /// 完整性密钥材料（Base64 32 字节）。
    pub integrity_key: String,
}

/// 商业根保护多版本密钥环响应（`GET /authorizations/root/materials/keyring`）。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootProtectionKeyring {
    /// 当前可用于新加密/新包装的密钥版本（仅当 `issuance_allowed` 为 `true` 时非空）。
    pub current_key_version: Option<String>,
    /// 当前许可证是否允许执行新签发与新密钥密封。
    pub issuance_allowed: bool,
    /// 全部可解密根材料版本（`active` + `deprecated`）。
    pub materials: Vec<RootProtectionMaterialBundle>,
}

/// 新包装的租户密钥材料。
#[derive(Debug, Clone)]
pub struct WrappedTenantKey {
    /// 明文租户密钥（标准 base64），仅下发瞬时使用。
    pub tenant_secret_key_b64: String,
    /// 包装后的信封 JSON。
    pub encrypted_secret_key: String,
    /// 完整性摘要（hex HMAC）。
    pub secret_key_digest: String,
    /// 包装密钥 id。
    pub wrapping_key_id: String,
}

#[must_use]
pub fn generate_tenant_secret_key() -> [u8; TENANT_SECRET_KEY_BYTES] {
    rand::random()
}

/// 按数据算法长度生成密钥字节。
///
/// # Errors
///
/// 算法无确定密钥长度时返回 [`TenantKeyError::Transport`]。
pub fn generate_data_key_for(data_alg: AlgorithmId) -> Result<Vec<u8>, TenantKeyError> {
    let len = super::transport::data_key_len(data_alg)?;
    Ok((0..len).map(|_| rand::random::<u8>()).collect())
}

#[must_use]
pub fn generate_key_version(existing_versions: &[String]) -> String {
    let max = existing_versions
        .iter()
        .filter_map(|version| version.strip_prefix('v'))
        .filter_map(|number| number.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("v{}", max.saturating_add(1))
}

/// # Errors
///
/// 环境变量缺失或材料非法时返回错误。
pub fn load_master_key_from_env() -> Result<Vec<u8>, TenantKeyError> {
    load_key_from_env(COMMERCIAL_ROOT_PROTECTION_MASTER_KEY_ENV).map_err(|error| match error {
        TenantKeyError::InvalidEncoding | TenantKeyError::InvalidKeyLength => error,
        _ => TenantKeyError::MasterKeyMissing,
    })
}

/// # Errors
///
/// 环境变量缺失或材料非法时返回错误。
pub fn load_integrity_key_from_env() -> Result<Vec<u8>, TenantKeyError> {
    load_key_from_env(COMMERCIAL_ROOT_PROTECTION_INTEGRITY_KEY_ENV).map_err(|error| match error {
        TenantKeyError::InvalidEncoding | TenantKeyError::InvalidKeyLength => error,
        _ => TenantKeyError::IntegrityKeyMissing,
    })
}

/// # Errors
///
/// HTTP / 解析失败时返回错误。
pub async fn fetch_root_protection_material_bundle(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
) -> Result<RootProtectionMaterialBundle, TenantKeyError> {
    let url = format!(
        "{}/authorizations/root/current/materials",
        base_url.trim_end_matches('/')
    );
    let request = client.get(url).bearer_auth(token);
    let response = request.send().await.map_err(|error| {
        TenantKeyError::RootMaterialRequestFailed(format!(
            "failed to query commercial root protection materials: {error}"
        ))
    })?;
    if !response.status().is_success() {
        return Err(TenantKeyError::RootMaterialRequestFailed(format!(
            "commercial root protection materials request failed with {}",
            response.status()
        )));
    }
    response
        .json::<RootProtectionMaterialBundle>()
        .await
        .map_err(|error| {
            TenantKeyError::RootMaterialRequestFailed(format!(
                "failed to parse commercial root protection materials: {error}"
            ))
        })
}

/// 拉取完整多版本根材料密钥环（`GET /authorizations/root/materials/keyring`）。
///
/// # Errors
///
/// HTTP 请求失败或 JSON 响应解析失败时返回错误。
pub async fn fetch_root_protection_keyring(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
) -> Result<RootProtectionKeyring, TenantKeyError> {
    let url = format!(
        "{}/authorizations/root/materials/keyring",
        base_url.trim_end_matches('/')
    );
    let response = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| {
            TenantKeyError::RootMaterialRequestFailed(format!(
                "failed to query commercial root protection keyring: {error}"
            ))
        })?;
    if !response.status().is_success() {
        return Err(TenantKeyError::RootMaterialRequestFailed(format!(
            "commercial root protection keyring request failed with {}",
            response.status()
        )));
    }
    response
        .json::<RootProtectionKeyring>()
        .await
        .map_err(|error| {
            TenantKeyError::RootMaterialRequestFailed(format!(
                "failed to parse commercial root protection keyring: {error}"
            ))
        })
}

/// # Errors
///
/// 环境变量缺失时返回错误。
pub fn load_key_from_env(name: &str) -> Result<Vec<u8>, TenantKeyError> {
    let value = std::env::var(name).map_err(|_| TenantKeyError::MasterKeyMissing)?;
    decode_configured_key(&value)
}

/// # Errors
///
/// 解码后长度不是 32 字节时返回错误。
pub fn decode_configured_key(value: &str) -> Result<Vec<u8>, TenantKeyError> {
    let trimmed = value.trim();
    let decoded = if let Ok(bytes) = STANDARD.decode(trimmed) {
        bytes
    } else if let Ok(bytes) = URL_SAFE_NO_PAD.decode(trimmed) {
        bytes
    } else if let Ok(bytes) = hex::decode(trimmed) {
        bytes
    } else {
        trimmed.as_bytes().to_vec()
    };
    if decoded.len() != TENANT_SECRET_KEY_BYTES {
        return Err(TenantKeyError::InvalidKeyLength);
    }
    Ok(decoded)
}

/// 生成并包装新的租户密钥（包装算法默认 `aes-256-gcm`）。
///
/// # Errors
///
/// 加密 / 信封 / 摘要失败时返回错误。
pub fn wrap_new_tenant_key(
    tenant_id: &str,
    key_version: &str,
    master_key: &[u8],
    integrity_key: &[u8],
    wrapping_key_id: &str,
) -> Result<WrappedTenantKey, TenantKeyError> {
    wrap_new_tenant_key_with_registry(
        AlgorithmRegistry::global(),
        tenant_id,
        key_version,
        master_key,
        integrity_key,
        wrapping_key_id,
        AlgorithmId::Aes256Gcm,
    )
}

/// 指定注册表与包装算法的包装入口。
///
/// # Errors
///
/// 加密 / 信封 / 摘要失败时返回错误。
pub fn wrap_new_tenant_key_with_registry(
    registry: &AlgorithmRegistry,
    tenant_id: &str,
    key_version: &str,
    master_key: &[u8],
    integrity_key: &[u8],
    wrapping_key_id: &str,
    wrapping_alg: AlgorithmId,
) -> Result<WrappedTenantKey, TenantKeyError> {
    let tenant_key = Zeroizing::new(generate_tenant_secret_key());
    let aad = envelope_aad(tenant_id, key_version);
    let parts = encrypt_with_alg(
        TransportCipher::strict(registry),
        wrapping_alg,
        master_key,
        tenant_key.as_slice(),
        aad.as_bytes(),
    )?;
    let envelope = TenantKeyEnvelope::new(
        wrapping_key_id,
        wrapping_alg.as_str(),
        parts.iv,
        parts.ciphertext,
        parts.tag,
        aad,
    );
    let tenant_secret_key_b64 = STANDARD.encode(tenant_key.as_slice());
    let secret_key_digest = secret_key_digest(
        registry,
        integrity_key,
        tenant_id,
        key_version,
        tenant_key.as_slice(),
    )?;
    Ok(WrappedTenantKey {
        tenant_secret_key_b64,
        encrypted_secret_key: serialize_envelope(&envelope, registry)?,
        secret_key_digest,
        wrapping_key_id: wrapping_key_id.to_owned(),
    })
}

/// 打开包装后的租户密钥。
///
/// # Errors
///
/// 信封 / 解密失败或长度非法时返回错误。
pub fn unwrap_tenant_key(
    encrypted_secret_key: &str,
    tenant_id: &str,
    key_version: &str,
    master_key: &[u8],
) -> Result<Vec<u8>, TenantKeyError> {
    unwrap_tenant_key_with_registry(
        AlgorithmRegistry::global(),
        encrypted_secret_key,
        tenant_id,
        key_version,
        master_key,
    )
}

/// 使用多版本 `RootMaterialKeyring` 解包并验证租户密钥摘要（支持许可证 `Rekey` 后无损解密历史租户密钥）。
///
/// # Errors
///
/// 信封格式非法、密钥环中无任何版本能解开或完整性摘要不匹配时返回错误。
pub fn unwrap_and_verify_tenant_key_with_keyring(
    encrypted_secret_key: &str,
    tenant_id: &str,
    key_version: &str,
    expected_digest: &str,
    keyring: &RootMaterialKeyring,
) -> Result<Zeroizing<Vec<u8>>, TenantKeyError> {
    let registry = AlgorithmRegistry::global();
    let envelope = parse_envelope(encrypted_secret_key, registry)?;
    let expected_aad = envelope_aad(tenant_id, key_version);
    if envelope.aad != expected_aad {
        return Err(TenantKeyError::InvalidEncoding);
    }
    let wrapping_alg = mazbri_crypto::parse_algorithm_id(&envelope.algorithm)
        .map_err(|_| TenantKeyError::InvalidEncoding)?;

    let hinted_version = envelope
        .wrapping_key_id
        .rsplit_once(':')
        .map(|(_, suffix)| suffix)
        .filter(|s| keyring.master_key_for_version(s).is_some());

    let mut candidate_versions: Vec<&str> = Vec::new();
    if let Some(hint) = hinted_version {
        candidate_versions.push(hint);
    }
    if let Some(cur) = keyring.current_key_version()
        && !candidate_versions.contains(&cur)
    {
        candidate_versions.push(cur);
    }
    for ver in keyring.versions() {
        if !candidate_versions.contains(&ver) {
            candidate_versions.push(ver);
        }
    }

    let mut last_err = TenantKeyError::MasterKeyMissing;
    for root_ver in candidate_versions {
        let Some(master_key) = keyring.master_key_for_version(root_ver) else {
            continue;
        };
        match decrypt_with_alg(
            TransportCipher::strict(registry),
            wrapping_alg,
            master_key,
            &envelope.iv,
            &envelope.ciphertext,
            &envelope.tag,
            envelope.aad.as_bytes(),
        ) {
            Ok(raw_key) => {
                let raw_key = Zeroizing::new(raw_key);
                if raw_key.len() != TENANT_SECRET_KEY_BYTES {
                    return Err(TenantKeyError::InvalidKeyLength);
                }
                if let Some(integrity_key) = keyring.integrity_key_for_version(root_ver) {
                    if verify_secret_key_digest(
                        integrity_key,
                        tenant_id,
                        key_version,
                        &raw_key,
                        expected_digest,
                    )? {
                        return Ok(raw_key);
                    }
                    last_err = TenantKeyError::InvalidEncoding;
                } else {
                    return Ok(raw_key);
                }
            }
            Err(e) => {
                last_err = TenantKeyError::Transport(e);
            }
        }
    }
    Err(last_err)
}

/// 指定注册表的打开入口。
///
/// # Errors
///
/// 信封 / 解密失败或长度非法时返回错误。
pub fn unwrap_tenant_key_with_registry(
    registry: &AlgorithmRegistry,
    encrypted_secret_key: &str,
    tenant_id: &str,
    key_version: &str,
    master_key: &[u8],
) -> Result<Vec<u8>, TenantKeyError> {
    let envelope = parse_envelope(encrypted_secret_key, registry)?;
    let expected_aad = envelope_aad(tenant_id, key_version);
    if envelope.aad != expected_aad {
        return Err(TenantKeyError::InvalidEncoding);
    }
    let wrapping_alg = mazbri_crypto::parse_algorithm_id(&envelope.algorithm)
        .map_err(|_| TenantKeyError::InvalidEncoding)?;
    let key = decrypt_with_alg(
        TransportCipher::strict(registry),
        wrapping_alg,
        master_key,
        &envelope.iv,
        &envelope.ciphertext,
        &envelope.tag,
        envelope.aad.as_bytes(),
    )?;
    if key.len() != TENANT_SECRET_KEY_BYTES {
        return Err(TenantKeyError::InvalidKeyLength);
    }
    Ok(key)
}

/// # Errors
///
/// MAC 计算失败时返回错误。
pub fn verify_secret_key_digest(
    integrity_key: &[u8],
    tenant_id: &str,
    key_version: &str,
    tenant_secret_key: &[u8],
    digest: &str,
) -> Result<bool, TenantKeyError> {
    use subtle::ConstantTimeEq;
    let computed = secret_key_digest(
        AlgorithmRegistry::global(),
        integrity_key,
        tenant_id,
        key_version,
        tenant_secret_key,
    )?;
    Ok(bool::from(computed.as_bytes().ct_eq(digest.as_bytes())))
}

/// # Errors
///
/// MAC 计算失败时返回错误。
pub fn secret_key_digest(
    registry: &AlgorithmRegistry,
    integrity_key: &[u8],
    tenant_id: &str,
    key_version: &str,
    tenant_secret_key: &[u8],
) -> Result<String, TenantKeyError> {
    let payload = Zeroizing::new(format!(
        "{}:{}:{}",
        tenant_id,
        key_version,
        STANDARD.encode(tenant_secret_key)
    ));
    Ok(sign_payload(
        TransportCipher::strict(registry),
        DEFAULT_INTEGRITY_MAC,
        integrity_key,
        &payload,
    )?)
}

#[must_use]
pub fn envelope_aad(tenant_id: &str, key_version: &str) -> String {
    format!("{tenant_id}:{key_version}")
}

/// 默认密钥元数据（kebab-case，对齐 Profile / DB 列语义）。
#[must_use]
pub fn default_key_metadata() -> serde_json::Value {
    let data = default_data_alg();
    serde_json::json!({
        "suite": default_suite(),
        "dataAlg": data.as_str(),
        "macAlg": serde_json::Value::Null,
        "wrappingAlgorithm": DEFAULT_WRAPPING_ALGORITHM,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_unwrap_roundtrip_and_multi_version_keyring_recovery() {
        let registry = AlgorithmRegistry::default_registry();
        let master_v1 = [11u8; 32];
        let integrity_v1 = [13u8; 32];
        let master_v2 = [21u8; 32];
        let integrity_v2 = [23u8; 32];

        let wrapped_v1 = wrap_new_tenant_key_with_registry(
            &registry,
            "t1",
            "v1",
            &master_v1,
            &integrity_v1,
            "commercial-root-protection:key_password:v1",
            AlgorithmId::Aes256Gcm,
        )
        .unwrap();

        let keyring = RootMaterialKeyring::from_protection_keyring(&RootProtectionKeyring {
            current_key_version: Some("v2".to_string()),
            issuance_allowed: true,
            materials: vec![
                RootProtectionMaterialBundle {
                    current_key_version: "v2".to_string(),
                    wrapping_key_id: "commercial-root-protection:key_password:v2".to_string(),
                    master_key: STANDARD.encode(master_v2),
                    integrity_key: STANDARD.encode(integrity_v2),
                },
                RootProtectionMaterialBundle {
                    current_key_version: "v1".to_string(),
                    wrapping_key_id: "commercial-root-protection:key_password:v1".to_string(),
                    master_key: STANDARD.encode(master_v1),
                    integrity_key: STANDARD.encode(integrity_v1),
                },
            ],
        })
        .unwrap();

        let opened = unwrap_and_verify_tenant_key_with_keyring(
            &wrapped_v1.encrypted_secret_key,
            "t1",
            "v1",
            &wrapped_v1.secret_key_digest,
            &keyring,
        )
        .unwrap();
        assert_eq!(STANDARD.encode(&*opened), wrapped_v1.tenant_secret_key_b64);

        let meta = default_key_metadata();
        assert_eq!(meta["dataAlg"], "aes-256-gcm");
        assert_eq!(meta["wrappingAlgorithm"], "aes-256-gcm");
    }
}
