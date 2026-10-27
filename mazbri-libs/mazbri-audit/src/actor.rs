//! 跨服务操作人声明（`x-mazbri-actor` 请求头）。
//!
//! mgmt 用 v2 HMAC-SHA256 签名（密钥由主令牌 HKDF 派生）。cert 同时接受 v2，
//! 以及在配置了公钥文件时的 v1 Ed25519。未配置 v1 公钥时，`v1.` 头是
//! [`ActorError::Malformed`]，任何路由都按无效声明拒绝。
//!
//! 格式：
//! - v1：`v1.<base64url(JSON claims)>.<base64url(Ed25519 签名)>`，签名覆盖 `v1.<claims>`
//! - v2：`v2.<base64url(JSON claims)>.<base64url(HMAC-SHA256)>`，签名覆盖 `v2.<claims>`
//!
//! 声明绑定受众（`aud`）、HTTP 方法（`htm`）与路径+查询（`htu`），有效期不超过
//! [`MAX_TTL_SECS`]，并带一次性 `jti`，验签方在有效期内拒绝重放。

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use zeroize::{Zeroize, Zeroizing};

/// 请求头名称。
pub const ACTOR_HEADER: &str = "x-mazbri-actor";
/// cert 服务的受众标识。
pub const AUDIENCE_CERT: &str = "mazbri-cert";
/// mgmt 作为签发方的标识。
pub const ISSUER_MGMT: &str = "mazbri-mgmt";
/// 声明最长有效期（秒）。
pub const MAX_TTL_SECS: i64 = 60;
/// 验签方允许的时钟偏差（秒）。
pub const CLOCK_SKEW_SECS: i64 = 5;
const V1_PREFIX: &str = "v1.";
const V2_PREFIX: &str = "v2.";
/// 重放缓存上限；超过后先清理过期项，仍超限则拒绝（防内存膨胀）。
const REPLAY_CACHE_LIMIT: usize = 100_000;
const MAX_TOKEN_LEN: usize = 4096;
const HMAC_LEN: usize = 32;

type HmacSha256 = Hmac<Sha256>;

/// PKCS#8 v1 Ed25519 私钥 DER 前缀（后接 32 字节种子）。
const PKCS8_ED25519_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];
/// SPKI Ed25519 公钥 DER 前缀（后接 32 字节公钥）。
const SPKI_ED25519_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

/// 操作人声明内容。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActorClaims {
    pub iss: String,
    pub aud: String,
    /// 管理员 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<uuid::Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub iat: i64,
    pub exp: i64,
    pub htm: String,
    pub htu: String,
    pub jti: String,
    /// 发起请求的 request id（与 mgmt 审计行关联）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rid: Option<String>,
    /// 最终用户 IP（mgmt 侧解析）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
}

/// 签名所需的操作人信息。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActorIdentity {
    pub user_id: Option<uuid::Uuid>,
    pub username: Option<String>,
    pub role: Option<String>,
    pub request_id: Option<String>,
    pub ip: Option<String>,
}

/// 密钥加载错误（不包含任何密钥内容）。
#[derive(Debug, thiserror::Error)]
pub enum ActorKeyError {
    #[error("无法读取密钥文件 {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("密钥格式无效：{0}")]
    Format(&'static str),
}

/// 验签失败原因；`as_str()` 写入审计 `failure_reason`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ActorError {
    #[error("actor header missing")]
    Missing,
    #[error("actor header malformed")]
    Malformed,
    #[error("actor signature invalid")]
    BadSignature,
    #[error("actor audience mismatch")]
    Audience,
    #[error("actor header expired")]
    Expired,
    #[error("actor header issued in the future")]
    NotYetValid,
    #[error("actor header lifetime too long")]
    LifetimeTooLong,
    #[error("actor header method mismatch")]
    Method,
    #[error("actor header path mismatch")]
    Path,
    #[error("actor header replayed")]
    Replayed,
}

