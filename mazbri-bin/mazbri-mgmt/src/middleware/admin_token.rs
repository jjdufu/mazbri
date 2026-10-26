//! 管理端 JWT 的签发与校验。
//!
//! 每种令牌按用途区分 `token_type`，并带有独立的 `aud`：
//!
//! | 类型 | `token_type` | `aud` | 用途 |
//! |------|--------------|-------|------|
//! | 访问令牌 | `access` | `mazbri-mgmt-api` | 访问受保护接口 |
//! | 刷新令牌 | `refresh` | `mazbri-mgmt-refresh` | 仅 `/auth/refresh` |
//! | MFA 待验证 | `mfa_pending` | `mazbri-mgmt-mfa-verify` | 仅 `/auth/mfa/verify` |
//! | MFA 强制设置 | `mfa_setup` | `mazbri-mgmt-mfa-setup` | 仅 `/auth/mfa/required-*` |
//! | 强制改密 | `password_reset` | `mazbri-mgmt-password-reset` | 仅 `/auth/first-login-change-password` |
//!
//! 解码时固定 HS256，并强制校验 `iss`、`aud`、`exp` 与 `token_type`；
//! 三种临时令牌有效期为 [`TEMP_TOKEN_TTL_SECS`]，使用成功后 `jti` 进入黑名单（一次性）。

use super::auth::{AdminClaims, revoked_jti_key};
use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::redis::RedisClient;

/// 管理端令牌签发者。
pub const ISSUER: &str = "mazbri-mgmt-api";
/// 临时令牌（MFA 验证 / MFA 设置 / 强制改密）有效期（秒）。
pub const TEMP_TOKEN_TTL_SECS: i64 = 300;
/// 管理端令牌唯一允许的签名算法。
const ALGORITHM: Algorithm = Algorithm::HS256;
/// 与 `jsonwebtoken` 默认 leeway 一致；黑名单 TTL 需覆盖这段宽限期。
const EXP_LEEWAY_SECS: u64 = 60;

/// 令牌用途。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Access,
    Refresh,
    MfaPending,
    MfaSetup,
    PasswordReset,
}

impl TokenKind {
    pub const ALL: [Self; 5] = [
        Self::Access,
        Self::Refresh,
        Self::MfaPending,
        Self::MfaSetup,
        Self::PasswordReset,
    ];

    /// 写入 `token_type` 声明的值。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Access => "access",
            Self::Refresh => "refresh",
            Self::MfaPending => "mfa_pending",
            Self::MfaSetup => "mfa_setup",
            Self::PasswordReset => "password_reset",
        }
    }

    /// 写入 `aud` 声明的值；每种用途互不相同。
    #[must_use]
    pub const fn audience(self) -> &'static str {
        match self {
            Self::Access => "mazbri-mgmt-api",
            Self::Refresh => "mazbri-mgmt-refresh",
            Self::MfaPending => "mazbri-mgmt-mfa-verify",
            Self::MfaSetup => "mazbri-mgmt-mfa-setup",
            Self::PasswordReset => "mazbri-mgmt-password-reset",
        }
    }

    /// 是否为一次性临时令牌。
    #[must_use]
    pub const fn is_temporary(self) -> bool {
        matches!(
            self,
            Self::MfaPending | Self::MfaSetup | Self::PasswordReset
        )
    }
}

/// 令牌主体（不含任何密钥材料）。
#[derive(Debug, Clone, Copy)]
pub struct TokenSubject<'a> {
    pub username: &'a str,
    pub user_id: &'a str,
    pub role: &'a str,
}

/// 令牌被拒绝的原因；`reason()` 即审计 `failure_reason`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenRejection {
    /// 签名、算法、`iss` 或格式不合法。
    Invalid,
    /// 已过期。
    Expired,
    /// 签名有效，但用途（`aud`/`token_type`）与当前接口不符。
    TypeMismatch,
    /// 签名有效，但为缺少 `aud`/`token_type` 的旧格式令牌。
    Legacy,
    /// `jti` 已注销或已被使用。
    Revoked,
    /// 黑名单存储不可用，临时令牌无法确认一次性，按拒绝处理。
    CheckUnavailable,
}

