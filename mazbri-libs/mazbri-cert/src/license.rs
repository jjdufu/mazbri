use crate::error::{CertError, CertResult};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD},
};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use uuid::Uuid;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroize;

pub const LICENSE_VERSION: i64 = 3;
pub const LICENSE_PURPOSE: &str = "commercial-license";
pub const LICENSE_TYP: &str = "mazbri-license+json";
include!(concat!(env!("OUT_DIR"), "/license_pub.rs"));

pub const LICENSE_KID_DEV: &str = "mazbri-license-dev";
pub const LICENSE_ASSET_INFO: &str = "mazbri-license-asset-v1";
pub const LICENSE_MAX_FILE_BYTES: usize = 64 * 1024;
pub const DEFAULT_GRACE_DAYS: i64 = 14;
pub const CLOCK_ROLLBACK_THRESHOLD: Duration = Duration::hours(24);

pub const LICENSE_SIGNATURE_INVALID: &str = "license_signature_invalid";
pub const LICENSE_KID_UNKNOWN: &str = "license_kid_unknown";
pub const LICENSE_INSTALLATION_MISMATCH: &str = "license_installation_mismatch";
pub const LICENSE_SEQUENCE_NOT_NEWER: &str = "license_sequence_not_newer";
pub const LICENSE_KEY_VERSION_MISMATCH: &str = "license_key_version_mismatch";
pub const LICENSE_ASSET_DIGEST_MISMATCH: &str = "license_asset_digest_mismatch";
pub const LICENSE_INSTALLATION_KEY_MISSING: &str = "license_installation_key_missing";