impl ActorError {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "actor_missing",
            Self::Malformed => "actor_malformed",
            Self::BadSignature => "actor_bad_signature",
            Self::Audience => "actor_audience",
            Self::Expired => "actor_expired",
            Self::NotYetValid => "actor_not_yet_valid",
            Self::LifetimeTooLong => "actor_lifetime_too_long",
            Self::Method => "actor_method_mismatch",
            Self::Path => "actor_path_mismatch",
            Self::Replayed => "actor_replayed",
        }
    }
}

/// 签发密钥。Debug 不输出密钥字节。
pub enum SignKey {
    Ed25519(SigningKey),
    Hmac(Zeroizing<[u8; 32]>),
}

impl std::fmt::Debug for SignKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ed25519(_) => f.write_str("Ed25519"),
            Self::Hmac(_) => f.write_str("Hmac"),
        }
    }
}

/// 验签密钥。Debug 不输出密钥字节。
pub enum VerifyKey {
    Ed25519(VerifyingKey),
    Hmac(Zeroizing<[u8; 32]>),
}

impl std::fmt::Debug for VerifyKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ed25519(_) => f.write_str("Ed25519"),
            Self::Hmac(_) => f.write_str("Hmac"),
        }
    }
}

/// v2 HMAC 密钥：HKDF-SHA256(ikm = master, salt = `mazbri-actor`, info = `x-mazbri-actor/v2`, L = 32)。
///
/// `Hkdf::new` 的参数顺序是 salt 在前、ikm 在后。
///
/// # Panics
///
/// HKDF-SHA256 展开 32 字节不会失败。若实现违反 RFC 5869 的长度上限则 panic。
#[must_use]
pub fn actor_hmac_key(master: &str) -> Zeroizing<[u8; 32]> {
    let hk = Hkdf::<Sha256>::new(Some(b"mazbri-actor"), master.as_bytes());
    let mut okm = [0u8; 32];
    // 32 字节远小于 HKDF-SHA256 的 255*32 上限，展开不会失败。
    hk.expand(b"x-mazbri-actor/v2", &mut okm)
        .expect("HKDF-SHA256 expand of 32 bytes");
    let key = Zeroizing::new(okm);
    okm.zeroize();
    key
}

fn pem_der(pem: &str, label: &str) -> Result<Vec<u8>, ActorKeyError> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let start = pem
        .find(&begin)
        .ok_or(ActorKeyError::Format("缺少 PEM 头"))?
        + begin.len();
    let stop = pem[start..]
        .find(&end)
        .ok_or(ActorKeyError::Format("缺少 PEM 尾"))?
        + start;
    let body: String = pem[start..stop]
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect();
    STANDARD
        .decode(body)
        .map_err(|_| ActorKeyError::Format("PEM base64 无效"))
}

fn read_key_file(path: &Path) -> Result<String, ActorKeyError> {
    std::fs::read_to_string(path).map_err(|source| ActorKeyError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

fn load_ed25519_verifying_key(pem: &str) -> Result<VerifyingKey, ActorKeyError> {
    let der = pem_der(pem, "PUBLIC KEY")?;
    let bytes = der
        .strip_prefix(SPKI_ED25519_PREFIX.as_slice())
        .and_then(|rest| <[u8; 32]>::try_from(rest).ok())
        .ok_or(ActorKeyError::Format("不是 Ed25519 SPKI 公钥"))?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| ActorKeyError::Format("Ed25519 公钥点无效"))
}

/// mgmt 侧签名器。
pub struct ActorSigner {
    key: SignKey,
    issuer: String,
    ttl_secs: i64,
}

impl std::fmt::Debug for ActorSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActorSigner")
            .field("key", &self.key)
            .field("issuer", &self.issuer)
            .field("ttl_secs", &self.ttl_secs)
            .finish_non_exhaustive()
    }
}

