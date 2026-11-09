use std::collections::BTreeMap;

use crate::auth::credential;
use crate::funcs::{nonce, now_unix};
use crate::http_call::{HttpCall, append_query, join_url};
use crate::types::{BizError, BodyKind, ExecuteError, OutboundResponse};
use mazbri_core::ChannelEgressSpec;
use mazbri_crypto::UpstreamCryptoProfile;
use serde_json::Value;

use super::crypto_wire::{
    encode_merged_form, ensure_profile_slots, prefer_sorted_kv_for_sign_form, seal_body,
    soft_verify_signed_object,
};
use super::{AdapterCtx, AdapterOutcome, parse_json_body};

fn resolve_sign_type(spec: &ChannelEgressSpec, profile: &UpstreamCryptoProfile) -> String {
    spec.sign_form
        .as_ref()
        .and_then(|s| s.sign_type.as_deref())
        .map_or_else(
            || {
                profile
                    .sign
                    .as_ref()
                    .map(|s| s.alg.as_str().to_string())
                    .unwrap_or_default()
            },
            str::to_string,
        )
}

fn build_meta(
    app_id: &str,
    operation: &str,
    charset: &str,
    version: &str,
    sign_type: String,
) -> BTreeMap<String, String> {
    let mut meta = BTreeMap::new();
    meta.insert("app_id".into(), app_id.to_string());
    meta.insert("method".into(), operation.to_string());
    meta.insert("charset".into(), charset.to_string());
    meta.insert("version".into(), version.to_string());
    meta.insert("timestamp".into(), now_unix().to_string());
    meta.insert("nonce".into(), nonce(24));
    meta.insert("sign_type".into(), sign_type);
    meta
}

async fn post_form(
    ctx: &AdapterCtx<'_>,
    form: String,
    sealed_headers: &[(String, String)],
    sealed_query: &[(String, String)],
) -> Result<OutboundResponse, ExecuteError> {
    let spec = &ctx.input.spec;
    let mut query = super::merge_request_query(ctx.auth, spec);
    let mut headers = super::merge_request_headers(ctx.auth, spec);
    for (k, v) in sealed_headers {
        headers.insert(k.clone(), v.clone());
    }
    for (k, v) in sealed_query {
        query.push((k.clone(), v.clone()));
    }
    super::force_urlencoded_content_type(&mut headers);
    let url = append_query(&join_url(ctx.base_url, spec.request.path.as_str()), &query);
    super::dispatch_http(
        ctx,
        HttpCall {
            method: spec.request.method.as_str(),
            url: &url,
            headers,
            body: Some(BodyKind::Form(form)),
            timeout: ctx.timeout,
        },
    )
    .await
}

fn outcome_from_json(
    json: Value,
    response: OutboundResponse,
    profile: &UpstreamCryptoProfile,
    ctx: &AdapterCtx<'_>,
) -> AdapterOutcome {
    if let Some(err) = json.get("error_response") {
        soft_verify_signed_object(err, profile, ctx.pipeline_keys, ctx.registry);
        let message = err
            .get("sub_msg")
            .or_else(|| err.get("msg"))
            .and_then(|v| v.as_str())
            .unwrap_or("sign_form error")
            .to_string();
        return AdapterOutcome {
            payload: Value::Null,
            biz_error: Some(BizError {
                code: "UPSTREAM_BIZ".into(),
                message,
                details: Some(err.clone()),
            }),
            response,
        };
    }
    let inner = json
        .as_object()
        .and_then(|obj| {
            obj.iter()
                .find(|(k, v)| k.ends_with("_response") && v.is_object())
                .map(|(_, v)| v.clone())
        })
        .unwrap_or(json);
    soft_verify_signed_object(&inner, profile, ctx.pipeline_keys, ctx.registry);
    let payload = match ctx.input.spec.response.as_ref().and_then(|r| r.unwrap.as_deref()) {
        Some(path) => mazbri_core::json_path::extract_by_path(&inner, path).unwrap_or(inner),
        None => inner,
    };
    AdapterOutcome {
        response,
        payload,
        biz_error: None,
    }
}

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &ctx.input.spec;
    let profile = spec.crypto.as_ref().ok_or_else(|| {
        ExecuteError::SpecInvalid(
            "sign_form 需要 egress.crypto.sign（算法权威在 Profile，不再默认 HMAC-SHA256）".into(),
        )
    })?;
    if profile.sign.is_none() {
        return Err(ExecuteError::SpecInvalid(
            "sign_form 需要 egress.crypto.sign".into(),
        ));
    }
    if ctx.pipeline_keys.mac.is_none() && ctx.pipeline_keys.sign_private.is_none() {
        return Err(ExecuteError::AuthTokenFailed(
            "sign_form missing MAC/sign key（请经 pipeline_keys 或 credentials 注入）".into(),
        ));
    }

    let operation = spec
        .operation
        .as_deref()
        .ok_or_else(|| ExecuteError::SpecInvalid("sign_form operation missing".into()))?;
    let app_id = credential(&ctx.input.credentials, &["app_id", "client_id"])
        .ok_or_else(|| ExecuteError::AuthTokenFailed("sign_form missing app_id".into()))?;
    let charset = spec
        .sign_form
        .as_ref()
        .and_then(|s| s.charset.as_deref())
        .unwrap_or("UTF-8");
    let version = spec
        .sign_form
        .as_ref()
        .and_then(|s| s.version.as_deref())
        .unwrap_or("1.0");

    let biz = serde_json::to_string(&ctx.input.payload)
        .map_err(|e| ExecuteError::SpecInvalid(format!("biz_content: {e}")))?;
    let meta = build_meta(app_id, operation, charset, version, resolve_sign_type(spec, profile));

    let mut profile = profile.clone();
    // SignOnly：biz 进 canon；加密订单：明文仅作 seal 输入，密文由 pipeline 写入槽位
    ensure_profile_slots(&mut profile, true);
    prefer_sorted_kv_for_sign_form(&mut profile);
    let mut canon_map = meta.clone();
    canon_map.insert("biz_content".into(), biz.clone());
    let canon_fields: Vec<(String, String)> = canon_map.into_iter().collect();
    let sealed = seal_body(
        biz.as_bytes(),
        &profile,
        ctx.pipeline_keys,
        ctx.registry,
        &canon_fields,
    )?;
    let form = encode_merged_form(&meta, &sealed);
    let hdrs: Vec<_> = sealed.slots.headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let q: Vec<_> = sealed.slots.query.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

    let response = post_form(ctx, form, &hdrs, &q).await?;
    let json = parse_json_body(&response.body)?;
    Ok(outcome_from_json(json, response, &profile, ctx))
}
