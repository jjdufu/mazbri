//! 供应商 `auth_flow.credentials` 与上游 mTLS 私钥落库加密（`enc:v3:` 单一格式）。
//!
//! - 唯一密文格式：`enc:v3:<key_version>:<kebab-alg>:<base64(nonce ‖ ciphertext ‖ tag)>`
//! - 密钥来源：统一收敛至 `cert-service` 下发的商业根材料多版本密钥环 [`RootMaterialKeyring`]
//!   （彻底移除硬编码弱种子与独立环境变量密钥）
//! - 密钥派生：`HKDF-SHA256(ikm = master_key[key_version], salt = key_version, info = "mazbri-supplier-secrets-v3")`
//! - 规范化 AAD：`supplier_secret|{owner_kind}|{owner_id}|{field_name}|{alg}`

use super::tenant_key::{
    RootProtectionKeyring, RootProtectionMaterialBundle, decode_configured_key,
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use hkdf::Hkdf;
use mazbri_crypto::aead::{AEAD_NONCE_LEN, AEAD_TAG_LEN};
use mazbri_crypto::id::AlgorithmId;
use mazbri_crypto::material::{KeyPurpose, KeyRef};
use mazbri_crypto::parse_algorithm_id;
use mazbri_crypto::registry::AlgorithmRegistry;
use mazbri_crypto::traits::AeadSeal;
use parking_lot::RwLock;
use rand::Rng;
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use zeroize::Zeroizing;

/// 落库密文唯一前缀（其后为 `<key_version>:<alg>:<b64>`）。
pub const ENCRYPTED_PREFIX: &str = "enc:v3:";

/// 默认落库 AEAD。
pub const DEFAULT_SECRET_ALGORITHM: &str = "aes-256-gcm";

/// 供应商凭据落库派生域标签（与租户传输层、配置中心、MFA 正交隔离）。
pub const SUPPLIER_SECRETS_HKDF_INFO: &[u8] = b"mazbri-supplier-secrets-v3";

const DEFAULT_UNSCOPED_OWNER_KIND: &str = "unscoped";
const DEFAULT_UNSCOPED_OWNER_ID: &str = "global";
const DEFAULT_UNSCOPED_FIELD: &str = "secret";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SupplierSecretsError {
    #[error("{0}")]
    Message(String),
}

impl SupplierSecretsError {
    #[must_use]
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Message(s.into())
    }
}

/// 内存态多版本商业根保护密钥环（析构时自动清零全部主密钥与完整性密钥）。
#[derive(Clone)]
pub struct RootMaterialKeyring {
    current_key_version: Option<String>,
    issuance_allowed: bool,
    master_keys: HashMap<String, Zeroizing<[u8; 32]>>,
    integrity_keys: HashMap<String, Zeroizing<[u8; 32]>>,
}

impl std::fmt::Debug for RootMaterialKeyring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut versions: Vec<&str> = self.master_keys.keys().map(String::as_str).collect();
        versions.sort_unstable();
        f.debug_struct("RootMaterialKeyring")
            .field("current_key_version", &self.current_key_version)
            .field("issuance_allowed", &self.issuance_allowed)
            .field("versions", &versions)
            .finish_non_exhaustive()
    }
}