impl ActorSigner {
    /// 从 PKCS#8 PEM（`openssl genpkey -algorithm ed25519` 输出）加载 v1 签名器。
    ///
    /// # Errors
    ///
    /// PEM 不是 Ed25519 PKCS#8 私钥时返回错误。
    pub fn from_pkcs8_pem(pem: &str, issuer: &str) -> Result<Self, ActorKeyError> {
        let der = pem_der(pem, "PRIVATE KEY")?;
        let seed = der
            .strip_prefix(PKCS8_ED25519_PREFIX.as_slice())
            .and_then(|rest| <[u8; 32]>::try_from(rest).ok())
            .ok_or(ActorKeyError::Format("不是 Ed25519 PKCS#8 私钥"))?;
        Ok(Self::from_seed(&seed, issuer))
    }

    /// 由 32 字节种子构造 v1 签名器（测试或密钥已在内存中时使用）。
    #[must_use]
    pub fn from_seed(seed: &[u8; 32], issuer: &str) -> Self {
        Self {
            key: SignKey::Ed25519(SigningKey::from_bytes(seed)),
            issuer: issuer.to_string(),
            ttl_secs: 30,
        }
    }

    /// 用主令牌构造 v2 HMAC 签名器。有效期默认 30 秒，可用 [`Self::with_ttl_secs`] 调整。
    #[must_use]
    pub fn hmac_v2(master: &str, issuer: &str) -> Self {
        Self {
            key: SignKey::Hmac(actor_hmac_key(master)),
            issuer: issuer.to_string(),
            ttl_secs: 30,
        }
    }

    /// 从文件加载 PKCS#8 PEM 私钥（仅 v1）。
    ///
    /// # Errors
    ///
    /// 文件不可读或格式无效时返回错误。
    pub fn from_file(path: &Path, issuer: &str) -> Result<Self, ActorKeyError> {
        Self::from_pkcs8_pem(&read_key_file(path)?, issuer)
    }

    /// 设置签发有效期（截断到 1..=[`MAX_TTL_SECS`]）。
    #[must_use]
    pub fn with_ttl_secs(mut self, ttl_secs: i64) -> Self {
        self.ttl_secs = ttl_secs.clamp(1, MAX_TTL_SECS);
        self
    }

    /// 仅 Ed25519 签名器有公钥。HMAC 没有 `VerifyingKey`。
    #[must_use]
    pub fn ed25519_verifying_key(&self) -> Option<VerifyingKey> {
        match &self.key {
            SignKey::Ed25519(key) => Some(key.verifying_key()),
            SignKey::Hmac(_) => None,
        }
    }

    /// 为一次出站请求签发声明。`path_and_query` 必须与下游看到的 URI 一致。
    #[must_use]
    pub fn sign(
        &self,
        audience: &str,
        actor: &ActorIdentity,
        method: &str,
        path_and_query: &str,
    ) -> String {
        self.sign_at(audience, actor, method, path_and_query, now_secs())
    }

    fn sign_at(
        &self,
        audience: &str,
        actor: &ActorIdentity,
        method: &str,
        path_and_query: &str,
        now: i64,
    ) -> String {
        let claims = ActorClaims {
            iss: self.issuer.clone(),
            aud: audience.to_string(),
            sub: actor.user_id,
            usr: actor.username.clone(),
            role: actor.role.clone(),
            iat: now,
            exp: now + self.ttl_secs,
            htm: method.to_ascii_uppercase(),
            htu: path_and_query.to_string(),
            jti: uuid::Uuid::new_v4().simple().to_string(),
            rid: actor.request_id.clone(),
            ip: actor.ip.clone(),
        };
        self.sign_claims(&claims)
    }

    fn sign_claims(&self, claims: &ActorClaims) -> String {
        // ActorClaims 只含字符串/整数/UUID，序列化不会失败。
        let json = serde_json::to_vec(claims).unwrap_or_default();
        let prefix = match &self.key {
            SignKey::Ed25519(_) => V1_PREFIX,
            SignKey::Hmac(_) => V2_PREFIX,
        };
        let signed = format!("{prefix}{}", URL_SAFE_NO_PAD.encode(json));
        let signature = match &self.key {
            SignKey::Ed25519(key) => URL_SAFE_NO_PAD.encode(key.sign(signed.as_bytes()).to_bytes()),
            SignKey::Hmac(secret) => {
                let mut mac = HmacSha256::new_from_slice(secret.as_ref())
                    .expect("HMAC accepts any key length");
                mac.update(signed.as_bytes());
                URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
            }
        };
        format!("{signed}.{signature}")
    }
}

