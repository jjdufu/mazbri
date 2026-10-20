use mazbri_core::{
    AuthInjectSpec, AuthProfile, ChannelEgressSpec, SupplierConfig, profile_is_undeclared,
    upsert_egress_in_channel_config,
};

use super::*;

pub(crate) fn endpoint_by_id<'a>(
    endpoints: &'a serde_json::Value,
    id: Option<&str>,
) -> Option<&'a serde_json::Value> {
    let arr = endpoints.as_array()?;
    if let Some(id) = id.filter(|s| !s.is_empty()) {
        if let Some(found) = arr.iter().find(|e| {
            e.get("id").and_then(|v| v.as_str()) == Some(id)
                || e.get("name").and_then(|v| v.as_str()) == Some(id)
        }) {
            return Some(found);
        }
    }
    arr.first()
}

/// Fill missing `egress.auth` fields from supplier `auth_flow` (credentials stay on supplier).
/// Inherit copies `auth_flow.profile` only. Never infers from token_url / method.
pub(crate) fn merge_auth_from_supplier(spec: &mut ChannelEgressSpec, cfg: &SupplierConfig) {
    let Some(flow) = cfg.auth_flow.as_ref().filter(|f| f.enabled) else {
        return;
    };
    let mut auth = spec.auth.clone().unwrap_or_default();
    if profile_is_undeclared(Some(auth.profile.as_str())) {
        if let Some(p) = flow
            .profile
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if !profile_is_undeclared(Some(p)) {
                auth.profile = p.to_string();
            }
        }
    }
    let Ok(target) = AuthProfile::parse(&auth.profile) else {
        spec.auth = Some(auth);
        return;
    };
    if target.uses_ticket() {
        copy_if_empty(&mut auth.token_url, nonempty(&flow.token_url));
        if auth
            .token_path
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            if let Some(p) = flow
                .token_path
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                auth.token_path = Some(p.to_string());
            }
        }
        copy_if_empty(&mut auth.token_jsonpath, nonempty(&flow.token_jsonpath));
        copy_if_empty(&mut auth.expiry_jsonpath, nonempty(&flow.expiry_jsonpath));
        copy_if_empty(&mut auth.token_method, nonempty(&flow.method));
        if auth
            .request_body_tpl
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
            && !flow.request_body_tpl.trim().is_empty()
        {
            auth.request_body_tpl = Some(flow.request_body_tpl.clone());
        }
        if auth.headers.is_empty() && !flow.headers.is_empty() {
            auth.headers = flow.headers.clone();
        }
    }
    if target.uses_inject() && auth.inject.is_none() {
        auth.inject = Some(AuthInjectSpec {
            to: flow.inject_to.clone(),
            key: flow.inject_key.clone(),
            format: flow.inject_value_format.clone(),
        });
    }
    match target {
        AuthProfile::Jwt if auth.jwt.is_none() => auth.jwt = flow.jwt.clone(),
        AuthProfile::Digest if auth.digest.is_none() => auth.digest = flow.digest.clone(),
        AuthProfile::AwsSigV4 if auth.aws.is_none() => auth.aws = flow.aws.clone(),
        AuthProfile::Hawk if auth.hawk.is_none() => auth.hawk = flow.hawk.clone(),
        _ => {}
    }
    spec.auth = Some(auth);
}

fn nonempty(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() { None } else { Some(t) }
}

fn copy_if_empty(slot: &mut Option<String>, src: Option<&str>) {
    if slot.as_deref().map(str::trim).unwrap_or("").is_empty() {
        if let Some(v) = src {
            *slot = Some(v.to_string());
        }
    }
}

pub(crate) fn inherit_requires_session_profile(
    cfg: Option<&SupplierConfig>,
    ep_profile_raw: Option<&str>,
) -> bool {
    let Some(flow) = cfg.and_then(|c| c.auth_flow.as_ref()).filter(|f| f.enabled) else {
        return false;
    };
    profile_is_undeclared(ep_profile_raw) && profile_is_undeclared(flow.profile.as_deref())
}