impl TokenRejection {
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Invalid => "invalid_token",
            Self::Expired => "token_expired",
            Self::TypeMismatch => "token_type_mismatch",
            Self::Legacy => "legacy_token",
            Self::Revoked => "token_revoked",
            Self::CheckUnavailable => "token_check_failed",
        }
    }

    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Invalid => "无效的令牌",
            Self::Expired => "令牌已过期",
            Self::TypeMismatch => "令牌类型不允许访问该接口",
            Self::Legacy => "令牌格式已过期，请重新登录",
            Self::Revoked => "令牌已注销或已使用",
            Self::CheckUnavailable => "暂时无法校验令牌，请稍后重试",
        }
    }
}

/// 签发指定用途的令牌。临时令牌忽略 `ttl_secs`，固定为 [`TEMP_TOKEN_TTL_SECS`]。
///
/// # Errors
///
/// JWT 编码失败时返回错误描述（不含令牌内容）。
pub fn issue(
    kind: TokenKind,
    subject: TokenSubject<'_>,
    secret: &str,
    ttl_secs: i64,
) -> Result<String, String> {
    let now = chrono::Utc::now().timestamp();
    let ttl = if kind.is_temporary() {
        TEMP_TOKEN_TTL_SECS
    } else {
        ttl_secs.max(1)
    };
    let claims = AdminClaims {
        iss: ISSUER.to_string(),
        aud: Some(kind.audience().to_string()),
        sub: subject.username.to_string(),
        user_id: subject.user_id.to_string(),
        role: subject.role.to_string(),
        iat: now,
        exp: now.saturating_add(ttl),
        jti: Some(uuid::Uuid::new_v4().to_string()),
        token_type: Some(kind.as_str().to_string()),
    };
    encode(
        &Header::new(ALGORITHM),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| format!("Failed to encode {} JWT: {e}", kind.as_str()))
}

/// 签发一次性临时令牌（有效期 [`TEMP_TOKEN_TTL_SECS`]）。
///
/// # Errors
///
/// JWT 编码失败时返回错误描述。
pub fn issue_temporary(
    kind: TokenKind,
    subject: TokenSubject<'_>,
    secret: &str,
) -> Result<String, String> {
    issue(kind, subject, secret, TEMP_TOKEN_TTL_SECS)
}

fn validation(audience: Option<&str>, validate_exp: bool) -> Validation {
    let mut validation = Validation::new(ALGORITHM);
    validation.set_issuer(&[ISSUER]);
    validation.validate_exp = validate_exp;
    validation.leeway = EXP_LEEWAY_SECS;
    if let Some(aud) = audience {
        validation.set_audience(&[aud]);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
    } else {
        validation.validate_aud = false;
        validation.set_required_spec_claims(&["exp", "iss"]);
    }
    validation
}

/// 只校验签名、算法与 `iss`（不校验 `aud`/`exp`），用于审计归属与拒绝原因判定。
#[must_use]
pub fn peek_verified(token: &str, secret: &str) -> Option<AdminClaims> {
    let key = DecodingKey::from_secret(secret.as_bytes());
    decode::<AdminClaims>(token, &key, &validation(None, false))
        .ok()
        .map(|data| data.claims)
}

/// 严格解码：算法 HS256、`iss`、`aud == kind.audience()`、`exp`、`token_type == kind`。
///
/// # Errors
///
/// 任一校验失败时返回对应 [`TokenRejection`]。
pub fn decode_kind(
    token: &str,
    secret: &str,
    kind: TokenKind,
) -> Result<AdminClaims, TokenRejection> {
    let key = DecodingKey::from_secret(secret.as_bytes());
    match decode::<AdminClaims>(token, &key, &validation(Some(kind.audience()), true)) {
        Ok(data) if data.claims.token_type.as_deref() == Some(kind.as_str()) => Ok(data.claims),
        Ok(_) => Err(TokenRejection::TypeMismatch),
        Err(error) => Err(classify(error.kind(), token, secret)),
    }
}

fn classify(kind: &ErrorKind, token: &str, secret: &str) -> TokenRejection {
    match kind {
        ErrorKind::ExpiredSignature => TokenRejection::Expired,
        ErrorKind::InvalidAudience | ErrorKind::MissingRequiredClaim(_) => {
            // 签名与 iss 已通过才会走到 aud 校验；区分「用途不符」与「旧格式」。
            match peek_verified(token, secret) {
                Some(claims) if claims.aud.is_none() && claims.token_type.is_none() => {
                    TokenRejection::Legacy
                }
                Some(_) => TokenRejection::TypeMismatch,
                None => TokenRejection::Invalid,
            }
        }
        _ => TokenRejection::Invalid,
    }
}

