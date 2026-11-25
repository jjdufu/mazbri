//! 适配器侧 `egress.crypto` → `crypto_pipeline` 共用接线（禁止在各 adapter 复制算法）。

use std::borrow::Cow;
use std::collections::HashMap;

use mazbri_crypto::{
    AlgorithmRegistry, CanonStyle, CryptoOrder, SlotLocation, UpstreamCryptoProfile,
};
use serde_json::Value;

use crate::crypto_pipeline::{
    PipelineContext, PipelineError, PipelineKeys, RequestParts, default_slots_json_cipher_sign_iv,
    default_slots_sign_form, open_response_with_validation, parts_from_response,
    seal_request_with_validation,
};
use crate::types::{BodyKind, ExecuteError};

pub(crate) fn map_pipeline_err(err: &PipelineError) -> ExecuteError {
    ExecuteError::AdapterFailed(format!("crypto pipeline: {err}"))
}

/// 当槽位非空时直接借用 `&UpstreamCryptoProfile`，仅在缺默认槽位时按需克隆补齐。
pub(crate) fn profile_with_slots(
    profile: &UpstreamCryptoProfile,
    form_style: bool,
) -> Cow<'_, UpstreamCryptoProfile> {
    if !profile.slots.is_empty() {
        return Cow::Borrowed(profile);
    }
    let mut owned = profile.clone();
    ensure_profile_slots(&mut owned, form_style);
    Cow::Owned(owned)
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
    skip_validate: bool,
) -> Result<RequestParts, ExecuteError> {
    let derived;
    let fields: &[(String, String)] = if canon_fields.is_empty() {
        derived = derive_canon_fields_from_body(body, profile)?;
        derived.as_slice()
    } else {
        canon_fields
    };
    let ctx = PipelineContext {
        registry,
        canon_fields: fields,
        canon_sep: "&",
        aad: &[],
        field_iv: None,
    };
    seal_request_with_validation(body, profile, keys, &ctx, skip_validate)
        .map_err(|e| map_pipeline_err(&e))
}

/// `sorted_kv` / `concat` 且调用方未给拼串字段时，从 JSON 对象扁平字段推导（跳过 sign 槽位名）。
fn derive_canon_fields_from_body(
    body: &[u8],
    profile: &UpstreamCryptoProfile,
) -> Result<Vec<(String, String)>, ExecuteError> {
    match profile.canon {
        CanonStyle::SortedKv | CanonStyle::Concat => {}
        _ => return Ok(Vec::new()),
    }
    let value: Value = serde_json::from_slice(body).map_err(|e| {
        ExecuteError::AdapterFailed(format!("crypto canon from json body: {e}"))
    })?;
    let obj = value.as_object().ok_or_else(|| {
        ExecuteError::AdapterFailed("crypto sorted_kv/concat 需要 JSON 对象 body".into())
    })?;
    let skip: std::collections::HashSet<&str> = profile
        .slots
        .iter()
        .filter(|s| s.role == crate::slots::role::SIGN)
        .map(|s| s.name.as_str())
        .collect();
    let mut out = Vec::new();
    for (k, v) in obj {
        if skip.contains(k.as_str()) || k == "sign" {
            continue;
        }
        match v {
            Value::Null => {}
            Value::String(s) => out.push((k.clone(), s.clone())),
            Value::Number(n) => out.push((k.clone(), n.to_string())),
            Value::Bool(b) => out.push((k.clone(), if *b { "true" } else { "false" }.into())),
            Value::Object(_) | Value::Array(_) => {
                return Err(ExecuteError::AdapterFailed(
                    "crypto canon 暂不支持嵌套 object/array 字段".into(),
                ));
            }
        }
    }
    Ok(out)
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
        // 若 body 已是 JSON 对象（明文业务 / named_fields），把槽位字段合并进去，避免只剩 sign。
        if let Ok(Value::Object(mut map)) = serde_json::from_slice::<Value>(&parts.body) {
            for (k, v) in &parts.slots.json {
                map.insert(k.clone(), v.clone());
            }
            let raw = serde_json::to_string(&map)
                .map_err(|e| ExecuteError::AdapterFailed(format!("crypto json merge: {e}")))?;
            return Ok(Some(BodyKind::Json(raw)));
        }
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

/// 根据 `verify_response` / `should_decrypt_response()` 裁剪响应侧生效的 `UpstreamCryptoProfile`。
fn profile_for_response_open(profile: &UpstreamCryptoProfile) -> Cow<'_, UpstreamCryptoProfile> {
    let want_verify = profile.verify_response;
    let want_decrypt = profile.should_decrypt_response();
    let effective = profile_with_slots(profile, false);
    let order = match (want_verify, want_decrypt, effective.order) {
        (true, false, _) => CryptoOrder::SignOnly,
        (false, true, CryptoOrder::AeadOnly) => CryptoOrder::AeadOnly,
        (false, true, _) => {
            let is_aead = effective
                .response_crypt
                .as_ref()
                .or(effective.crypt.as_ref())
                .is_some_and(|c| c.alg.kind() == mazbri_crypto::AlgorithmKind::Aead);
            if is_aead {
                CryptoOrder::AeadOnly
            } else {
                CryptoOrder::EncryptOnly
            }
        }
        (true, true, existing) | (false, false, existing) => existing,
    };
    if order == effective.order {
        return effective;
    }
    let mut owned = effective.into_owned();
    owned.order = order;
    if !want_decrypt {
        owned.crypt = None;
        owned.response_crypt = None;
    }
    if !want_verify {
        owned.sign = None;
    }
    Cow::Owned(owned)
}