/// 下游验签器（含进程内 jti 重放缓存，按副本独立）。
pub struct ActorVerifier {
    keys: Vec<VerifyKey>,
    audience: String,
    seen: Mutex<HashMap<String, i64>>,
}

impl std::fmt::Debug for ActorVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let v1 = self
            .keys
            .iter()
            .any(|key| matches!(key, VerifyKey::Ed25519(_)));
        let v2 = self
            .keys
            .iter()
            .filter(|key| matches!(key, VerifyKey::Hmac(_)))
            .count();
        f.debug_struct("ActorVerifier")
            .field("audience", &self.audience)
            .field("v1", &v1)
            .field("v2_keys", &v2)
            .finish_non_exhaustive()
    }
}

impl ActorVerifier {
    /// `v1` 缺省时不接受 `v1.` 头（返回 [`ActorError::Malformed`]）。
    /// `v2_masters` 是当前主令牌和可选的上一代，各自派生一把 HMAC 密钥。
    #[must_use]
    pub fn new(audience: &str, v1: Option<VerifyingKey>, v2_masters: &[&str]) -> Self {
        let mut keys = Vec::with_capacity(v2_masters.len() + usize::from(v1.is_some()));
        if let Some(key) = v1 {
            keys.push(VerifyKey::Ed25519(key));
        }
        for master in v2_masters {
            keys.push(VerifyKey::Hmac(actor_hmac_key(master)));
        }
        Self {
            keys,
            audience: audience.to_string(),
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// 从 SPKI PEM（`openssl pkey -pubout` 输出）加载仅 v1 的验签器。
    ///
    /// # Errors
    ///
    /// PEM 不是 Ed25519 公钥时返回错误。
    pub fn from_spki_pem(pem: &str, audience: &str) -> Result<Self, ActorKeyError> {
        let key = load_ed25519_verifying_key(pem)?;
        Ok(Self::new(audience, Some(key), &[]))
    }

    /// 从文件加载 SPKI PEM 公钥（仅 v1）。
    ///
    /// # Errors
    ///
    /// 文件不可读或格式无效时返回错误。
    pub fn from_file(path: &Path, audience: &str) -> Result<Self, ActorKeyError> {
        Self::from_spki_pem(&read_key_file(path)?, audience)
    }

    /// 校验签名、受众、时效、方法/路径绑定与重放。
    ///
    /// # Errors
    ///
    /// 任一校验失败返回对应 [`ActorError`]。
    pub fn verify(
        &self,
        token: &str,
        method: &str,
        path_and_query: &str,
    ) -> Result<ActorClaims, ActorError> {
        self.verify_at(token, method, path_and_query, now_secs())
    }

    fn verify_at(
        &self,
        token: &str,
        method: &str,
        path_and_query: &str,
        now: i64,
    ) -> Result<ActorClaims, ActorError> {
        if token.len() > MAX_TOKEN_LEN {
            return Err(ActorError::Malformed);
        }
        let (signed, signature_b64) = token.rsplit_once('.').ok_or(ActorError::Malformed)?;
        let (prefix, payload_b64) = if let Some(payload) = signed.strip_prefix(V2_PREFIX) {
            (V2_PREFIX, payload)
        } else if let Some(payload) = signed.strip_prefix(V1_PREFIX) {
            (V1_PREFIX, payload)
        } else {
            return Err(ActorError::Malformed);
        };
        let signature = URL_SAFE_NO_PAD
            .decode(signature_b64)
            .map_err(|_| ActorError::Malformed)?;
        match prefix {
            V2_PREFIX => self.verify_hmac(signed, &signature)?,
            V1_PREFIX => self.verify_ed25519(signed, &signature)?,
            _ => return Err(ActorError::Malformed),
        }
        self.finish_claims(payload_b64, method, path_and_query, now)
    }

    fn verify_hmac(&self, signed: &str, signature: &[u8]) -> Result<(), ActorError> {
        if signature.len() != HMAC_LEN {
            return Err(ActorError::Malformed);
        }
        let mut saw_key = false;
        let mut ok = false;
        for key in &self.keys {
            let VerifyKey::Hmac(secret) = key else {
                continue;
            };
            saw_key = true;
            let mut mac = HmacSha256::new_from_slice(secret.as_ref())
                .expect("HMAC accepts any key length");
            mac.update(signed.as_bytes());
            // verify_slice 对单把密钥是常量时间；两代密钥都算完再取值，不提前返回。
            ok |= mac.verify_slice(signature).is_ok();
        }
        if !saw_key {
            return Err(ActorError::Malformed);
        }
        if ok {
            Ok(())
        } else {
            Err(ActorError::BadSignature)
        }
    }

    fn verify_ed25519(&self, signed: &str, signature: &[u8]) -> Result<(), ActorError> {
        let Some(key) = self.keys.iter().find_map(|key| match key {
            VerifyKey::Ed25519(key) => Some(key),
            VerifyKey::Hmac(_) => None,
        }) else {
            // 没配 v1 公钥时，v1. 必须是 Malformed，不能变成“无法验签后放行”。
            return Err(ActorError::Malformed);
        };
        let signature = Signature::from_slice(signature).map_err(|_| ActorError::Malformed)?;
        key.verify_strict(signed.as_bytes(), &signature)
            .map_err(|_| ActorError::BadSignature)
    }

    fn finish_claims(
        &self,
        payload_b64: &str,
        method: &str,
        path_and_query: &str,
        now: i64,
    ) -> Result<ActorClaims, ActorError> {
        let payload = URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|_| ActorError::Malformed)?;
        let claims: ActorClaims =
            serde_json::from_slice(&payload).map_err(|_| ActorError::Malformed)?;
        if claims.aud != self.audience {
            return Err(ActorError::Audience);
        }
        if claims.exp <= claims.iat || claims.exp - claims.iat > MAX_TTL_SECS {
            return Err(ActorError::LifetimeTooLong);
        }
        if claims.iat > now + CLOCK_SKEW_SECS {
            return Err(ActorError::NotYetValid);
        }
        if now > claims.exp + CLOCK_SKEW_SECS {
            return Err(ActorError::Expired);
        }
        if !claims.htm.eq_ignore_ascii_case(method) {
            return Err(ActorError::Method);
        }
        if claims.htu != path_and_query {
            return Err(ActorError::Path);
        }
        self.remember_jti(&claims.jti, claims.exp + CLOCK_SKEW_SECS, now)?;
        Ok(claims)
    }

    fn remember_jti(&self, jti: &str, expires_at: i64, now: i64) -> Result<(), ActorError> {
        if jti.is_empty() || jti.len() > 64 {
            return Err(ActorError::Malformed);
        }
        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        if seen.get(jti).is_some_and(|&exp| exp >= now) {
            return Err(ActorError::Replayed);
        }
        if seen.len() >= REPLAY_CACHE_LIMIT {
            seen.retain(|_, exp| *exp >= now);
            if seen.len() >= REPLAY_CACHE_LIMIT {
                // 有效期内的 jti 过多：宁可拒绝也不放过可能的重放。
                return Err(ActorError::Replayed);
            }
        }
        seen.insert(jti.to_string(), expires_at);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer() -> ActorSigner {
        ActorSigner::from_seed(&[7u8; 32], ISSUER_MGMT)
    }

    fn verifier(signer: &ActorSigner) -> ActorVerifier {
        ActorVerifier::new(AUDIENCE_CERT, signer.ed25519_verifying_key(), &[])
    }

    fn identity() -> ActorIdentity {
        ActorIdentity {
            user_id: Some(uuid::Uuid::from_u128(42)),
            username: Some("alice".to_string()),
            role: Some("R_ADMIN".to_string()),
            request_id: Some("req-1".to_string()),
            ip: Some("203.0.113.9".to_string()),
        }
    }

    fn hmac_signer(master: &str) -> ActorSigner {
        ActorSigner::hmac_v2(master, ISSUER_MGMT)
    }

    #[test]
    fn roundtrip_binds_method_path_and_actor() {
        let s = signer();
        let v = verifier(&s);
        let token = s.sign_at(AUDIENCE_CERT, &identity(), "post", "/issue?x=1", 1_000);
        assert!(token.starts_with(V1_PREFIX), "{token}");
        let claims = v
            .verify_at(&token, "POST", "/issue?x=1", 1_010)
            .expect("valid");
        assert_eq!(claims.sub, Some(uuid::Uuid::from_u128(42)));
        assert_eq!(claims.usr.as_deref(), Some("alice"));
        assert_eq!(claims.rid.as_deref(), Some("req-1"));
        assert_eq!(claims.exp - claims.iat, 30);
    }

    #[test]
    fn rejects_replay_method_path_audience() {
        let s = signer();
        let v = verifier(&s);
        let token = s.sign_at(AUDIENCE_CERT, &identity(), "POST", "/issue", 1_000);
        assert_eq!(
            v.verify_at(&token, "PUT", "/issue", 1_001),
            Err(ActorError::Method)
        );
        assert_eq!(
            v.verify_at(&token, "POST", "/revoke/1", 1_001),
            Err(ActorError::Path)
        );
        v.verify_at(&token, "POST", "/issue", 1_001).expect("first");
        assert_eq!(
            v.verify_at(&token, "POST", "/issue", 1_002),
            Err(ActorError::Replayed)
        );
        let other = s.sign_at("mazbri-other", &identity(), "POST", "/issue", 1_000);
        assert_eq!(
            v.verify_at(&other, "POST", "/issue", 1_001),
            Err(ActorError::Audience)
        );
    }

    #[test]
    fn rejects_expired_future_and_long_lived() {
        let s = signer();
        let v = verifier(&s);
        let token = s.sign_at(AUDIENCE_CERT, &identity(), "GET", "/x", 1_000);
        assert_eq!(
            v.verify_at(&token, "GET", "/x", 1_000 + 30 + CLOCK_SKEW_SECS + 1),
            Err(ActorError::Expired)
        );
        assert_eq!(
            v.verify_at(&token, "GET", "/x", 1_000 - CLOCK_SKEW_SECS - 1),
            Err(ActorError::NotYetValid)
        );
        let mut claims: ActorClaims = {
            let payload = token.split('.').nth(1).expect("payload");
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).expect("b64")).expect("json")
        };
        claims.exp = claims.iat + MAX_TTL_SECS + 1;
        let long = s.sign_claims(&claims);
        assert_eq!(
            v.verify_at(&long, "GET", "/x", 1_001),
            Err(ActorError::LifetimeTooLong)
        );
        assert_eq!(s.with_ttl_secs(3_600).ttl_secs, MAX_TTL_SECS);
    }