impl RootMaterialKeyring {
    /// 从 `GET /authorizations/root/materials/keyring` 响应构造密钥环。
    ///
    /// # Errors
    ///
    /// 材料为空、版本号非法或密钥字节长度不是 32 字节时返回错误。
    pub fn from_protection_keyring(
        keyring: &RootProtectionKeyring,
    ) -> Result<Self, SupplierSecretsError> {
        if keyring.materials.is_empty() {
            return Err(SupplierSecretsError::msg(
                "root material keyring contains no key versions",
            ));
        }
        let mut master_keys = HashMap::with_capacity(keyring.materials.len());
        let mut integrity_keys = HashMap::with_capacity(keyring.materials.len());
        for item in &keyring.materials {
            validate_key_version(&item.current_key_version)?;
            let m_vec = Zeroizing::new(
                decode_configured_key(&item.master_key)
                    .map_err(|e| SupplierSecretsError::msg(format!("invalid master_key: {e}")))?,
            );
            let i_vec = Zeroizing::new(decode_configured_key(&item.integrity_key).map_err(
                |e| SupplierSecretsError::msg(format!("invalid integrity_key: {e}")),
            )?);
            let mut m_arr = Zeroizing::new([0u8; 32]);
            m_arr.copy_from_slice(&m_vec);
            let mut i_arr = Zeroizing::new([0u8; 32]);
            i_arr.copy_from_slice(&i_vec);
            master_keys.insert(item.current_key_version.clone(), m_arr);
            integrity_keys.insert(item.current_key_version.clone(), i_arr);
        }
        let current = if keyring.issuance_allowed {
            keyring.current_key_version.clone()
        } else {
            None
        };
        if let Some(ref cur) = current
            && !master_keys.contains_key(cur)
        {
            return Err(SupplierSecretsError::msg(format!(
                "current_key_version '{cur}' missing from keyring materials"
            )));
        }
        Ok(Self {
            current_key_version: current,
            issuance_allowed: keyring.issuance_allowed,
            master_keys,
            integrity_keys,
        })
    }

    /// 从单版本 `RootProtectionMaterialBundle` 构造密钥环。
    ///
    /// # Errors
    ///
    /// 密钥长度或版本号非法时返回错误。
    pub fn from_bundle(
        bundle: &RootProtectionMaterialBundle,
        issuance_allowed: bool,
    ) -> Result<Self, SupplierSecretsError> {
        Self::from_protection_keyring(&RootProtectionKeyring {
            current_key_version: issuance_allowed.then(|| bundle.current_key_version.clone()),
            issuance_allowed,
            materials: vec![bundle.clone()],
        })
    }

    /// 从原始 32 字节主密钥表构造密钥环（供本地工具与单元测试显式注入）。
    ///
    /// # Errors
    ///
    /// 密钥表为空、版本号含冒号或 `current_key_version` 未包含在表中时返回错误。
    pub fn from_raw_keys<I>(
        current_key_version: Option<String>,
        issuance_allowed: bool,
        entries: I,
    ) -> Result<Self, SupplierSecretsError>
    where
        I: IntoIterator<Item = (String, [u8; 32])>,
    {
        let mut master_keys = HashMap::new();
        for (ver, raw) in entries {
            validate_key_version(&ver)?;
            master_keys.insert(ver, Zeroizing::new(raw));
        }
        if master_keys.is_empty() {
            return Err(SupplierSecretsError::msg("keyring entries cannot be empty"));
        }
        let effective_current = if issuance_allowed {
            current_key_version
        } else {
            None
        };
        if let Some(ref cur) = effective_current
            && !master_keys.contains_key(cur)
        {
            return Err(SupplierSecretsError::msg(format!(
                "current_key_version '{cur}' missing from raw keyring"
            )));
        }
        Ok(Self {
            current_key_version: effective_current,
            issuance_allowed,
            master_keys,
            integrity_keys: HashMap::new(),
        })
    }

    #[must_use]
    pub fn current_key_version(&self) -> Option<&str> {
        self.current_key_version.as_deref()
    }

    #[must_use]
    pub const fn issuance_allowed(&self) -> bool {
        self.issuance_allowed
    }

    #[must_use]
    pub fn master_key_for_version(&self, version: &str) -> Option<&[u8; 32]> {
        self.master_keys.get(version).map(|z| &**z)
    }

    #[must_use]
    pub fn integrity_key_for_version(&self, version: &str) -> Option<&[u8; 32]> {
        self.integrity_keys.get(version).map(|z| &**z)
    }

    #[must_use]
    pub fn versions(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self.master_keys.keys().map(String::as_str).collect();
        out.sort_unstable();
        out
    }

