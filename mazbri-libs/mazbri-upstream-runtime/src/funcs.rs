//! Closed helper functions for adapters (not a general template language).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use rand::RngExt;
use sha2::Sha256;
use std::collections::BTreeMap;

type HmacSha256 = Hmac<Sha256>;

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

/// ASCII-sort keys, drop empty values, join `k=v` with `&`.
pub fn sort_kv_join(fields: &BTreeMap<String, String>) -> String {
    fields
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

pub fn hmac_sha256_b64(secret: &str, message: &str) -> String {
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => match HmacSha256::new_from_slice(&[0u8]) {
            Ok(m) => m,
            Err(_) => return String::new(),
        },
    };
    mac.update(message.as_bytes());
    STANDARD.encode(mac.finalize().into_bytes())
}

pub fn hmac_sha256_b64_verify(secret: &str, message: &str, signature: &str) -> bool {
    let expected = hmac_sha256_b64(secret, message);
    expected == signature
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
    fn sort_and_hmac_stable() {
        let mut m = BTreeMap::new();
        m.insert("b".into(), "2".into());
        m.insert("a".into(), "1".into());
        m.insert("z".into(), String::new());
        assert_eq!(sort_kv_join(&m), "a=1&b=2");
        let sig = hmac_sha256_b64("secret", "a=1&b=2");
        assert!(hmac_sha256_b64_verify("secret", "a=1&b=2", &sig));
        assert!(!hmac_sha256_b64_verify("secret", "a=1&b=3", &sig));
    }

    #[test]
    fn xml_escape_basic() {
        assert_eq!(xml_escape("<a&b>"), "&lt;a&amp;b&gt;");
    }
}
