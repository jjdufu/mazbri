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
            if norm.client_key.as_deref().is_some_and(|ck| {
                let trimmed = ck.trim();
                !trimmed.is_empty()
                    && !mazbri_core::crypto::supplier_secrets::is_encrypted_secret(trimmed)
            }) {
                let key = supplier_secrets_key()
                    .map_err(|e| ApiError::internal(format!("load supplier secrets key: {e}")))?;
                norm.encrypt_for_storage(key)
                    .map_err(|e| ApiError::internal(format!("encrypt endpoint tls key: {e}")))?;
            }
            let encoded = serde_json::to_value(norm)
                .map_err(|e| ApiError::internal(format!("serialize endpoint tls: {e}")))?;
            ep_obj.insert("tls".into(), encoded);
        } else {
            ep_obj.remove("tls");
        }
    }
    Ok(out)
}

/// 提取 URL 规范化源元组 `(scheme, host, port)`，用于防混淆代理人（Confused Deputy）Origin Pinning 校验。
#[must_use]
pub fn canonical_origin(raw_url: &str) -> Option<(String, String, u16)> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = url::Url::parse(trimmed).ok()?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    let host = parsed.host_str()?.to_ascii_lowercase();
    let default_port = match scheme.as_str() {
        "http" | "ws" => Some(80),
        "https" | "wss" => Some(443),
        _ => None,
    };
    let port = parsed.port().or(default_port)?;
    Some((scheme, host, port))
}

fn same_canonical_url_target(a: Option<&str>, b: Option<&str>) -> bool {
    let a = a.map(str::trim).filter(|s| !s.is_empty());
    let b = b.map(str::trim).filter(|s| !s.is_empty());
    match (a, b) {
        (None, None) => true,
        (Some(sa), Some(sb)) => match (canonical_origin(sa), canonical_origin(sb)) {
            (Some(oa), Some(ob)) => oa == ob,
            _ => sa.eq_ignore_ascii_case(sb),
        },
        _ => false,
    }
}

fn is_masked_or_placeholder_secret(val: &str) -> bool {
    let t = val.trim();
    t.contains('•') || t == "********"
}

fn starts_with_enc_prefix(val: &str) -> bool {
    val.trim_start()
        .get(..4)
        .is_some_and(|p| p.eq_ignore_ascii_case("enc:"))
}

