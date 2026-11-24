use crate::http_call::{HttpCall, append_query, join_url};
use crate::types::{BodyKind, ExecuteError};
use mazbri_core::BodyEncoding;

use super::crypto_wire::{apply_parts, maybe_open_json_body, profile_with_slots, seal_body};
use super::{
    AdapterCtx, AdapterOutcome, apply_ok_and_unwrap, can_passthrough_raw_json, parse_json_body,
    try_take_valid_raw_json, wrap_by_name,
};

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &*ctx.input.spec;
    let skip_validate = ctx.input.spec_validated;
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
        let effective_profile = profile_with_slots(profile, false);
        let sealed = seal_body(
            &raw,
            &effective_profile,
            ctx.pipeline_keys,
            ctx.registry,
            &[],
            skip_validate,
        )?;
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
    let mut response = super::dispatch_http(
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
    if ctx.is_reject_status(response.status) {
        return Ok(AdapterOutcome::raw(response));
    }

    if spec.crypto.is_none()
        && can_passthrough_raw_json(ctx, spec)
        && let Some(raw_json) =
            try_take_valid_raw_json(&mut response, ctx.needs_raw_body_for_invalidation())
    {
        return Ok(AdapterOutcome {
            auth_rejection: None,
            response,
            payload: serde_json::Value::Null,
            raw_payload_json: Some(raw_json),
            biz_error: None,
        });
    }

    let decrypted;
    let body_slice: &[u8] = if let Some(profile) = spec.crypto.as_ref() {
        decrypted = maybe_open_json_body(
            &response.body,
            profile,
            ctx.pipeline_keys,
            ctx.registry,
            &[],
            skip_validate,
        );
        &decrypted
    } else {
        &response.body
    };
    let json = parse_json_body(body_slice).or_else(|_| parse_json_body(&response.body))?;
    let (payload, biz_error) = apply_ok_and_unwrap(&json, spec)?;
    Ok(AdapterOutcome {
        auth_rejection: None,
        response,
        payload,
        raw_payload_json: None,
        biz_error,
    })
}
