use crate::config::CertConfig;
use crate::error::{CertError, CertResult};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use hkdf::Hkdf;
use mazbri_crypto::aead::AEAD_NONCE_LEN;
use mazbri_crypto::{AeadAlgorithm, AeadSeal, KeyPurpose, KeyRef};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType, Ia5String, IsCa,
    KeyPair, PKCS_ECDSA_P256_SHA256, SanType, SerialNumber,
};
use sha2::Sha256;
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use zeroize::Zeroizing;

pub struct CaManager {
    ca_cert: Certificate,
    ca_cert_pem: String,
    ca_key_pair: KeyPair,
    config: Arc<CertConfig>,
    encryption_key: Zeroizing<[u8; 32]>,
    ca_expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl CaManager {
    fn parse_cert_not_after(pem: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        let (_, pem_obj) = x509_parser::pem::parse_x509_pem(pem.as_bytes()).ok()?;
        let (_, cert) = x509_parser::parse_x509_certificate(&pem_obj.contents).ok()?;
        chrono::DateTime::from_timestamp(cert.validity().not_after.timestamp(), 0)
    }

    fn parse_cert_serial(pem: &str) -> Option<SerialNumber> {
        let (_, pem_obj) = x509_parser::pem::parse_x509_pem(pem.as_bytes()).ok()?;
        let (_, cert) = x509_parser::parse_x509_certificate(&pem_obj.contents).ok()?;
        Some(SerialNumber::from_slice(cert.raw_serial()))
    }

    /// 返回自 `from` 时刻起 CA 证书剩余的有效整天数上限（若已过期则返回 `0`）。
    #[must_use]
    pub fn ca_remaining_days(&self, from: chrono::DateTime<chrono::Utc>) -> Option<i64> {
        self.ca_expires_at.map(|expires_at| {
            let remaining_secs = expires_at.timestamp().saturating_sub(from.timestamp());
            if remaining_secs <= 0 {
                0
            } else {
                remaining_secs / 86_400
            }
        })
    }

    /// 从持久化 CA PEM 与加密私钥恢复 `CaManager`，严格保留原始 CA PEM 与序列号，绝不重签篡改根证书。
    ///
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
        let mut ca_params = CertificateParams::from_ca_cert_pem(ca_cert_pem)
            .map_err(|e| CertError::CaInitFailed(format!("Failed to parse CA cert: {e}")))?;
        if let Some(serial) = Self::parse_cert_serial(ca_cert_pem) {
            ca_params.serial_number = Some(serial);
        }
        let ca_expires_at = Self::parse_cert_not_after(ca_cert_pem);
        let ca_cert = ca_params
            .self_signed(&ca_key_pair)
            .map_err(|e| CertError::CaInitFailed(format!("Failed to reconstruct CA: {e}")))?;
        Ok(Self {
            ca_cert,
            ca_cert_pem: ca_cert_pem.to_owned(),
            ca_key_pair,
            config,
            encryption_key,
            ca_expires_at,
        })
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn new(config: Arc<CertConfig>) -> CertResult<Self> {
        let (ca_cert, ca_key_pair) = Self::generate_ca(&config)?;
        let ca_cert_pem = ca_cert.pem();
        let ca_expires_at = Self::parse_cert_not_after(&ca_cert_pem);
        let encryption_key = Self::derive_key(config.key_password());
        Ok(Self {
            ca_cert,
            ca_cert_pem,
            ca_key_pair,
            config,
            encryption_key,
            ca_expires_at,
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

    /// 按显式有效期窗口 `[not_before, not_after]` 签发服务端 mTLS 证书，保证 X.509 与数据库到期时间一致。
    ///
    /// # Errors
    ///
    /// 时间戳越界、密钥生成或签名失败时返回错误。
    pub fn issue_certificate_until(
        &self,
        subject: &str,
        dns_names: &[String],
        not_before: chrono::DateTime<chrono::Utc>,
        not_after: chrono::DateTime<chrono::Utc>,
    ) -> CertResult<(String, String)> {
        if not_after <= not_before {
            return Err(CertError::InvalidRequest(
                "certificate not_after must be greater than not_before".into(),
            ));
        }
        let mut params = CertificateParams::default();
        params.not_before = OffsetDateTime::from_unix_timestamp(not_before.timestamp())
            .map_err(|e| CertError::InvalidRequest(format!("invalid not_before timestamp: {e}")))?;
        params.not_after = OffsetDateTime::from_unix_timestamp(not_after.timestamp())
            .map_err(|e| CertError::InvalidRequest(format!("invalid not_after timestamp: {e}")))?;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, subject);
        dn.push(DnType::OrganizationName, "Mazbri Service");
        params.distinguished_name = dn;
        if !dns_names.is_empty() {
            params.subject_alt_names = dns_names
                .iter()
                .map(|name| {
                    Ia5String::try_from(name.as_str())
                        .map(SanType::DnsName)
                        .map_err(|e| {
                            CertError::InvalidRequest(format!("invalid SAN DNS name '{name}': {e}"))
                        })
                })
                .collect::<CertResult<Vec<_>>>()?;
        }
        params.is_ca = IsCa::NoCa;
        let key_pair = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)
            .map_err(|e| CertError::GenerationFailed(format!("Failed to generate key: {e}")))?;
        let cert = params
            .signed_by(&key_pair, &self.ca_cert, &self.ca_key_pair)
            .map_err(|e| CertError::GenerationFailed(format!("Failed to sign cert: {e}")))?;
        let cert_pem = cert.pem();
        let key_pem = Zeroizing::new(key_pair.serialize_pem());
        let encrypted_key = self.encrypt_key(&key_pem)?;
        Ok((cert_pem, encrypted_key))
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn issue_certificate(
        &self,
        subject: &str,
        dns_names: &[String],
    ) -> CertResult<(String, String)> {
        let now = chrono::Utc::now();
        let delta = chrono::TimeDelta::try_days(self.config.cert_validity_days)
            .ok_or_else(|| CertError::InvalidRequest("invalid cert_validity_days".into()))?;
        let not_after = now
            .checked_add_signed(delta)
            .ok_or_else(|| CertError::InvalidRequest("certificate expiry overflow".into()))?;
        self.issue_certificate_until(subject, dns_names, now, not_after)
    }

    #[must_use]
    pub fn get_ca_pem(&self) -> String {
        self.ca_cert_pem.clone()
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn get_ca_key_encrypted(&self) -> CertResult<String> {
        let key_pem = Zeroizing::new(self.ca_key_pair.serialize_pem());
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
        let key_ref = KeyRef::raw(KeyPurpose::KeyWrap, encryption_key.to_vec());
        let nonce_bytes: [u8; AEAD_NONCE_LEN] = rand::random();
        let packed = AeadAlgorithm::Aes256Gcm
            .seal(&key_ref, &nonce_bytes, b"", key_pem.as_bytes())
            .map_err(|e| CertError::KeyEncryption(format!("Encryption failed: {e}")))?;
        let mut combined = nonce_bytes.to_vec();
        combined.extend(packed);
        Ok(BASE64.encode(combined))
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn decrypt_key(&self, encrypted_key: &str) -> CertResult<String> {
        Self::decrypt_key_string(encrypted_key, &self.encryption_key)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn decrypt_with_password(encrypted_key: &str, password: &str) -> CertResult<String> {
        let encryption_key = Self::derive_key(password);
        Self::decrypt_key_string(encrypted_key, &encryption_key)
    }

    fn decrypt_key_internal(
        encrypted_key: &str,
        encryption_key: &[u8; 32],
    ) -> CertResult<Zeroizing<String>> {
        let combined = BASE64
            .decode(encrypted_key)
            .map_err(|e| CertError::KeyEncryption(format!("Base64 decode failed: {e}")))?;
        if combined.len() < AEAD_NONCE_LEN {
            return Err(CertError::KeyEncryption(
                "Invalid encrypted key format".into(),
            ));
        }
        let (nonce_bytes, ciphertext) = combined.split_at(AEAD_NONCE_LEN);
        let key_ref = KeyRef::raw(KeyPurpose::KeyWrap, encryption_key.to_vec());
        let plaintext = Zeroizing::new(
            AeadAlgorithm::Aes256Gcm
                .open(&key_ref, nonce_bytes, b"", ciphertext)
                .map_err(|e| CertError::KeyEncryption(format!("Decryption failed: {e}")))?,
        );
        let text = std::str::from_utf8(&plaintext)
            .map(str::to_owned)
            .map_err(|e| CertError::KeyEncryption(format!("Invalid UTF-8: {e}")))?;
        Ok(Zeroizing::new(text))
    }

    fn decrypt_key_string(encrypted_key: &str, encryption_key: &[u8; 32]) -> CertResult<String> {
        Ok(Self::decrypt_key_internal(encrypted_key, encryption_key)?.to_string())
    }

    fn derive_key(password: &str) -> Zeroizing<[u8; 32]> {
        let salt = b"mazbri-cert-ca-key-v1";
        let info = b"ca-key-encryption";
        let hkdf = Hkdf::<Sha256>::new(Some(salt), password.as_bytes());
        let mut key = Zeroizing::new([0u8; 32]);
        hkdf.expand(info, &mut *key)
            .expect("HKDF expand should not fail with valid parameters");
        key
    }
}

#[cfg(test)]
mod hkdf_golden_tests {
    use super::CaManager;
    use crate::config::CertConfig;
    use std::sync::Arc;

    /// 金标准由 hkdf 0.12 / sha2 0.10 生成；升级后 CA 私钥加密密钥派生必须不变。
    #[test]
    fn derive_key_matches_pre_upgrade_golden() {
        assert_eq!(
            hex::encode(*CaManager::derive_key("ca-pw-upg3")),
            "f9798fccc20c816f865b6697555e6caee5c17a889d9ef308bd96cc9016c73998"
        );
    }

    #[test]
    fn from_existing_preserves_exact_ca_pem_serial_and_validity_window() {
        let cfg = Arc::new(CertConfig {
            ca_validity_days: 3650,
            cert_validity_days: 365,
            key_password: Some("test-ca-pw".to_string()),
        });
        let ca1 = CaManager::new(cfg.clone()).expect("new ca");
        let pem1 = ca1.get_ca_pem();
        let key_enc = ca1.get_ca_key_encrypted().expect("enc key");

        let ca2 = CaManager::from_existing(&pem1, &key_enc, cfg).expect("from_existing");
        assert_eq!(
            ca2.get_ca_pem(),
            pem1,
            "from_existing must never re-sign or alter CA PEM / serial number"
        );

        let now = chrono::Utc::now();
        let target_exp = now + chrono::TimeDelta::days(45);
        let (leaf_pem, _) = ca2
            .issue_certificate_until(
                "mazbri-gateway",
                &["mazbri-gateway".to_string()],
                now,
                target_exp,
            )
            .expect("issue leaf");
        let (_, leaf_obj) = x509_parser::pem::parse_x509_pem(leaf_pem.as_bytes()).expect("pem");
        let (_, leaf_x509) =
            x509_parser::parse_x509_certificate(&leaf_obj.contents).expect("x509");
        assert_eq!(
            leaf_x509.validity().not_after.timestamp(),
            target_exp.timestamp()
        );
    }
}
