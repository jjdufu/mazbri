//! 多租户隔离幂等状态机：
//! - Redis 键：`{prefix}:idempotency:{tenant_id}:{product_id}:{sha256(method\npath\nfingerprint)}`
//! - 载荷绑定：规范化请求体与参数 SHA-256 不一致时返回 `422 Unprocessable Entity` (`E4221`)
//! - 双层租约：`Processing` 仅持 60s 短锁，附带 `epoch_id`；上游 `>= 400`、异常或客户端取消 Drop 时
//!   经 Lua 脚本校验 `epoch_id` 一致后立即 `DEL` 释放锁；仅 `2xx` 且非降级响应写入 `86400s` 完成记录
//! - 缓存保密：启用传输加密（`encryption_mode != "none"`）时，响应字节使用进程级临时 AEAD 密钥密封后再入 Redis

use super::metrics::GATEWAY_IDEMPOTENCY_UNAVAILABLE_TOTAL;
use super::response::ProxyError;
use super::serializer::SerializedProxyResponse;
use super::validation::GatewayRequest;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use mazbri_core::redis::{EphemeralCacheCipher, RedisClient};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;
use tracing::{debug, error, warn};
use uuid::Uuid;

pub const IDEMPOTENCY_PROCESSING_TTL_SECS: usize = 60;
pub const IDEMPOTENCY_COMPLETED_TTL_SECS: usize = 86_400;

/// Atomically releases a processing lock if and only if the lock is still `"processing"`
/// and owned by `ARGV[1]` (`"epoch_id":"<uuid>"`).
const RELEASE_LOCK_LUA: &str = r#"
local raw = redis.call('GET', KEYS[1])
if not raw then
    return 0
end
if string.find(raw, '"status":"processing"', 1, true) and string.find(raw, ARGV[1], 1, true) then
    return redis.call('DEL', KEYS[1])
end
return 0
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredIdempotencyRecord {
    status: String,
    epoch_id: Uuid,
    payload_sha256: String,
    #[serde(default)]
    http_status: u16,
    #[serde(default)]
    response_bytes: u64,
    #[serde(default)]
    content_type: Option<String>,
    #[serde(default)]
    sealed: bool,
    #[serde(default)]
    body_b64: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ReplayedIdempotencyResponse {
    pub(crate) epoch_id: Uuid,
    pub(crate) serialized: SerializedProxyResponse,
}

pub(crate) enum IdempotencyCheckOutcome {
    Acquired(IdempotencyInFlightGuard),
    Replay(ReplayedIdempotencyResponse),
}

pub(crate) struct IdempotencyInFlightGuard {
    redis: RedisClient,
    cipher: Arc<EphemeralCacheCipher>,
    redis_key: String,
    tenant_id: Uuid,
    epoch_id: Uuid,
    payload_sha256: String,
    committed: bool,
}

impl IdempotencyInFlightGuard {
    #[must_use]
    pub(crate) const fn epoch_id(&self) -> Uuid {
        self.epoch_id
    }

    fn aad_bytes(&self) -> Vec<u8> {
        format!("{}:{}", self.tenant_id, self.redis_key).into_bytes()
    }

    /// Commits a `2xx` live response with `86400s` retention TTL.
    pub(crate) async fn complete(
        &mut self,
        serialized: &SerializedProxyResponse,
        encryption_mode: &str,
    ) {
        self.committed = true;
        let should_seal = encryption_mode != "none";
        let encoded_bytes = if should_seal {
            match self.cipher.seal(&self.aad_bytes(), &serialized.bytes) {
                Ok(ciphertext) => ciphertext,
                Err(err) => {
                    warn!(error = %err, "Failed to seal idempotency response; releasing lock");
                    self.release_redis_lock().await;
                    return;
                }
            }
        } else {
            serialized.bytes.clone()
        };
        let record = StoredIdempotencyRecord {
            status: "completed".to_string(),
            epoch_id: self.epoch_id,
            payload_sha256: self.payload_sha256.clone(),
            http_status: serialized.status,
            response_bytes: serialized.response_bytes,
            content_type: serialized.content_type.clone(),
            sealed: should_seal,
            body_b64: STANDARD.encode(encoded_bytes),
        };
        match serde_json::to_vec(&record) {
            Ok(bytes) => {
                if let Err(err) = self
                    .redis
                    .set_ex_bytes(&self.redis_key, &bytes, IDEMPOTENCY_COMPLETED_TTL_SECS)
                    .await
                {
                    warn!(
                        redis_key = %self.redis_key,
                        error = %err,
                        "Failed to persist completed idempotency record"
                    );
                }
            }
            Err(err) => {
                warn!(error = %err, "Failed to serialize completed idempotency record");
            }
        }
    }

    /// Explicitly releases the 60s processing lock when upstream returns `>= 400` or errors.
    pub(crate) async fn release_now(&mut self) {
        if self.committed {
            return;
        }
        self.committed = true;
        self.release_redis_lock().await;
    }

    async fn release_redis_lock(&self) {
        let marker = epoch_json_marker(self.epoch_id);
        if let Err(err) = self
            .redis
            .eval_i64(
                RELEASE_LOCK_LUA,
                std::slice::from_ref(&self.redis_key),
                std::slice::from_ref(&marker),
            )
            .await
        {
            debug!(
                redis_key = %self.redis_key,
                epoch_id = %self.epoch_id,
                error = %err,
                "Failed to release in-flight idempotency lock"
            );
        }
    }
}

impl Drop for IdempotencyInFlightGuard {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        self.committed = true;
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let redis = self.redis.clone();
        let redis_key = self.redis_key.clone();
        let marker = epoch_json_marker(self.epoch_id);
        handle.spawn(async move {
            let _ = redis
                .eval_i64(
                    RELEASE_LOCK_LUA,
                    std::slice::from_ref(&redis_key),
                    std::slice::from_ref(&marker),
                )
                .await;
        });
    }
}

