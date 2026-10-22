//! Auth profiles applied before adapter invoke.
use mazbri_core::num::SaturatingFrom;

use chrono::Utc;
use reqwest::Method;
use reqwest::header::{HeaderName, HeaderValue};
use std::collections::HashMap;
use std::sync::Arc;

use mazbri_core::{AdapterKind, AuthProfile, AuthSpec, ChannelEgressSpec};

use crate::auth_digest::DigestAttempt;
use crate::http_call::HttpCall;

use crate::funcs::{basic_b64, url_encode};
use crate::ssrf::SsrfChecker;
use crate::template::extract_by_path;
use crate::token_cache::{CachedToken, TokenCache};
use crate::types::ExecuteError;

pub struct AppliedAuth {
    pub headers: HashMap<String, String>,
    pub query: Vec<(String, String)>,
    pub metadata: HashMap<String, String>,
    pub token: Option<String>,
}

pub fn token_cache_key(supplier_id: &str, channel_id: &str, spec: &ChannelEgressSpec) -> String {
    let fingerprint = spec
        .auth
        .as_ref()
        .map(|a| {
            format!(
                "{}|{}|{}|{}",
                a.profile,
                a.token_url.as_deref().unwrap_or(""),
                a.token_path.as_deref().unwrap_or(""),
                a.token_jsonpath.as_deref().unwrap_or("")
            )
        })
        .unwrap_or_default();
    format!("{supplier_id}:{channel_id}:{fingerprint}")
}

pub fn credential<'a>(creds: &'a HashMap<String, String>, names: &[&str]) -> Option<&'a str> {
    for n in names {
        if let Some(v) = creds.get(*n) {
            if !v.is_empty() {
                return Some(v.as_str());
            }
        }
    }
    None
}

pub struct AuthApply<'a> {
    pub client: &'a reqwest::Client,
    pub ssrf: &'a SsrfChecker,
    pub token_cache: &'a Arc<TokenCache>,
    pub spec: &'a ChannelEgressSpec,
    pub base_url: &'a str,
    pub credentials: &'a HashMap<String, String>,
    pub cache_key: &'a str,
    pub evict_first: bool,
    pub request_id: &'a str,
}

