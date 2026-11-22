use mazbri_core::{
    AuthInjectSpec, AuthProfile, ChannelEgressSpec, SupplierConfig, UpstreamTlsSpec,
    mask_tls_in_json, merge_tls_secret_in_json, profile_is_undeclared, supplier_secrets_key,
    upsert_egress_in_channel_config,
};

use super::{ApiError, Arc, Duration, RuntimeEngine, SsrfChecker};

/// 脱敏端点数组中的 `tls.client_key`（用于 `DraftDto` 及版本快照响应）。
#[must_use]
pub(crate) fn mask_endpoints_secrets(mut endpoints: serde_json::Value) -> serde_json::Value {
    if let Some(arr) = endpoints.as_array_mut() {
        for ep in arr {
            if let Some(tls) = ep
                .as_object_mut()
                .and_then(|o| o.get_mut("tls"))
            {
                mask_tls_in_json(tls);
            }
        }
    }
    endpoints
}

fn find_existing_endpoint_tls<'a>(
    existing: Option<&'a serde_json::Value>,
    ep_id: Option<&str>,
    idx: usize,
) -> Option<&'a serde_json::Value> {
    let arr = existing?.as_array()?;
    if let Some(id) = ep_id.filter(|s| !s.is_empty())
        && let Some(found) = arr
            .iter()
            .find(|e| e.get("id").and_then(serde_json::Value::as_str) == Some(id))
    {
        return found.get("tls");
    }
    arr.get(idx).and_then(|e| e.get("tls"))
}