fn epoch_json_marker(epoch_id: Uuid) -> String {
    format!("\"epoch_id\":\"{epoch_id}\"")
}

#[derive(Clone)]
pub struct IdempotencyManager {
    redis: RedisClient,
    cipher: Arc<EphemeralCacheCipher>,
}

impl IdempotencyManager {
    #[must_use]
    pub fn new(redis: RedisClient) -> Self {
        Self {
            redis,
            cipher: Arc::new(EphemeralCacheCipher::new_random()),
        }
    }

    pub(crate) async fn check_or_acquire(
        &self,
        tenant_id: Uuid,
        req: &GatewayRequest,
        fingerprint: &str,
        request_id: &str,
    ) -> Result<IdempotencyCheckOutcome, ProxyError> {
        let redis_key = self.redis.tenant_idempotency_key(
            tenant_id,
            &req.product_id,
            &req.method,
            &req.path,
            fingerprint,
        );
        let payload_sha256 = canonical_request_payload_sha256(req);
        let epoch_id = Uuid::now_v7();
        let processing_record = StoredIdempotencyRecord {
            status: "processing".to_string(),
            epoch_id,
            payload_sha256: payload_sha256.clone(),
            http_status: 0,
            response_bytes: 0,
            content_type: None,
            sealed: false,
            body_b64: String::new(),
        };
        let processing_bytes = serde_json::to_vec(&processing_record).map_err(|err| {
            ProxyError::InternalError(format!("serialize idempotency lock failed: {err}"))
        })?;

        for _attempt in 0..2 {
            match self
                .redis
                .set_nx_ex(&redis_key, &processing_bytes, IDEMPOTENCY_PROCESSING_TTL_SECS)
                .await
            {
                Ok(true) => {
                    return Ok(IdempotencyCheckOutcome::Acquired(IdempotencyInFlightGuard {
                        redis: self.redis.clone(),
                        cipher: Arc::clone(&self.cipher),
                        redis_key,
                        tenant_id,
                        epoch_id,
                        payload_sha256,
                        committed: false,
                    }));
                }
                Ok(false) => {
                    if let Some(replayed) = self
                        .inspect_existing_record(
                            tenant_id,
                            &redis_key,
                            &payload_sha256,
                            fingerprint,
                            request_id,
                        )
                        .await?
                    {
                        return Ok(IdempotencyCheckOutcome::Replay(replayed));
                    }
                }
                Err(err) => {
                    return Err(idempotency_store_unavailable("SET NX", request_id, &err));
                }
            }
        }

        Err(ProxyError::Conflict(format!(
            "Duplicate request is still processing: {fingerprint}"
        )))
    }

