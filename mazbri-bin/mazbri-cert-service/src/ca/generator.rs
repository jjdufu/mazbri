use crate::config::CertConfig;
use crate::error::{CertError, CertResult};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use hkdf::Hkdf;
use rcgen::Ia5String;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair,
    PKCS_ECDSA_P256_SHA256, SanType,
};
use sha2::Sha256;
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
pub struct CaManager {
    ca_cert: Certificate,
    ca_key_pair: KeyPair,
    config: Arc<CertConfig>,
    encryption_key: [u8; 32],
}
impl CaManager {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn from_existing(
        ca_cert_pem: &str,
        ca_key_encrypted: &str,
        config: Arc<CertConfig>,
    ) -> CertResult<Self> {
        let encryption_key = Self::derive_key(config.key_password());
        let ca_key_pem = Self::decrypt_key_internal(ca_key_encrypted, &encryption_key)?;
        let ca_key_pair = KeyPair::from_pem(&ca_key_pem)
            .map_err(|e| CertError::CaInitFailed(format!("Failed to parse CA key: {e}")))?;
        let ca_params = CertificateParams::from_ca_cert_pem(ca_cert_pem)
            .map_err(|e| CertError::CaInitFailed(format!("Failed to parse CA cert: {e}")))?;
        let ca_cert = ca_params
            .self_signed(&ca_key_pair)
            .map_err(|e| CertError::CaInitFailed(format!("Failed to reconstruct CA: {e}")))?;
        Ok(Self {
            ca_cert,
            ca_key_pair,
            config,
            encryption_key,
        })
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn new(config: Arc<CertConfig>) -> CertResult<Self> {
        let (ca_cert, ca_key_pair) = Self::generate_ca(&config)?;
        let encryption_key = Self::derive_key(config.key_password());
        Ok(Self {
            ca_cert,
            ca_key_pair,
            config,
            encryption_key,
        })
    }
    fn generate_ca(config: &CertConfig) -> CertResult<(Certificate, KeyPair)> {
        let mut params = CertificateParams::default();
        params.not_before = OffsetDateTime::now_utc();
        params.not_after = OffsetDateTime::now_utc() + Duration::days(config.ca_validity_days);
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "Mazbri Internal CA");
        dn.push(DnType::OrganizationName, "Mazbri");
        dn.push(DnType::CountryName, "CN");
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let key_pair = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)
            .map_err(|e| CertError::GenerationFailed(format!("Failed to generate CA key: {e}")))?;
        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| CertError::GenerationFailed(format!("Failed to create CA cert: {e}")))?;
        Ok((cert, key_pair))
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn issue_certificate(
        &self,
        subject: &str,
        dns_names: &[String],
    ) -> CertResult<(String, String)> {
        let mut params = CertificateParams::default();
        params.not_before = OffsetDateTime::now_utc();
        params.not_after =
            OffsetDateTime::now_utc() + Duration::days(self.config.cert_validity_days);
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, subject);
        dn.push(DnType::OrganizationName, "Mazbri Service");
        params.distinguished_name = dn;
        if !dns_names.is_empty() {
            params.subject_alt_names = dns_names
                .iter()
                .filter_map(|name| {
                    Ia5String::try_from(name.as_str())
                        .map(SanType::DnsName)
                        .ok()
                })
                .collect();
        }
        params.is_ca = IsCa::NoCa;
        let key_pair = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)
            .map_err(|e| CertError::GenerationFailed(format!("Failed to generate key: {e}")))?;
        let cert = params
            .signed_by(&key_pair, &self.ca_cert, &self.ca_key_pair)
            .map_err(|e| CertError::GenerationFailed(format!("Failed to sign cert: {e}")))?;
        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();
        let encrypted_key = self.encrypt_key(&key_pem)?;
        Ok((cert_pem, encrypted_key))
    }
    #[must_use]
    pub fn get_ca_pem(&self) -> String {
        self.ca_cert.pem()
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn get_ca_key_encrypted(&self) -> CertResult<String> {
        let key_pem = self.ca_key_pair.serialize_pem();
        self.encrypt_key(&key_pem)
    }
    fn encrypt_key(&self, key_pem: &str) -> CertResult<String> {
        Self::encrypt_with_key(key_pem, &self.encryption_key)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn reencrypt_stored_key(
        encrypted_key: &str,
        old_password: &str,
        new_password: &str,
    ) -> CertResult<String> {
        let old_key = Self::derive_key(old_password);
        let key_pem = Self::decrypt_key_internal(encrypted_key, &old_key)?;
        let new_key = Self::derive_key(new_password);
        Self::encrypt_with_key(&key_pem, &new_key)
    }
    fn encrypt_with_key(key_pem: &str, encryption_key: &[u8; 32]) -> CertResult<String> {
        let cipher = Aes256Gcm::new_from_slice(encryption_key)
            .map_err(|e| CertError::KeyEncryption(format!("Failed to create cipher: {e}")))?;
        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, key_pem.as_bytes())
            .map_err(|e| CertError::KeyEncryption(format!("Encryption failed: {e}")))?;
        let mut combined = nonce_bytes.to_vec();
        combined.extend(ciphertext);
        Ok(BASE64.encode(&combined))
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn decrypt_key(&self, encrypted_key: &str) -> CertResult<String> {
        Self::decrypt_key_internal(encrypted_key, &self.encryption_key)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn decrypt_with_password(encrypted_key: &str, password: &str) -> CertResult<String> {
        let encryption_key = Self::derive_key(password);
        Self::decrypt_key_internal(encrypted_key, &encryption_key)
    }
    fn decrypt_key_internal(encrypted_key: &str, encryption_key: &[u8; 32]) -> CertResult<String> {
        let combined = BASE64
            .decode(encrypted_key)
            .map_err(|e| CertError::KeyEncryption(format!("Base64 decode failed: {e}")))?;
        if combined.len() < 12 {
            return Err(CertError::KeyEncryption(
                "Invalid encrypted key format".into(),
            ));
        }
        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let cipher = Aes256Gcm::new_from_slice(encryption_key)
            .map_err(|e| CertError::KeyEncryption(format!("Failed to create cipher: {e}")))?;
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| CertError::KeyEncryption(format!("Decryption failed: {e}")))?;
        String::from_utf8(plaintext)
            .map_err(|e| CertError::KeyEncryption(format!("Invalid UTF-8: {e}")))
    }
    fn derive_key(password: &str) -> [u8; 32] {
        let salt = b"mazbri-cert-ca-key-v1";
        let info = b"ca-key-encryption";
        let hkdf = Hkdf::<Sha256>::new(Some(salt), password.as_bytes());
        let mut key = [0u8; 32];
        hkdf.expand(info, &mut key)
            .expect("HKDF expand should not fail with valid parameters");
        key
    }
}