    fn require_encryption_version(&self) -> Result<(&str, &[u8; 32]), SupplierSecretsError> {
        if !self.issuance_allowed {
            return Err(SupplierSecretsError::msg(
                "commercial root material issuance/encryption is currently disallowed by license state",
            ));
        }
        let ver = self.current_key_version.as_deref().ok_or_else(|| {
            SupplierSecretsError::msg(
                "no active current_key_version available for encrypting new secrets",
            )
        })?;
        let master = self.master_key_for_version(ver).ok_or_else(|| {
            SupplierSecretsError::msg(format!("active root material version '{ver}' not loaded"))
        })?;
        Ok((ver, master))
    }
}

/// 统一密钥环借用或持有包装。
#[derive(Debug)]
pub enum KeyringBorrow<'a> {
    /// 引用借用。
    Borrowed(&'a RootMaterialKeyring),
    /// 共享引用计数持有。
    Shared(Arc<RootMaterialKeyring>),
    /// 按值持有。
    Owned(RootMaterialKeyring),
}

impl std::ops::Deref for KeyringBorrow<'_> {
    type Target = RootMaterialKeyring;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(r) => r,
            Self::Shared(a) => a.as_ref(),
            Self::Owned(o) => o,
        }
    }
}

/// 统一借用特质，兼容 `&RootMaterialKeyring`、`Arc<RootMaterialKeyring>` 与 `&Arc<RootMaterialKeyring>`。
pub trait KeyringRef<'a> {
    /// 消费入参并转换为统一密钥环借用包装。
    fn into_keyring(self) -> KeyringBorrow<'a>;
}

impl KeyringRef<'static> for RootMaterialKeyring {
    fn into_keyring(self) -> KeyringBorrow<'static> {
        KeyringBorrow::Owned(self)
    }
}

impl<'a> KeyringRef<'a> for &'a RootMaterialKeyring {
    fn into_keyring(self) -> KeyringBorrow<'a> {
        KeyringBorrow::Borrowed(self)
    }
}

impl KeyringRef<'static> for Arc<RootMaterialKeyring> {
    fn into_keyring(self) -> KeyringBorrow<'static> {
        KeyringBorrow::Shared(self)
    }
}

impl<'a> KeyringRef<'a> for &'a Arc<RootMaterialKeyring> {
    fn into_keyring(self) -> KeyringBorrow<'a> {
        KeyringBorrow::Borrowed(self.as_ref())
    }
}

static GLOBAL_KEYRING: RwLock<Option<Arc<RootMaterialKeyring>>> = RwLock::new(None);

/// 原子更新当前进程共享的商业根材料密钥环。
pub fn install_root_material_keyring(keyring: RootMaterialKeyring) -> Arc<RootMaterialKeyring> {
    let arc = Arc::new(keyring);
    *GLOBAL_KEYRING.write() = Some(Arc::clone(&arc));
    arc
}

/// 清空进程级根材料密钥环。
pub fn clear_root_material_keyring() {
    *GLOBAL_KEYRING.write() = None;
}

/// 获取当前进程已装载的根材料密钥环；未装载时直接拒绝（无任何硬编码种子或环境变量回退）。
///
/// # Errors
///
/// 进程尚未从 `cert-service` 同步根材料密钥环时返回错误。
pub fn supplier_secrets_key() -> Result<Arc<RootMaterialKeyring>, SupplierSecretsError> {
    GLOBAL_KEYRING.read().clone().ok_or_else(|| {
        SupplierSecretsError::msg(
            "commercial root material keyring is not initialized; sync from cert-service /authorizations/root/materials/keyring first",
        )
    })
}

fn validate_key_version(version: &str) -> Result<(), SupplierSecretsError> {
    let trimmed = version.trim();
    if trimmed.is_empty() || trimmed.len() > 32 || trimmed.contains(':') {
        return Err(SupplierSecretsError::msg(format!(
            "invalid root material key_version '{version}'"
        )));
    }
    Ok(())
}

