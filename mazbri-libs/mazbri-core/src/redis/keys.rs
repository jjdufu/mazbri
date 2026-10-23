use sha2::{Digest, Sha256};
use uuid::Uuid;

#[must_use]
pub fn redis_key(prefix: &str, domain: &str, parts: &[&str]) -> String {
    let mut key = format!("{prefix}:{domain}");
    for part in parts {
        key.push(':');
        key.push_str(part);
    }
    key
}

#[must_use]
pub fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

#[must_use]
pub fn nonce_key(prefix: &str, tenant_id: Uuid, key_version: &str, nonce: &str) -> String {
    let nonce_hash = sha256_hex(nonce);
    redis_key(
        prefix,
        "crypto:nonce",
        &[&tenant_id.to_string(), key_version, &nonce_hash],
    )
}

#[must_use]
pub fn gateway_binding_nonce_key(prefix: &str, tenant_id: Uuid, nonce: &str) -> String {
    format!("{}:{}:{}", prefix.trim_end_matches(':'), tenant_id, nonce)
}

#[must_use]
pub fn cache_key(prefix: &str, tenant_id: &str, cache_key: &str) -> String {
    let cache_key_hash = sha256_hex(cache_key);
    redis_key(prefix, "cache", &[tenant_id, &cache_key_hash])
}

#[must_use]
pub fn permission_key(prefix: &str, tenant_id: Uuid, product_id: &str) -> String {
    redis_key(prefix, "permission", &[&tenant_id.to_string(), product_id])
}

#[must_use]
pub fn fallback_key(prefix: &str, cache_key: &str) -> String {
    let cache_key_hash = sha256_hex(cache_key);
    redis_key(prefix, "cbpro:fallback", &[&cache_key_hash])
}

#[must_use]
pub fn idempotency_key(prefix: &str, fingerprint: &str) -> String {
    let fingerprint_hash = sha256_hex(fingerprint);
    redis_key(prefix, "idempotency", &[&fingerprint_hash])
}

#[must_use]
pub fn rate_limit_key(prefix: &str, parts: &[&str]) -> String {
    redis_key(prefix, "ratelimit", parts)
}
