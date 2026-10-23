//! HTTP Digest (RFC 7616 qop=auth). Challenge-response lives in `send_http`.

use md5::Md5;
use sha2::{Digest, Sha256};

use crate::funcs::nonce;
use crate::types::ExecuteError;

#[derive(Debug, Clone)]
pub struct DigestAttempt {
    pub username: String,
    pub password: String,
    pub algorithm: String,
    pub realm: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DigestChallenge {
    pub realm: String,
    pub nonce: String,
    pub opaque: Option<String>,
    pub algorithm: String,
    pub qop: Option<String>,
}

pub fn parse_www_authenticate_digest(header: &str) -> Option<DigestChallenge> {
    let raw = header.trim();
    let rest = raw
        .strip_prefix("Digest")
        .or_else(|| raw.strip_prefix("digest"))?;
    let mut realm = String::new();
    let mut nonce_v = String::new();
    let mut opaque = None;
    let mut algorithm = String::new();
    let mut qop = None;
    for part in rest.split(',') {
        let part = part.trim();
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim().trim_matches('"');
        match key.to_ascii_lowercase().as_str() {
            "realm" => realm = val.to_string(),
            "nonce" => nonce_v = val.to_string(),
            "opaque" => opaque = Some(val.to_string()),
            "algorithm" => algorithm = val.to_string(),
            "qop" => qop = Some(val.to_string()),
            _ => {}
        }
    }
    if nonce_v.is_empty() {
        return None;
    }
    Some(DigestChallenge {
        realm,
        nonce: nonce_v,
        opaque,
        algorithm,
        qop,
    })
}

pub fn build_digest_authorization(
    attempt: &DigestAttempt,
    challenge: &DigestChallenge,
    method: &str,
    url: &str,
) -> Result<String, ExecuteError> {
    let algo = normalize_algo(if !attempt.algorithm.trim().is_empty() {
        attempt.algorithm.as_str()
    } else if !challenge.algorithm.trim().is_empty() {
        challenge.algorithm.as_str()
    } else {
        "MD5"
    });
    let realm = attempt
        .realm
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(challenge.realm.as_str());
    let uri = request_uri(url);
    let qop = challenge
        .qop
        .as_deref()
        .unwrap_or("auth")
        .split(',')
        .map(str::trim)
        .find(|s| s.eq_ignore_ascii_case("auth"))
        .unwrap_or("auth");
    if !qop.eq_ignore_ascii_case("auth") {
        return Err(ExecuteError::AuthTokenFailed(
            "digest qop=auth required".into(),
        ));
    }
    let ha1 = hash(
        algo,
        &format!("{}:{realm}:{}", attempt.username, attempt.password),
    );
    let ha2 = hash(algo, &format!("{}:{uri}", method.to_ascii_uppercase()));
    let nc = "00000001";
    let cnonce = hex::encode(nonce(16).as_bytes());
    let response = hash(
        algo,
        &format!("{ha1}:{}:{nc}:{cnonce}:{qop}:{ha2}", challenge.nonce),
    );
    let mut parts = vec![
        format!("username=\"{}\"", attempt.username),
        format!("realm=\"{realm}\""),
        format!("nonce=\"{}\"", challenge.nonce),
        format!("uri=\"{uri}\""),
        format!("algorithm={algo}"),
        format!("response=\"{response}\""),
        format!("qop={qop}"),
        format!("nc={nc}"),
        format!("cnonce=\"{cnonce}\""),
    ];
    if let Some(op) = challenge.opaque.as_deref() {
        parts.push(format!("opaque=\"{op}\""));
    }
    Ok(format!("Digest {}", parts.join(", ")))
}

fn normalize_algo(a: &str) -> &'static str {
    match a.trim().to_ascii_uppercase().as_str() {
        "SHA-256" | "SHA256" => "SHA-256",
        _ => "MD5",
    }
}

fn hash(algo: &str, data: &str) -> String {
    if algo == "SHA-256" {
        hex::encode(Sha256::digest(data.as_bytes()))
    } else {
        hex::encode(Md5::digest(data.as_bytes()))
    }
}

fn request_uri(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        let mut path = parsed.path().to_string();
        if path.is_empty() {
            path = "/".into();
        }
        if let Some(q) = parsed.query() {
            path.push('?');
            path.push_str(q);
        }
        return path;
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_digest_challenge() {
        let h = r#"Digest realm="testrealm@host.com", qop="auth,auth-int", nonce="dcd98b7102dd2f0e8b11d0f600bfb0c093", opaque="5ccc069c403ebaf9f0171e9517f40e41""#;
        let c = parse_www_authenticate_digest(h).unwrap();
        assert_eq!(c.realm, "testrealm@host.com");
        assert!(c.nonce.starts_with("dcd98b"));
    }

    #[test]
    fn builds_digest_header() {
        let attempt = DigestAttempt {
            username: "Mufasa".into(),
            password: "Circle Of Life".into(),
            algorithm: "MD5".into(),
            realm: None,
        };
        let chal = DigestChallenge {
            realm: "testrealm@host.com".into(),
            nonce: "dcd98b7102dd2f0e8b11d0f600bfb0c093".into(),
            opaque: Some("5ccc069c403ebaf9f0171e9517f40e41".into()),
            algorithm: "MD5".into(),
            qop: Some("auth".into()),
        };
        let h =
            build_digest_authorization(&attempt, &chal, "GET", "http://example.com/dir/index.html")
                .unwrap();
        assert!(h.starts_with("Digest "));
        assert!(h.contains("username=\"Mufasa\""));
        assert!(h.contains("qop=auth"));
        assert!(!h.contains("qop=auth-int"));
    }
}
