//! Hawk (MAC over method + url + optional body) — signed on the assembled HttpCall.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use mazbri_core::AuthHawkSpec;

use crate::funcs::nonce;
use crate::http_call::HttpCall;
use crate::types::ExecuteError;

type HmacSha256 = Hmac<Sha256>;

pub fn sign_hawk(
    call: &mut HttpCall<'_>,
    hawk_id: &str,
    hawk_key: &str,
    spec: &AuthHawkSpec,
    now: DateTime<Utc>,
    nonce_override: Option<&str>,
) -> Result<(), ExecuteError> {
    let algo = spec.algorithm.trim();
    if !algo.is_empty() && !algo.eq_ignore_ascii_case("sha256") {
        return Err(ExecuteError::SpecInvalid(
            "hawk algorithm only supports sha256".into(),
        ));
    }
    let parsed = url::Url::parse(call.url)
        .map_err(|e| ExecuteError::SpecInvalid(format!("hawk url: {e}")))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| ExecuteError::SpecInvalid("hawk url missing host".into()))?;
    let port = parsed
        .port()
        .unwrap_or(if parsed.scheme() == "https" { 443 } else { 80 });
    let resource = {
        let mut r = parsed.path().to_string();
        if r.is_empty() {
            r = "/".into();
        }
        if let Some(q) = parsed.query() {
            r.push('?');
            r.push_str(q);
        }
        r
    };
    let ts = now.timestamp();
    let nonce = nonce_override
        .map(str::to_string)
        .unwrap_or_else(|| nonce(16));
    let ext = spec.ext.as_deref().unwrap_or("");
    let payload_hash = payload_hash(call);
    let normalized = format!(
        "hawk.1.header\n{ts}\n{nonce}\n{}\n{resource}\n{host}\n{port}\n{payload_hash}\n{ext}\n",
        call.method.to_ascii_uppercase()
    );
    let mac = hmac_b64(hawk_key.as_bytes(), normalized.as_bytes())?;
    let mut parts = vec![
        format!("id=\"{hawk_id}\""),
        format!("ts=\"{ts}\""),
        format!("nonce=\"{nonce}\""),
        format!("mac=\"{mac}\""),
    ];
    if !payload_hash.is_empty() {
        parts.push(format!("hash=\"{payload_hash}\""));
    }
    if !ext.is_empty() {
        parts.push(format!("ext=\"{ext}\""));
    }
    call.headers
        .insert("Authorization".into(), format!("Hawk {}", parts.join(", ")));
    Ok(())
}

fn payload_hash(call: &HttpCall<'_>) -> String {
    let Some(body) = call.body.as_ref() else {
        return String::new();
    };
    let ct = body.content_type();
    let content_type = ct.split(';').next().unwrap_or(ct).trim();
    let normalized = format!("hawk.1.payload\n{content_type}\n{}\n", body.as_str());
    let digest = Sha256::digest(normalized.as_bytes());
    STANDARD.encode(digest)
}

fn hmac_b64(key: &[u8], data: &[u8]) -> Result<String, ExecuteError> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|e| ExecuteError::AuthTokenFailed(format!("hawk hmac: {e}")))?;
    mac.update(data);
    Ok(STANDARD.encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use mazbri_core::AuthHawkSpec;
    use std::collections::HashMap;

    #[test]
    fn hawk_mac_is_stable_with_fixed_nonce() {
        let now = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
        let spec = AuthHawkSpec {
            algorithm: "sha256".into(),
            ext: None,
        };
        let mut call = HttpCall {
            method: "GET",
            url: "http://example.com:8000/resource/1?b=1&a=2",
            headers: HashMap::new(),
            body: None,
            timeout: std::time::Duration::from_secs(1),
        };
        sign_hawk(
            &mut call,
            "dh37fgj492je",
            "werxhqb98rpaxn39848xrunpaw3489ruxnpa98w4rxn",
            &spec,
            now,
            Some("j4h3g2"),
        )
        .unwrap();
        let auth = call.headers.get("Authorization").expect("hawk").clone();
        assert!(auth.starts_with("Hawk "));
        assert!(auth.contains("id=\"dh37fgj492je\""));
        sign_hawk(
            &mut call,
            "dh37fgj492je",
            "werxhqb98rpaxn39848xrunpaw3489ruxnpa98w4rxn",
            &spec,
            now,
            Some("j4h3g2"),
        )
        .unwrap();
        assert_eq!(auth, *call.headers.get("Authorization").expect("hawk2"));
    }
}