/// 严格解封响应体（Fail-Closed）：
/// - 若 `profile.should_open_response() == false`（单向仅保护请求），直接原样返回响应字节；
/// - 若启用了 `verify_response` 或响应解密，则把响应 `headers` 与 `body` 一并装填进 `SlotBags`
///   并执行 `open_response`，任何缺槽位、验签失败或解密失败一律返回 `Err(ExecuteError)`，
///   严禁静默回退明文 JSON。
pub(crate) fn open_response_body_strict(
    body: &[u8],
    headers: &HashMap<String, String>,
    profile: &UpstreamCryptoProfile,
    keys: &PipelineKeys,
    registry: &AlgorithmRegistry,
    canon_fields: &[(String, String)],
    skip_validate: bool,
) -> Result<Vec<u8>, ExecuteError> {
    if !profile.should_open_response() {
        return Ok(body.to_vec());
    }
    let open_profile = profile_for_response_open(profile);
    let derived;
    let fields: &[(String, String)] = if canon_fields.is_empty() {
        if matches!(open_profile.canon, CanonStyle::SortedKv | CanonStyle::Concat) {
            derived = derive_canon_fields_from_body(body, &open_profile)?;
            derived.as_slice()
        } else {
            &[]
        }
    } else {
        canon_fields
    };
    let parts = parts_from_response(body, headers, &open_profile);
    let ctx = PipelineContext {
        registry,
        canon_fields: fields,
        canon_sep: "&",
        aad: &[],
        field_iv: None,
    };
    open_response_with_validation(&parts, &open_profile, keys, &ctx, skip_validate)
        .map_err(|e| map_pipeline_err(&e))
}

