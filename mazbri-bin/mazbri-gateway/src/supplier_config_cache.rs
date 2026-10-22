//! Gateway 侧：记住本 Pod 已向 bypass 完整下发过的 supplier config 哈希，
//! 后续同 hash 只发 hash，减少 hop payload。

use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct SupplierConfigSendCache {
    /// `supplier_code` -> last full-config hash sent from this pod
    last_full_hash: Arc<DashMap<String, String>>,
}

impl SupplierConfigSendCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn hash_config_json(config_json: &str) -> String {
        let digest = Sha256::digest(config_json.as_bytes());
        hex::encode(digest)
    }

    /// 若本 Pod 已对该 supplier 下发过相同 hash，则无需再带全文。
    #[must_use]
    pub fn should_send_full(&self, supplier_code: &str, config_hash: &str) -> bool {
        match self.last_full_hash.get(supplier_code) {
            Some(prev) => prev.as_str() != config_hash,
            None => true,
        }
    }

    pub fn mark_full_sent(&self, supplier_code: &str, config_hash: &str) {
        self.last_full_hash
            .insert(supplier_code.to_string(), config_hash.to_string());
    }

    /// bypass 报 cache miss 时强制下次带全文
    pub fn invalidate(&self, supplier_code: &str) {
        self.last_full_hash.remove(supplier_code);
    }
}