fn derive_secret_key(
    master_key: &[u8; 32],
    key_version: &str,
) -> Result<Zeroizing<[u8; 32]>, SupplierSecretsError> {
    let hkdf = Hkdf::<Sha256>::new(Some(key_version.as_bytes()), master_key);
    let mut subkey = Zeroizing::new([0u8; 32]);
    hkdf.expand(SUPPLIER_SECRETS_HKDF_INFO, &mut *subkey)
        .map_err(|e| SupplierSecretsError::msg(format!("HKDF expand failed: {e}")))?;
    Ok(subkey)
}

/// 构造 `enc:v3:` 规范化 AAD：`supplier_secret|{owner_kind}|{owner_id}|{field_name}|{alg}`。
#[must_use]
pub fn build_supplier_secret_aad(
    owner_kind: &str,
    owner_id: &str,
    field_name: &str,
    alg: &str,
) -> String {
    format!("supplier_secret|{owner_kind}|{owner_id}|{field_name}|{alg}")
}

#[must_use]
pub fn is_encrypted_secret(value: &str) -> bool {
    value.starts_with(ENCRYPTED_PREFIX)
}

fn reject_legacy_at_rest_prefix(value: &str) -> Result<(), SupplierSecretsError> {
    if value.starts_with("enc:") && !is_encrypted_secret(value) {
        return Err(SupplierSecretsError::msg(
            "unsupported legacy at-rest secret format; expected enc:v3:<key_ver>:<alg>:<b64>",
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

/// 使用显式归属上下文 `(owner_kind, owner_id, field_name)` 密封明文凭据（默认 `aes-256-gcm`）。
///
/// # Errors
///
/// 许可证禁止新加密、算法非法、遇到旧前缀或 AEAD 密封失败时返回错误。
pub fn encrypt_secret_with_aad<'a>(
    plaintext: &str,
    keyring: impl KeyringRef<'a>,
    owner_kind: &str,
    owner_id: &str,
    field_name: &str,
) -> Result<String, SupplierSecretsError> {
    encrypt_secret_with_alg_and_aad(
        plaintext,
        keyring,
        AlgorithmId::Aes256Gcm,
        owner_kind,
        owner_id,
        field_name,
    )
}

/// 指定 AEAD 算法与归属上下文密封明文凭据，输出 `enc:v3:<key_ver>:<alg>:<b64>`。
///
/// # Errors
///
/// 许可证禁止新加密、算法非法、遇到旧前缀或 AEAD 密封失败时返回错误。
pub fn encrypt_secret_with_alg_and_aad<'a>(
    plaintext: &str,
    keyring: impl KeyringRef<'a>,
    alg: AlgorithmId,
    owner_kind: &str,
    owner_id: &str,
    field_name: &str,
) -> Result<String, SupplierSecretsError> {
    if is_encrypted_secret(plaintext) {
        return Ok(plaintext.to_string());
    }
    reject_legacy_at_rest_prefix(plaintext)?;

    let kr = keyring.into_keyring();
    let (key_version, master_key) = kr.require_encryption_version()?;
    let (_, sealer) = resolve_aead(AlgorithmRegistry::global(), alg.as_str())?;
    let derived = derive_secret_key(master_key, key_version)?;
    let key_ref = KeyRef::raw(KeyPurpose::KeyWrap, derived.to_vec());
    let mut nonce = [0u8; AEAD_NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);
    let aad = build_supplier_secret_aad(owner_kind, owner_id, field_name, alg.as_str());
    let packed = sealer
        .seal(&key_ref, &nonce, aad.as_bytes(), plaintext.as_bytes())
        .map_err(|e| SupplierSecretsError::msg(format!("encrypt credential failed: {e}")))?;
    if packed.len() < AEAD_TAG_LEN {
        return Err(SupplierSecretsError::msg(
            "encrypt credential produced short ciphertext",
        ));
    }
    let mut blob = Vec::with_capacity(AEAD_NONCE_LEN + packed.len());
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&packed);
    Ok(format!(
        "{ENCRYPTED_PREFIX}{key_version}:{}:{}",
        alg.as_str(),
        B64.encode(blob)
    ))
}

/// 使用显式归属上下文 `(owner_kind, owner_id, field_name)` 解密 `enc:v3:` 凭据。
///
/// # Errors
///
/// 旧前缀、`key_version` 未加载、AAD 归属不匹配或认证标签失败时返回错误。
pub fn decrypt_secret_with_aad<'a>(
    value: &str,
    keyring: impl KeyringRef<'a>,
    owner_kind: &str,
    owner_id: &str,
    field_name: &str,
) -> Result<String, SupplierSecretsError> {
    if !is_encrypted_secret(value) {
        reject_legacy_at_rest_prefix(value)?;
        return Ok(value.to_string());
    }
    let rest = &value[ENCRYPTED_PREFIX.len()..];
    let (key_version, after_ver) = rest.split_once(':').ok_or_else(|| {
        SupplierSecretsError::msg("invalid enc:v3 credential format (missing key_version)")
    })?;
    let (alg_raw, b64) = after_ver.split_once(':').ok_or_else(|| {
        SupplierSecretsError::msg("invalid enc:v3 credential format (missing alg)")
    })?;
    if key_version.is_empty() || alg_raw.is_empty() || b64.is_empty() {
        return Err(SupplierSecretsError::msg(
            "invalid enc:v3 credential format (empty segment)",
        ));
    }
    let kr = keyring.into_keyring();
    let master_key = kr.master_key_for_version(key_version).ok_or_else(|| {
        SupplierSecretsError::msg(format!(
            "root material key_version '{key_version}' not found in keyring"
        ))
    })?;
    let (alg_id, opener) = resolve_aead(AlgorithmRegistry::global(), alg_raw)?;
    let blob = B64
        .decode(b64.as_bytes())
        .map_err(|e| SupplierSecretsError::msg(format!("invalid credential ciphertext: {e}")))?;
    if blob.len() < AEAD_NONCE_LEN + AEAD_TAG_LEN {
        return Err(SupplierSecretsError::msg(
            "credential ciphertext too short",
        ));
    }
    let (nonce, ct) = blob.split_at(AEAD_NONCE_LEN);
    let derived = derive_secret_key(master_key, key_version)?;
    let key_ref = KeyRef::raw(KeyPurpose::KeyWrap, derived.to_vec());
    let aad = build_supplier_secret_aad(owner_kind, owner_id, field_name, alg_id.as_str());
    let pt = Zeroizing::new(
        opener
            .open(&key_ref, nonce, aad.as_bytes(), ct)
            .map_err(|_| {
                SupplierSecretsError::msg(
                    "decrypt credential failed (wrong key_version, AAD mismatch, or tampered ciphertext)",
                )
            })?,
    );
    std::str::from_utf8(&pt)
        .map(str::to_owned)
        .map_err(|_| SupplierSecretsError::msg("credential plaintext not utf-8"))
}

