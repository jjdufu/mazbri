use super::envelope::{TenantKeyEnvelope, parse_envelope, serialize_envelope};
use super::transport::{
    DATA_ALGORITHM, SIGNATURE_ALGORITHM, TENANT_SECRET_KEY_BYTES, decrypt_aes_256_gcm,
    encrypt_aes_256_gcm, sign_payload,
};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::Deserialize;

pub const COMMERCIAL_ROOT_PROTECTION_WRAPPING_KEY_ID: &str =
    "env:MAZBRI_COMMERCIAL_ROOT_PROTECTION_MASTER_KEY";
pub const COMMERCIAL_ROOT_PROTECTION_MASTER_KEY_ENV: &str =
    "MAZBRI_COMMERCIAL_ROOT_PROTECTION_MASTER_KEY";
pub const COMMERCIAL_ROOT_PROTECTION_INTEGRITY_KEY_ENV: &str =
    "MAZBRI_COMMERCIAL_ROOT_PROTECTION_INTEGRITY_KEY";

#[derive(Debug, thiserror::Error)]
pub enum TenantKeyError {
    #[error("tenant crypto master key is not configured")]
    MasterKeyMissing,
    #[error("tenant crypto integrity key is not configured")]
    IntegrityKeyMissing,
    #[error("tenant crypto key material must be 32 bytes")]
    InvalidKeyLength,
    #[error("tenant root material request failed: {0}")]
    RootMaterialRequestFailed(String),
    #[error("tenant key envelope error: {0}")]
    Envelope(#[from] super::envelope::CryptoEnvelopeError),
    #[error("tenant transport crypto error: {0}")]
    Transport(#[from] super::transport::TransportCryptoError),
    #[error("tenant key encoding is invalid")]
    InvalidEncoding,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootProtectionMaterialBundle {
    pub current_key_version: String,
    pub wrapping_key_id: String,
    pub master_key: String,
    pub integrity_key: String,
}

#[derive(Debug, Clone)]
pub struct WrappedTenantKey {
    pub tenant_secret_key_b64: String,
    pub encrypted_secret_key: String,
    pub secret_key_digest: String,
    pub wrapping_key_id: String,
}

#[must_use]
pub fn generate_tenant_secret_key() -> [u8; TENANT_SECRET_KEY_BYTES] {
    rand::random()
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

pub fn load_master_key_from_env() -> Result<Vec<u8>, TenantKeyError> {
    load_key_from_env(COMMERCIAL_ROOT_PROTECTION_MASTER_KEY_ENV).map_err(|error| match error {
        TenantKeyError::InvalidEncoding | TenantKeyError::InvalidKeyLength => error,
        _ => TenantKeyError::MasterKeyMissing,
    })
}

pub fn load_integrity_key_from_env() -> Result<Vec<u8>, TenantKeyError> {
    load_key_from_env(COMMERCIAL_ROOT_PROTECTION_INTEGRITY_KEY_ENV).map_err(|error| match error {
        TenantKeyError::InvalidEncoding | TenantKeyError::InvalidKeyLength => error,
        _ => TenantKeyError::IntegrityKeyMissing,
    })
}

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

pub fn load_key_from_env(name: &str) -> Result<Vec<u8>, TenantKeyError> {
    let value = std::env::var(name).map_err(|_| TenantKeyError::MasterKeyMissing)?;
    decode_configured_key(&value)
}

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

pub fn wrap_new_tenant_key(
    tenant_id: &str,
    key_version: &str,
    master_key: &[u8],
    integrity_key: &[u8],
    wrapping_key_id: &str,
) -> Result<WrappedTenantKey, TenantKeyError> {
    let tenant_key = generate_tenant_secret_key();
    let aad = envelope_aad(tenant_id, key_version);
    let parts = encrypt_aes_256_gcm(master_key, &tenant_key, aad.as_bytes())?;
    let envelope =
        TenantKeyEnvelope::new(wrapping_key_id, parts.iv, parts.ciphertext, parts.tag, aad);
    let tenant_secret_key_b64 = STANDARD.encode(tenant_key);
    let secret_key_digest = secret_key_digest(integrity_key, tenant_id, key_version, &tenant_key)?;
    Ok(WrappedTenantKey {
        tenant_secret_key_b64,
        encrypted_secret_key: serialize_envelope(&envelope)?,
        secret_key_digest,
        wrapping_key_id: wrapping_key_id.to_string(),
    })
}

pub fn unwrap_tenant_key(
    encrypted_secret_key: &str,
    tenant_id: &str,
    key_version: &str,
    master_key: &[u8],
) -> Result<Vec<u8>, TenantKeyError> {
    let envelope = parse_envelope(encrypted_secret_key)?;
    let expected_aad = envelope_aad(tenant_id, key_version);
    if envelope.aad != expected_aad {
        return Err(TenantKeyError::InvalidEncoding);
    }
    let key = decrypt_aes_256_gcm(
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

pub fn verify_secret_key_digest(
    integrity_key: &[u8],
    tenant_id: &str,
    key_version: &str,
    tenant_secret_key: &[u8],
    digest: &str,
) -> Result<bool, TenantKeyError> {
    Ok(secret_key_digest(integrity_key, tenant_id, key_version, tenant_secret_key)? == digest)
}

pub fn secret_key_digest(
    integrity_key: &[u8],
    tenant_id: &str,
    key_version: &str,
    tenant_secret_key: &[u8],
) -> Result<String, TenantKeyError> {
    let payload = format!(
        "{}:{}:{}",
        tenant_id,
        key_version,
        STANDARD.encode(tenant_secret_key)
    );
    Ok(sign_payload(integrity_key, &payload)?)
}

#[must_use]
pub fn envelope_aad(tenant_id: &str, key_version: &str) -> String {
    format!("{tenant_id}:{key_version}")
}

#[must_use]
pub fn default_key_metadata() -> serde_json::Value {
    serde_json::json!({
        "dataAlgorithm": DATA_ALGORITHM,
        "signatureAlgorithm": SIGNATURE_ALGORITHM
    })
}
