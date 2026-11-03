//! 适配器侧 `egress.crypto` → `crypto_pipeline` 共用接线（禁止在各 adapter 复制算法）。

use std::collections::HashMap;

use mazbri_crypto::{
    AlgorithmRegistry, CanonStyle, CryptoOrder, SlotLocation, UpstreamCryptoProfile,
};
use serde_json::Value;

use crate::crypto_pipeline::{
    default_slots_json_cipher_sign_iv, default_slots_sign_form, open_response,
    parts_from_json_object, seal_request, PipelineContext, PipelineError, PipelineKeys,
    RequestParts,
};
use crate::types::{BodyKind, ExecuteError};

pub(crate) fn map_pipeline_err(err: &PipelineError) -> ExecuteError {
    ExecuteError::AdapterFailed(format!("crypto pipeline: {err}"))
}

/// 缺槽位时按适配器惯例补齐（envelope/json → JSON `cipher`/`sign`/`iv`；`sign_form` → form）。
pub(crate) fn ensure_profile_slots(
    profile: &mut UpstreamCryptoProfile,
    form_style: bool,
) {
    if !profile.slots.is_empty() {
        return;
    }
    if form_style {
        profile.slots = default_slots_sign_form(profile.order);
        return;
    }
    match profile.order {
        CryptoOrder::SignOnly => {
            profile.slots = vec![
                mazbri_crypto::SlotSpec {
                    role: crate::slots::role::SIGN.into(),
                    location: SlotLocation::Header,
                    name: "X-Sign".into(),
                },
                mazbri_crypto::SlotSpec {
                    role: crate::slots::role::BIZ.into(),
                    location: SlotLocation::Json,
                    name: "biz".into(),
                },
            ];
        }
        CryptoOrder::EncryptOnly | CryptoOrder::AeadOnly => {
            profile.slots = default_slots_json_cipher_sign_iv()
                .into_iter()
                .filter(|s| s.role != crate::slots::role::SIGN)
                .collect();
        }
        CryptoOrder::EncryptThenSign | CryptoOrder::SignThenEncrypt => {
            profile.slots = default_slots_json_cipher_sign_iv();
        }
    }
}

/// `sign_form`：`SignOnly` 且 `canon=Raw` 时升为 `SortedKv`（表单拼串惯例）。
pub(crate) fn prefer_sorted_kv_for_sign_form(profile: &mut UpstreamCryptoProfile) {
    if matches!(profile.order, CryptoOrder::SignOnly) && profile.canon == CanonStyle::Raw {
        profile.canon = CanonStyle::SortedKv;
    }
}

pub(crate) fn seal_body(
    body: &[u8],
    profile: &UpstreamCryptoProfile,
    keys: &PipelineKeys,
    registry: &AlgorithmRegistry,
    canon_fields: &[(String, String)],
) -> Result<RequestParts, ExecuteError> {
    let ctx = PipelineContext {
        registry,
        canon_fields,
        canon_sep: "&",
        aad: &[],
        field_iv: None,
    };
    seal_request(body, profile, keys, &ctx).map_err(|e| map_pipeline_err(&e))
}

/// 将槽位合并进 headers/query，并得到 HTTP body。
pub(crate) fn apply_parts(
    headers: &mut HashMap<String, String>,
    query: &mut Vec<(String, String)>,
    parts: &RequestParts,
) -> Result<Option<BodyKind>, ExecuteError> {
    for (k, v) in &parts.slots.headers {
        headers.insert(k.clone(), v.clone());
    }
    for (k, v) in &parts.slots.query {
        query.push((k.clone(), v.clone()));
    }
    if !parts.slots.form.is_empty() {
        let encoded: String = parts
            .slots
            .form
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    crate::funcs::url_encode(k),
                    crate::funcs::url_encode(v)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        return Ok(Some(BodyKind::Form(encoded)));
    }
    if !parts.slots.json.is_empty() {
        let raw = serde_json::to_string(&parts.slots.json)
            .map_err(|e| ExecuteError::AdapterFailed(format!("crypto json body: {e}")))?;
        return Ok(Some(BodyKind::Json(raw)));
    }
    if parts.body.is_empty() {
        return Ok(None);
    }
    if let Ok(s) = String::from_utf8(parts.body.clone()) {
        let trimmed = s.trim_start();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            Ok(Some(BodyKind::Json(s)))
        } else {
            Ok(Some(BodyKind::Raw {
                body: s,
                content_type: "application/octet-stream".into(),
            }))
        }
    } else {
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&parts.body);
        Ok(Some(BodyKind::Raw {
            body: encoded,
            content_type: "application/octet-stream".into(),
        }))
    }
}