/// 解码刷新令牌。
///
/// 过渡期：升级前签发的刷新令牌没有 `aud`，但带有 `token_type = "refresh"`
/// （旧临时令牌/访问令牌从未带过该类型），因此仍允许其换发新格式访问令牌，
/// 避免已登录会话全部被强制下线。旧刷新令牌最长存活 `refresh_token_expiration_secs`。
///
/// # Errors
///
/// 校验失败时返回对应 [`TokenRejection`]。
pub fn decode_refresh(token: &str, secret: &str) -> Result<AdminClaims, TokenRejection> {
    match decode_kind(token, secret, TokenKind::Refresh) {
        Err(TokenRejection::Legacy | TokenRejection::TypeMismatch) => {
            let key = DecodingKey::from_secret(secret.as_bytes());
            match decode::<AdminClaims>(token, &key, &validation(None, true)) {
                Ok(data)
                    if data.claims.aud.is_none()
                        && data.claims.token_type.as_deref()
                            == Some(TokenKind::Refresh.as_str()) =>
                {
                    Ok(data.claims)
                }
                Ok(data) if data.claims.aud.is_none() && data.claims.token_type.is_none() => {
                    Err(TokenRejection::Legacy)
                }
                Ok(_) => Err(TokenRejection::TypeMismatch),
                Err(error) => Err(classify(error.kind(), token, secret)),
            }
        }
        other => other,
    }
}

/// 黑名单 TTL：覆盖剩余有效期与 leeway。
fn denylist_ttl(claims: &AdminClaims) -> usize {
    let remaining = claims
        .exp
        .saturating_sub(chrono::Utc::now().timestamp())
        .max(1);
    usize::saturating_from(remaining).saturating_add(usize::saturating_from(EXP_LEEWAY_SECS))
}

/// 临时令牌在执行业务前检查是否已被使用（存储不可用时按拒绝处理）。
///
/// # Errors
///
/// 已使用返回 [`TokenRejection::Revoked`]；缺少 `jti` 返回 [`TokenRejection::Invalid`]；
/// Redis 不可用返回 [`TokenRejection::CheckUnavailable`]。
pub async fn ensure_temporary_unused(
    redis: &RedisClient,
    claims: &AdminClaims,
) -> Result<(), TokenRejection> {
    let Some(jti) = claims.jti.as_deref() else {
        return Err(TokenRejection::Invalid);
    };
    match redis.exists(&revoked_jti_key(jti)).await {
        Ok(false) => Ok(()),
        Ok(true) => Err(TokenRejection::Revoked),
        Err(_) => Err(TokenRejection::CheckUnavailable),
    }
}

/// 原子地消费临时令牌（`SET NX EX`）：只有第一个调用者成功，并发重放会被拒绝。
///
/// # Errors
///
/// 已被消费返回 [`TokenRejection::Revoked`]；缺少 `jti` 返回 [`TokenRejection::Invalid`]；
/// Redis 不可用返回 [`TokenRejection::CheckUnavailable`]。
pub async fn consume_temporary(
    redis: &RedisClient,
    claims: &AdminClaims,
) -> Result<(), TokenRejection> {
    let Some(jti) = claims.jti.as_deref() else {
        return Err(TokenRejection::Invalid);
    };
    match redis
        .set_nx_ex(&revoked_jti_key(jti), b"1", denylist_ttl(claims))
        .await
    {
        Ok(true) => Ok(()),
        Ok(false) => Err(TokenRejection::Revoked),
        Err(_) => Err(TokenRejection::CheckUnavailable),
    }
}

/// 记录临时令牌被拒绝的审计信息（不记录令牌本身）。
pub fn audit_rejection(rejection: TokenRejection, token: &str, secret: &str) {
    if let Some(claims) = peek_verified(token, secret) {
        crate::audit::set_actor(
            uuid::Uuid::parse_str(&claims.user_id).ok(),
            Some(&claims.sub),
            Some(&claims.role),
        );
    }
    crate::audit::set_failure_reason(rejection.reason());
    if matches!(
        rejection,
        TokenRejection::TypeMismatch | TokenRejection::Revoked
    ) {
        crate::audit::mark_security_event();
    }
}

#[cfg(test)]
mod flow_tests;
#[cfg(test)]
mod tests;