pub async fn apply_auth(args: AuthApply<'_>) -> Result<AppliedAuth, ExecuteError> {
    let AuthApply {
        client,
        ssrf,
        token_cache,
        spec,
        base_url,
        credentials,
        cache_key,
        evict_first,
        request_id,
    } = args;
    let mut headers = HashMap::new();
    let mut query = Vec::new();
    let mut metadata = HashMap::new();
    let Some(auth) = spec.auth.as_ref() else {
        return Ok(AppliedAuth {
            headers,
            query,
            metadata,
            token: None,
        });
    };
    let profile = auth
        .profile_kind()
        .map_err(|e| ExecuteError::SpecInvalid(e.to_string()))?;
    if evict_first {
        token_cache.evict_token(cache_key).await;
    }

    let inject = auth.inject.clone().unwrap_or_default();
    let inject_to = if inject.to.is_empty() {
        "header"
    } else {
        inject.to.as_str()
    };
    let inject_key = if inject.key.is_empty() {
        "Authorization"
    } else {
        inject.key.as_str()
    };
    let inject_fmt = if inject.format.is_empty() {
        "{}"
    } else {
        inject.format.as_str()
    };

    let id_key = auth.credential_keys.as_ref().and_then(|k| k.id.as_deref());
    let secret_key = auth
        .credential_keys
        .as_ref()
        .and_then(|k| k.secret.as_deref());
    let id = credential(
        credentials,
        &[
            id_key.unwrap_or("client_id"),
            "client_id",
            "app_id",
            "username",
        ],
    );
    let secret = credential(
        credentials,
        &[
            secret_key.unwrap_or("client_secret"),
            "client_secret",
            "api_key",
            "secret",
            "password",
        ],
    );

    match profile {
        AuthProfile::None => {}
        AuthProfile::ApiKey => {
            let key = secret.ok_or_else(|| {
                ExecuteError::AuthTokenFailed("api_key profile missing secret credential".into())
            })?;
            let val = inject_fmt.replace("{}", key);
            place_inject(
                inject_to,
                inject_key,
                &val,
                &mut headers,
                &mut query,
                &mut metadata,
            );
        }
        AuthProfile::Basic => {
            let user = id.unwrap_or("");
            let pass = secret.ok_or_else(|| {
                ExecuteError::AuthTokenFailed("basic profile missing secret credential".into())
            })?;
            let val = format!("Basic {}", basic_b64(user, pass));
            headers.insert("Authorization".into(), val);
        }
        AuthProfile::SignForm => {
            // Per-request HMAC is assembled by the sign_form adapter.
        }
        AuthProfile::GrpcMetadata => {
            if let Some(s) = secret {
                let val = inject_fmt.replace("{}", s);
                metadata.insert(inject_key.to_string(), val.clone());
                headers.insert(inject_key.to_string(), val);
            }
        }
        AuthProfile::Bearer => {
            let key =
                credential(credentials, &["token", "api_key", "secret"]).ok_or_else(|| {
                    ExecuteError::AuthTokenFailed("bearer profile missing token credential".into())
                })?;
            let val = inject_fmt.replace("{}", key);
            place_inject(
                inject_to,
                inject_key,
                &val,
                &mut headers,
                &mut query,
                &mut metadata,
            );
        }
        AuthProfile::Jwt => {
            let token = crate::auth_jwt::sign_jwt(
                token_cache,
                cache_key,
                auth.jwt.as_ref(),
                credentials,
                Utc::now(),
            )
            .await?;
            let val = inject_fmt.replace("{}", &token);
            place_inject(
                inject_to,
                inject_key,
                &val,
                &mut headers,
                &mut query,
                &mut metadata,
            );
            for (k, v) in &auth.headers {
                headers.insert(k.clone(), v.clone());
            }
            return Ok(AppliedAuth {
                headers,
                query,
                metadata,
                token: Some(token),
            });
        }
        AuthProfile::Digest | AuthProfile::AwsSigV4 | AuthProfile::Hawk => {}
        AuthProfile::OauthCcForm | AuthProfile::Gettoken => {
            let token_url = resolve_token_url(base_url, auth)?;
            if evict_first {
                token_cache.evict_token(cache_key).await;
            }
            let token = fetch_token(
                token_cache,
                cache_key,
                TokenFetch {
                    client: client.clone(),
                    ssrf: ssrf.clone(),
                    auth: auth.clone(),
                    token_url,
                    credentials: credentials.clone(),
                    profile,
                    id: id.map(str::to_string),
                    secret: secret.map(str::to_string),
                    request_id: request_id.to_string(),
                },
            )
            .await?;
            let val = inject_fmt.replace("{}", &token);
            place_inject(
                inject_to,
                inject_key,
                &val,
                &mut headers,
                &mut query,
                &mut metadata,
            );
            return Ok(AppliedAuth {
                headers,
                query,
                metadata,
                token: Some(token),
            });
        }
    }

    for (k, v) in &auth.headers {
        headers.insert(k.clone(), v.clone());
    }
    Ok(AppliedAuth {
        headers,
        query,
        metadata,
        token: None,
    })
}

fn place_inject(
    to: &str,
    key: &str,
    val: &str,
    headers: &mut HashMap<String, String>,
    query: &mut Vec<(String, String)>,
    metadata: &mut HashMap<String, String>,
) {
    match to {
        "query" => query.push((key.to_string(), val.to_string())),
        "metadata" | "first_frame" => {
            metadata.insert(key.to_string(), val.to_string());
            headers.insert(key.to_string(), val.to_string());
        }
        _ => {
            headers.insert(key.to_string(), val.to_string());
        }
    }
}

fn resolve_token_url(base_url: &str, auth: &AuthSpec) -> Result<String, ExecuteError> {
    if let Some(u) = auth
        .token_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(u.to_string());
    }
    if let Some(p) = auth
        .token_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(crate::http_call::join_url(base_url, p));
    }
    Err(ExecuteError::SpecInvalid(
        "token_url / token_path missing".into(),
    ))
}

