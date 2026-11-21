use jsonwebtoken::{DecodingKey, EncodingKey};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct JwtHs256Keyring {
    current_kid: String,
    keys: Arc<HashMap<String, String>>,
}

impl JwtHs256Keyring {
    pub fn new(current_kid: impl Into<String>, current_secret: impl Into<String>) -> Self {
        let current_kid = current_kid.into();
        let current_secret = current_secret.into();
        let mut keys = HashMap::new();
        keys.insert(current_kid.clone(), current_secret);
        Self {
            current_kid,
            keys: Arc::new(keys),
        }
    }

    #[must_use]
    pub fn with_extra_keys(mut self, extra: Vec<(String, String)>) -> Self {
        let keys = Arc::make_mut(&mut self.keys);
        for (kid, secret) in extra {
            keys.entry(kid).or_insert(secret);
        }
        self
    }

    #[must_use]
    pub fn current_kid(&self) -> &str {
        &self.current_kid
    }

    /// # Errors
    ///
    /// 当前密钥为空（无法用于 HMAC 签名）时返回错误。
    pub fn encoding_key(&self) -> Result<EncodingKey, jsonwebtoken::errors::Error> {
        EncodingKey::from_secret(self.current_secret().as_bytes())
    }

    #[must_use]
    pub fn decoding_key(&self, kid: &str) -> Option<DecodingKey> {
        self.keys
            .get(kid)
            .and_then(|secret| DecodingKey::from_secret(secret.as_bytes()).ok())
    }

    fn current_secret(&self) -> &str {
        self.keys
            .get(&self.current_kid)
            .map_or_default(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::JwtHs256Keyring;

    #[test]
    fn empty_secret_rejected_and_valid_secret_accepted() {
        let empty = JwtHs256Keyring::new("k0", "");
        assert!(empty.encoding_key().is_err());
        assert!(empty.decoding_key("k0").is_none());

        let valid = JwtHs256Keyring::new("k1", "non-empty-secret")
            .with_extra_keys(vec![("k-empty".to_string(), String::new())]);
        assert!(valid.encoding_key().is_ok());
        assert!(valid.decoding_key("k1").is_some());
        assert!(valid.decoding_key("k-empty").is_none());
    }
}
