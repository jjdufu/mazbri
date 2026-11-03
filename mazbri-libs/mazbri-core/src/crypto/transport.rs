//! 租户 ↔ 网关传输信封：经 `mazbri-crypto` 注册表密封 / 打开（无硬编码唯一算法）。

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use mazbri_crypto::aead::{AEAD_NONCE_LEN, AEAD_TAG_LEN};
use mazbri_crypto::id::{AlgorithmId, AlgorithmKind};
use mazbri_crypto::legacy::assert_not_legacy_or_allowed;
use mazbri_crypto::material::{KeyPurpose, KeyRef};
use mazbri_crypto::parse_algorithm_id;
use mazbri_crypto::profile::{TenantTransportProfile, TransportMode};
use mazbri_crypto::registry::AlgorithmRegistry;
use mazbri_crypto::sm::SM4_BLOCK_LEN;
use mazbri_crypto::traits::{AeadSeal, BlockCipher, CryptoError, Mac};
use mazbri_crypto::{SUITE_TENANT_DEFAULT_2026_11, resolve_suite};
use serde::{Deserialize, Serialize};

/// 默认租户传输套件名（替代旧 `DATA_ALGORITHM` 常量唯一真相）。
#[must_use]
pub fn default_suite() -> &'static str {
    SUITE_TENANT_DEFAULT_2026_11
}

/// 默认租户数据密钥字节数（`aes-256-gcm` / `chacha20-poly1305`）。
pub const TENANT_SECRET_KEY_BYTES: usize = 32;
/// AEAD nonce / IV 字节数。
pub const AEAD_IV_BYTES: usize = AEAD_NONCE_LEN;
/// AEAD tag 字节数。
pub const AEAD_TAG_BYTES: usize = AEAD_TAG_LEN;
/// 传输防重放 nonce 原始字节数。
pub const NONCE_BYTES: usize = 16;
/// AES 分组 / CBC·CTR·CFB IV 长度。
pub const AES_BLOCK_IV_BYTES: usize = 16;

/// 线上传输信封（算法字段为注册表 kebab-case id）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TransportEnvelope {
    /// 是否含密文载荷。
    pub encrypted: bool,
    /// 密钥版本。
    pub key_version: String,
    /// 数据算法（kebab-case，如 `aes-256-gcm`）。
    pub data_alg: String,
    /// 外挂 MAC 算法；AEAD 且不要求外层签名时可省略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac_alg: Option<String>,
    /// Unix 秒时间戳。
    pub timestamp: i64,
    /// 防重放 nonce（base64url）。
    pub nonce: String,
    /// IV / AEAD nonce（标准 base64）。
    pub iv: String,
    /// 密文（标准 base64；不含 AEAD tag）。
    pub ciphertext: String,
    /// AEAD tag（标准 base64）；非 AEAD 时为空串。
    pub tag: String,
    /// MAC / 外层签名（hex）；无 MAC 时为空串。
    pub signature: String,
}

/// AEAD / 对称加密分段（base64 编码）。
#[derive(Debug, Clone)]
pub struct EncryptionParts {
    /// IV / nonce（标准 base64）。
    pub iv: String,
    /// 密文（标准 base64）。
    pub ciphertext: String,
    /// AEAD tag（标准 base64）；非 AEAD 为空。
    pub tag: String,
}