struct TokenFetch {
    client: reqwest::Client,
    ssrf: SsrfChecker,
    auth: AuthSpec,
    token_url: String,
    credentials: HashMap<String, String>,
    profile: AuthProfile,
    id: Option<String>,
    secret: Option<String>,
    request_id: String,
}

async fn fetch_token(
    token_cache: &Arc<TokenCache>,
    cache_key: &str,
    fetch: TokenFetch,
) -> Result<String, ExecuteError> {
    token_cache
        .get_token(cache_key, || async move {
            if let Err(e) = fetch.ssrf.validate_url(&fetch.token_url).await {
                return Err(format!("Auth SSRF Violation: {e}"));
            }
            match fetch.profile {
                AuthProfile::Gettoken => {
                    fetch_gettoken(
                        &fetch.client,
                        &fetch.token_url,
                        &fetch.auth,
                        fetch.id.as_deref(),
                        fetch.secret.as_deref(),
                        &fetch.request_id,
                    )
                    .await
                }
                _ => {
                    fetch_oauth_cc_form(
                        &fetch.client,
                        &fetch.token_url,
                        &fetch.auth,
                        &fetch.credentials,
                        fetch.id.as_deref(),
                        fetch.secret.as_deref(),
                        &fetch.request_id,
                    )
                    .await
                }
            }
        })
        .await
        .map_err(ExecuteError::AuthTokenFailed)
}

async fn fetch_gettoken(
    client: &reqwest::Client,
    token_url: &str,
    auth: &AuthSpec,
    id: Option<&str>,
    secret: Option<&str>,
    request_id: &str,
) -> Result<CachedToken, String> {
    let mut q = Vec::new();
    if let Some(i) = id {
        q.push(format!("app_id={}", url_encode(i)));
    }
    if let Some(s) = secret {
        q.push(format!("secret={}", url_encode(s)));
    }
    let url = if q.is_empty() {
        token_url.to_string()
    } else {
        let sep = if token_url.contains('?') { "&" } else { "?" };
        format!("{token_url}{sep}{}", q.join("&"))
    };
    let mut builder = client.request(Method::GET, &url);
    builder = apply_auth_headers(builder, &auth.headers, request_id);
    let resp = builder
        .send()
        .await
        .map_err(|e| format!("Token request failed: {e}"))?;
    parse_token_response(resp, auth).await
}

async fn fetch_oauth_cc_form(
    client: &reqwest::Client,
    token_url: &str,
    auth: &AuthSpec,
    credentials: &HashMap<String, String>,
    id: Option<&str>,
    secret: Option<&str>,
    request_id: &str,
) -> Result<CachedToken, String> {
    let method = auth
        .token_method
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("POST");
    let method = if method.eq_ignore_ascii_case("GET") {
        Method::GET
    } else {
        Method::POST
    };
    let mut builder = client.request(method.clone(), token_url);
    let has_ct = auth
        .headers
        .keys()
        .any(|k| k.eq_ignore_ascii_case("content-type"));
    let tpl = auth
        .request_body_tpl
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(tpl) = tpl {
        let mut body = tpl.to_string();
        for (k, v) in credentials {
            let placeholder = format!("{{{{{k}}}}}");
            body = body.replace(&placeholder, v);
        }
        builder = apply_auth_headers(builder, &auth.headers, request_id);
        if method != Method::GET {
            if !has_ct {
                builder = builder.header("content-type", "application/json");
            }
            builder = builder.body(body);
        }
        let resp = builder
            .send()
            .await
            .map_err(|e| format!("Token request failed: {e}"))?;
        return parse_token_response(resp, auth).await;
    }
    let mut form: Vec<(String, String)> = vec![("grant_type".into(), "client_credentials".into())];
    let basic_in_header = auth
        .headers
        .keys()
        .any(|k| k.eq_ignore_ascii_case("authorization"));
    if !basic_in_header {
        if let (Some(i), Some(s)) = (id, secret) {
            builder = builder.header("authorization", format!("Basic {}", basic_b64(i, s)));
        }
    }
    if let Some(i) = id {
        form.push(("client_id".into(), i.to_string()));
    }
    if let Some(s) = secret {
        form.push(("client_secret".into(), s.to_string()));
    }
    builder = apply_auth_headers(builder, &auth.headers, request_id);
    if !has_ct {
        builder = builder.header("content-type", "application/x-www-form-urlencoded");
    }
    let body = form
        .iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    builder = builder.body(body);
    let resp = builder
        .send()
        .await
        .map_err(|e| format!("Token request failed: {e}"))?;
    parse_token_response(resp, auth).await
}