    #[test]
    fn rejects_tampering_and_foreign_keys() {
        let s = signer();
        let v = verifier(&s);
        let token = s.sign_at(AUDIENCE_CERT, &identity(), "GET", "/x", 1_000);
        let (head, sig) = token.rsplit_once('.').expect("token");
        let mut claims: ActorClaims = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(head.strip_prefix(V1_PREFIX).expect("prefix"))
                .expect("b64"),
        )
        .expect("json");
        claims.role = Some("R_SUPER".to_string());
        let forged = format!(
            "{V1_PREFIX}{}.{sig}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).expect("json"))
        );
        assert_eq!(
            v.verify_at(&forged, "GET", "/x", 1_001),
            Err(ActorError::BadSignature)
        );
        let foreign = ActorSigner::from_seed(&[9u8; 32], ISSUER_MGMT);
        let token = foreign.sign_at(AUDIENCE_CERT, &identity(), "GET", "/x", 1_000);
        assert_eq!(
            v.verify_at(&token, "GET", "/x", 1_001),
            Err(ActorError::BadSignature)
        );
        for garbage in ["", "v1.", "v1.a.b", "v1.!!.??", "v3.x.y"] {
            assert_eq!(
                v.verify_at(garbage, "GET", "/x", 1_001),
                Err(ActorError::Malformed),
                "{garbage}"
            );
        }
        // v2 已是合法前缀；格式错误（签名不是 32 字节）仍是 Malformed，不是未知版本。
        assert_eq!(
            v.verify_at("v2.x.y", "GET", "/x", 1_001),
            Err(ActorError::Malformed)
        );
    }

    #[test]
    fn parses_openssl_style_pem() {
        let seed = [3u8; 32];
        let mut der = PKCS8_ED25519_PREFIX.to_vec();
        der.extend_from_slice(&seed);
        let pem = format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            STANDARD.encode(&der)
        );
        let s = ActorSigner::from_pkcs8_pem(&pem, ISSUER_MGMT).expect("private pem");
        let mut spki = SPKI_ED25519_PREFIX.to_vec();
        spki.extend_from_slice(
            s.ed25519_verifying_key()
                .expect("ed25519 signer")
                .as_bytes(),
        );
        let pub_pem = format!(
            "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
            STANDARD.encode(&spki)
        );
        let v = ActorVerifier::from_spki_pem(&pub_pem, AUDIENCE_CERT).expect("public pem");
        let token = s.sign(AUDIENCE_CERT, &identity(), "GET", "/ca");
        assert!(v.verify(&token, "GET", "/ca").is_ok());
        assert!(ActorSigner::from_pkcs8_pem(&pub_pem, ISSUER_MGMT).is_err());
        assert!(ActorVerifier::from_spki_pem("garbage", AUDIENCE_CERT).is_err());
        assert!(s.ed25519_verifying_key().is_some());
        assert!(hmac_signer("example-master").ed25519_verifying_key().is_none());
    }

    /// 用 openssl 生成的真实密钥文件互通：
    /// `openssl genpkey -algorithm ed25519 -out $D/k.pem && openssl pkey -in $D/k.pem -pubout -out $D/k.pub.pem`
    /// 然后 `MAZBRI_ACTOR_TEST_KEY_DIR=$D cargo test -p mazbri-audit -- --ignored openssl`。
    #[test]
    #[ignore = "requires openssl-generated keys in MAZBRI_ACTOR_TEST_KEY_DIR"]
    fn openssl_generated_keys_roundtrip() {
        let dir = std::path::PathBuf::from(
            std::env::var("MAZBRI_ACTOR_TEST_KEY_DIR").expect("MAZBRI_ACTOR_TEST_KEY_DIR"),
        );
        let s = ActorSigner::from_file(&dir.join("k.pem"), ISSUER_MGMT).expect("private key");
        let v =
            ActorVerifier::from_file(&dir.join("k.pub.pem"), AUDIENCE_CERT).expect("public key");
        let token = s.sign(AUDIENCE_CERT, &identity(), "POST", "/issue");
        assert!(v.verify(&token, "POST", "/issue").is_ok());
        assert!(ActorSigner::from_file(&dir.join("missing.pem"), ISSUER_MGMT).is_err());
    }

    #[test]
    fn replay_cache_purges_expired_entries() {
        let s = signer();
        let v = verifier(&s);
        v.remember_jti("a", 10, 0).expect("insert");
        assert_eq!(v.remember_jti("a", 10, 5), Err(ActorError::Replayed));
        // 过期后同一 jti 不再视为重放（签名本身也已过期，会先被时效检查拒绝）。
        v.remember_jti("a", 30, 11).expect("expired entry reusable");
    }

    #[test]
    fn hkdf_example_vector_and_v2_roundtrip() {
        let key = actor_hmac_key("example-master");
        assert_eq!(
            hex::encode(key.as_slice()),
            "737b8a206fce5b1ac2681c7a08e1d407c88fa24cdead36a17ab4186eeba2f741"
        );
        let s = hmac_signer("example-master");
        let v = ActorVerifier::new(AUDIENCE_CERT, None, &["example-master"]);
        let token = s.sign_at(AUDIENCE_CERT, &identity(), "POST", "/revoke/1", 1_000);
        assert!(token.starts_with(V2_PREFIX), "{token}");
        let claims = v
            .verify_at(&token, "POST", "/revoke/1", 1_001)
            .expect("v2 valid");
        assert_eq!(claims.usr.as_deref(), Some("alice"));
        assert_eq!(
            v.verify_at(&token, "POST", "/revoke/1", 1_002),
            Err(ActorError::Replayed)
        );
    }

    #[test]
    fn v2_rejects_expiry_audience_method_path_and_bad_signature() {
        let s = hmac_signer("example-master").with_ttl_secs(30);
        let v = ActorVerifier::new(AUDIENCE_CERT, None, &["example-master"]);
        let token = s.sign_at(AUDIENCE_CERT, &identity(), "GET", "/x", 1_000);
        assert_eq!(
            v.verify_at(&token, "GET", "/x", 1_000 + 30 + CLOCK_SKEW_SECS + 1),
            Err(ActorError::Expired)
        );
        assert_eq!(
            v.verify_at(&token, "PUT", "/x", 1_001),
            Err(ActorError::Method)
        );
        assert_eq!(
            v.verify_at(&token, "GET", "/y", 1_001),
            Err(ActorError::Path)
        );
        let other = s.sign_at("mazbri-other", &identity(), "GET", "/x", 1_000);
        assert_eq!(
            v.verify_at(&other, "GET", "/x", 1_001),
            Err(ActorError::Audience)
        );
        let (head, _) = token.rsplit_once('.').expect("token");
        let bad = format!("{head}.{}", URL_SAFE_NO_PAD.encode([9u8; 32]));
        assert_eq!(
            v.verify_at(&bad, "GET", "/x", 1_001),
            Err(ActorError::BadSignature)
        );
        let truncated = format!("{head}.{}", URL_SAFE_NO_PAD.encode([1u8, 2, 3]));
        assert_eq!(
            v.verify_at(&truncated, "GET", "/x", 1_001),
            Err(ActorError::Malformed)
        );
        let overlong = format!("v2.{}", "a".repeat(MAX_TOKEN_LEN));
        assert!(overlong.len() > MAX_TOKEN_LEN);
        assert_eq!(
            v.verify_at(&overlong, "GET", "/x", 1_001),
            Err(ActorError::Malformed)
        );
        assert_eq!(
            v.verify_at("v9.not-a-version", "GET", "/x", 1_001),
            Err(ActorError::Malformed)
        );
    }

    #[test]
    fn previous_master_verifies_and_v1_without_key_is_malformed() {
        let previous = "previous-master";
        let current = "example-master";
        let signed_by_previous = hmac_signer(previous).sign_at(
            AUDIENCE_CERT,
            &identity(),
            "POST",
            "/issue",
            1_000,
        );
        let both = ActorVerifier::new(AUDIENCE_CERT, None, &[current, previous]);
        assert!(
            both.verify_at(&signed_by_previous, "POST", "/issue", 1_001)
                .is_ok()
        );
        let only_current = ActorVerifier::new(AUDIENCE_CERT, None, &[current]);
        assert_eq!(
            only_current.verify_at(&signed_by_previous, "POST", "/issue", 1_001),
            Err(ActorError::BadSignature)
        );
        let v1 = signer().sign_at(AUDIENCE_CERT, &identity(), "POST", "/issue", 1_000);
        assert!(v1.starts_with(V1_PREFIX));
        assert_eq!(
            only_current.verify_at(&v1, "POST", "/issue", 1_001),
            Err(ActorError::Malformed)
        );
    }
}
