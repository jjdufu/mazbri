use crate::error::{CertError, CertResult};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use hkdf::Hkdf;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const COMMERCIAL_ROOT_PROTECTION_STORAGE_INFO: &str = "commercial-root-protection-storage-v1";
pub const COMMERCIAL_ROOT_PROTECTION_STORAGE_KEY_ID: &str =
    "commercial-root-protection:key_password:v1";
pub const COMMERCIAL_ROOT_PROTECTION_MASTER_MATERIAL_TYPE: &str = "master";
pub const COMMERCIAL_ROOT_PROTECTION_INTEGRITY_MATERIAL_TYPE: &str = "integrity";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootPackageSummary {
    pub package_id: Option<uuid::Uuid>,
    pub package_version: Option<String>,
    pub package_format: Option<String>,
    pub purpose: Option<String>,
    pub package_sha256: Option<String>,
    pub kdf_name: Option<String>,
    pub kdf_params: Option<serde_json::Value>,
    pub status: String,
    pub available: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
    pub seconds_to_expiry: Option<i64>,
    pub renewable: bool,
    pub renewed_from_package_id: Option<uuid::Uuid>,
    pub superseded_by_package_id: Option<uuid::Uuid>,
    pub current_key_version: Option<String>,
    pub wrapping_key_id: Option<String>,
    pub created_by: Option<uuid::Uuid>,
    pub imported_by: Option<uuid::Uuid>,
    pub revoked_by: Option<uuid::Uuid>,
    pub created_at: Option<DateTime<Utc>>,
    pub imported_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub active_material_count: i64,
    pub deprecated_material_count: i64,
    pub disabled_material_count: i64,
    pub latest_event_type: Option<String>,
    pub latest_event_at: Option<DateTime<Utc>>,
    pub latest_event_actor_id: Option<uuid::Uuid>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootPackageAction {
    Import,
    Rotate,
    Renew,
}

impl RootPackageAction {
    #[must_use]
    pub fn event_type(self) -> &'static str {
        match self {
            Self::Import => "package_import",
            Self::Rotate => "material_rewrap",
            Self::Renew => "package_renew",
        }
    }
}

pub fn decode_material_bytes(value: &str) -> CertResult<Vec<u8>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CertError::InvalidRequest("Material is empty".to_string()));
    }
    if let Ok(bytes) = BASE64.decode(trimmed.as_bytes()) {
        return Ok(bytes);
    }
    if let Ok(bytes) = hex::decode(trimmed) {
        return Ok(bytes);
    }
    Err(CertError::InvalidRequest(
        "Material must be base64 or hex encoded".to_string(),
    ))
}

pub fn validate_material_encoding(value: &str) -> CertResult<()> {
    let bytes = decode_material_bytes(value)?;
    if bytes.len() != 32 {
        return Err(CertError::InvalidRequest(
            "Root material must be 32 bytes".to_string(),
        ));
    }
    Ok(())
}

#[must_use]
pub fn material_digest(
    package_id: uuid::Uuid,
    material_type: &str,
    key_version: &str,
    plaintext: &[u8],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(package_id.as_bytes());
    hasher.update(material_type.as_bytes());
    hasher.update(key_version.as_bytes());
    hasher.update(plaintext);
    hex::encode(hasher.finalize())
}
#[must_use]
pub fn derive_root_package_status(
    package_status: &str,
    active_material_count: i64,
    expires_at: Option<DateTime<Utc>>,
    expired_at: Option<DateTime<Utc>>,
) -> String {
    let expired = is_root_package_expired(Utc::now(), expires_at, expired_at);
    if expired && package_status != "revoked" && package_status != "deleted" {
        "expired".to_string()
    } else if active_material_count > 0
        && package_status != "revoked"
        && package_status != "deleted"
    {
        "active".to_string()
    } else if package_status == "revoked" {
        "revoked".to_string()
    } else if package_status == "deleted" {
        "deleted".to_string()
    } else if package_status == "imported" {
        "imported".to_string()
    } else if package_status == "pending" {
        "pending".to_string()
    } else {
        "unconfigured".to_string()
    }
}

#[must_use]
pub fn is_root_package_expired(
    now: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    expired_at: Option<DateTime<Utc>>,
) -> bool {
    expired_at.is_some() || expires_at.is_some_and(|expires_at| expires_at <= now)
}

#[must_use]
pub fn is_root_package_renewable(status: &str) -> bool {
    status != "revoked" && status != "deleted"
}

fn derive_storage_key(password: &str) -> [u8; 32] {
    let hkdf = Hkdf::<Sha256>::new(Some(b"mazbri-cert-root-storage-salt"), password.as_bytes());
    let mut key = [0u8; 32];
    hkdf.expand(COMMERCIAL_ROOT_PROTECTION_STORAGE_INFO.as_bytes(), &mut key)
        .expect("HKDF expand should not fail");
    key
}

pub fn wrap_root_material(
    storage_password: &str,
    package_id: uuid::Uuid,
    material_type: &str,
    key_version: &str,
    plaintext: &[u8],
) -> CertResult<(String, String, String)> {
    let storage_key = derive_storage_key(storage_password);
    let cipher = Aes256Gcm::new_from_slice(&storage_key)
        .map_err(|e| CertError::KeyEncryption(format!("Failed to create storage cipher: {e}")))?;
    let iv_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&iv_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| CertError::KeyEncryption(format!("Failed to wrap root material: {e}")))?;
    let digest = material_digest(package_id, material_type, key_version, plaintext);
    let mut encoded = iv_bytes.to_vec();
    encoded.extend(ciphertext);
    Ok((
        BASE64.encode(encoded),
        digest,
        COMMERCIAL_ROOT_PROTECTION_STORAGE_KEY_ID.to_string(),
    ))
}

pub fn unwrap_root_material(storage_password: &str, encrypted_secret: &str) -> CertResult<Vec<u8>> {
    let storage_key = derive_storage_key(storage_password);
    let encrypted = BASE64
        .decode(encrypted_secret.as_bytes())
        .map_err(|e| CertError::InvalidRequest(format!("Invalid encrypted root material: {e}")))?;
    if encrypted.len() < 28 {
        return Err(CertError::InvalidRequest(
            "Encrypted root material is too short".to_string(),
        ));
    }
    let (iv_bytes, ciphertext_and_tag) = encrypted.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(&storage_key)
        .map_err(|e| CertError::KeyEncryption(format!("Failed to create storage cipher: {e}")))?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(iv_bytes), ciphertext_and_tag)
        .map_err(|e| CertError::KeyEncryption(format!("Failed to unwrap root material: {e}")))?;
    Ok(plaintext)
}