fn apply_auth_headers(
    mut builder: reqwest::RequestBuilder,
    headers: &HashMap<String, String>,
    request_id: &str,
) -> reqwest::RequestBuilder {
    for (k, v) in headers {
        if let (Ok(hn), Ok(hv)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            builder = builder.header(hn, hv);
        }
    }
    if !request_id.is_empty()
        && !headers
            .keys()
            .any(|k| k.eq_ignore_ascii_case("x-request-id"))
    {
        if let Ok(hv) = HeaderValue::from_str(request_id) {
            builder = builder.header(HeaderName::from_static("x-request-id"), hv);
        }
    }
    builder
}

async fn parse_token_response(
    resp: reqwest::Response,
    auth: &AuthSpec,
) -> Result<CachedToken, String> {
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("Token request returned status code {status}"));
    }
    let json = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse token response as JSON: {e}"))?;
    let path = auth
        .token_jsonpath
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("$.access_token");
    let t_val =
        extract_by_path(&json, path).ok_or_else(|| format!("Token not found at path: {path}"))?;
    let t_str = t_val
        .as_str()
        .ok_or_else(|| "Token is not a string".to_string())?
        .to_string();
    let mut expires_in_sec = 3600u64;
    if let Some(ep) = auth.expiry_jsonpath.as_deref().filter(|s| !s.is_empty()) {
        if let Some(expiry_val) = extract_by_path(&json, ep) {
            if let Some(n) = expiry_val.as_u64() {
                expires_in_sec = n;
            } else if let Some(s) = expiry_val.as_str() {
                if let Ok(n) = s.parse::<u64>() {
                    expires_in_sec = n;
                }
            }
        }
    }
    let expires_at = Utc::now() + chrono::Duration::seconds(i64::saturating_from(expires_in_sec));
    Ok(CachedToken {
        token: t_str,
        expires_at,
    })
}

/// Sign the assembled HTTP call (`SigV4` / HAWK). `sign_form` adapter never enters this match.
pub fn apply_request_signature(
    call: &mut HttpCall<'_>,
    adapter: AdapterKind,
    spec: &ChannelEgressSpec,
    credentials: &HashMap<String, String>,
) -> Result<(), ExecuteError> {
    if adapter == AdapterKind::SignForm {
        return Ok(());
    }
    let Some(auth) = spec.auth.as_ref() else {
        return Ok(());
    };
    let profile = auth
        .profile_kind()
        .map_err(|e| ExecuteError::SpecInvalid(e.to_string()))?;
    match profile {
        AuthProfile::AwsSigV4 => {
            let aws = auth
                .aws
                .as_ref()
                .ok_or_else(|| ExecuteError::SpecInvalid("aws_sigv4 missing aws spec".into()))?;
            let access = credential(credentials, &["access_key_id"]).ok_or_else(|| {
                ExecuteError::AuthTokenFailed("aws_sigv4 missing access_key_id".into())
            })?;
            let secret = credential(credentials, &["secret_access_key"]).ok_or_else(|| {
                ExecuteError::AuthTokenFailed("aws_sigv4 missing secret_access_key".into())
            })?;
            let session = credential(credentials, &["session_token"]);
            crate::auth_aws::sign_aws_sigv4(call, access, secret, session, aws, Utc::now())
        }
        AuthProfile::Hawk => {
            let hawk = auth.hawk.clone().unwrap_or_default();
            let id = credential(credentials, &["hawk_id", "id", "app_id"])
                .ok_or_else(|| ExecuteError::AuthTokenFailed("hawk missing hawk_id".into()))?;
            let key = credential(credentials, &["hawk_key", "secret"])
                .ok_or_else(|| ExecuteError::AuthTokenFailed("hawk missing hawk_key".into()))?;
            crate::auth_hawk::sign_hawk(call, id, key, &hawk, Utc::now(), None)
        }
        _ => Ok(()),
    }
}