/// 使用默认上下文密封凭据（兼容尚未传入 `owner_id` 的调用点）。
///
/// # Errors
///
/// 同 [`encrypt_secret_with_aad`]。
pub fn encrypt_secret<'a>(
    plaintext: &str,
    keyring: impl KeyringRef<'a>,
) -> Result<String, SupplierSecretsError> {
    encrypt_secret_with_aad(
        plaintext,
        keyring,
        DEFAULT_UNSCOPED_OWNER_KIND,
        DEFAULT_UNSCOPED_OWNER_ID,
        DEFAULT_UNSCOPED_FIELD,
    )
}

/// 指定算法并使用默认上下文密封凭据。
///
/// # Errors
///
/// 同 [`encrypt_secret_with_alg_and_aad`]。
pub fn encrypt_secret_with_alg<'a>(
    plaintext: &str,
    keyring: impl KeyringRef<'a>,
    alg: AlgorithmId,
) -> Result<String, SupplierSecretsError> {
    encrypt_secret_with_alg_and_aad(
        plaintext,
        keyring,
        alg,
        DEFAULT_UNSCOPED_OWNER_KIND,
        DEFAULT_UNSCOPED_OWNER_ID,
        DEFAULT_UNSCOPED_FIELD,
    )
}

/// 使用默认上下文解密凭据。
///
/// # Errors
///
/// 同 [`decrypt_secret_with_aad`]。
pub fn decrypt_secret<'a>(
    value: &str,
    keyring: impl KeyringRef<'a>,
) -> Result<String, SupplierSecretsError> {
    decrypt_secret_with_aad(
        value,
        keyring,
        DEFAULT_UNSCOPED_OWNER_KIND,
        DEFAULT_UNSCOPED_OWNER_ID,
        DEFAULT_UNSCOPED_FIELD,
    )
}