    async fn inspect_existing_record(
        &self,
        tenant_id: Uuid,
        redis_key: &str,
        payload_sha256: &str,
        fingerprint: &str,
        request_id: &str,
    ) -> Result<Option<ReplayedIdempotencyResponse>, ProxyError> {
        let maybe_bytes = self
            .redis
            .get_bytes(redis_key)
            .await
            .map_err(|err| idempotency_store_unavailable("GET", request_id, &err))?;
        let Some(existing_bytes) = maybe_bytes else {
            return Ok(None);
        };
        let existing: StoredIdempotencyRecord =
            serde_json::from_slice(&existing_bytes).map_err(|_| {
                ProxyError::Conflict(format!(
                    "Duplicate request is still processing: {fingerprint}"
                ))
            })?;
        if existing.payload_sha256 != payload_sha256 {
            return Err(ProxyError::UnprocessableEntity(
                "Idempotency fingerprint reused with a different request payload".to_string(),
            ));
        }
        if existing.status != "completed" {
            return Err(ProxyError::Conflict(format!(
                "Duplicate request is still processing: {fingerprint}"
            )));
        }
        match self.decode_replay_body(tenant_id, redis_key, &existing) {
            Ok(plain_bytes) => {
                let response_bytes = if existing.response_bytes > 0 {
                    existing.response_bytes
                } else {
                    plain_bytes.len() as u64
                };
                Ok(Some(ReplayedIdempotencyResponse {
                    epoch_id: existing.epoch_id,
                    serialized: SerializedProxyResponse {
                        status: existing.http_status,
                        success: (200..300).contains(&existing.http_status),
                        response_bytes,
                        body_kind: "idempotent_replay",
                        body_encoding: "json",
                        content_type: existing.content_type,
                        bytes: plain_bytes,
                    },
                }))
            }
            Err(err) => {
                debug!(
                    redis_key = %redis_key,
                    error = %err,
                    "Cannot unseal prior ephemeral idempotency body; evicting stale record"
                );
                let _ = self.redis.del(redis_key).await;
                Ok(None)
            }
        }
    }

    fn decode_replay_body(
        &self,
        tenant_id: Uuid,
        redis_key: &str,
        record: &StoredIdempotencyRecord,
    ) -> Result<Vec<u8>, String> {
        let decoded = STANDARD
            .decode(&record.body_b64)
            .map_err(|err| format!("base64 decode failed: {err}"))?;
        if !record.sealed {
            return Ok(decoded);
        }
        let aad = format!("{tenant_id}:{redis_key}");
        self.cipher
            .open(aad.as_bytes(), &decoded)
            .map_err(|err| err.to_string())
    }
}

fn idempotency_store_unavailable(
    op: &str,
    request_id: &str,
    error: &impl std::fmt::Display,
) -> ProxyError {
    GATEWAY_IDEMPOTENCY_UNAVAILABLE_TOTAL.inc();
    error!(
        request_id = %request_id,
        op,
        error = %error,
        "Idempotency Redis store unavailable; failing closed with 503"
    );
    ProxyError::IdempotencyUnavailable {
        retry_after_secs: 1,
    }
}