pub fn digest_attempt_from(
    spec: &ChannelEgressSpec,
    credentials: &HashMap<String, String>,
) -> Option<DigestAttempt> {
    let auth = spec.auth.as_ref()?;
    let Ok(AuthProfile::Digest) = auth.profile_kind() else {
        return None;
    };
    let username = credential(credentials, &["username", "client_id", "app_id"])?;
    let password = credential(credentials, &["password", "client_secret", "secret"])?;
    let digest = auth.digest.clone().unwrap_or_default();
    Some(DigestAttempt {
        username: username.to_string(),
        password: password.to_string(),
        algorithm: digest.algorithm,
        realm: digest.realm,
    })
}

#[cfg(test)]
mod tests {
    use super::{fetch_gettoken, fetch_oauth_cc_form};
    use crate::token_cache::CachedToken;
    use mazbri_core::AuthSpec;
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn capture_token_raw<F, Fut>(handler: F) -> String
    where
        F: FnOnce(String) -> Fut,
        Fut: std::future::Future<Output = Result<CachedToken, String>>,
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap();
            let body = br#"{"access_token":"tok","expires_in":3600}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                std::str::from_utf8(body).unwrap()
            );
            let _ = sock.write_all(resp.as_bytes()).await;
            let _ = tx.send(String::from_utf8_lossy(&buf[..n]).into_owned());
        });
        let url = format!("http://{addr}/");
        handler(url).await.unwrap();
        rx.await.unwrap()
    }

    fn header_line<'a>(raw: &'a str, name: &str) -> Option<&'a str> {
        raw.lines().find_map(|line| {
            if line.to_ascii_lowercase().starts_with(name) {
                Some(line.split_once(':').map_or("", |(_, v)| v.trim()))
            } else {
                None
            }
        })
    }

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    #[tokio::test]
    async fn gettoken_builder_inserts_x_request_id() {
        let client = test_client();
        let auth = AuthSpec::default();
        let raw = capture_token_raw(|url| async move {
            fetch_gettoken(&client, &url, &auth, None, None, "rid-token").await
        })
        .await;
        assert_eq!(
            header_line(&raw.to_ascii_lowercase(), "x-request-id:"),
            Some("rid-token")
        );
    }

    #[tokio::test]
    async fn gettoken_builder_does_not_overwrite_existing() {
        let client = test_client();
        let mut auth = AuthSpec::default();
        auth.headers.insert("X-Request-Id".into(), "kept".into());
        let raw = capture_token_raw(|url| async move {
            fetch_gettoken(&client, &url, &auth, None, None, "new").await
        })
        .await;
        assert_eq!(
            header_line(&raw.to_ascii_lowercase(), "x-request-id:"),
            Some("kept")
        );
    }

    #[tokio::test]
    async fn gettoken_builder_skips_empty_request_id() {
        let client = test_client();
        let auth = AuthSpec::default();
        let raw = capture_token_raw(|url| async move {
            fetch_gettoken(&client, &url, &auth, None, None, "").await
        })
        .await;
        assert!(header_line(&raw.to_ascii_lowercase(), "x-request-id:").is_none());
    }

    #[tokio::test]
    async fn oauth_cc_form_builder_inserts_x_request_id() {
        let client = test_client();
        let auth = AuthSpec::default();
        let creds = HashMap::new();
        let raw = capture_token_raw(|url| async move {
            fetch_oauth_cc_form(&client, &url, &auth, &creds, None, None, "rid-oauth").await
        })
        .await;
        assert_eq!(
            header_line(&raw.to_ascii_lowercase(), "x-request-id:"),
            Some("rid-oauth")
        );
    }
}