/// 绑定显式 `(owner_kind, owner_id)` 与字段名 `field_name = key` 加密凭据表。
///
/// # Errors
///
/// 任一字段加密失败时返回错误。
pub fn encrypt_credentials_map_with_owner<'a, S: std::hash::BuildHasher>(
    creds: &mut HashMap<String, String, S>,
    keyring: impl KeyringRef<'a>,
    owner_kind: &str,
    owner_id: &str,
) -> Result<(), SupplierSecretsError> {
    let kr = keyring.into_keyring();
    for (field_name, v) in creds.iter_mut() {
        if v.is_empty() || is_encrypted_secret(v) {
            continue;
        }
        *v = encrypt_secret_with_aad(v, &*kr, owner_kind, owner_id, field_name)?;
    }
    Ok(())
}

/// 解密绑定显式 `(owner_kind, owner_id)` 与字段名 `field_name = key` 的凭据表。
///
/// # Errors
///
/// 任一字段解密失败时返回错误。
pub fn decrypt_credentials_map_with_owner<'a, S: std::hash::BuildHasher>(
    creds: &mut HashMap<String, String, S>,
    keyring: impl KeyringRef<'a>,
    owner_kind: &str,
    owner_id: &str,
) -> Result<(), SupplierSecretsError> {
    let kr = keyring.into_keyring();
    for (field_name, v) in creds.iter_mut() {
        *v = decrypt_secret_with_aad(v, &*kr, owner_kind, owner_id, field_name)?;
    }
    Ok(())
}

/// 加密 `auth_flow.credentials` 中尚未加密的值（绑定字段名 `field_name`）。
///
/// # Errors
///
/// 任一字段加密失败时返回错误。
pub fn encrypt_credentials_map<'a, S: std::hash::BuildHasher>(
    creds: &mut HashMap<String, String, S>,
    keyring: impl KeyringRef<'a>,
) -> Result<(), SupplierSecretsError> {
    encrypt_credentials_map_with_owner(creds, keyring, "supplier", DEFAULT_UNSCOPED_OWNER_ID)
}

/// 解密 `auth_flow.credentials`（数据面出站前）。
///
/// # Errors
///
/// 任一字段解密失败时返回错误。
pub fn decrypt_credentials_map<'a, S: std::hash::BuildHasher>(
    creds: &mut HashMap<String, String, S>,
    keyring: impl KeyringRef<'a>,
) -> Result<(), SupplierSecretsError> {
    decrypt_credentials_map_with_owner(creds, keyring, "supplier", DEFAULT_UNSCOPED_OWNER_ID)
}

