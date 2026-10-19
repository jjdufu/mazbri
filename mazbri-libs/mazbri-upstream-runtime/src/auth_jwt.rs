//! Sign JWT at apply_auth time (not a token-endpoint exchange).

use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use mazbri_core::AuthJwtSpec;

use crate::auth::credential;
use crate::token_cache::{CachedToken, TokenCache};
use crate::types::ExecuteError;

pub fn jwt_algorithm(spec: Option<&AuthJwtSpec>) -> Result<Algorithm, ExecuteError> {
    let raw = spec
        .map(|s| s.algorithm.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("HS256");
    match raw {
        "HS256" => Ok(Algorithm::HS256),
        "HS384" => Ok(Algorithm::HS384),
        "HS512" => Ok(Algorithm::HS512),
        "RS256" => Ok(Algorithm::RS256),
        "RS384" => Ok(Algorithm::RS384),
        "RS512" => Ok(Algorithm::RS512),
        other => Err(ExecuteError::SpecInvalid(format!(
            "jwt algorithm '{other}' unsupported"
        ))),
    }
}

pub async fn sign_jwt(
    token_cache: &Arc<TokenCache>,
    cache_key: &str,
    spec: Option<&AuthJwtSpec>,
    credentials: &HashMap<String, String>,
    now: DateTime<Utc>,
) -> Result<String, ExecuteError> {
    let algo = jwt_algorithm(spec)?;
    let ttl = spec
        .and_then(|s| s.ttl_secs)
        .filter(|n| *n > 0)
        .unwrap_or(300);
    let encoding_key = encoding_key(algo, credentials)?;
    let kid = spec
        .and_then(|s| s.kid.as_deref().map(str::trim).filter(|s| !s.is_empty()))
        .map(str::to_string);
    let claims_tpl = spec
        .and_then(|s| s.claims_tpl.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("{\"sub\":\"mazbri\"}")
        .to_string();
    let key = format!("{cache_key}:jwt:{algo:?}:{ttl}:{claims_tpl}");
    let now_c = now;
    token_cache
        .get_token(&key, || async move {
            let token = encode_jwt(algo, &encoding_key, kid.as_deref(), &claims_tpl, ttl, now_c)
                .map_err(|e| e.to_string())?;
            Ok(CachedToken {
                token,
                expires_at: now_c + chrono::Duration::seconds(ttl as i64),
            })
        })
        .await
        .map_err(ExecuteError::AuthTokenFailed)
}

pub fn encode_jwt(
    algo: Algorithm,
    encoding_key: &EncodingKey,
    kid: Option<&str>,
    claims_tpl: &str,
    ttl_secs: u64,
    now: DateTime<Utc>,
) -> Result<String, ExecuteError> {
    let mut claims: Value =
        serde_json::from_str(claims_tpl).unwrap_or_else(|_| serde_json::json!({"sub": "mazbri"}));
    let obj = claims
        .as_object_mut()
        .ok_or_else(|| ExecuteError::SpecInvalid("jwt claims_tpl must be a JSON object".into()))?;
    let exp = now.timestamp() + ttl_secs as i64;
    obj.insert("iat".into(), Value::from(now.timestamp()));
    obj.insert("exp".into(), Value::from(exp));
    let mut header = Header::new(algo);
    if let Some(k) = kid.filter(|s| !s.is_empty()) {
        header.kid = Some(k.to_string());
    }
    encode(&header, obj, encoding_key)
        .map_err(|e| ExecuteError::AuthTokenFailed(format!("jwt encode: {e}")))
}

fn encoding_key(
    algo: Algorithm,
    credentials: &HashMap<String, String>,
) -> Result<EncodingKey, ExecuteError> {
    match algo {
        Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => {
            let secret = credential(credentials, &["jwt_secret", "secret"]).ok_or_else(|| {
                ExecuteError::AuthTokenFailed("jwt HS* missing jwt_secret/secret".into())
            })?;
            Ok(EncodingKey::from_secret(secret.as_bytes()))
        }
        Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => {
            let pem =
                credential(credentials, &["jwt_private_key", "private_key"]).ok_or_else(|| {
                    ExecuteError::AuthTokenFailed(
                        "jwt RS* missing jwt_private_key/private_key".into(),
                    )
                })?;
            EncodingKey::from_rsa_pem(pem.as_bytes())
                .map_err(|e| ExecuteError::AuthTokenFailed(format!("jwt rsa pem: {e}")))
        }
        _ => Err(ExecuteError::SpecInvalid(
            "jwt algorithm unsupported".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use jsonwebtoken::{DecodingKey, Validation, decode};

    #[test]
    fn hs256_roundtrip() {
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let key = EncodingKey::from_secret(b"s3cret");
        let token = encode_jwt(
            Algorithm::HS256,
            &key,
            None,
            r#"{"sub":"mazbri"}"#,
            300,
            now,
        )
        .unwrap();
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = false;
        let data =
            decode::<Value>(&token, &DecodingKey::from_secret(b"s3cret"), &validation).unwrap();
        assert_eq!(data.claims["sub"], "mazbri");
        assert_eq!(data.claims["exp"], now.timestamp() + 300);
    }
}