/// 合并 meta 表单字段与 pipeline form 槽位后编码。
pub(crate) fn encode_merged_form(
    meta: &std::collections::BTreeMap<String, String>,
    parts: &RequestParts,
) -> String {
    let mut form = meta.clone();
    for (k, v) in &parts.slots.form {
        form.insert(k.clone(), v.clone());
    }
    form.iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                crate::funcs::url_encode(k),
                crate::funcs::url_encode(v)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// 若响应 JSON 含 profile 声明的 cipher/sign 槽，则尝试 `open_response`；否则原样返回。
pub(crate) fn maybe_open_json_body(
    body: &[u8],
    profile: &UpstreamCryptoProfile,
    keys: &PipelineKeys,
    registry: &AlgorithmRegistry,
    canon_fields: &[(String, String)],
) -> Vec<u8> {
    let Ok(Value::Object(map)) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let interested: Vec<&str> = profile
        .slots
        .iter()
        .filter(|s| {
            s.location == SlotLocation::Json
                && matches!(
                    s.role.as_str(),
                    crate::slots::role::CIPHER
                        | crate::slots::role::SIGN
                        | crate::slots::role::IV
                        | crate::slots::role::BIZ
                )
        })
        .map(|s| s.name.as_str())
        .collect();
    if interested.is_empty() || !interested.iter().any(|n| map.contains_key(*n)) {
        return body.to_vec();
    }
    let parts = parts_from_json_object(&map, profile);
    let ctx = PipelineContext {
        registry,
        canon_fields,
        canon_sep: "&",
        aad: &[],
        field_iv: None,
    };
    match open_response(&parts, profile, keys, &ctx) {
        Ok(plain) => plain,
        Err(err) => {
            tracing::debug!(error = %err, "open_response skipped; using raw JSON body");
            body.to_vec()
        }
    }
}

/// 响应对象验签（软失败：仅 warn，与历史 `sign_form` 行为一致）。
pub(crate) fn soft_verify_signed_object(
    obj: &Value,
    profile: &UpstreamCryptoProfile,
    keys: &PipelineKeys,
    registry: &AlgorithmRegistry,
) {
    let Some(map) = obj.as_object() else {
        return;
    };
    if profile.sign.is_none() {
        return;
    }
    let sign_name = profile
        .slots
        .iter()
        .find(|s| s.role == crate::slots::role::SIGN)
        .map_or("sign", |s| s.name.as_str());
    let Some(Value::String(sig)) = map.get(sign_name) else {
        return;
    };
    let mut fields: Vec<(String, String)> = Vec::new();
    for (k, v) in map {
        if k == sign_name {
            continue;
        }
        let s = match v {
            Value::String(s) => s.clone(),
            Value::Null => continue,
            other => other.to_string(),
        };
        if s.is_empty() {
            continue;
        }
        fields.push((k.clone(), s));
    }
    fields.sort_by(|a, b| a.0.cmp(&b.0));

    let mut verify_profile = profile.clone();
    verify_profile.canon = CanonStyle::SortedKv;
    verify_profile.order = CryptoOrder::SignOnly;
    verify_profile.crypt = None;
    verify_profile.slots = vec![mazbri_crypto::SlotSpec {
        role: crate::slots::role::SIGN.into(),
        location: SlotLocation::Json,
        name: sign_name.into(),
    }];

    let mut parts = RequestParts::blank();
    let _ = parts.slots.write(SlotLocation::Json, sign_name, sig.clone());
    parts.body = Vec::new();

    let ctx = PipelineContext {
        registry,
        canon_fields: &fields,
        canon_sep: "&",
        aad: &[],
        field_iv: None,
    };
    if let Err(err) = open_response(&parts, &verify_profile, keys, &ctx) {
        tracing::warn!(error = %err, "response signature verify failed");
    }
}
