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

    #[must_use]
    pub fn encoding_key(&self) -> EncodingKey {
        EncodingKey::from_secret(self.current_secret().as_bytes())
    }

    #[must_use]
    pub fn decoding_key(&self, kid: &str) -> Option<DecodingKey> {
        self.keys
            .get(kid)
            .map(|secret| DecodingKey::from_secret(secret.as_bytes()))
    }

    fn current_secret(&self) -> &str {
        self.keys
            .get(&self.current_kid)
            .map(String::as_str)
            .unwrap_or_default()
    }
}
