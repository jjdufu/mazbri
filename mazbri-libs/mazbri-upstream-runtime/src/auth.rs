//! Auth profiles applied before adapter invoke.
use chrono::Utc;
use reqwest::Method;
use reqwest::header::{HeaderName, HeaderValue};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use mazbri_core::{AdapterKind, AuthInjectSpec, AuthProfile, AuthSpec, ChannelEgressSpec};

use crate::auth_digest::DigestAttempt;
use crate::http_call::HttpCall;

use crate::funcs::{basic_b64, url_encode};
use crate::ssrf::SsrfChecker;
use crate::token_cache::{CachedToken, TokenCache};
use crate::types::ExecuteError;
use mazbri_core::json_path::extract_by_path;

pub struct AppliedAuth {
    pub headers: HashMap<String, String>,
    pub query: Vec<(String, String)>,
    pub metadata: HashMap<String, String>,
    pub token: Option<String>,
}

impl AppliedAuth {
    fn empty() -> Self {
        Self {
            headers: HashMap::new(),
            query: Vec::new(),
            metadata: HashMap::new(),
            token: None,
        }
    }

    fn set_basic(&mut self, id: Option<&str>, secret: Option<&str>) -> Result<(), ExecuteError> {
        let user = id.unwrap_or("");
        let pass = secret.ok_or_else(|| {
            ExecuteError::AuthTokenFailed("basic profile missing secret credential".into())
        })?;
        let val = format!("Basic {}", basic_b64(user, pass));
        self.headers.insert("Authorization".into(), val);
        Ok(())
    }

    fn place(&mut self, to: &str, key: &str, val: &str) {
        place_inject(
            to,
            key,
            val,
            &mut self.headers,
            &mut self.query,
            &mut self.metadata,
        );
    }
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
        if let Some(v) = creds.get(*n)
            && !v.is_empty()
        {
            return Some(v.as_str());
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
    /// 换票响应既无配置的过期字段、也无 `expires_in` 时的缓存时长。
    pub token_default_ttl: Duration,
    pub request_id: &'a str,
}

/// 注入位置、键名与格式（空值回落到默认）。
fn inject_settings(inject: &AuthInjectSpec) -> (&str, &str, &str) {
    let to = if inject.to.is_empty() {
        "header"
    } else {
        inject.to.as_str()
    };
    let key = if inject.key.is_empty() {
        "Authorization"
    } else {
        inject.key.as_str()
    };
    let fmt = if inject.format.is_empty() {
        "{}"
    } else {
        inject.format.as_str()
    };
    (to, key, fmt)
}

/// 按 `credential_keys` 与常见别名解析 (id, secret)。
fn resolve_id_secret<'c>(
    auth: &AuthSpec,
    credentials: &'c HashMap<String, String>,
) -> (Option<&'c str>, Option<&'c str>) {
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
    (id, secret)
}

pub async fn apply_auth(args: AuthApply<'_>) -> Result<AppliedAuth, ExecuteError> {
    let mut applied = AppliedAuth::empty();
    let Some(auth) = args.spec.auth.as_ref() else {
        return Ok(applied);
    };
    let profile = auth
        .profile_kind()
        .map_err(|e| ExecuteError::SpecInvalid(e.to_string()))?;
    let credentials = args.credentials;

    let inject = auth.inject.clone().unwrap_or_default();
    let (inject_to, inject_key, inject_fmt) = inject_settings(&inject);
    let (id, secret) = resolve_id_secret(auth, credentials);
    let fmt = |raw: &str| inject_fmt.replace("{}", raw);

    match profile {
        // SignForm: per-request MAC/签名由 sign_form 适配器经 crypto_pipeline 完成。
        AuthProfile::None
        | AuthProfile::SignForm
        | AuthProfile::Digest
        | AuthProfile::AwsSigV4
        | AuthProfile::Hawk => {}
        AuthProfile::ApiKey => {
            let key = secret.ok_or_else(|| {
                ExecuteError::AuthTokenFailed("api_key profile missing secret credential".into())
            })?;
            applied.place(inject_to, inject_key, &fmt(key));
        }
        AuthProfile::Basic => applied.set_basic(id, secret)?,
        AuthProfile::GrpcMetadata => {
            if let Some(s) = secret {
                let val = fmt(s);
                applied.metadata.insert(inject_key.to_string(), val.clone());
                applied.headers.insert(inject_key.to_string(), val);
            }
        }
        AuthProfile::Bearer => {
            let key =
                credential(credentials, &["token", "api_key", "secret"]).ok_or_else(|| {
                    ExecuteError::AuthTokenFailed("bearer profile missing token credential".into())
                })?;
            applied.place(inject_to, inject_key, &fmt(key));
        }
        AuthProfile::Jwt => {
            let token = jwt_token(&args, auth).await?;
            applied.place(inject_to, inject_key, &fmt(&token));
            applied.token = Some(token);
        }
        AuthProfile::OauthCcForm | AuthProfile::Gettoken => {
            let token = ticket_token(&args, auth, profile, id, secret).await?;
            applied.place(inject_to, inject_key, &fmt(&token));
            applied.token = Some(token);
            // 换票档不追加 auth.headers（保持既有行为）。
            return Ok(applied);
        }
    }

    for (k, v) in &auth.headers {
        applied.headers.insert(k.clone(), v.clone());
    }
    Ok(applied)
}

