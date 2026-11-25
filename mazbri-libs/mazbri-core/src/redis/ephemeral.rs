use crate::redis::error::{RedisError, Result};
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use hmac::{Hmac, KeyInit as HmacKeyInit, Mac};
use rand::Rng;
use sha2::Sha256;
use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;

const NONCE_LEN: usize = 12;

/// Process-ephemeral AES-256-GCM cipher for encrypting sensitive transient records
/// stored in shared Redis (such as decrypted replay bodies).
#[derive(Clone)]
pub struct EphemeralCacheCipher {
    key: std::sync::Arc<Zeroizing<[u8; 32]>>,
}

impl Default for EphemeralCacheCipher {
    fn default() -> Self {
        Self::new_random()
    }
}

impl EphemeralCacheCipher {
    #[must_use]
    pub fn new_random() -> Self {
        let mut key = Zeroizing::new([0u8; 32]);
        rand::rng().fill_bytes(&mut *key);
        Self {
            key: std::sync::Arc::new(key),
        }
    }

    /// Encrypts `plaintext` with a fresh random 96-bit nonce and `aad`, returning `nonce || ciphertext`.
    ///
    /// # Errors
    ///
    /// Returns an error if AEAD encryption fails.
    pub fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        let cipher = Aes256Gcm::new_from_slice(&**self.key)
            .map_err(|e| RedisError::InvalidInput(format!("cipher key init failed: {e}")))?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| RedisError::InvalidInput("ephemeral cache seal failed".to_string()))?;
        let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypts `nonce || ciphertext` with `aad`.
    ///
    /// # Errors
    ///
    /// Returns an error if `sealed` is truncated or authentication fails (e.g. from another process epoch).
    pub fn open(&self, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>> {
        let Some((nonce_slice, ciphertext)) = sealed.split_at_checked(NONCE_LEN) else {
            return Err(RedisError::InvalidInput(
                "ephemeral cache ciphertext truncated".to_string(),
            ));
        };
        let nonce_arr: [u8; NONCE_LEN] = nonce_slice.try_into().map_err(|_| {
            RedisError::InvalidInput("ephemeral cache invalid nonce length".to_string())
        })?;
        let cipher = Aes256Gcm::new_from_slice(&**self.key)
            .map_err(|e| RedisError::InvalidInput(format!("cipher key init failed: {e}")))?;
        let nonce = Nonce::from(nonce_arr);
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| {
                RedisError::InvalidInput("ephemeral cache authentication failed".to_string())
            })
    }
}

/// Process-ephemeral HMAC-SHA256 key for keyed hashing of in-memory cache keys (e.g. API keys).
#[derive(Clone)]
pub struct EphemeralHmacKey {
    secret: std::sync::Arc<Zeroizing<[u8; 64]>>,
}

impl Default for EphemeralHmacKey {
    fn default() -> Self {
        Self::new_random()
    }
}

impl EphemeralHmacKey {
    #[must_use]
    pub fn new_random() -> Self {
        let mut secret = Zeroizing::new([0u8; 64]);
        rand::rng().fill_bytes(&mut *secret);
        Self {
            secret: std::sync::Arc::new(secret),
        }
    }

    #[must_use]
    pub fn digest(&self, input: &[u8]) -> [u8; 32] {
        let key_arr: [u8; 64] = **self.secret;
        let mut mac = <HmacSha256 as HmacKeyInit>::new(&key_arr.into());
        mac.update(input);
        let bytes = mac.finalize().into_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ephemeral_cipher_roundtrips_and_rejects_wrong_aad_or_key() {
        let cipher = EphemeralCacheCipher::new_random();
        let other = EphemeralCacheCipher::new_random();
        let sealed = cipher
            .seal(b"tenant-a:prod-1", b"secret-body")
            .expect("seal");
        assert_ne!(&sealed[NONCE_LEN..], b"secret-body");
        let opened = cipher
            .open(b"tenant-a:prod-1", &sealed)
            .expect("open matching aad");
        assert_eq!(opened, b"secret-body");
        assert!(cipher.open(b"tenant-b:prod-1", &sealed).is_err());
        assert!(other.open(b"tenant-a:prod-1", &sealed).is_err());
    }

    #[test]
    fn ephemeral_hmac_is_deterministic_within_epoch_and_differs_across_keys() {
        let k1 = EphemeralHmacKey::new_random();
        let k2 = EphemeralHmacKey::new_random();
        assert_eq!(k1.digest(b"mazbri_live_123"), k1.digest(b"mazbri_live_123"));
        assert_ne!(k1.digest(b"mazbri_live_123"), k1.digest(b"mazbri_live_456"));
        assert_ne!(k1.digest(b"mazbri_live_123"), k2.digest(b"mazbri_live_123"));
    }
}