/// 校验、合并掩码并加密草稿端点列表中的 `tls.client_key`，同时守卫 `insecure_skip_verify` 超管权限。
pub(crate) fn sanitize_and_encrypt_endpoints_tls(
    raw: &serde_json::Value,
    existing: Option<&serde_json::Value>,
    is_super_admin: bool,
) -> Result<serde_json::Value, ApiError> {
    let mut out = raw.clone();
    let Some(arr) = out.as_array_mut() else {
        return Ok(out);
    };
    for (idx, ep) in arr.iter_mut().enumerate() {
        let Some(ep_obj) = ep.as_object_mut() else {
            continue;
        };
        let ep_id = ep_obj
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let Some(tls_val) = ep_obj.get_mut("tls") else {
            continue;
        };
        if tls_val.is_null() {
            ep_obj.remove("tls");
            continue;
        }
        let old_tls = find_existing_endpoint_tls(existing, ep_id.as_deref(), idx);
        merge_tls_secret_in_json(tls_val, old_tls);
        let tls: UpstreamTlsSpec = serde_json::from_value(tls_val.clone()).map_err(|e| {
            ApiError::validation(format!("CONFIG_INVALID: endpoints[{idx}].tls: {e}"))
        })?;
        let old_insecure = old_tls
            .and_then(|t| t.get("insecure_skip_verify"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if tls.insecure_skip_verify && !old_insecure && !is_super_admin {
            return Err(ApiError::Forbidden);
        }
        if let Some(mut norm) = tls.normalized() {
            norm.validate(&format!("endpoints[{idx}].tls"))
                .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
            let key = supplier_secrets_key()
                .map_err(|e| ApiError::internal(format!("load supplier secrets key: {e}")))?;
            norm.encrypt_for_storage(key)
                .map_err(|e| ApiError::internal(format!("encrypt endpoint tls key: {e}")))?;
            let encoded = serde_json::to_value(norm)
                .map_err(|e| ApiError::internal(format!("serialize endpoint tls: {e}")))?;
            ep_obj.insert("tls".into(), encoded);
        } else {
            ep_obj.remove("tls");
        }
    }
    Ok(out)
}

/// 守卫供应商 `config.tls.insecure_skip_verify`：仅超级管理员可新开启跳过证书校验。
///
/// # Errors
///
/// 非超级管理员尝试新开启 `insecure_skip_verify` 时返回 `ApiError::Forbidden`。
pub fn guard_supplier_tls_privilege(
    new_cfg: &serde_json::Value,
    existing_cfg: Option<&serde_json::Value>,
    is_super_admin: bool,
) -> Result<(), ApiError> {
    let new_insecure = new_cfg
        .get("tls")
        .and_then(|t| t.get("insecure_skip_verify"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !new_insecure || is_super_admin {
        return Ok(());
    }
    let old_insecure = existing_cfg
        .and_then(|c| c.get("tls"))
        .and_then(|t| t.get("insecure_skip_verify"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if old_insecure {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

/// 守卫通道 `config.egress.tls.insecure_skip_verify`：仅超级管理员可新开启跳过证书校验。
///
/// # Errors
///
/// 非超级管理员尝试新开启 `insecure_skip_verify` 时返回 `ApiError::Forbidden`。
pub fn guard_channel_tls_privilege(
    new_cfg: &serde_json::Value,
    existing_cfg: Option<&serde_json::Value>,
    is_super_admin: bool,
) -> Result<(), ApiError> {
    let new_insecure = new_cfg
        .get("egress")
        .and_then(|e| e.get("tls"))
        .and_then(|t| t.get("insecure_skip_verify"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !new_insecure || is_super_admin {
        return Ok(());
    }
    let old_insecure = existing_cfg
        .and_then(|c| c.get("egress"))
        .and_then(|e| e.get("tls"))
        .and_then(|t| t.get("insecure_skip_verify"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if old_insecure {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

pub(crate) fn endpoint_by_id<'a>(
    endpoints: &'a serde_json::Value,
    id: Option<&str>,
) -> Option<&'a serde_json::Value> {
    let arr = endpoints.as_array()?;
    if let Some(id) = id.filter(|s| !s.is_empty())
        && let Some(found) = arr.iter().find(|e| {
            e.get("id").and_then(|v| v.as_str()) == Some(id)
                || e.get("name").and_then(|v| v.as_str()) == Some(id)
        })
    {
        return Some(found);
    }
    arr.first()
}

/// Fill missing `egress.auth` fields from supplier `auth_flow` (credentials stay on supplier).
/// Inherit copies `auth_flow.profile` only. Never infers from `token_url` / method.
pub(crate) fn merge_auth_from_supplier(spec: &mut ChannelEgressSpec, cfg: &SupplierConfig) {
    if spec
        .auth
        .as_ref()
        .is_some_and(|a| a.profile.trim() == mazbri_core::AUTH_SKIP)
    {
        return;
    }
    let Some(flow) = cfg.auth_flow.as_ref().filter(|f| f.enabled) else {
        return;
    };
    let mut auth = spec.auth.clone().unwrap_or_default();
    if profile_is_undeclared(Some(auth.profile.as_str()))
        && let Some(p) = flow
            .profile
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        && !profile_is_undeclared(Some(p))
    {
        auth.profile = p.to_string();
    }
    let Ok(target) = AuthProfile::parse(&auth.profile) else {
        spec.auth = Some(auth);
        return;
    };
    if target.uses_ticket() {
        copy_if_empty(&mut auth.token_url, nonempty(&flow.token_url));
        if auth.token_path.as_deref().map_or("", str::trim).is_empty()
            && let Some(p) = flow
                .token_path
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        {
            auth.token_path = Some(p.to_string());
        }
        copy_if_empty(&mut auth.token_jsonpath, nonempty(&flow.token_jsonpath));
        copy_if_empty(&mut auth.expiry_jsonpath, nonempty(&flow.expiry_jsonpath));
        copy_if_empty(&mut auth.token_method, nonempty(&flow.method));
        if auth
            .request_body_tpl
            .as_deref()
            .map_or("", str::trim)
            .is_empty()
            && !flow.request_body_tpl.trim().is_empty()
        {
            auth.request_body_tpl = Some(flow.request_body_tpl.clone());
        }
        if auth.headers.is_empty() && !flow.headers.is_empty() {
            auth.headers.clone_from(&flow.headers);
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
        AuthProfile::Jwt if auth.jwt.is_none() => auth.jwt.clone_from(&flow.jwt),
        AuthProfile::Digest if auth.digest.is_none() => auth.digest.clone_from(&flow.digest),
        AuthProfile::AwsSigV4 if auth.aws.is_none() => auth.aws.clone_from(&flow.aws),
        AuthProfile::Hawk if auth.hawk.is_none() => auth.hawk.clone_from(&flow.hawk),
        _ => {}
    }
    // 认证失效规则按整个对象继承（不逐字段合并），并裁掉与本渠道适配器不适用的字段。
    if auth.invalidation.is_none()
        && target != AuthProfile::None
        && target != AuthProfile::Skip
        && let Some(ref inv) = flow.invalidation
    {
        auth.invalidation = Some(match spec.adapter_kind() {
            Ok(kind) => inv.clone().for_adapter(kind),
            Err(_) => inv.clone(),
        });
    }
    spec.auth = Some(auth);
}

fn nonempty(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() { None } else { Some(t) }
}

fn copy_if_empty(slot: &mut Option<String>, src: Option<&str>) {
    if slot.as_deref().map_or("", str::trim).is_empty()
        && let Some(v) = src
    {
        *slot = Some(v.to_string());
    }
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

/// 构建仅用于 SSRF 预检 / `auth_only` / `transform_preview` / microservice 本地探测的 [`RuntimeEngine`]。
/// **不再作为 business/live 默认 Send**；门控只认 bypass 管理探针（`bypass_mgmt_probe`）。
///
/// `ssrf` 由 mgmt `[ssrf]` 配置生成（启动时构建一次，存于 `AppState::connector_runtime`），
/// 同时用于连接期 DNS 过滤与 `validate_public_http_url` 预检。
#[must_use]
pub fn build_connector_runtime(
    ssrf: SsrfChecker,
    options: mazbri_upstream_runtime::RuntimeOptions,
) -> RuntimeEngine {
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
    // 试跑与 bypass 同一套运行时实现；参数来自 `[connector_probe.runtime]`。
    RuntimeEngine::new(Arc::new(client), ssrf).with_options(options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_core::{
        ADAPTER_HTTP_JSON, AUTH_BASIC, AUTH_NONE, AUTH_OAUTH_CC_FORM, AuthFlowConfig, AuthSpec,
    };

    fn oauth_supplier(
        invalidation: Option<mazbri_core::AuthInvalidationSpec>,
        enabled: bool,
    ) -> SupplierConfig {
        SupplierConfig {
            auth_flow: Some(AuthFlowConfig {
                enabled,
                profile: Some(AUTH_OAUTH_CC_FORM.into()),
                token_url: "https://idp.example/token".into(),
                invalidation,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn jsonrpc_spec(invalidation: Option<mazbri_core::AuthInvalidationSpec>) -> ChannelEgressSpec {
        ChannelEgressSpec {
            adapter: mazbri_core::channel_egress::ADAPTER_JSONRPC.to_string(),
            auth: Some(AuthSpec {
                profile: AUTH_OAUTH_CC_FORM.to_string(),
                invalidation,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn merge_inherits_supplier_invalidation_when_channel_omits() {
        let inv = mazbri_core::AuthInvalidationSpec {
            jsonrpc_codes: vec![-32001],
            ws_close_before_reply: true,
            ..Default::default()
        };
        let mut spec = jsonrpc_spec(None);
        merge_auth_from_supplier(&mut spec, &oauth_supplier(Some(inv), true));
        let got = spec.auth.unwrap().invalidation.unwrap();
        assert_eq!(got.jsonrpc_codes, vec![-32001]);
        assert_eq!(got.http_status, vec![401]);
        // ws_close_before_reply 不适用于 jsonrpc 渠道，继承时裁掉。
        assert!(!got.ws_close_before_reply);
    }

    #[test]
    fn merge_keeps_channel_invalidation_over_supplier() {
        let channel_inv = mazbri_core::AuthInvalidationSpec {
            http_status: vec![419],
            ..Default::default()
        };
        let supplier_inv = mazbri_core::AuthInvalidationSpec {
            jsonrpc_codes: vec![-32001],
            ..Default::default()
        };
        let mut spec = jsonrpc_spec(Some(channel_inv.clone()));
        merge_auth_from_supplier(&mut spec, &oauth_supplier(Some(supplier_inv), true));
        assert_eq!(spec.auth.unwrap().invalidation, Some(channel_inv));
    }

    #[test]
    fn merge_skips_invalidation_when_auth_flow_disabled() {
        let supplier_inv = mazbri_core::AuthInvalidationSpec {
            jsonrpc_codes: vec![-32001],
            ..Default::default()
        };
        let mut spec = jsonrpc_spec(None);
        merge_auth_from_supplier(&mut spec, &oauth_supplier(Some(supplier_inv), false));
        assert!(spec.auth.unwrap().invalidation.is_none());
    }

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