/// 拦截外部 HTTP DTO 注入裸 `enc:` 密文（供应商 `auth_flow.credentials` 与 `tls.client_key`）。
///
/// # Errors
///
/// 客户端请求携带 `enc:` 前缀值时返回 `400 Bad Request`。
pub fn reject_client_enc_in_supplier_config(raw: &serde_json::Value) -> Result<(), ApiError> {
    if let Some(creds) = raw
        .get("auth_flow")
        .and_then(|a| a.get("credentials"))
        .and_then(serde_json::Value::as_object)
    {
        for (k, v) in creds {
            if v.as_str().is_some_and(starts_with_enc_prefix) {
                return Err(ApiError::validation(format!(
                    "凭据字段 '{k}' 禁止直接提交 enc: 密文，请输入明文或保持掩码"
                )));
            }
        }
    }
    if raw
        .get("tls")
        .and_then(|t| t.get("client_key"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(starts_with_enc_prefix)
    {
        return Err(ApiError::validation(
            "TLS 私钥字段禁止直接提交 enc: 密文，请输入明文 PEM 或保持掩码".to_string(),
        ));
    }
    Ok(())
}

/// 拦截外部 HTTP DTO 在通道配置中注入裸 `enc:` 密文。
///
/// # Errors
///
/// 客户端请求携带 `enc:` 前缀值时返回 `400 Bad Request`。
pub fn reject_client_enc_in_channel_config(raw: &serde_json::Value) -> Result<(), ApiError> {
    if raw
        .get("egress")
        .and_then(|e| e.get("tls"))
        .and_then(|t| t.get("client_key"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(starts_with_enc_prefix)
    {
        return Err(ApiError::validation(
            "通道 TLS 私钥字段禁止直接提交 enc: 密文，请输入明文 PEM 或保持掩码".to_string(),
        ));
    }
    Ok(())
}

/// 拦截外部 HTTP DTO 在工作台端点列表中注入裸 `enc:` 密文。
///
/// # Errors
///
/// 客户端请求携带 `enc:` 前缀值时返回 `400 Bad Request`。
pub fn reject_client_enc_in_endpoints(raw: &serde_json::Value) -> Result<(), ApiError> {
    if let Some(arr) = raw.as_array() {
        for (idx, ep) in arr.iter().enumerate() {
            if ep
                .get("tls")
                .and_then(|t| t.get("client_key"))
                .and_then(serde_json::Value::as_str)
                .is_some_and(starts_with_enc_prefix)
            {
                return Err(ApiError::validation(format!(
                    "endpoints[{idx}].tls.client_key 禁止直接提交 enc: 密文"
                )));
            }
        }
    }
    Ok(())
}

/// 拦截外部 HTTP DTO 在工作台环境变量中注入裸 `enc:` 密文。
///
/// # Errors
///
/// 客户端请求携带 `enc:` 前缀值时返回 `400 Bad Request`。
pub fn reject_client_enc_in_workbench_env(raw: &serde_json::Value) -> Result<(), ApiError> {
    fn check_kv_array(arr: &[serde_json::Value]) -> Result<(), ApiError> {
        for item in arr {
            if item
                .get("value")
                .and_then(serde_json::Value::as_str)
                .is_some_and(starts_with_enc_prefix)
            {
                return Err(ApiError::validation(
                    "workbench_env 变量值禁止直接提交 enc: 密文".to_string(),
                ));
            }
        }
        Ok(())
    }
    if let Some(arr) = raw.as_array() {
        check_kv_array(arr)?;
    } else if let Some(obj) = raw.as_object() {
        if let Some(global) = obj.get("global").and_then(serde_json::Value::as_array) {
            check_kv_array(global)?;
        }
        if let Some(envs) = obj.get("environments").and_then(serde_json::Value::as_array) {
            for env in envs {
                if let Some(vars) = env.get("variables").and_then(serde_json::Value::as_array) {
                    check_kv_array(vars)?;
                }
            }
        }
    }
    Ok(())
}

/// 判断供应商配置中是否需要加密新明文凭据或私钥（用于按需触发根密钥环加载）。
#[must_use]
pub fn supplier_config_has_secrets_to_seal(raw: &serde_json::Value) -> bool {
    let has_auth = raw.get("auth_flow").is_some_and(|v| !v.is_null());
    let has_tls = raw.get("tls").is_some_and(|v| !v.is_null());
    has_auth || has_tls
}

/// 判断端点列表中是否包含 `tls` 配置（需要密钥环加密）。
#[must_use]
pub fn endpoints_have_tls_to_seal(raw: &serde_json::Value) -> bool {
    raw.as_array().is_some_and(|arr| {
        arr.iter()
            .any(|ep| ep.get("tls").is_some_and(|t| !t.is_null()))
    })
}

fn supplier_config_has_stored_secrets(cfg: &serde_json::Value) -> bool {
    let has_creds = cfg
        .get("auth_flow")
        .and_then(|a| a.get("credentials"))
        .and_then(serde_json::Value::as_object)
        .is_some_and(|m| m.values().any(|v| v.as_str().is_some_and(|s| !s.trim().is_empty())));
    let has_tls_key = cfg
        .get("tls")
        .and_then(|t| t.get("client_key"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|s| !s.trim().is_empty());
    has_creds || has_tls_key
}

fn supplier_config_relies_on_backfill(
    new_cfg: &serde_json::Value,
    old_cfg: &serde_json::Value,
    allow_empty_backfill: bool,
) -> bool {
    if let (Some(new_creds), Some(old_creds)) = (
        new_cfg
            .get("auth_flow")
            .and_then(|a| a.get("credentials"))
            .and_then(serde_json::Value::as_object),
        old_cfg
            .get("auth_flow")
            .and_then(|a| a.get("credentials"))
            .and_then(serde_json::Value::as_object),
    ) {
        for (k, v) in new_creds {
            let Some(s) = v.as_str() else {
                continue;
            };
            let is_placeholder =
                is_masked_or_placeholder_secret(s) || (allow_empty_backfill && s.is_empty());
            if is_placeholder
                && old_creds
                    .get(k)
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|old_s| !old_s.trim().is_empty())
            {
                return true;
            }
        }
    }
    if let (Some(new_key), Some(old_key)) = (
        new_cfg
            .get("tls")
            .and_then(|t| t.get("client_key"))
            .and_then(serde_json::Value::as_str),
        old_cfg
            .get("tls")
            .and_then(|t| t.get("client_key"))
            .and_then(serde_json::Value::as_str),
    ) && is_masked_or_placeholder_secret(new_key)
        && !old_key.trim().is_empty()
    {
        return true;
    }
    false
}

fn extract_supplier_auth_surface(cfg: &serde_json::Value) -> (Option<&str>, String) {
    let Some(auth) = cfg.get("auth_flow").and_then(serde_json::Value::as_object) else {
        return (None, "none".to_string());
    };
    let enabled = auth
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !enabled {
        return (None, "none".to_string());
    }
    let profile = auth
        .get("profile")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("none")
        .to_ascii_lowercase();
    let token_url = auth
        .get("token_url")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    (token_url, profile)
}

/// 供应商更新时的源站绑定守卫（Origin Pinning）：
/// 当 `(upstream_origin, token_origin, auth_profile)` 任一发生变化时，拒绝沿用掩码或省略 `config` 回填存量加密凭据。
///
/// # Errors
///
/// 源地址或鉴权协议变更但未重新提供完整凭据时返回 `400 Bad Request`。
pub fn guard_supplier_origin_pinning(
    new_upstream_url: &str,
    new_cfg: Option<&serde_json::Value>,
    old_upstream_url: &str,
    old_cfg: &serde_json::Value,
) -> Result<(), ApiError> {
    let upstream_changed =
        !same_canonical_url_target(Some(new_upstream_url), Some(old_upstream_url));
    match new_cfg {
        None => {
            if upstream_changed && supplier_config_has_stored_secrets(old_cfg) {
                return Err(ApiError::validation(
                    "ORIGIN_PINNING_VIOLATION: 上游源站变更时禁止直接沿用历史加密凭据，请同步提交完整凭据配置"
                        .to_string(),
                ));
            }
        }
        Some(cfg) => {
            let (new_token_url, new_profile) = extract_supplier_auth_surface(cfg);
            let (old_token_url, old_profile) = extract_supplier_auth_surface(old_cfg);
            let token_changed = !same_canonical_url_target(new_token_url, old_token_url);
            let profile_changed = new_profile != old_profile;
            if (upstream_changed || token_changed || profile_changed)
                && supplier_config_relies_on_backfill(cfg, old_cfg, false)
            {
                return Err(ApiError::validation(
                    "ORIGIN_PINNING_VIOLATION: 上游源站、换票地址或鉴权协议变更时禁止沿用掩码凭据，请重新输入完整密钥"
                        .to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn endpoints_have_stored_tls_keys(endpoints: &serde_json::Value) -> bool {
    endpoints.as_array().is_some_and(|arr| {
        arr.iter().any(|ep| {
            ep.get("tls")
                .and_then(|t| t.get("client_key"))
                .and_then(serde_json::Value::as_str)
                .is_some_and(|s| !s.trim().is_empty())
        })
    })
}

fn endpoints_rely_on_tls_backfill(
    new_eps: &serde_json::Value,
    old_eps: &serde_json::Value,
) -> bool {
    let Some(arr) = new_eps.as_array() else {
        return false;
    };
    for (idx, ep) in arr.iter().enumerate() {
        let ep_id = ep.get("id").and_then(serde_json::Value::as_str);
        let Some(new_key) = ep
            .get("tls")
            .and_then(|t| t.get("client_key"))
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        if !is_masked_or_placeholder_secret(new_key) {
            continue;
        }
        let old_has_key = find_existing_endpoint_tls(Some(old_eps), ep_id, idx)
            .and_then(|t| t.get("client_key"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|s| !s.trim().is_empty());
        if old_has_key {
            return true;
        }
    }
    false
}

fn endpoint_origin_changed(
    new_eps: &serde_json::Value,
    old_eps: &serde_json::Value,
    new_fallback_url: &str,
    old_fallback_url: &str,
) -> bool {
    let Some(new_arr) = new_eps.as_array() else {
        return false;
    };
    let Some(old_arr) = old_eps.as_array() else {
        return !new_arr.is_empty()
            && !same_canonical_url_target(Some(new_fallback_url), Some(old_fallback_url));
    };
    for (idx, ep) in new_arr.iter().enumerate() {
        let ep_id = ep.get("id").and_then(serde_json::Value::as_str);
        let old_ep = ep_id
            .filter(|s| !s.is_empty())
            .and_then(|id| {
                old_arr
                    .iter()
                    .find(|e| e.get("id").and_then(serde_json::Value::as_str) == Some(id))
            })
            .or_else(|| old_arr.get(idx));
        let new_base = ep
            .get("base_url")
            .or_else(|| ep.get("upstream_url"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(new_fallback_url);
        let Some(old_ep) = old_ep else {
            if !same_canonical_url_target(Some(new_base), Some(old_fallback_url)) {
                return true;
            }
            continue;
        };
        let old_base = old_ep
            .get("base_url")
            .or_else(|| old_ep.get("upstream_url"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(old_fallback_url);
        if !same_canonical_url_target(Some(new_base), Some(old_base)) {
            return true;
        }
        let new_ep_token = ep
            .get("auth")
            .and_then(|a| a.get("token_url"))
            .and_then(serde_json::Value::as_str);
        let old_ep_token = old_ep
            .get("auth")
            .and_then(|a| a.get("token_url"))
            .and_then(serde_json::Value::as_str);
        if !same_canonical_url_target(new_ep_token, old_ep_token) {
            return true;
        }
        let new_ep_prof = ep
            .get("auth")
            .and_then(|a| a.get("profile"))
            .and_then(serde_json::Value::as_str)
            .map_or("", str::trim);
        let old_ep_prof = old_ep
            .get("auth")
            .and_then(|a| a.get("profile"))
            .and_then(serde_json::Value::as_str)
            .map_or("", str::trim);
        if !new_ep_prof.eq_ignore_ascii_case(old_ep_prof) {
            return true;
        }
    }
    false
}

/// 工作台草稿更新与试跑快照物化的 Origin Pinning 守卫。
///
/// # Errors
///
/// `(upstream_origin, token_origin, endpoint_origins, auth_profile)` 发生变更且依赖掩码/空槽回填存量密钥时返回 `400 Bad Request`。
pub fn guard_draft_origin_pinning(
    new_upstream_url: Option<&str>,
    new_cfg: Option<&serde_json::Value>,
    new_endpoints: Option<&serde_json::Value>,
    old_upstream_url: &str,
    old_cfg: &serde_json::Value,
    old_endpoints: &serde_json::Value,
) -> Result<(), ApiError> {
    let eff_new_url = new_upstream_url.unwrap_or(old_upstream_url);
    let upstream_changed = !same_canonical_url_target(Some(eff_new_url), Some(old_upstream_url));
    let (token_changed, profile_changed) = if let Some(cfg) = new_cfg {
        let (nt, np) = extract_supplier_auth_surface(cfg);
        let (ot, op) = extract_supplier_auth_surface(old_cfg);
        (!same_canonical_url_target(nt, ot), np != op)
    } else {
        (false, false)
    };
    let ep_changed = if let Some(eps) = new_endpoints {
        endpoint_origin_changed(eps, old_endpoints, eff_new_url, old_upstream_url)
    } else {
        false
    };
    if !(upstream_changed || token_changed || profile_changed || ep_changed) {
        return Ok(());
    }

    let cfg_backfill = match new_cfg {
        Some(cfg) => supplier_config_relies_on_backfill(cfg, old_cfg, true),
        None => supplier_config_has_stored_secrets(old_cfg),
    };
    let ep_backfill = match new_endpoints {
        Some(eps) => endpoints_rely_on_tls_backfill(eps, old_endpoints),
        None => endpoints_have_stored_tls_keys(old_endpoints),
    };

    if cfg_backfill || ep_backfill {
        return Err(ApiError::validation(
            "ORIGIN_PINNING_VIOLATION: 工作台源站地址、端点源站或鉴权协议变更时禁止回填历史掩码凭据，请重新输入完整密钥"
                .to_string(),
        ));
    }
    Ok(())
}

/// 通道更新时的源站绑定守卫：当 `egress.base_url` 或 `egress.auth.token_url` 源变更时禁止沿用掩码 `tls.client_key`。
///
/// # Errors
///
/// 源地址变更但沿用掩码私钥时返回 `400 Bad Request`。
pub fn guard_channel_origin_pinning(
    new_cfg: &serde_json::Value,
    old_cfg: &serde_json::Value,
) -> Result<(), ApiError> {
    let Some(new_egress) = new_cfg.get("egress") else {
        return Ok(());
    };
    let Some(old_egress) = old_cfg.get("egress") else {
        return Ok(());
    };
    let base_changed = !same_canonical_url_target(
        new_egress.get("base_url").and_then(serde_json::Value::as_str),
        old_egress.get("base_url").and_then(serde_json::Value::as_str),
    );
    let token_changed = !same_canonical_url_target(
        new_egress
            .get("auth")
            .and_then(|a| a.get("token_url"))
            .and_then(serde_json::Value::as_str),
        old_egress
            .get("auth")
            .and_then(|a| a.get("token_url"))
            .and_then(serde_json::Value::as_str),
    );
    let tls_masked_backfill = new_egress
        .get("tls")
        .and_then(|t| t.get("client_key"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(is_masked_or_placeholder_secret)
        && old_egress
            .get("tls")
            .and_then(|t| t.get("client_key"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|s| !s.trim().is_empty());
    if (base_changed || token_changed) && tls_masked_backfill {
        return Err(ApiError::validation(
            "ORIGIN_PINNING_VIOLATION: 通道出站地址或换票地址变更时禁止沿用掩码 TLS 私钥，请重新输入完整私钥"
                .to_string(),
        ));
    }
    Ok(())
}

/// 检查供应商配置或渠道端点中是否包含高特权 TLS 项（mTLS 客户端私钥或 `insecure_skip_verify`）。
#[must_use]
pub fn has_privileged_tls_material(
    supplier_cfg: &serde_json::Value,
    endpoints: &serde_json::Value,
) -> bool {
    let sup_priv = supplier_cfg.get("tls").is_some_and(|t| {
        t.get("insecure_skip_verify")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
            || t.get("client_key")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|s| !s.trim().is_empty())
    });
    if sup_priv {
        return true;
    }
    endpoints.as_array().is_some_and(|arr| {
        arr.iter().any(|ep| {
            ep.get("tls").is_some_and(|t| {
                t.get("insecure_skip_verify")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
                    || t.get("client_key")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|s| !s.trim().is_empty())
            })
        })
    })
}

/// 守卫端点列表中的 `insecure_skip_verify`（用于回滚快照校验）。
///
/// # Errors
///
/// 非超级管理员尝试回滚启用 `insecure_skip_verify` 的端点快照时返回 `403 Forbidden`。
pub fn guard_endpoints_tls_privilege(
    endpoints: &serde_json::Value,
    is_super_admin: bool,
) -> Result<(), ApiError> {
    if is_super_admin {
        return Ok(());
    }
    let has_insecure = endpoints.as_array().is_some_and(|arr| {
        arr.iter().any(|ep| {
            ep.get("tls")
                .and_then(|t| t.get("insecure_skip_verify"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
    });
    if has_insecure {
        return Err(ApiError::Forbidden);
    }
    Ok(())
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

    #[test]
    fn canonical_origin_normalizes_default_ports_and_case() {
        assert_eq!(
            canonical_origin("HTTPS://Api.Example.COM/v1/query"),
            Some(("https".to_string(), "api.example.com".to_string(), 443))
        );
        assert_eq!(
            canonical_origin("https://api.example.com:443/v2/other"),
            Some(("https".to_string(), "api.example.com".to_string(), 443))
        );
        assert_eq!(
            canonical_origin("http://api.example.com:8080/v1"),
            Some(("http".to_string(), "api.example.com".to_string(), 8080))
        );
    }

    #[test]
    fn supplier_origin_pinning_blocks_masked_backfill_on_origin_or_token_change() {
        let old_cfg = serde_json::json!({
            "auth_flow": {
                "enabled": true,
                "profile": "oauth2_client_credentials_form",
                "token_url": "https://idp.trusted.example/oauth/token",
                "credentials": { "client_secret": "enc:v3:1:aes-256-gcm:abc" }
            }
        });
        let masked_same_token = serde_json::json!({
            "auth_flow": {
                "enabled": true,
                "profile": "oauth2_client_credentials_form",
                "token_url": "https://idp.trusted.example/oauth/v2/token",
                "credentials": { "client_secret": "••••••••" }
            }
        });
        // 仅路径变化、同源允许掩码回填
        assert!(
            guard_supplier_origin_pinning(
                "https://api.trusted.example:443/v2",
                Some(&masked_same_token),
                "https://api.trusted.example/v1",
                &old_cfg,
            )
            .is_ok()
        );
        // 上游主机变更且仍使用掩码回填：拒绝
        assert!(
            guard_supplier_origin_pinning(
                "https://attacker.example/v1",
                Some(&masked_same_token),
                "https://api.trusted.example/v1",
                &old_cfg,
            )
            .is_err()
        );
        // 省略 config 且更改上游源站：拒绝
        assert!(
            guard_supplier_origin_pinning(
                "https://attacker.example/v1",
                None,
                "https://api.trusted.example/v1",
                &old_cfg,
            )
            .is_err()
        );
        // token_url 跨源变更且仍使用掩码回填：拒绝
        let masked_evil_token = serde_json::json!({
            "auth_flow": {
                "enabled": true,
                "profile": "oauth2_client_credentials_form",
                "token_url": "https://evil-idp.example/oauth/token",
                "credentials": { "client_secret": "••••••••" }
            }
        });
        assert!(
            guard_supplier_origin_pinning(
                "https://api.trusted.example/v1",
                Some(&masked_evil_token),
                "https://api.trusted.example/v1",
                &old_cfg,
            )
            .is_err()
        );
    }

    #[test]
    fn draft_origin_pinning_blocks_empty_backfill_when_endpoint_origin_changes() {
        let old_cfg = serde_json::json!({
            "auth_flow": {
                "enabled": true,
                "profile": "oauth2_client_credentials_form",
                "token_url": "https://idp.trusted.example/token",
                "credentials": { "client_secret": "enc:v3:1:aes-256-gcm:abc" }
            }
        });
        let old_eps = serde_json::json!([
            { "id": "ep1", "base_url": "https://api.trusted.example/v1" }
        ]);
        let new_cfg_empty_secret = serde_json::json!({
            "auth_flow": {
                "enabled": true,
                "profile": "oauth2_client_credentials_form",
                "token_url": "https://idp.trusted.example/token",
                "credentials": { "client_secret": "" }
            }
        });
        let new_eps_hijacked = serde_json::json!([
            { "id": "ep1", "base_url": "https://evil.example/capture" }
        ]);
        assert!(
            guard_draft_origin_pinning(
                Some("https://api.trusted.example/v1"),
                Some(&new_cfg_empty_secret),
                Some(&new_eps_hijacked),
                "https://api.trusted.example/v1",
                &old_cfg,
                &old_eps,
            )
            .is_err()
        );
    }

    #[test]
    fn reject_client_enc_and_detect_privileged_tls_material() {
        let injected = serde_json::json!({
            "auth_flow": {
                "credentials": { "api_key": "enc:v3:1:aes-256-gcm:stolen" }
            }
        });
        assert!(reject_client_enc_in_supplier_config(&injected).is_err());

        let sup_with_mtls = serde_json::json!({
            "tls": { "client_key": "enc:v3:1:aes-256-gcm:key" }
        });
        assert!(has_privileged_tls_material(
            &sup_with_mtls,
            &serde_json::json!([])
        ));

        let ep_insecure = serde_json::json!([
            { "id": "ep1", "tls": { "insecure_skip_verify": true } }
        ]);
        assert!(has_privileged_tls_material(
            &serde_json::json!({}),
            &ep_insecure
        ));
        assert!(!has_privileged_tls_material(
            &serde_json::json!({}),
            &serde_json::json!([])
        ));
    }
}