/// 严格响应对象验签（Fail-Closed）：
/// 仅在 `profile.verify_response == true` 时启用；缺签名或验签不通过一律返回 `Err(ExecuteError)`。
pub(crate) fn verify_signed_object_strict(
    obj: &Value,
    headers: &HashMap<String, String>,
    profile: &UpstreamCryptoProfile,
    keys: &PipelineKeys,
    registry: &AlgorithmRegistry,
) -> Result<(), ExecuteError> {
    if !profile.verify_response || profile.sign.is_none() {
        return Ok(());
    }
    let map = obj.as_object().ok_or_else(|| {
        ExecuteError::AdapterFailed("signed response payload must be a JSON object".into())
    })?;
    let sign_spec = profile
        .slots
        .iter()
        .find(|s| s.role == crate::slots::role::SIGN);
    let (sign_location, sign_name) = sign_spec.map_or(
        (SlotLocation::Json, "sign"),
        |s| (s.location, s.name.as_str()),
    );
    let sig = match sign_location {
        SlotLocation::Header => headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(sign_name))
            .map(|(_, v)| v.clone())
            .or_else(|| map.get(sign_name).and_then(Value::as_str).map(str::to_owned)),
        _ => map.get(sign_name).and_then(Value::as_str).map(str::to_owned),
    }
    .filter(|s| !s.is_empty())
    .ok_or_else(|| {
        ExecuteError::AdapterFailed(format!("missing response signature slot '{sign_name}'"))
    })?;

    let mut fields: Vec<(String, String)> = Vec::new();
    for (k, v) in map {
        if sign_location == SlotLocation::Json && k == sign_name {
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
    verify_profile.response_crypt = None;
    verify_profile.slots = vec![mazbri_crypto::SlotSpec {
        role: crate::slots::role::SIGN.into(),
        location: SlotLocation::Json,
        name: sign_name.into(),
    }];

    let mut parts = RequestParts::blank();
    let _ = parts.slots.write(SlotLocation::Json, sign_name, sig);
    parts.body = Vec::new();

    let ctx = PipelineContext {
        registry,
        canon_fields: &fields,
        canon_sep: "&",
        aad: &[],
        field_iv: None,
    };
    open_response_with_validation(&parts, &verify_profile, keys, &ctx, true)
        .map_err(|e| map_pipeline_err(&e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_crypto::{
        AlgorithmId, CanonKeyAffix, CipherScope, CryptSpec, IvSource, KeyPurpose, KeyRef, SignSpec,
        SlotSpec, WireEncodings,
    };

    #[test]
    fn request_only_profile_leaves_plaintext_response_untouched() {
        let registry = AlgorithmRegistry::default_registry();
        let profile = UpstreamCryptoProfile {
            version: 1,
            order: CryptoOrder::AeadOnly,
            scope: CipherScope::WholeBody,
            crypt: Some(CryptSpec {
                alg: AlgorithmId::Aes256Gcm,
                fields: Vec::new(),
                encoding: None,
            }),
            response_crypt: None,
            verify_response: false,
            decrypt_response: false,
            sign: None,
            wrap: None,
            canon: CanonStyle::Raw,
            canon_key_affix: CanonKeyAffix::None,
            iv: IvSource::Random,
            encodings: WireEncodings::base64_all(),
            slots: Vec::new(),
            allow_insecure: false,
        };
        let keys = PipelineKeys {
            data: Some(KeyRef::raw(KeyPurpose::DataEncryption, vec![0x42; 32])),
            ..PipelineKeys::empty()
        };
        let raw = br#"{"payload":"plain-upstream-field","code":0}"#;
        let out = open_response_body_strict(
            raw,
            &HashMap::new(),
            &profile,
            &keys,
            &registry,
            &[],
            false,
        )
        .expect("passthrough when response crypto disabled");
        assert_eq!(out, raw);
    }

    #[test]
    fn bidirectional_profile_fails_closed_on_forged_plaintext_and_verifies_header_sig() {
        let registry = AlgorithmRegistry::default_registry();
        let profile = UpstreamCryptoProfile {
            version: 1,
            order: CryptoOrder::SignOnly,
            scope: CipherScope::WholeBody,
            crypt: None,
            response_crypt: None,
            verify_response: true,
            decrypt_response: false,
            sign: Some(SignSpec {
                alg: AlgorithmId::HmacSha256,
                encoding: None,
            }),
            wrap: None,
            canon: CanonStyle::Raw,
            canon_key_affix: CanonKeyAffix::None,
            iv: IvSource::Random,
            encodings: WireEncodings::base64_all(),
            slots: vec![SlotSpec {
                role: crate::slots::role::SIGN.into(),
                location: SlotLocation::Header,
                name: "X-Sign".into(),
            }],
            allow_insecure: false,
        };
        let keys = PipelineKeys {
            mac: Some(KeyRef::raw(KeyPurpose::Mac, b"shared-secret".to_vec())),
            ..PipelineKeys::empty()
        };
        let body = br#"{"status":"ok"}"#;
        // Forged plaintext response without X-Sign header must fail closed.
        assert!(
            open_response_body_strict(
                body,
                &HashMap::new(),
                &profile,
                &keys,
                &registry,
                &[],
                false,
            )
            .is_err()
        );

        let sealed = seal_body(body, &profile, &keys, &registry, &[], false).expect("seal");
        let mut resp_headers = HashMap::new();
        resp_headers.insert(
            "x-sign".to_owned(),
            sealed.slots.headers["X-Sign"].clone(),
        );
        let verified = open_response_body_strict(
            body,
            &resp_headers,
            &profile,
            &keys,
            &registry,
            &[],
            false,
        )
        .expect("valid header signature");
        assert_eq!(verified, body);
    }
}
