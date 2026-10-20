//! Hand-written AWS SigV4 (no aws-sigv4 crate, no AWS SDK).

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use mazbri_core::AuthAwsSpec;

use crate::http_call::HttpCall;
use crate::types::ExecuteError;

#[cfg(test)]
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;

pub fn sign_aws_sigv4(
    call: &mut HttpCall<'_>,
    access_key_id: &str,
    secret_access_key: &str,
    session_token: Option<&str>,
    spec: &AuthAwsSpec,
    now: DateTime<Utc>,
) -> Result<(), ExecuteError> {
    let region = spec.region.trim();
    let service = spec.service.trim();
    if region.is_empty() || service.is_empty() {
        return Err(ExecuteError::SpecInvalid(
            "aws_sigv4 requires region and service".into(),
        ));
    }
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let host = host_from_url(call.url)?;
    let payload = call
        .body
        .as_ref()
        .map(|b| b.as_str().as_bytes())
        .unwrap_or(b"");
    let payload_hash = hex::encode(Sha256::digest(payload));

    call.headers.insert("Host".into(), host.clone());
    call.headers.insert("X-Amz-Date".into(), amz_date.clone());
    call.headers
        .insert("X-Amz-Content-Sha256".into(), payload_hash.clone());
    if let Some(tok) = session_token.filter(|s| !s.is_empty()) {
        call.headers
            .insert("X-Amz-Security-Token".into(), tok.to_string());
    }

    let mut signed: Vec<(&str, String)> = Vec::new();
    signed.push(("host", host.to_ascii_lowercase()));
    signed.push(("x-amz-content-sha256", payload_hash.clone()));
    signed.push(("x-amz-date", amz_date.clone()));
    if let Some(tok) = session_token.filter(|s| !s.is_empty()) {
        signed.push(("x-amz-security-token", tok.to_string()));
    }
    signed.sort_by(|a, b| a.0.cmp(b.0));

    let mut canonical_headers = String::new();
    let mut signed_names = Vec::new();
    for (name, value) in &signed {
        canonical_headers.push_str(name);
        canonical_headers.push(':');
        canonical_headers.push_str(value.trim());
        canonical_headers.push('\n');
        signed_names.push(*name);
    }
    let signed_headers = signed_names.join(";");
    let canonical_uri = canonical_uri(call.url);
    let canonical_query = canonical_query(call.url);
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        call.method.to_ascii_uppercase(),
        canonical_uri,
        canonical_query,
        canonical_headers,
        signed_headers,
        payload_hash
    );
    let canonical_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
    let credential_scope = format!("{date_stamp}/{region}/{service}/aws4_request");
    let string_to_sign =
        format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{canonical_hash}");
    let signing_key = aws_signing_key(secret_access_key, &date_stamp, region, service)?;
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes())?);
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={access_key_id}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );
    call.headers.insert("Authorization".into(), authorization);
    Ok(())
}

fn aws_signing_key(
    secret: &str,
    date_stamp: &str,
    region: &str,
    service: &str,
) -> Result<Vec<u8>, ExecuteError> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date_stamp.as_bytes())?;
    let k_region = hmac_sha256(&k_date, region.as_bytes())?;
    let k_service = hmac_sha256(&k_region, service.as_bytes())?;
    hmac_sha256(&k_service, b"aws4_request")
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>, ExecuteError> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|e| ExecuteError::AuthTokenFailed(format!("hmac key: {e}")))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

pub fn host_from_url(url: &str) -> Result<String, ExecuteError> {
    let parsed = url::Url::parse(url)
        .map_err(|e| ExecuteError::SpecInvalid(format!("aws_sigv4 url: {e}")))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| ExecuteError::SpecInvalid("aws_sigv4 url missing host".into()))?;
    match parsed.port() {
        Some(p) => Ok(format!("{host}:{p}")),
        None => Ok(host.to_string()),
    }
}

fn canonical_uri(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .map(|u| {
            let p = u.path();
            if p.is_empty() {
                "/".into()
            } else {
                p.to_string()
            }
        })
        .unwrap_or_else(|| "/".into())
}

fn canonical_query(url: &str) -> String {
    let Ok(parsed) = url::Url::parse(url) else {
        return String::new();
    };
    let mut pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(k, v)| (uri_encode(&k), uri_encode(&v)))
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn uri_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
struct SignedHeadersTestInput<'a> {
    method: &'a str,
    url: &'a str,
    extra: HashMap<String, String>,
    body: Option<&'a str>,
    access_key_id: &'a str,
    secret_access_key: &'a str,
    session_token: Option<&'a str>,
    spec: &'a AuthAwsSpec,
    now: DateTime<Utc>,
}

/// Test helper: headers after signing, without sending.
#[cfg(test)]
fn signed_headers_for_test(
    input: SignedHeadersTestInput<'_>,
) -> Result<HashMap<String, String>, ExecuteError> {
    let SignedHeadersTestInput {
        method,
        url,
        extra,
        body,
        access_key_id,
        secret_access_key,
        session_token,
        spec,
        now,
    } = input;
    let body_owned = body.map(|s| crate::types::BodyKind::Raw {
        body: s.to_string(),
        content_type: "application/octet-stream".into(),
    });
    let mut call = HttpCall {
        method,
        url,
        headers: extra,
        body: body_owned,
        timeout: std::time::Duration::from_secs(1),
    };
    sign_aws_sigv4(
        &mut call,
        access_key_id,
        secret_access_key,
        session_token,
        spec,
        now,
    )?;
    Ok(call.headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::collections::HashMap;

    #[test]
    fn sigv4_is_deterministic_and_has_required_headers() {
        let now = Utc.with_ymd_and_hms(2015, 8, 30, 12, 36, 0).unwrap();
        let spec = AuthAwsSpec {
            region: "us-east-1".into(),
            service: "iam".into(),
        };
        let h1 = signed_headers_for_test(SignedHeadersTestInput {
            method: "GET",
            url: "https://iam.amazonaws.com/?Action=ListUsers&Version=2010-05-08",
            extra: HashMap::new(),
            body: None,
            access_key_id: "AKIDEXAMPLE",
            secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            session_token: None,
            spec: &spec,
            now,
        })
        .unwrap();
        let h2 = signed_headers_for_test(SignedHeadersTestInput {
            method: "GET",
            url: "https://iam.amazonaws.com/?Action=ListUsers&Version=2010-05-08",
            extra: HashMap::new(),
            body: None,
            access_key_id: "AKIDEXAMPLE",
            secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            session_token: None,
            spec: &spec,
            now,
        })
        .unwrap();
        let auth = h1.get("Authorization").expect("authorization");
        assert!(auth.starts_with(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request"
        ));
        assert!(auth.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date"));
        assert_eq!(
            h1.get("X-Amz-Date").map(String::as_str),
            Some("20150830T123600Z")
        );
        assert_eq!(
            h1.get("X-Amz-Content-Sha256").map(String::as_str),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
        assert_eq!(h1.get("Authorization"), h2.get("Authorization"));
        let with_body = signed_headers_for_test(SignedHeadersTestInput {
            method: "GET",
            url: "https://iam.amazonaws.com/?Action=ListUsers&Version=2010-05-08",
            extra: HashMap::new(),
            body: Some("{}"),
            access_key_id: "AKIDEXAMPLE",
            secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            session_token: None,
            spec: &spec,
            now,
        })
        .unwrap();
        assert_ne!(h1.get("Authorization"), with_body.get("Authorization"));
    }
}