#[cfg(test)]
pub(crate) fn install_deterministic_test_keyring() -> Arc<RootMaterialKeyring> {
    if let Ok(existing) = supplier_secrets_key() {
        return existing;
    }
    let kr = RootMaterialKeyring::from_raw_keys(
        Some("v1".to_string()),
        true,
        [("v1".to_string(), [0x42u8; 32])],
    )
    .expect("deterministic test keyring");
    install_root_material_keyring(kr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keyring_v1() -> RootMaterialKeyring {
        RootMaterialKeyring::from_raw_keys(
            Some("v1".to_string()),
            true,
            [("v1".to_string(), [0x11u8; 32])],
        )
        .expect("v1 keyring")
    }

    #[test]
    fn roundtrip_enc_v3_and_cross_owner_aad_rejection() {
        let kr = test_keyring_v1();
        let enc = encrypt_secret_with_aad("s3cret", &kr, "supplier", "sup-1", "client_secret")
            .expect("encrypt");
        assert!(is_encrypted_secret(&enc));
        assert!(enc.starts_with("enc:v3:v1:aes-256-gcm:"));
        assert_eq!(
            decrypt_secret_with_aad(&enc, &kr, "supplier", "sup-1", "client_secret")
                .expect("decrypt"),
            "s3cret"
        );
        // 幂等
        assert_eq!(
            encrypt_secret_with_aad(&enc, &kr, "supplier", "sup-1", "client_secret")
                .expect("idempotent"),
            enc
        );

        // 跨供应商拷贝或跨字段替换必须触发 AAD 认证失败
        assert!(
            decrypt_secret_with_aad(&enc, &kr, "supplier", "sup-2", "client_secret").is_err(),
            "cross-supplier ciphertext transplant must fail"
        );
        assert!(
            decrypt_secret_with_aad(&enc, &kr, "supplier", "sup-1", "api_key").is_err(),
            "cross-field ciphertext transplant must fail"
        );
    }

    #[test]
    fn multi_version_keyring_decrypts_historical_v1_after_rekey_to_v2_and_blocks_expired_seal() {
        let kr_v1 = test_keyring_v1();
        let enc_v1 = encrypt_secret_with_alg_and_aad(
            "legacy-secret",
            &kr_v1,
            AlgorithmId::Chacha20Poly1305,
            "channel",
            "ch-9",
            "tls.client_key",
        )
        .expect("seal v1");
        assert!(enc_v1.starts_with("enc:v3:v1:chacha20-poly1305:"));

        let kr_v2 = RootMaterialKeyring::from_raw_keys(
            Some("v2".to_string()),
            true,
            [
                ("v1".to_string(), [0x11u8; 32]),
                ("v2".to_string(), [0x22u8; 32]),
            ],
        )
        .expect("v2 keyring");
        assert_eq!(
            decrypt_secret_with_aad(&enc_v1, &kr_v2, "channel", "ch-9", "tls.client_key")
                .expect("decrypt v1 with v2 keyring"),
            "legacy-secret"
        );
        let enc_v2 =
            encrypt_secret_with_aad("fresh-secret", &kr_v2, "channel", "ch-9", "tls.client_key")
                .expect("seal v2");
        assert!(enc_v2.starts_with("enc:v3:v2:aes-256-gcm:"));

        // Grace / expired license state: decryption allowed, new encryption disallowed.
        let kr_grace = RootMaterialKeyring::from_raw_keys(
            Some("v2".to_string()),
            false,
            [
                ("v1".to_string(), [0x11u8; 32]),
                ("v2".to_string(), [0x22u8; 32]),
            ],
        )
        .expect("grace keyring");
        assert_eq!(
            decrypt_secret_with_aad(&enc_v2, &kr_grace, "channel", "ch-9", "tls.client_key")
                .expect("decrypt in grace"),
            "fresh-secret"
        );
        assert!(
            encrypt_secret_with_aad("denied", &kr_grace, "channel", "ch-9", "tls.client_key")
                .is_err(),
            "new encryption must be rejected when issuance_allowed == false"
        );
    }

    #[test]
    fn rejects_legacy_enc_v2_and_enc_hex_prefixes() {
        let kr = test_keyring_v1();
        for legacy in ["enc:v2:aes-256-gcm:AAAA", "enc:deadbeef"] {
            assert!(decrypt_secret(legacy, &kr).is_err(), "{legacy}");
            assert!(encrypt_secret(legacy, &kr).is_err(), "{legacy}");
        }
    }
}