/// 传输加解密错误。
#[derive(Debug, thiserror::Error)]
pub enum TransportCryptoError {
    /// 密钥长度或材料不合算法要求。
    #[error("invalid key length")]
    InvalidKeyLength,
    /// IV / nonce 非法。
    #[error("invalid iv")]
    InvalidIv,
    /// 密文或 tag 编码/长度非法。
    #[error("invalid ciphertext or tag")]
    InvalidCiphertext,
    /// 加密失败。
    #[error("encrypt failed")]
    EncryptFailed,
    /// 解密或 AEAD 认证失败。
    #[error("decrypt failed")]
    DecryptFailed,
    /// MAC / 签名不匹配。
    #[error("signature mismatch")]
    SignatureMismatch,
    /// 签名编码非法。
    #[error("invalid hex signature")]
    InvalidSignature,
    /// 算法未登记或不支持。
    #[error("unsupported algorithm")]
    UnsupportedAlgorithm,
    /// Profile 与信封不一致或模式不允许。
    #[error("invalid transport profile: {0}")]
    InvalidProfile(String),
    /// 底层 `mazbri-crypto` 错误。
    #[error(transparent)]
    Crypto(#[from] CryptoError),
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

/// 数据算法所需 IV / nonce 长度。
///
/// # Errors
///
/// 非传输侧数据算法时返回 [`TransportCryptoError::UnsupportedAlgorithm`]。
pub fn iv_len_for(data_alg: AlgorithmId) -> Result<usize, TransportCryptoError> {
    match data_alg.kind() {
        AlgorithmKind::Aead => Ok(AEAD_IV_BYTES),
        AlgorithmKind::BlockCipher => match data_alg {
            AlgorithmId::Sm4CbcPkcs7 | AlgorithmId::Sm4Ctr | AlgorithmId::Sm4EcbPkcs7 => {
                Ok(SM4_BLOCK_LEN)
            }
            AlgorithmId::Aes256Cbc
            | AlgorithmId::Aes128Cbc
            | AlgorithmId::Aes256Ctr
            | AlgorithmId::Aes128Ctr
            | AlgorithmId::Aes256Cfb
            | AlgorithmId::Aes128Cfb => Ok(AES_BLOCK_IV_BYTES),
            AlgorithmId::DesCbcPkcs5 => Ok(8),
            AlgorithmId::Aes256Ecb | AlgorithmId::Aes128Ecb => Ok(0),
            _ => Err(TransportCryptoError::UnsupportedAlgorithm),
        },
        _ => Err(TransportCryptoError::UnsupportedAlgorithm),
    }
}

/// 数据算法所需密钥字节数。
///
/// # Errors
///
/// 非已实现传输数据算法时返回 [`TransportCryptoError::UnsupportedAlgorithm`]。
pub fn data_key_len(data_alg: AlgorithmId) -> Result<usize, TransportCryptoError> {
    match data_alg {
        AlgorithmId::Aes128Gcm | AlgorithmId::Aes128Cbc | AlgorithmId::Aes128Ctr | AlgorithmId::Aes128Cfb => {
            Ok(16)
        }
        AlgorithmId::Aes192Gcm => Ok(24),
        AlgorithmId::Aes256Gcm
        | AlgorithmId::Chacha20Poly1305
        | AlgorithmId::Aes256Cbc
        | AlgorithmId::Aes256Ctr
        | AlgorithmId::Aes256Cfb => Ok(32),
        AlgorithmId::Sm4CbcPkcs7 | AlgorithmId::Sm4Ctr | AlgorithmId::Sm4EcbPkcs7 => Ok(16),
        AlgorithmId::DesCbcPkcs5 => Ok(8),
        AlgorithmId::Aes256Ecb | AlgorithmId::Aes128Ecb => Ok(if matches!(data_alg, AlgorithmId::Aes128Ecb) {
            16
        } else {
            32
        }),
        _ => Err(TransportCryptoError::UnsupportedAlgorithm),
    }
}

/// 使用注册表 MAC 对载荷签名，返回 hex。
///
/// # Errors
///
/// 算法未登记或密钥非法时返回错误。
pub fn sign_payload(
    registry: &AlgorithmRegistry,
    mac_alg: AlgorithmId,
    key: &[u8],
    payload: &str,
) -> Result<String, TransportCryptoError> {
    let mac = resolve_mac(registry, mac_alg)?;
    let key_ref = KeyRef::raw(KeyPurpose::Mac, key.to_vec());
    let tag = mac.sign(&key_ref, payload.as_bytes())?;
    Ok(hex::encode(tag))
}

/// 校验 hex MAC。
///
/// # Errors
///
/// 编码非法或标签不匹配时返回错误。
pub fn verify_payload_signature(
    registry: &AlgorithmRegistry,
    mac_alg: AlgorithmId,
    key: &[u8],
    payload: &str,
    signature_hex: &str,
) -> Result<(), TransportCryptoError> {
    let mac = resolve_mac(registry, mac_alg)?;
    let key_ref = KeyRef::raw(KeyPurpose::Mac, key.to_vec());
    let actual = hex::decode(signature_hex.trim()).map_err(|_| TransportCryptoError::InvalidSignature)?;
    mac.verify(&key_ref, payload.as_bytes(), &actual)
        .map_err(|err| match err {
            CryptoError::AuthenticationFailed => TransportCryptoError::SignatureMismatch,
            other => TransportCryptoError::Crypto(other),
        })
}

/// 按算法密封明文（AEAD 或需外挂 MAC 的对称模式）。
///
/// # Errors
///
/// 算法/密钥/IV 非法或底层加密失败时返回错误。
pub fn encrypt_with_alg(
    registry: &AlgorithmRegistry,
    data_alg: AlgorithmId,
    key: &[u8],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<EncryptionParts, TransportCryptoError> {
    let iv_len = iv_len_for(data_alg)?;
    let iv = random_bytes(iv_len);
    encrypt_with_alg_iv(registry, data_alg, key, &iv, plaintext, aad)
}

/// 指定 IV 的密封。
///
/// # Errors
///
/// 算法/密钥/IV 非法或底层加密失败时返回错误。
pub fn encrypt_with_alg_iv(
    registry: &AlgorithmRegistry,
    data_alg: AlgorithmId,
    key: &[u8],
    iv: &[u8],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<EncryptionParts, TransportCryptoError> {
    let expect = iv_len_for(data_alg)?;
    if iv.len() != expect {
        return Err(TransportCryptoError::InvalidIv);
    }
    match data_alg.kind() {
        AlgorithmKind::Aead => {
            let cipher = registry.aead(data_alg)?;
            let key_ref = KeyRef::raw(KeyPurpose::DataEncryption, key.to_vec());
            let packed = cipher
                .seal(&key_ref, iv, aad, plaintext)
                .map_err(|err| match err {
                    CryptoError::InvalidKey { .. } => TransportCryptoError::InvalidKeyLength,
                    other => TransportCryptoError::Crypto(other),
                })?;
            let split_at = packed
                .len()
                .checked_sub(AEAD_TAG_BYTES)
                .ok_or(TransportCryptoError::EncryptFailed)?;
            let (ciphertext, tag) = packed.split_at(split_at);
            Ok(EncryptionParts {
                iv: STANDARD.encode(iv),
                ciphertext: STANDARD.encode(ciphertext),
                tag: STANDARD.encode(tag),
            })
        }
        AlgorithmKind::BlockCipher => {
            let key_ref = KeyRef::raw(KeyPurpose::DataEncryption, key.to_vec());
            let ciphertext = encrypt_block(registry, data_alg, &key_ref, iv, plaintext)?;
            Ok(EncryptionParts {
                iv: STANDARD.encode(iv),
                ciphertext: STANDARD.encode(ciphertext),
                tag: String::new(),
            })
        }
        _ => Err(TransportCryptoError::UnsupportedAlgorithm),
    }
}

/// 按算法打开密文。
///
/// # Errors
///
/// 算法/密钥/编码非法或认证失败时返回错误。
pub fn decrypt_with_alg(
    registry: &AlgorithmRegistry,
    data_alg: AlgorithmId,
    key: &[u8],
    iv_b64: &str,
    ciphertext_b64: &str,
    tag_b64: &str,
    aad: &[u8],
) -> Result<Vec<u8>, TransportCryptoError> {
    let iv = STANDARD
        .decode(iv_b64)
        .map_err(|_| TransportCryptoError::InvalidIv)?;
    let expect = iv_len_for(data_alg)?;
    if iv.len() != expect {
        return Err(TransportCryptoError::InvalidIv);
    }
    let ciphertext = STANDARD
        .decode(ciphertext_b64)
        .map_err(|_| TransportCryptoError::InvalidCiphertext)?;
    match data_alg.kind() {
        AlgorithmKind::Aead => {
            let tag = STANDARD
                .decode(tag_b64)
                .map_err(|_| TransportCryptoError::InvalidCiphertext)?;
            if tag.len() != AEAD_TAG_BYTES {
                return Err(TransportCryptoError::InvalidCiphertext);
            }
            let mut combined = ciphertext;
            combined.extend_from_slice(&tag);
            let opener = registry.aead(data_alg)?;
            let key_ref = KeyRef::raw(KeyPurpose::DataEncryption, key.to_vec());
            opener.open(&key_ref, &iv, aad, &combined)
                .map_err(|err| match err {
                    CryptoError::AuthenticationFailed => TransportCryptoError::DecryptFailed,
                    CryptoError::InvalidKey { .. } => TransportCryptoError::InvalidKeyLength,
                    other => TransportCryptoError::Crypto(other),
                })
        }
        AlgorithmKind::BlockCipher => {
            if !tag_b64.is_empty() {
                return Err(TransportCryptoError::InvalidCiphertext);
            }
            let key_ref = KeyRef::raw(KeyPurpose::DataEncryption, key.to_vec());
            decrypt_block(registry, data_alg, &key_ref, &iv, &ciphertext)
        }
        _ => Err(TransportCryptoError::UnsupportedAlgorithm),
    }
}

#[must_use]
pub fn generate_nonce_base64url() -> String {
    let nonce: [u8; NONCE_BYTES] = rand::random();
    URL_SAFE_NO_PAD.encode(nonce)
}

/// 校验信封算法字段为闭集 kebab-case，且相对 Profile 生效算法一致。
///
/// # Errors
///
/// 未加密、算法未知/冲突、或不在注册表时返回错误。
pub fn validate_transport_algorithms(
    envelope: &TransportEnvelope,
    profile: &TenantTransportProfile,
    registry: &AlgorithmRegistry,
) -> Result<(), TransportCryptoError> {
    if !envelope.encrypted {
        return Err(TransportCryptoError::UnsupportedAlgorithm);
    }
    let data_alg = parse_algorithm_id(&envelope.data_alg)?;
    let _ = registry.get(data_alg)?;
    assert_not_legacy_or_allowed(data_alg, profile.allow_insecure)?;

    let envelope_mac = match envelope.mac_alg.as_deref() {
        Some(raw) => {
            let mac = parse_algorithm_id(raw)?;
            let _ = registry.get(mac)?;
            assert_not_legacy_or_allowed(mac, profile.allow_insecure)?;
            Some(mac)
        }
        None => None,
    };

    let (expect_data, expect_mac) = profile.effective_algs()?;
    let expect_data = expect_data.ok_or_else(|| {
        TransportCryptoError::InvalidProfile("profile has no data_alg".to_owned())
    })?;
    if data_alg != expect_data {
        return Err(TransportCryptoError::InvalidProfile(format!(
            "envelope data_alg {data_alg} != profile {expect_data}"
        )));
    }
    if envelope_mac != expect_mac {
        return Err(TransportCryptoError::InvalidProfile(format!(
            "envelope mac_alg {envelope_mac:?} != profile {expect_mac:?}"
        )));
    }
    Ok(())
}

/// [`seal_envelope`] 入参（避免超长参数列表）。
#[derive(Debug, Clone, Copy)]
pub struct SealEnvelopeParams<'a> {
    /// 租户传输 Profile。
    pub profile: &'a TenantTransportProfile,
    /// 算法注册表。
    pub registry: &'a AlgorithmRegistry,
    /// 数据密钥。
    pub data_key: &'a [u8],
    /// 外挂 MAC 密钥（无 `mac_alg` 时可 `None`）。
    pub mac_key: Option<&'a [u8]>,
    /// 密钥版本。
    pub key_version: &'a str,
    /// 租户 id（亦作 AEAD AAD）。
    pub tenant_id: &'a str,
    /// HTTP 路径（参与 MAC 规范串）。
    pub http_path: &'a str,
    /// 明文载荷。
    pub plaintext: &'a [u8],
    /// Unix 秒时间戳。
    pub timestamp: i64,
}

/// 按 Profile 密封传输信封。
///
/// # Errors
///
/// Profile 非法、算法/密钥不合要求或加密失败时返回错误。
pub fn seal_envelope(
    params: SealEnvelopeParams<'_>,
) -> Result<TransportEnvelope, TransportCryptoError> {
    let SealEnvelopeParams {
        profile,
        registry,
        data_key,
        mac_key,
        key_version,
        tenant_id,
        http_path,
        plaintext,
        timestamp,
    } = params;
    if matches!(profile.mode, TransportMode::None) {
        return Err(TransportCryptoError::InvalidProfile(
            "mode=none cannot seal".to_owned(),
        ));
    }
    let (data_alg, mac_alg) = profile.effective_algs()?;
    let data_alg = data_alg.ok_or_else(|| {
        TransportCryptoError::InvalidProfile("missing data_alg".to_owned())
    })?;
    assert_not_legacy_or_allowed(data_alg, profile.allow_insecure)?;
    if let Some(mac) = mac_alg {
        assert_not_legacy_or_allowed(mac, profile.allow_insecure)?;
    }
    if mac_alg.is_none() && profile.require_signature {
        return Err(TransportCryptoError::InvalidProfile(
            "require_signature needs mac_alg".to_owned(),
        ));
    }

    let parts = encrypt_with_alg(registry, data_alg, data_key, plaintext, tenant_id.as_bytes())?;
    let nonce = generate_nonce_base64url();
    let mut envelope = TransportEnvelope {
        encrypted: true,
        key_version: key_version.to_owned(),
        data_alg: data_alg.as_str().to_owned(),
        mac_alg: mac_alg.map(|id| id.as_str().to_owned()),
        timestamp,
        nonce,
        iv: parts.iv,
        ciphertext: parts.ciphertext,
        tag: parts.tag,
        signature: String::new(),
    };

    if let Some(mac_id) = mac_alg {
        let key = mac_key.ok_or(TransportCryptoError::InvalidKeyLength)?;
        let payload = canonical_signature_payload(&envelope, tenant_id, http_path);
        envelope.signature = sign_payload(registry, mac_id, key, &payload)?;
    }
    Ok(envelope)
}

/// 按 Profile 打开传输信封。
///
/// # Errors
///
/// 算法不一致、签名失败或解密失败时返回错误。
pub fn open_envelope(
    profile: &TenantTransportProfile,
    registry: &AlgorithmRegistry,
    data_key: &[u8],
    mac_key: Option<&[u8]>,
    tenant_id: &str,
    http_path: &str,
    envelope: &TransportEnvelope,
) -> Result<Vec<u8>, TransportCryptoError> {
    validate_transport_algorithms(envelope, profile, registry)?;
    let data_alg = parse_algorithm_id(&envelope.data_alg)?;
    if let Some(mac_raw) = envelope.mac_alg.as_deref() {
        let mac_id = parse_algorithm_id(mac_raw)?;
        let key = mac_key.ok_or(TransportCryptoError::InvalidKeyLength)?;
        let payload = canonical_signature_payload(envelope, tenant_id, http_path);
        verify_payload_signature(registry, mac_id, key, &payload, &envelope.signature)?;
    } else if !envelope.signature.is_empty() {
        return Err(TransportCryptoError::InvalidProfile(
            "signature present without mac_alg".to_owned(),
        ));
    }
    decrypt_with_alg(
        registry,
        data_alg,
        data_key,
        &envelope.iv,
        &envelope.ciphertext,
        &envelope.tag,
        tenant_id.as_bytes(),
    )
}

/// 解析默认套件的数据算法 id。
///
/// 若命名套件表异常缺失默认条目，回退为 [`AlgorithmId::Aes256Gcm`]。
#[must_use]
pub fn default_data_alg() -> AlgorithmId {
    resolve_suite(default_suite()).map_or(AlgorithmId::Aes256Gcm, |suite| suite.data_alg)
}

fn resolve_mac(
    registry: &AlgorithmRegistry,
    mac_alg: AlgorithmId,
) -> Result<mazbri_crypto::MacAlgorithm, TransportCryptoError> {
    Ok(registry.mac(mac_alg)?)
}

fn encrypt_block(
    registry: &AlgorithmRegistry,
    data_alg: AlgorithmId,
    key: &KeyRef,
    iv: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, TransportCryptoError> {
    if let Ok(sm4) = registry.sm4(data_alg) {
        return sm4.encrypt(key, iv, plaintext).map_err(|err| match err {
            CryptoError::InvalidKey { .. } => TransportCryptoError::InvalidKeyLength,
            other => TransportCryptoError::Crypto(other),
        });
    }
    let cipher = registry.block_cipher(data_alg)?;
    cipher.encrypt(key, iv, plaintext).map_err(|err| match err {
        CryptoError::InvalidKey { .. } => TransportCryptoError::InvalidKeyLength,
        other => TransportCryptoError::Crypto(other),
    })
}

fn decrypt_block(
    registry: &AlgorithmRegistry,
    data_alg: AlgorithmId,
    key: &KeyRef,
    iv: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, TransportCryptoError> {
    if let Ok(sm4) = registry.sm4(data_alg) {
        return sm4.decrypt(key, iv, ciphertext).map_err(|err| match err {
            CryptoError::InvalidKey { .. } => TransportCryptoError::InvalidKeyLength,
            CryptoError::InvalidInput { .. } => TransportCryptoError::DecryptFailed,
            other => TransportCryptoError::Crypto(other),
        });
    }
    let cipher = registry.block_cipher(data_alg)?;
    cipher.decrypt(key, iv, ciphertext).map_err(|err| match err {
        CryptoError::InvalidKey { .. } => TransportCryptoError::InvalidKeyLength,
        CryptoError::InvalidInput { .. } => TransportCryptoError::DecryptFailed,
        other => TransportCryptoError::Crypto(other),
    })
}

fn random_bytes(len: usize) -> Vec<u8> {
    (0..len).map(|_| rand::random::<u8>()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_crypto::profile::{KeySelection, TransportMode};
    use mazbri_crypto::{
        SUITE_GM_SM4_HMAC_SM3, SUITE_TENANT_CHACHA20, SUITE_TENANT_DEFAULT_2026_11,
    };

    fn registry() -> AlgorithmRegistry {
        AlgorithmRegistry::default_registry()
    }

    fn profile_for_suite(suite: &str) -> TenantTransportProfile {
        let mut profile =
            TenantTransportProfile::from_named_suite(suite, TransportMode::Full).unwrap();
        profile.key = Some(KeySelection::Latest {
            key_id: "tenant-default".to_owned(),
        });
        profile.require_signature = profile.mac_alg.is_some();
        profile
    }

    fn roundtrip(suite: &str, data_key: &[u8], mac_key: Option<&[u8]>) {
        let registry = registry();
        let profile = profile_for_suite(suite);
        profile.validate(&registry).unwrap();
        let plaintext = b"mazbri-transport-roundtrip";
        let sealed = seal_envelope(SealEnvelopeParams {
            profile: &profile,
            registry: &registry,
            data_key,
            mac_key,
            key_version: "v1",
            tenant_id: "tenant-1",
            http_path: "/api/v1/gateway",
            plaintext,
            timestamp: 1_710_000_000,
        })
        .unwrap();
        assert!(sealed.encrypted);
        assert!(!sealed.data_alg.contains('A')); // kebab-case, no uppercase alg
        let opened = open_envelope(
            &profile,
            &registry,
            data_key,
            mac_key,
            "tenant-1",
            "/api/v1/gateway",
            &sealed,
        )
        .unwrap();
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn roundtrip_aes_256_gcm_default_suite() {
        let key = [9u8; 32];
        roundtrip(SUITE_TENANT_DEFAULT_2026_11, &key, None);
    }

    #[test]
    fn roundtrip_chacha20_poly1305() {
        let key = [3u8; 32];
        roundtrip(SUITE_TENANT_CHACHA20, &key, None);
    }

    #[test]
    fn roundtrip_gm_sm4_hmac_sm3() {
        let data_key = [5u8; 16];
        let mac_key = [7u8; 32];
        roundtrip(SUITE_GM_SM4_HMAC_SM3, &data_key, Some(&mac_key));
    }

    #[test]
    fn rejects_uppercase_algorithm_id() {
        let registry = registry();
        let profile = profile_for_suite(SUITE_TENANT_DEFAULT_2026_11);
        let envelope = TransportEnvelope {
            encrypted: true,
            key_version: "v1".into(),
            data_alg: "AES-256-GCM".into(),
            mac_alg: None,
            timestamp: 1,
            nonce: "n".into(),
            iv: String::new(),
            ciphertext: String::new(),
            tag: String::new(),
            signature: String::new(),
        };
        assert!(validate_transport_algorithms(&envelope, &profile, &registry).is_err());
    }

    #[test]
    fn default_suite_resolves_aes_256_gcm() {
        assert_eq!(default_data_alg(), AlgorithmId::Aes256Gcm);
        assert_eq!(default_suite(), SUITE_TENANT_DEFAULT_2026_11);
    }
}