/// 本次请求所用令牌在 [`TokenCache`] 中的键：JWT 用 [`crate::auth_jwt::jwt_cache_key`]，
/// 换票档用渠道缓存键；静态凭证档（`api_key` / `basic` 等）不经缓存，返回 `None`。
///
/// # Errors
///
/// 鉴权档或 JWT 参数无效时返回 [`ExecuteError::SpecInvalid`]。
pub fn issued_token_key(
    cache_key: &str,
    spec: &ChannelEgressSpec,
) -> Result<Option<String>, ExecuteError> {
    let Some(auth) = spec.auth.as_ref() else {
        return Ok(None);
    };
    let profile = auth
        .profile_kind()
        .map_err(|e| ExecuteError::SpecInvalid(e.to_string()))?;
    match profile {
        AuthProfile::Jwt => crate::auth_jwt::jwt_cache_key(cache_key, auth.jwt.as_ref()).map(Some),
        AuthProfile::OauthCcForm | AuthProfile::Gettoken => Ok(Some(cache_key.to_owned())),
        _ => Ok(None),
    }
}

async fn jwt_token(args: &AuthApply<'_>, auth: &AuthSpec) -> Result<String, ExecuteError> {
    let jwt_key = crate::auth_jwt::jwt_cache_key(args.cache_key, auth.jwt.as_ref())?;
    crate::auth_jwt::sign_jwt(
        args.token_cache,
        &jwt_key,
        auth.jwt.as_ref(),
        args.credentials,
        Utc::now(),
    )
    .await
}

async fn ticket_token(
    args: &AuthApply<'_>,
    auth: &AuthSpec,
    profile: AuthProfile,
    id: Option<&str>,
    secret: Option<&str>,
) -> Result<String, ExecuteError> {
    let token_url = resolve_token_url(args.base_url, auth)?;
    fetch_token(
        args.token_cache,
        args.cache_key,
        TokenFetch {
            client: args.client.clone(),
            ssrf: args.ssrf.clone(),
            auth: auth.clone(),
            token_url,
            credentials: args.credentials.clone(),
            profile,
            id: id.map(str::to_string),
            secret: secret.map(str::to_string),
            request_id: args.request_id.to_string(),
            expiry: ExpiryPolicy {
                default_ttl: args.token_default_ttl,
                refresh_skew: args.token_cache.refresh_skew(),
            },
        },
    )
    .await
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
    expiry: ExpiryPolicy,
}

/// 换票响应的过期时间口径（来自运行时配置）。
#[derive(Debug, Clone, Copy)]
struct ExpiryPolicy {
    default_ttl: Duration,
    refresh_skew: Duration,
}

impl ExpiryPolicy {
    /// 取令牌有效秒数：
    /// 1. 配了 `expiry_jsonpath` 就按它取；取不到算配置错误，记 WARN 并用 `default_ttl`；
    /// 2. 没配就读 RFC 6749 §5.1 标准字段 `expires_in`；
    /// 3. 都取不到用 `default_ttl`；
    /// 4. 结果不大于 `refresh_skew` 时按 `refresh_skew + 1s` 缓存，避免每个请求都去换票。
    fn ttl(&self, json: &serde_json::Value, expiry_jsonpath: Option<&str>) -> Duration {
        let configured = expiry_jsonpath.map(str::trim).filter(|s| !s.is_empty());
        let secs = match configured {
            Some(path) => {
                let v = expiry_seconds(json, path);
                if v.is_none() {
                    tracing::warn!(
                        expiry_jsonpath = path,
                        default_ttl_secs = self.default_ttl.as_secs(),
                        "token response has no usable expiry at configured expiry_jsonpath; using default ttl"
                    );
                }
                v
            }
            None => expiry_seconds(json, "$.expires_in"),
        };
        let ttl = secs.map_or(self.default_ttl, Duration::from_secs);
        if ttl <= self.refresh_skew {
            self.refresh_skew + Duration::from_secs(1)
        } else {
            ttl
        }
    }
}