pub(crate) fn channel_config_with_egress(
    existing: &serde_json::Value,
    spec: &ChannelEgressSpec,
) -> Result<serde_json::Value, ApiError> {
    upsert_egress_in_channel_config(existing, spec)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))
}

pub(crate) const GATE_TTL_HOURS: i32 = 24;
pub(crate) const DEFAULT_RUN_LIMIT: i64 = 50;

pub(crate) fn runtime_engine() -> &'static RuntimeEngine {
    static ENGINE: OnceLock<RuntimeEngine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let ssrf = SsrfChecker::new();
        let client = ssrf
            .configure_http_client(
                reqwest::Client::builder()
                    .timeout(Duration::from_secs(30))
                    .connect_timeout(Duration::from_secs(10))
                    .redirect(reqwest::redirect::Policy::none()),
            )
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(
                    error = %e,
                    "Failed to build connector runtime HTTP client; using default client"
                );
                // Default client lacks SSRF DNS filter — still blocked by preflight validate_url.
                reqwest::Client::new()
            });
        RuntimeEngine::new(Arc::new(client), ssrf)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_core::{
        ADAPTER_HTTP_JSON, AUTH_BASIC, AUTH_NONE, AUTH_OAUTH_CC_FORM, AuthFlowConfig, AuthSpec,
    };

    #[test]
    fn merge_fills_oauth_from_supplier_auth_flow() {
        let mut spec = ChannelEgressSpec {
            adapter: ADAPTER_HTTP_JSON.to_string(),
            auth: Some(AuthSpec {
                profile: AUTH_NONE.to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let cfg = SupplierConfig {
            auth_flow: Some(AuthFlowConfig {
                enabled: true,
                profile: Some(AUTH_OAUTH_CC_FORM.into()),
                token_url: "https://idp.example/token".into(),
                method: "POST".into(),
                headers: [(
                    "Content-Type".into(),
                    "application/x-www-form-urlencoded".into(),
                )]
                .into_iter()
                .collect(),
                token_jsonpath: "$.access_token".into(),
                expiry_jsonpath: "$.expires_in".into(),
                inject_key: "Authorization".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        merge_auth_from_supplier(&mut spec, &cfg);
        let auth = spec.auth.expect("auth");
        assert_eq!(auth.profile, AUTH_OAUTH_CC_FORM);
        assert_eq!(auth.token_url.as_deref(), Some("https://idp.example/token"));
        assert_eq!(
            auth.headers.get("Content-Type").map(String::as_str),
            Some("application/x-www-form-urlencoded")
        );
    }

    #[test]
    fn merge_without_session_profile_does_not_guess_oauth() {
        let mut spec = ChannelEgressSpec {
            adapter: ADAPTER_HTTP_JSON.to_string(),
            auth: Some(AuthSpec {
                profile: AUTH_NONE.to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let cfg = SupplierConfig {
            auth_flow: Some(AuthFlowConfig {
                enabled: true,
                token_url: "https://idp.example/token".into(),
                method: "POST".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        merge_auth_from_supplier(&mut spec, &cfg);
        let auth = spec.auth.expect("auth");
        assert_eq!(auth.profile, AUTH_NONE);
        assert!(auth.token_url.is_none());
    }

    #[test]
    fn merge_basic_does_not_copy_token_url() {
        let mut spec = ChannelEgressSpec {
            adapter: ADAPTER_HTTP_JSON.to_string(),
            auth: Some(AuthSpec {
                profile: AUTH_BASIC.to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let cfg = SupplierConfig {
            auth_flow: Some(AuthFlowConfig {
                enabled: true,
                profile: Some(AUTH_BASIC.into()),
                token_url: "https://idp.example/token".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        merge_auth_from_supplier(&mut spec, &cfg);
        let auth = spec.auth.expect("auth");
        assert_eq!(auth.profile, AUTH_BASIC);
        assert!(auth.token_url.is_none());
        assert!(auth.inject.is_none());
    }
}