#[cfg(debug_assertions)]
const DEV_PUBLIC_KEY: [u8; 32] = [
    0xcb, 0x86, 0x7d, 0x1d, 0xc2, 0x71, 0x74, 0xfa, 0x0a, 0xa1, 0x29, 0x75, 0xfa, 0xaa, 0x4c, 0x90,
    0x61, 0x57, 0x5e, 0xe2, 0x5f, 0x03, 0x10, 0xb4, 0x16, 0x01, 0x91, 0xc0, 0x6d, 0xc9, 0x63, 0xe5,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseOperation {
    Bootstrap,
    Renewal,
    Rekey,
}

impl LicenseOperation {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Renewal => "renewal",
            Self::Rekey => "rekey",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "bootstrap" => Some(Self::Bootstrap),
            "renewal" => Some(Self::Renewal),
            "rekey" => Some(Self::Rekey),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct LicenseHeader {
    alg: String,
    kid: String,
    typ: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LicensePayload {
    pub version: i64,
    pub purpose: String,
    pub operation: String,
    pub package_id: String,
    pub sequence: i64,
    pub customer_id: String,
    pub installation_id: String,
    pub not_before: String,
    pub expires_at: String,
    pub grace_days: i64,
    pub key_version: String,
    pub previous_key_version: Option<String>,
    pub asset_digest: String,
    pub sealed_assets: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VerifiedLicense {
    pub raw: String,
    pub kid: String,
    pub payload: LicensePayload,
    pub operation: LicenseOperation,
    pub package_id: Uuid,
    pub installation_id: Uuid,
    pub not_before: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub grace_until: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LicenseAssets {
    pub key_password: String,
    pub master_key: String,
    pub integrity_key: String,
    pub ca_validity_days: i64,
    pub cert_validity_days: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InstallationKeyFile {
    pub version: i64,
    pub installation_id: Uuid,
    pub public_key: String,
    pub private_key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseRequestFile {
    pub version: i64,
    pub installation_id: Uuid,
    pub public_key: String,
    pub current_sequence: i64,
    pub key_version: Option<String>,
    pub customer_id: Option<String>,
    pub product_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicenseDecision {
    pub status: String,
    pub issuance_allowed: bool,
    pub decryption_allowed: bool,
    pub sequence: Option<i64>,
    pub key_version: Option<String>,
    pub customer_id: Option<String>,
    pub installation_id: Option<Uuid>,
    pub not_before: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub grace_until: Option<DateTime<Utc>>,
    pub seconds_to_expiry: Option<i64>,
    pub seconds_to_grace_end: Option<i64>,
    pub clock_rollback: bool,
    pub mark_expired_sticky: bool,
}

pub fn license_error(code: &'static str, message: impl Into<String>) -> CertError {
    CertError::License {
        code,
        message: message.into(),
    }
}

#[must_use]
pub fn trusted_verifying_key(kid: &str) -> Option<VerifyingKey> {
    let bytes = match kid {
        LICENSE_KID => &PRODUCTION_PUBLIC_KEY,
        #[cfg(debug_assertions)]
        LICENSE_KID_DEV => &DEV_PUBLIC_KEY,
        _ => return None,
    };
    VerifyingKey::from_bytes(bytes).ok()
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn verify_compact_license(raw: &str) -> CertResult<VerifiedLicense> {
    if raw.len() > LICENSE_MAX_FILE_BYTES || raw.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return Err(license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"));
    }
    let mut parts = raw.split('.');
    let header_b64 = parts.next().unwrap_or_default();
    let payload_b64 = parts.next().unwrap_or_default();
    let signature_b64 = parts.next().unwrap_or_default();
    if header_b64.is_empty()
        || payload_b64.is_empty()
        || signature_b64.is_empty()
        || parts.next().is_some()
    {
        return Err(license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"));
    }
    let header_bytes = decode_b64url(header_b64)?;
    let header: LicenseHeader = serde_json::from_slice(&header_bytes)
        .map_err(|_| license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"))?;
    if header.alg != "EdDSA" || header.typ != LICENSE_TYP {
        return Err(license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"));
    }
    let verifying_key = trusted_verifying_key(&header.kid)
        .ok_or_else(|| license_error(LICENSE_KID_UNKNOWN, "未知的许可证密钥编号"))?;
    let signature_bytes = decode_b64url(signature_b64)?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"))?;
    let signing_input = format!("{header_b64}.{payload_b64}");
    verifying_key
        .verify_strict(signing_input.as_bytes(), &signature)
        .map_err(|_| license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"))?;
    let payload_bytes = decode_b64url(payload_b64)?;
    let payload: LicensePayload = serde_json::from_slice(&payload_bytes)
        .map_err(|_| license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"))?;
    let verified = interpret_payload(raw.to_string(), header.kid, payload)?;
    Ok(verified)
}

fn interpret_payload(
    raw: String,
    kid: String,
    payload: LicensePayload,
) -> CertResult<VerifiedLicense> {
    if payload.version != LICENSE_VERSION || payload.purpose != LICENSE_PURPOSE {
        return Err(license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"));
    }
    let operation = LicenseOperation::parse(&payload.operation)
        .ok_or_else(|| license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"))?;
    if payload.sequence <= 0
        || payload.customer_id.is_empty()
        || payload.customer_id.chars().count() > 128
        || payload.key_version.is_empty()
        || payload.key_version.chars().count() > 32
        || payload.grace_days < 0
        || payload.grace_days > 30
        || !is_lower_hex_sha256(&payload.asset_digest)
    {
        return Err(license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"));
    }
    let package_id = Uuid::parse_str(&payload.package_id)
        .map_err(|_| license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"))?;
    let installation_id = Uuid::parse_str(&payload.installation_id)
        .map_err(|_| license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"))?;
    let not_before = parse_rfc3339(&payload.not_before)?;
    let expires_at = parse_rfc3339(&payload.expires_at)?;
    if expires_at <= not_before {
        return Err(license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"));
    }
    match operation {
        LicenseOperation::Renewal => {
            if payload.previous_key_version.is_some() || payload.sealed_assets.is_some() {
                return Err(license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"));
            }
        }
        LicenseOperation::Bootstrap => {
            if payload.previous_key_version.is_some() || payload.sealed_assets.is_none() {
                return Err(license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"));
            }
        }
        LicenseOperation::Rekey => {
            if payload
                .previous_key_version
                .as_ref()
                .is_none_or(std::string::String::is_empty)
                || payload.sealed_assets.is_none()
            {
                return Err(license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"));
            }
        }
    }
    let grace_until = expires_at + Duration::days(payload.grace_days);
    Ok(VerifiedLicense {
        raw,
        kid,
        payload,
        operation,
        package_id,
        installation_id,
        not_before,
        expires_at,
        grace_until,
    })
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn canonical_asset_json(assets: &LicenseAssets) -> CertResult<Vec<u8>> {
    validate_assets(assets)?;
    serde_json::to_vec(assets).map_err(|error| {
        CertError::InvalidRequest(format!("Failed to serialize license assets: {error}"))
    })
}

#[must_use]
pub fn asset_digest_hex(plaintext: &[u8]) -> String {
    hex::encode(Sha256::digest(plaintext))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_assets(assets: &LicenseAssets) -> CertResult<()> {
    if assets.key_password.trim().is_empty() {
        return Err(CertError::InvalidRequest(
            "keyPassword is required".to_string(),
        ));
    }
    if assets.ca_validity_days <= 0 || assets.cert_validity_days <= 0 {
        return Err(CertError::InvalidRequest(
            "certificate validity values must be positive".to_string(),
        ));
    }
    if assets.cert_validity_days >= assets.ca_validity_days {
        return Err(CertError::InvalidRequest(
            "certValidityDays must be less than caValidityDays".to_string(),
        ));
    }
    crate::root_package::validate_material_encoding(&assets.master_key)?;
    crate::root_package::validate_material_encoding(&assets.integrity_key)?;
    Ok(())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn seal_for_installation(
    installation_public: &[u8; 32],
    aad: &str,
    plaintext: &[u8],
) -> CertResult<String> {
    let ephemeral_bytes: [u8; 32] = rand::random();
    let ephemeral = StaticSecret::from(ephemeral_bytes);
    let ephemeral_public = PublicKey::from(&ephemeral);
    let installation = PublicKey::from(*installation_public);
    let shared = ephemeral.diffie_hellman(&installation);
    seal_with_shared(
        ephemeral_public.as_bytes(),
        installation_public,
        shared.as_bytes(),
        aad,
        plaintext,
    )
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn unseal_for_installation(
    installation_private: &[u8; 32],
    installation_public: &[u8; 32],
    aad: &str,
    sealed: &str,
) -> CertResult<Vec<u8>> {
    let bytes = BASE64
        .decode(sealed.as_bytes())
        .map_err(|_| license_error(LICENSE_ASSET_DIGEST_MISMATCH, "许可证资产摘要不一致"))?;
    if bytes.len() < 32 + 12 + 16 {
        return Err(license_error(
            LICENSE_ASSET_DIGEST_MISMATCH,
            "许可证资产摘要不一致",
        ));
    }
    let (ephemeral_public, rest) = bytes.split_at(32);
    let (nonce, ciphertext) = rest.split_at(12);
    let mut ephemeral_public_arr = [0u8; 32];
    ephemeral_public_arr.copy_from_slice(ephemeral_public);
    let static_secret = StaticSecret::from(*installation_private);
    let shared = static_secret.diffie_hellman(&PublicKey::from(ephemeral_public_arr));
    let mut cek = derive_cek(
        shared.as_bytes(),
        &ephemeral_public_arr,
        installation_public,
    )?;
    let cipher = Aes256Gcm::new_from_slice(&cek).map_err(|error| {
        CertError::KeyEncryption(format!("Failed to create license cipher: {error}"))
    })?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| license_error(LICENSE_ASSET_DIGEST_MISMATCH, "许可证资产摘要不一致"));
    cek.zeroize();
    plaintext
}

#[must_use]
pub fn asset_aad(installation_id: Uuid, key_version: &str, package_id: Uuid) -> String {
    format!("assets|{installation_id}|{key_version}|{package_id}")
}

#[must_use]
pub fn key_password_aad(installation_id: Uuid, key_version: &str) -> String {
    format!("key-password|{installation_id}|{key_version}")
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn open_assets(
    installation_private: &[u8; 32],
    installation_public: &[u8; 32],
    license: &VerifiedLicense,
) -> CertResult<LicenseAssets> {
    let sealed = license
        .payload
        .sealed_assets
        .as_deref()
        .ok_or_else(|| license_error(LICENSE_ASSET_DIGEST_MISMATCH, "许可证资产摘要不一致"))?;
    let aad = asset_aad(
        license.installation_id,
        &license.payload.key_version,
        license.package_id,
    );
    let plaintext =
        unseal_for_installation(installation_private, installation_public, &aad, sealed)?;
    if asset_digest_hex(&plaintext) != license.payload.asset_digest {
        return Err(license_error(
            LICENSE_ASSET_DIGEST_MISMATCH,
            "许可证资产摘要不一致",
        ));
    }
    let assets: LicenseAssets = serde_json::from_slice(&plaintext)
        .map_err(|_| license_error(LICENSE_ASSET_DIGEST_MISMATCH, "许可证资产摘要不一致"))?;
    validate_assets(&assets)?;
    Ok(assets)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn seal_key_password(
    installation_public: &[u8; 32],
    installation_id: Uuid,
    key_version: &str,
    password: &str,
) -> CertResult<String> {
    seal_for_installation(
        installation_public,
        &key_password_aad(installation_id, key_version),
        password.as_bytes(),
    )
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn open_key_password(
    installation_private: &[u8; 32],
    installation_public: &[u8; 32],
    installation_id: Uuid,
    key_version: &str,
    sealed: &str,
) -> CertResult<String> {
    let plaintext = unseal_for_installation(
        installation_private,
        installation_public,
        &key_password_aad(installation_id, key_version),
        sealed,
    )?;
    String::from_utf8(plaintext)
        .map_err(|_| license_error(LICENSE_ASSET_DIGEST_MISMATCH, "许可证资产摘要不一致"))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn decode_key_material(value: &str, label: &str) -> CertResult<[u8; 32]> {
    let bytes = BASE64
        .decode(value.as_bytes())
        .map_err(|_| CertError::InvalidRequest(format!("{label} 不是有效的 Base64")))?;
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| CertError::InvalidRequest(format!("{label} 必须是 32 字节")))?;
    Ok(array)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn load_installation_key(path: &Path) -> CertResult<InstallationKeyFile> {
    let raw = std::fs::read(path).map_err(|_| {
        license_error(
            LICENSE_INSTALLATION_KEY_MISSING,
            "未配置或无法读取安装私钥文件，请设置 installation_key_path",
        )
    })?;
    let file: InstallationKeyFile = serde_json::from_slice(&raw).map_err(|_| {
        license_error(
            LICENSE_INSTALLATION_KEY_MISSING,
            "未配置或无法读取安装私钥文件，请设置 installation_key_path",
        )
    })?;
    if file.version != 1 {
        return Err(license_error(
            LICENSE_INSTALLATION_KEY_MISSING,
            "未配置或无法读取安装私钥文件，请设置 installation_key_path",
        ));
    }
    let _ = decode_key_material(&file.public_key, "publicKey")?;
    let _ = decode_key_material(&file.private_key, "privateKey")?;
    Ok(file)
}

#[must_use]
pub fn generate_installation_key() -> InstallationKeyFile {
    let secret_bytes: [u8; 32] = rand::random();
    let secret = StaticSecret::from(secret_bytes);
    let public = PublicKey::from(&secret);
    InstallationKeyFile {
        version: 1,
        installation_id: Uuid::new_v4(),
        public_key: BASE64.encode(public.as_bytes()),
        private_key: BASE64.encode(secret.to_bytes()),
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn ensure_installation_key(path: &Path) -> CertResult<InstallationKeyFile> {
    if path.exists() {
        return load_installation_key(path);
    }
    let Some(parent) = path.parent() else {
        return Err(license_error(
            LICENSE_INSTALLATION_KEY_MISSING,
            "未配置或无法读取安装私钥文件，请设置 installation_key_path",
        ));
    };
    std::fs::create_dir_all(parent)
        .map_err(|error| CertError::InvalidRequest(format!("无法创建安装私钥目录: {error}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).map_err(
            |error| CertError::InvalidRequest(format!("无法设置安装私钥目录权限: {error}")),
        )?;
    }
    let secret_bytes: [u8; 32] = rand::random();
    let secret = StaticSecret::from(secret_bytes);
    let public = PublicKey::from(&secret);
    let file = InstallationKeyFile {
        version: 1,
        installation_id: Uuid::new_v4(),
        public_key: BASE64.encode(public.as_bytes()),
        private_key: BASE64.encode(secret.to_bytes()),
    };
    let encoded = serde_json::to_vec_pretty(&file)
        .map_err(|error| CertError::InvalidRequest(format!("无法序列化安装私钥: {error}")))?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|error| CertError::InvalidRequest(format!("无法写入安装私钥: {error}")))?;
        output
            .write_all(&encoded)
            .map_err(|error| CertError::InvalidRequest(format!("无法写入安装私钥: {error}")))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, &encoded)
            .map_err(|error| CertError::InvalidRequest(format!("无法写入安装私钥: {error}")))?;
    }
    if path.exists() {
        return load_installation_key(path);
    }
    Ok(file)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn require_installation_key_path(path: Option<&str>) -> CertResult<&str> {
    path.map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            license_error(
                LICENSE_INSTALLATION_KEY_MISSING,
                "未配置或无法读取安装私钥文件，请设置 installation_key_path",
            )
        })
}

#[must_use]
pub fn decide_license(
    verified: Option<&VerifiedLicense>,
    installation_matches: bool,
    now: DateTime<Utc>,
    expired_sticky_sequence: Option<i64>,
    last_seen_at: Option<DateTime<Utc>>,
    password_unseals: bool,
    materials_present: bool,
) -> LicenseDecision {
    let clock_rollback = last_seen_at.is_some_and(|seen| seen - now > CLOCK_ROLLBACK_THRESHOLD);
    let Some(license) = verified else {
        return empty_decision("invalid", clock_rollback);
    };
    if !installation_matches {
        return empty_decision("invalid", clock_rollback);
    }
    let sticky = expired_sticky_sequence == Some(license.payload.sequence);
    let in_active = license.not_before <= now && now < license.expires_at;
    let in_grace = license.expires_at <= now && now < license.grace_until;
    let (status, issuance_allowed, mark_expired_sticky) = if sticky || (!in_active && !in_grace) {
        let mark = !sticky && now >= license.grace_until;
        ("expired", false, mark)
    } else if in_grace {
        ("grace", false, false)
    } else if in_active {
        ("active", true, false)
    } else {
        ("pending", false, false)
    };
    let decryption_allowed = password_unseals && materials_present;
    LicenseDecision {
        status: status.to_string(),
        issuance_allowed,
        decryption_allowed,
        sequence: Some(license.payload.sequence),
        key_version: Some(license.payload.key_version.clone()),
        customer_id: Some(license.payload.customer_id.clone()),
        installation_id: Some(license.installation_id),
        not_before: Some(license.not_before),
        expires_at: Some(license.expires_at),
        grace_until: Some(license.grace_until),
        seconds_to_expiry: Some((license.expires_at - now).num_seconds()),
        seconds_to_grace_end: Some((license.grace_until - now).num_seconds()),
        clock_rollback,
        mark_expired_sticky,
    }
}

#[must_use]
pub fn unconfigured_decision() -> LicenseDecision {
    empty_decision("unconfigured", false)
}

fn empty_decision(status: &str, clock_rollback: bool) -> LicenseDecision {
    LicenseDecision {
        status: status.to_string(),
        issuance_allowed: false,
        decryption_allowed: false,
        sequence: None,
        key_version: None,
        customer_id: None,
        installation_id: None,
        not_before: None,
        expires_at: None,
        grace_until: None,
        seconds_to_expiry: None,
        seconds_to_grace_end: None,
        clock_rollback,
        mark_expired_sticky: false,
    }
}

#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn seal_with_shared(
    ephemeral_public: &[u8; 32],
    installation_public: &[u8; 32],
    shared: &[u8],
    aad: &str,
    plaintext: &[u8],
) -> CertResult<String> {
    let mut cek = derive_cek(shared, ephemeral_public, installation_public)?;
    let cipher = Aes256Gcm::new_from_slice(&cek).map_err(|error| {
        CertError::KeyEncryption(format!("Failed to create license cipher: {error}"))
    })?;
    let nonce_bytes: [u8; 12] = rand::random();
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: plaintext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|error| {
            CertError::KeyEncryption(format!("Failed to seal license asset: {error}"))
        })?;
    cek.zeroize();
    let mut packed = Vec::with_capacity(32 + 12 + encrypted.len());
    packed.extend_from_slice(ephemeral_public);
    packed.extend_from_slice(&nonce_bytes);
    packed.extend_from_slice(&encrypted);
    Ok(BASE64.encode(packed))
}

fn derive_cek(
    shared: &[u8],
    ephemeral_public: &[u8; 32],
    installation_public: &[u8; 32],
) -> CertResult<[u8; 32]> {
    let mut salt = Vec::with_capacity(64);
    salt.extend_from_slice(ephemeral_public);
    salt.extend_from_slice(installation_public);
    let hkdf = Hkdf::<Sha256>::new(Some(&salt), shared);
    let mut cek = [0u8; 32];
    hkdf.expand(LICENSE_ASSET_INFO.as_bytes(), &mut cek)
        .map_err(|error| {
            CertError::KeyEncryption(format!("Failed to derive license key: {error}"))
        })?;
    Ok(cek)
}

fn decode_b64url(value: &str) -> CertResult<Vec<u8>> {
    if value.contains('=') || value.contains('+') || value.contains('/') {
        return Err(license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"));
    }
    URL_SAFE_NO_PAD
        .decode(value.as_bytes())
        .map_err(|_| license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"))
}

fn parse_rfc3339(value: &str) -> CertResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| license_error(LICENSE_SIGNATURE_INVALID, "许可证签名无效"))
}

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use mazbri_core::ErrorResponse;

    fn dev_signing_key() -> SigningKey {
        let raw = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/mazbri-license-dev.json"
        ))
        .expect("dev signing fixture");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("fixture json");
        let private = BASE64
            .decode(value["privateKey"].as_str().expect("privateKey"))
            .expect("private key bytes");
        let bytes: [u8; 32] = private.try_into().expect("32 byte key");
        SigningKey::from_bytes(&bytes)
    }

    fn sample_payload(operation: &str, sealed: Option<&str>, previous: Option<&str>) -> String {
        let payload = LicensePayload {
            version: 3,
            purpose: LICENSE_PURPOSE.to_string(),
            operation: operation.to_string(),
            package_id: "6f2d9c12-3b4e-4f8a-9a1c-5d8e7b6a4c21".to_string(),
            sequence: 1,
            customer_id: "enterprise-client-001".to_string(),
            installation_id: "3b1f0c2a-7e44-4d0a-9c11-2a6e5d8b7c90".to_string(),
            not_before: "2026-05-18T00:00:00Z".to_string(),
            expires_at: "2027-05-18T00:00:00Z".to_string(),
            grace_days: 14,
            key_version: "v1".to_string(),
            previous_key_version: previous.map(str::to_string),
            asset_digest: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
            sealed_assets: sealed.map(str::to_string),
        };
        serde_json::to_string(&payload).expect("payload")
    }

    fn sign_with(key: &SigningKey, kid: &str, payload_json: &str) -> String {
        let header = format!(r#"{{"alg":"EdDSA","kid":"{kid}","typ":"mazbri-license+json"}}"#);
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
        let input = format!("{header_b64}.{payload_b64}");
        let signature = key.sign(input.as_bytes());
        format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()))
    }

    #[test]
    fn signed_license_verifies_and_one_byte_change_fails() {
        if !cfg!(debug_assertions) {
            return;
        }
        let key = dev_signing_key();
        let jws = sign_with(
            &key,
            LICENSE_KID_DEV,
            &sample_payload("renewal", None, None),
        );
        let verified = verify_compact_license(&jws).expect("verify");
        assert_eq!(verified.payload.sequence, 1);
        let mut tampered = jws.into_bytes();
        let dot = tampered.iter().position(|byte| *byte == b'.').expect("dot");
        let index = dot + 2;
        tampered[index] = if tampered[index] == b'A' { b'B' } else { b'A' };
        let tampered = String::from_utf8(tampered).expect("ascii");
        let error = verify_compact_license(&tampered).expect_err("tamper");
        assert_eq!(error.error_code(), LICENSE_SIGNATURE_INVALID);
    }

    #[test]
    fn unknown_kid_is_rejected() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let jws = sign_with(
            &key,
            "mazbri-license-unknown",
            &sample_payload("renewal", None, None),
        );
        let error = verify_compact_license(&jws).expect_err("kid");
        assert_eq!(error.error_code(), LICENSE_KID_UNKNOWN);
    }

    #[test]
    fn dev_kid_is_only_trusted_in_debug() {
        let trusted = trusted_verifying_key(LICENSE_KID_DEV).is_some();
        assert_eq!(trusted, cfg!(debug_assertions));
        if !cfg!(debug_assertions) {
            let key = SigningKey::from_bytes(&[9u8; 32]);
            let jws = sign_with(
                &key,
                LICENSE_KID_DEV,
                &sample_payload("renewal", None, None),
            );
            let error = verify_compact_license(&jws).expect_err("dev kid");
            assert_eq!(error.error_code(), LICENSE_KID_UNKNOWN);
        }
    }

    #[test]
    fn grace_stops_issuance_but_keeps_decryption() {
        let key = dev_signing_key();
        if !cfg!(debug_assertions) {
            return;
        }
        let jws = sign_with(
            &key,
            LICENSE_KID_DEV,
            &sample_payload("renewal", None, None),
        );
        let verified = verify_compact_license(&jws).expect("verify");
        let now = verified.expires_at + Duration::days(1);
        let decision = decide_license(Some(&verified), true, now, None, None, true, true);
        assert_eq!(decision.status, "grace");
        assert!(!decision.issuance_allowed);
        assert!(decision.decryption_allowed);
        assert!(decision.seconds_to_expiry.is_some_and(|value| value <= 0));
        assert!(decision.seconds_to_grace_end.is_some_and(|value| value > 0));
    }

    #[test]
    fn sticky_expiry_survives_clock_rollback_inside_the_window() {
        if !cfg!(debug_assertions) {
            return;
        }
        let key = dev_signing_key();
        let jws = sign_with(
            &key,
            LICENSE_KID_DEV,
            &sample_payload("renewal", None, None),
        );
        let verified = verify_compact_license(&jws).expect("verify");
        let expired_now = verified.grace_until + Duration::seconds(1);
        let expired = decide_license(Some(&verified), true, expired_now, None, None, true, true);
        assert!(expired.mark_expired_sticky);
        assert_eq!(expired.status, "expired");
        let rolled = verified.not_before + Duration::hours(1);
        let sticky = decide_license(
            Some(&verified),
            true,
            rolled,
            Some(verified.payload.sequence),
            Some(expired_now),
            true,
            true,
        );
        assert_eq!(sticky.status, "expired");
        assert!(!sticky.issuance_allowed);
        assert!(sticky.decryption_allowed);
    }

    #[test]
    fn one_hour_clock_rollback_does_not_change_issuance() {
        if !cfg!(debug_assertions) {
            return;
        }
        let key = dev_signing_key();
        let jws = sign_with(
            &key,
            LICENSE_KID_DEV,
            &sample_payload("renewal", None, None),
        );
        let verified = verify_compact_license(&jws).expect("verify");
        let now = verified.not_before + Duration::days(10);
        let seen = now + Duration::hours(1);
        let decision = decide_license(Some(&verified), true, now, None, Some(seen), true, true);
        assert!(!decision.clock_rollback);
        assert!(decision.issuance_allowed);
        let far = decide_license(
            Some(&verified),
            true,
            now,
            None,
            Some(now + Duration::hours(25)),
            true,
            true,
        );
        assert!(far.clock_rollback);
        assert!(far.issuance_allowed);
    }

    #[test]
    fn table_expiry_is_not_an_input_to_the_decision() {
        if !cfg!(debug_assertions) {
            return;
        }
        let key = dev_signing_key();
        let jws = sign_with(
            &key,
            LICENSE_KID_DEV,
            &sample_payload("renewal", None, None),
        );
        let verified = verify_compact_license(&jws).expect("verify");
        let now = verified.not_before + Duration::days(1);
        let decision = decide_license(Some(&verified), true, now, None, None, true, true);
        assert!(decision.issuance_allowed);
        assert_eq!(decision.expires_at, Some(verified.expires_at));
    }

    #[test]
    fn asset_roundtrip_covers_digest_and_aad_separation() {
        let secret_bytes: [u8; 32] = rand::random();
        let installation_secret = StaticSecret::from(secret_bytes);
        let installation_public = PublicKey::from(&installation_secret);
        let installation_id =
            Uuid::parse_str("3b1f0c2a-7e44-4d0a-9c11-2a6e5d8b7c90").expect("uuid");
        let package_id = Uuid::parse_str("6f2d9c12-3b4e-4f8a-9a1c-5d8e7b6a4c21").expect("uuid");
        let assets = LicenseAssets {
            key_password: "password".to_string(),
            master_key: BASE64.encode([1u8; 32]),
            integrity_key: BASE64.encode([2u8; 32]),
            ca_validity_days: 3650,
            cert_validity_days: 365,
        };
        let plaintext = canonical_asset_json(&assets).expect("json");
        let digest = asset_digest_hex(&plaintext);
        let sealed = seal_for_installation(
            installation_public.as_bytes(),
            &asset_aad(installation_id, "v1", package_id),
            &plaintext,
        )
        .expect("seal");
        let opened = unseal_for_installation(
            installation_secret.as_bytes(),
            installation_public.as_bytes(),
            &asset_aad(installation_id, "v1", package_id),
            &sealed,
        )
        .expect("open");
        assert_eq!(asset_digest_hex(&opened), digest);
        let password_sealed = seal_key_password(
            installation_public.as_bytes(),
            installation_id,
            "v1",
            "password",
        )
        .expect("seal password");
        assert!(
            unseal_for_installation(
                installation_secret.as_bytes(),
                installation_public.as_bytes(),
                &asset_aad(installation_id, "v1", package_id),
                &password_sealed,
            )
            .is_err()
        );
    }

    #[test]
    fn canonical_asset_json_is_byte_stable_for_issued_digests() {
        let assets = LicenseAssets {
            key_password: "p".to_string(),
            master_key: BASE64.encode([1u8; 32]),
            integrity_key: BASE64.encode([2u8; 32]),
            ca_validity_days: 3650,
            cert_validity_days: 365,
        };
        let plaintext = canonical_asset_json(&assets).expect("json");
        let expected = format!(
            r#"{{"keyPassword":"p","masterKey":"{}","integrityKey":"{}","caValidityDays":3650,"certValidityDays":365}}"#,
            assets.master_key, assets.integrity_key
        );
        assert_eq!(String::from_utf8(plaintext).expect("utf8"), expected);
    }
}