/// 过期秒数：数字或数字字符串。
fn expiry_seconds(json: &serde_json::Value, path: &str) -> Option<u64> {
    let v = extract_by_path(json, path)?;
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
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
                AuthProfile::Gettoken => fetch_gettoken(&fetch).await,
                _ => fetch_oauth_cc_form(&fetch).await,
            }
        })
        .await
        .map_err(ExecuteError::AuthTokenFailed)
}

async fn fetch_gettoken(fetch: &TokenFetch) -> Result<CachedToken, String> {
    let (client, token_url, auth) = (&fetch.client, fetch.token_url.as_str(), &fetch.auth);
    let (id, secret) = (fetch.id.as_deref(), fetch.secret.as_deref());
    let (request_id, expiry) = (fetch.request_id.as_str(), fetch.expiry);
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
    parse_token_response(resp, auth, expiry).await
}

async fn fetch_oauth_cc_form(fetch: &TokenFetch) -> Result<CachedToken, String> {
    let (client, token_url, auth) = (&fetch.client, fetch.token_url.as_str(), &fetch.auth);
    let (credentials, id, secret) = (
        &fetch.credentials,
        fetch.id.as_deref(),
        fetch.secret.as_deref(),
    );
    let (request_id, expiry) = (fetch.request_id.as_str(), fetch.expiry);
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
        let form_ct = auth.headers.iter().any(|(k, v)| {
            k.eq_ignore_ascii_case("content-type")
                && v.to_ascii_lowercase().contains("application/x-www-form-urlencoded")
        });
        let mut body = tpl.to_string();
        for (k, v) in credentials {
            let placeholder = format!("{{{{{k}}}}}");
            let repl = if form_ct {
                url_encode(v)
            } else {
                v.clone()
            };
            body = body.replace(&placeholder, &repl);
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
        return parse_token_response(resp, auth, expiry).await;
    }
    let mut form: Vec<(String, String)> = vec![("grant_type".into(), "client_credentials".into())];
    let basic_in_header = auth
        .headers
        .keys()
        .any(|k| k.eq_ignore_ascii_case("authorization"));
    if !basic_in_header && let (Some(i), Some(s)) = (id, secret) {
        builder = builder.header("authorization", format!("Basic {}", basic_b64(i, s)));
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
    parse_token_response(resp, auth, expiry).await
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
        && let Ok(hv) = HeaderValue::from_str(request_id)
    {
        builder = builder.header(HeaderName::from_static("x-request-id"), hv);
    }
    builder
}

async fn parse_token_response(
    resp: reqwest::Response,
    auth: &AuthSpec,
    expiry: ExpiryPolicy,
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
    let ttl = expiry.ttl(&json, auth.expiry_jsonpath.as_deref());
    let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or(chrono::Duration::MAX);
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
    use super::{ExpiryPolicy, TokenFetch, fetch_gettoken, fetch_oauth_cc_form};
    use std::time::Duration;

    fn tf(
        client: reqwest::Client,
        token_url: String,
        auth: AuthSpec,
        credentials: HashMap<String, String>,
        request_id: &str,
    ) -> TokenFetch {
        TokenFetch {
            client,
            ssrf: crate::ssrf::SsrfChecker::with_allow_private(true),
            auth,
            token_url,
            credentials,
            profile: mazbri_core::AuthProfile::OauthCcForm,
            id: None,
            secret: None,
            request_id: request_id.to_string(),
            expiry: test_expiry(),
        }
    }

    fn test_expiry() -> ExpiryPolicy {
        ExpiryPolicy {
            default_ttl: Duration::from_hours(1),
            refresh_skew: Duration::from_secs(30),
        }
    }

    #[test]
    fn expiry_reads_standard_expires_in_when_path_not_configured() {
        let json = serde_json::json!({"access_token": "t", "expires_in": 1800});
        assert_eq!(test_expiry().ttl(&json, None), Duration::from_mins(30));
        assert_eq!(
            test_expiry().ttl(&json, Some("  ")),
            Duration::from_mins(30)
        );
        let json = serde_json::json!({"access_token": "t", "expires_in": "1200"});
        assert_eq!(test_expiry().ttl(&json, None), Duration::from_mins(20));
    }

    #[test]
    fn expiry_falls_back_to_default_ttl_without_expires_in() {
        let json = serde_json::json!({"access_token": "t"});
        assert_eq!(test_expiry().ttl(&json, None), Duration::from_hours(1));
    }

    #[test]
    fn expiry_zero_or_tiny_is_clamped_above_skew() {
        let json = serde_json::json!({"expires_in": 0});
        assert_eq!(test_expiry().ttl(&json, None), Duration::from_secs(31));
        let json = serde_json::json!({"expires_in": 30});
        assert_eq!(test_expiry().ttl(&json, None), Duration::from_secs(31));
    }

    #[test]
    fn expiry_configured_path_missing_warns_and_uses_default() {
        use std::io::Write;
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct Buf(Arc<Mutex<Vec<u8>>>);
        impl Write for Buf {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let buf = Buf::default();
        let writer = buf.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        let json = serde_json::json!({"access_token": "t", "expires_in": 1800});
        let ttl = tracing::subscriber::with_default(subscriber, || {
            test_expiry().ttl(&json, Some("$.data.ttl"))
        });
        assert_eq!(
            ttl,
            Duration::from_hours(1),
            "配置路径取不到时不回退到 expires_in"
        );
        let logs = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        assert!(
            logs.contains("WARN") && logs.contains("$.data.ttl"),
            "{logs}"
        );
    }

    /// 计数换票服务：第 n 次请求返回令牌 `T{n}`。
    async fn spawn_counting_token_server(
        hits: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let hits = hits.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let _ = sock.read(&mut buf).await;
                    let n = hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    let body = format!(r#"{{"access_token":"T{n}","expires_in":3600}}"#);
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}/oauth/token")
    }

    #[tokio::test]
    async fn gated_eviction_then_refetch() {
        use super::{AuthApply, apply_auth};
        use crate::ssrf::SsrfChecker;
        use crate::token_cache::{StaleEvict, TokenCache};
        use mazbri_core::{ADAPTER_HTTP_JSON, AUTH_OAUTH_CC_FORM, ChannelEgressSpec};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let hits = Arc::new(AtomicUsize::new(0));
        let token_url = spawn_counting_token_server(hits.clone()).await;
        let spec = ChannelEgressSpec {
            adapter: ADAPTER_HTTP_JSON.into(),
            base_url: "http://127.0.0.1".into(),
            auth: Some(AuthSpec {
                profile: AUTH_OAUTH_CC_FORM.into(),
                token_url: Some(token_url),
                ..Default::default()
            }),
            ..Default::default()
        };
        let client = test_client();
        let ssrf = SsrfChecker::with_allow_private(true);
        let cache = Arc::new(TokenCache::new(Duration::from_secs(30)));
        let creds = HashMap::new();
        let key = super::issued_token_key("s:c:k", &spec).unwrap().unwrap();
        let run = || {
            let (client, ssrf, cache, spec, creds) = (&client, &ssrf, &cache, &spec, &creds);
            async move {
                apply_auth(AuthApply {
                    client,
                    ssrf,
                    token_cache: cache,
                    spec,
                    base_url: "http://127.0.0.1",
                    credentials: creds,
                    cache_key: "s:c:k",
                    token_default_ttl: Duration::from_hours(1),
                    request_id: "rid",
                })
                .await
                .unwrap()
            }
        };
        assert_eq!(run().await.token.as_deref(), Some("T1"));
        assert_eq!(run().await.token.as_deref(), Some("T1"), "未过期命中缓存");
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        assert_eq!(
            cache.evict_if_current_gated(&key, "T1", || false).await,
            StaleEvict::Denied,
            "预算不放行：保留缓存"
        );
        assert_eq!(run().await.token.as_deref(), Some("T1"));
        assert_eq!(
            cache.evict_if_current_gated(&key, "T1", || true).await,
            StaleEvict::Evicted
        );
        assert_eq!(run().await.token.as_deref(), Some("T2"));
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(
            cache
                .evict_if_current_gated(&key, "T1", || panic!("superseded must not consume budget"))
                .await,
            StaleEvict::Superseded,
            "已被换新：不再作废、不消耗预算"
        );
        assert_eq!(hits.load(Ordering::SeqCst), 2, "不重复换票");
    }

    #[test]
    fn expiry_configured_path_is_used() {
        let json = serde_json::json!({"data": {"ttl": 900}, "expires_in": 1800});
        assert_eq!(
            test_expiry().ttl(&json, Some("$.data.ttl")),
            Duration::from_mins(15)
        );
    }

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
            fetch_gettoken(&tf(client, url, auth, HashMap::new(), "rid-token")).await
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
            fetch_gettoken(&tf(client, url, auth, HashMap::new(), "new")).await
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
            fetch_gettoken(&tf(client, url, auth, HashMap::new(), "")).await
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
            fetch_oauth_cc_form(&tf(client, url, auth, creds, "rid-oauth")).await
        })
        .await;
        assert_eq!(
            header_line(&raw.to_ascii_lowercase(), "x-request-id:"),
            Some("rid-oauth")
        );
    }
}