#[must_use]
pub(crate) fn canonical_request_payload_sha256(req: &GatewayRequest) -> String {
    let mut hasher = Sha256::new();
    update_len_prefixed(&mut hasher, req.product_id.as_bytes());
    let upper_method = req.method.trim().to_ascii_uppercase();
    update_len_prefixed(&mut hasher, upper_method.as_bytes());
    update_len_prefixed(&mut hasher, req.path.as_bytes());

    if let Some(ref query) = req.query {
        let sorted: BTreeMap<&str, &str> = query
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        hasher.update((sorted.len() as u64).to_be_bytes());
        for (k, v) in sorted {
            update_len_prefixed(&mut hasher, k.as_bytes());
            update_len_prefixed(&mut hasher, v.as_bytes());
        }
    } else {
        hasher.update(0_u64.to_be_bytes());
    }

    if let Some(ref body) = req.body {
        let raw = body.as_raw_json_str().trim();
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) {
            let canonical = canonicalize_json_value(val);
            let encoded = serde_json::to_vec(&canonical).unwrap_or_else(|_| raw.as_bytes().to_vec());
            update_len_prefixed(&mut hasher, &encoded);
        } else {
            update_len_prefixed(&mut hasher, raw.as_bytes());
        }
    } else {
        hasher.update(0_u64.to_be_bytes());
    }

    hex::encode(hasher.finalize())
}

fn update_len_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn canonicalize_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted = BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k, canonicalize_json_value(v));
            }
            let mut out = serde_json::Map::with_capacity(sorted.len());
            for (k, v) in sorted {
                out.insert(k, v);
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonicalize_json_value).collect())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_payload_hash_is_order_independent_and_detects_mutations() {
        let req1: GatewayRequest = serde_json::from_str(
            r#"{"product_id":"8000000001","path":"/v1/check","method":"post","query":{"b":"2","a":"1"},"body":{"y":2,"x":{"k2":"v2","k1":"v1"}}}"#,
        )
        .unwrap();
        let req2: GatewayRequest = serde_json::from_str(
            r#"{"product_id":"8000000001","path":"/v1/check","method":"POST","query":{"a":"1","b":"2"},"body":{"x":{"k1":"v1","k2":"v2"},"y":2}}"#,
        )
        .unwrap();
        let req_mutated: GatewayRequest = serde_json::from_str(
            r#"{"product_id":"8000000001","path":"/v1/check","method":"POST","query":{"a":"1","b":"2"},"body":{"x":{"k1":"TAMPERED","k2":"v2"},"y":2}}"#,
        )
        .unwrap();

        assert_eq!(
            canonical_request_payload_sha256(&req1),
            canonical_request_payload_sha256(&req2)
        );
        assert_ne!(
            canonical_request_payload_sha256(&req1),
            canonical_request_payload_sha256(&req_mutated)
        );
    }

    #[test]
    fn multi_tenant_idempotency_keys_and_ephemeral_seal_isolate_tenants() {
        let redis = RedisClient::unconnected("redis://127.0.0.1:1").unwrap();
        let mgr = IdempotencyManager::new(redis.clone());
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        let key_a = redis.tenant_idempotency_key(tenant_a, "8000000001", "POST", "/v1", "fp-same");
        let key_b = redis.tenant_idempotency_key(tenant_b, "8000000001", "POST", "/v1", "fp-same");
        assert_ne!(key_a, key_b);

        let secret_body = br#"{"success":true,"data":{"id_card":"110101199001011234"},"status":200}"#;
        let aad_a = format!("{tenant_a}:{key_a}");
        let sealed = mgr.cipher.seal(aad_a.as_bytes(), secret_body).unwrap();
        assert!(
            !String::from_utf8_lossy(&sealed).contains("110101199001011234"),
            "Redis ciphertext must not expose plaintext payload"
        );
        let record = StoredIdempotencyRecord {
            status: "completed".into(),
            epoch_id: Uuid::now_v7(),
            payload_sha256: "abc".into(),
            http_status: 200,
            response_bytes: secret_body.len() as u64,
            content_type: Some("application/json".into()),
            sealed: true,
            body_b64: STANDARD.encode(&sealed),
        };
        assert_eq!(
            mgr.decode_replay_body(tenant_a, &key_a, &record).unwrap(),
            secret_body
        );
        assert!(
            mgr.decode_replay_body(tenant_b, &key_a, &record).is_err(),
            "cross-tenant AAD mismatch must fail decryption"
        );
    }
}
