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
pub fn nonce_key(prefix: &str, tenant_id: Uuid, nonce: &str) -> String {
    let nonce_hash = sha256_hex(nonce);
    redis_key(
        prefix,
        "crypto:nonce",
        &[&tenant_id.to_string(), &nonce_hash],
    )
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
pub fn tenant_idempotency_key(
    prefix: &str,
    tenant_id: Uuid,
    product_id: &str,
    method: &str,
    path: &str,
    fingerprint: &str,
) -> String {
    let method_upper = method.trim().to_ascii_uppercase();
    let mut hasher = Sha256::new();
    for segment in [method_upper.as_str(), path, fingerprint] {
        hasher.update((segment.len() as u64).to_be_bytes());
        hasher.update(segment.as_bytes());
    }
    let scope_hash = hex::encode(hasher.finalize());
    redis_key(
        prefix,
        "idempotency",
        &[&tenant_id.to_string(), product_id, &scope_hash],
    )
}

#[must_use]
pub fn billing_balance_key(prefix: &str, tenant_id: Uuid) -> String {
    redis_key(prefix, "billing:bal", &[&tenant_id.to_string()])
}

#[must_use]
pub fn billing_balance_invalidation_channel(prefix: &str) -> String {
    format!("{prefix}:billing:bal_inv")
}

#[must_use]
pub fn cache_l1_invalidation_channel(prefix: &str) -> String {
    format!("{prefix}:cache:l1_inv")
}

#[must_use]
pub fn rate_limit_key(prefix: &str, parts: &[&str]) -> String {
    redis_key(prefix, "ratelimit", parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenant_idempotency_key_isolates_tenant_product_method_path_and_fingerprint() {
        let t1 = Uuid::from_u128(1);
        let t2 = Uuid::from_u128(2);
        let k_t1 = tenant_idempotency_key("mz", t1, "8000000001", "POST", "/v1/pay", "fp-1");
        let k_t2 = tenant_idempotency_key("mz", t2, "8000000001", "POST", "/v1/pay", "fp-1");
        let k_prod = tenant_idempotency_key("mz", t1, "8000000002", "POST", "/v1/pay", "fp-1");
        let k_method = tenant_idempotency_key("mz", t1, "8000000001", "GET", "/v1/pay", "fp-1");
        let k_path = tenant_idempotency_key("mz", t1, "8000000001", "POST", "/v1/refund", "fp-1");
        assert_ne!(k_t1, k_t2);
        assert_ne!(k_t1, k_prod);
        assert_ne!(k_t1, k_method);
        assert_ne!(k_t1, k_path);
        assert_eq!(
            k_t1,
            tenant_idempotency_key("mz", t1, "8000000001", "post", "/v1/pay", "fp-1")
        );
    }

    #[test]
    fn billing_channel_and_balance_keys_use_prefix() {
        let t1 = Uuid::from_u128(42);
        assert_eq!(
            billing_balance_invalidation_channel("mazbri"),
            "mazbri:billing:bal_inv"
        );
        assert_eq!(
            billing_balance_key("mazbri", t1),
            format!("mazbri:billing:bal:{t1}")
        );
    }
}
