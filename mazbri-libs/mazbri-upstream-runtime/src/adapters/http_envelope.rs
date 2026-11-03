use crate::http_call::{HttpCall, append_query, join_url};
use crate::types::{BodyKind, ExecuteError};
use mazbri_core::BodyEncoding;

use super::crypto_wire::{apply_parts, ensure_profile_slots, maybe_open_json_body, seal_body};
use super::{AdapterCtx, AdapterOutcome, apply_ok_and_unwrap, parse_json_body, wrap_by_name};

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &ctx.input.spec;
    let method = spec.request.method.as_str();
    let path = spec.request.path.as_str();
    let mut query = super::merge_request_query(ctx.auth, spec);
    let mut headers = super::merge_request_headers(ctx.auth, spec);

    let body = if spec.request.body_encoding == BodyEncoding::None {
        None
    } else if let Some(profile) = spec.crypto.as_ref() {
        let wrapped = wrap_by_name(spec, &ctx.input.payload)?;
        let raw = if wrapped.is_null() {
            b"{}".to_vec()
        } else {
            serde_json::to_vec(&wrapped)
                .map_err(|e| ExecuteError::SpecInvalid(format!("wrap json: {e}")))?
        };
        let mut profile = profile.clone();
        ensure_profile_slots(&mut profile, false);
        let sealed = seal_body(&raw, &profile, ctx.pipeline_keys, ctx.registry, &[])?;
        let wire = apply_parts(&mut headers, &mut query, &sealed)?;
        if matches!(wire, Some(BodyKind::Form(_))) {
            super::force_urlencoded_content_type(&mut headers);
        } else {
            super::apply_json_content_type_override(&mut headers, spec);
        }
        wire
    } else {
        let wrapped = wrap_by_name(spec, &ctx.input.payload)?;
        let raw = if wrapped.is_null() {
            "{}".to_string()
        } else {
            serde_json::to_string(&wrapped)
                .map_err(|e| ExecuteError::SpecInvalid(format!("wrap json: {e}")))?
        };
        super::apply_json_content_type_override(&mut headers, spec);
        Some(BodyKind::Json(raw))
    };

    let url = append_query(&join_url(ctx.base_url, path), &query);
    let response = super::dispatch_http(
        ctx,
        HttpCall {
            method,
            url: &url,
            headers,
            body,
            timeout: ctx.timeout,
        },
    )
    .await?;
    if response.status == 401 {
        return Ok(AdapterOutcome {
            payload: serde_json::Value::Null,
            biz_error: None,
            response,
        });
    }

    let body_for_parse = if let Some(profile) = spec.crypto.as_ref() {
        maybe_open_json_body(
            &response.body,
            profile,
            ctx.pipeline_keys,
            ctx.registry,
            &[],
        )
    } else {
        response.body.clone()
    };
    let json = parse_json_body(&body_for_parse).or_else(|_| parse_json_body(&response.body))?;
    let (payload, biz_error) = apply_ok_and_unwrap(&json, spec)?;
    Ok(AdapterOutcome {
        response,
        payload,
        biz_error,
    })
}
