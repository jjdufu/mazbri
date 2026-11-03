//! Closed helper functions for adapters (not a general template language).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use rand::RngExt;

pub fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

/// `[A-Za-z0-9._-]{16,64}` — default 24 chars.
pub fn nonce(len: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-";
    let n = len.clamp(16, 64);
    let mut rng = rand::rng();
    (0..n)
        .map(|_| {
            let idx = rng.random_range(0..ALPHABET.len());
            ALPHABET[idx] as char
        })
        .collect()
}


pub fn basic_b64(id: &str, secret: &str) -> String {
    STANDARD.encode(format!("{id}:{secret}").as_bytes())
}

pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

pub fn url_encode(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_crypto::{AlgorithmId, AlgorithmRegistry, KeyPurpose, KeyRef, Mac};

    #[test]
    fn nonce_charset_and_len() {
        let n = nonce(24);
        assert_eq!(n.len(), 24);
        assert!(
            n.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
        );
    }

    #[test]
    fn mac_via_registry_replaces_adapter_hmac_helper() {
        let reg = AlgorithmRegistry::default_registry();
        let mac = reg.mac(AlgorithmId::HmacSha256).expect("mac");
        let key = KeyRef::raw(KeyPurpose::Mac, b"secret".to_vec());
        let tag = mac.sign(&key, b"a=1&b=2").expect("sign");
        mac.verify(&key, b"a=1&b=2", &tag).expect("verify");
        assert!(mac.verify(&key, b"a=1&b=3", &tag).is_err());
    }

    #[test]
    fn xml_escape_basic() {
        assert_eq!(xml_escape("<a&b>"), "&lt;a&amp;b&gt;");
    }
}
