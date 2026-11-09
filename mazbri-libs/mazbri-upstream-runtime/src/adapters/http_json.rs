use crate::http_call::{HttpCall, append_query, join_url};
use crate::types::{BodyKind, ExecuteError};
use mazbri_core::BodyEncoding;
use serde_json::Value;

use super::crypto_wire::{apply_parts, ensure_profile_slots, maybe_open_json_body, seal_body};
use super::{AdapterCtx, AdapterOutcome, apply_ok_and_unwrap, parse_json_body};

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &ctx.input.spec;
    let method = spec.request.method.as_str();
    let path = spec.request.path.as_str();
    let mut query = super::merge_request_query(ctx.auth, spec);
    let encoding = spec.request.body_encoding;
    let mut headers = super::merge_request_headers(ctx.auth, spec);

    // GET/HEAD 无 body：把 JSON 对象标量字段摊进 query（通用，便于 posts/search 等）。
    if method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD") {
        append_payload_to_query(&mut query, &ctx.input.payload)?;
    }

    let body = if let Some(profile) = spec.crypto.as_ref() {
        if method.eq_ignore_ascii_case("GET")
            || method.eq_ignore_ascii_case("HEAD")
            || encoding == BodyEncoding::None
        {
            None
        } else {
            let raw = if ctx.input.payload.is_null() {
                b"{}".to_vec()
            } else {
                serde_json::to_vec(&ctx.input.payload)
                    .map_err(|e| ExecuteError::SpecInvalid(format!("payload json: {e}")))?
            };
            let mut profile = profile.clone();
            let form_style = encoding == BodyEncoding::Form;
            ensure_profile_slots(&mut profile, form_style);
            let sealed = seal_body(&raw, &profile, ctx.pipeline_keys, ctx.registry, &[])?;
            let mut wire = apply_parts(&mut headers, &mut query, &sealed)?;
            // named_fields 等场景 seal 后 body 仍是 JSON 对象：按请求编码压成 form。
            if form_style
                && let Some(BodyKind::Json(s)) = wire
            {
                let v: Value = serde_json::from_str(&s).map_err(|e| {
                    ExecuteError::AdapterFailed(format!("crypto form from json: {e}"))
                })?;
                wire = Some(BodyKind::Form(encode_form_object(&v)?));
            }
            if matches!(wire, Some(BodyKind::Form(_))) {
                super::force_urlencoded_content_type(&mut headers);
            } else {
                super::apply_json_content_type_override(&mut headers, spec);
            }
            wire
        }
    } else {
        let body = outbound_body(method, encoding, &ctx.input.payload)?;
        if body.is_some() {
            if encoding == BodyEncoding::Form {
                super::force_urlencoded_content_type(&mut headers);
            } else {
                super::apply_json_content_type_override(&mut headers, spec);
            }
        }
        body
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
    if ctx.is_reject_status(response.status) {
        return Ok(AdapterOutcome::raw(response));
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
    let json = parse_json_body(&body_for_parse).unwrap_or_else(|_| {
        serde_json::Value::String(String::from_utf8_lossy(&response.body).into_owned())
    });
    let (payload, biz_error) = apply_ok_and_unwrap(&json, spec)?;
    Ok(AdapterOutcome {
        auth_rejection: None,
        response,
        payload,
        biz_error,
    })
}

/// GET/HEAD never send a body. `none` encoding skips the body. JSON + Null payload → `{}`.
pub(crate) fn outbound_body(
    method: &str,
    encoding: BodyEncoding,
    payload: &Value,
) -> Result<Option<BodyKind>, ExecuteError> {
    if method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD") {
        return Ok(None);
    }
    match encoding {
        BodyEncoding::None => return Ok(None),
        BodyEncoding::Form => return Ok(Some(BodyKind::Form(encode_form_object(payload)?))),
        BodyEncoding::Json => {}
    }
    let raw = if payload.is_null() {
        "{}".to_string()
    } else {
        serde_json::to_string(payload)
            .map_err(|e| ExecuteError::SpecInvalid(format!("payload json: {e}")))?
    };
    Ok(Some(BodyKind::Json(raw)))
}


/// Flatten a JSON object of scalars into query pairs (skip null; reject nest).
fn append_payload_to_query(
    query: &mut Vec<(String, String)>,
    payload: &Value,
) -> Result<(), ExecuteError> {
    if payload.is_null() {
        return Ok(());
    }
    let obj = payload.as_object().ok_or_else(|| {
        ExecuteError::SpecInvalid("GET/HEAD payload 须为 JSON 对象（标量字段→query）".into())
    })?;
    let mut keys: Vec<&String> = obj.keys().collect();
    keys.sort();
    for k in keys {
        match &obj[k] {
            Value::Null => {}
            Value::Object(_) | Value::Array(_) => {
                return Err(ExecuteError::SpecInvalid(
                    "GET/HEAD query 扁平化不支持嵌套 object/array".into(),
                ));
            }
            Value::String(s) => query.push((k.clone(), s.clone())),
            Value::Number(n) => query.push((k.clone(), n.to_string())),
            Value::Bool(b) => query.push((k.clone(), if *b { "true" } else { "false" }.into())),
        }
    }
    Ok(())
}

/// Flat JSON object → `k=v&k=v` with sorted keys. Nested object/array is `SpecInvalid`.
pub(crate) fn encode_form_object(payload: &Value) -> Result<String, ExecuteError> {
    let obj = payload
        .as_object()
        .ok_or_else(|| ExecuteError::SpecInvalid("form body_encoding 仅支持 JSON 对象".into()))?;
    let mut keys: Vec<&String> = obj.keys().collect();
    keys.sort();
    let mut parts = Vec::with_capacity(keys.len());
    for k in keys {
        match &obj[k] {
            Value::Null => {}
            Value::Object(_) | Value::Array(_) => {
                return Err(ExecuteError::SpecInvalid(
                    "form body_encoding 第一期不支持嵌套 object/array".into(),
                ));
            }
            Value::String(s) => parts.push(form_pair(k, s)),
            Value::Number(n) => parts.push(form_pair(k, &n.to_string())),
            Value::Bool(b) => parts.push(form_pair(k, if *b { "true" } else { "false" })),
        }
    }
    Ok(parts.join("&"))
}

fn form_pair(k: &str, v: &str) -> String {
    format!(
        "{}={}",
        crate::funcs::url_encode(k),
        crate::funcs::url_encode(v)
    )
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn get_payload_flattens_to_query_helper() {
        let mut q = vec![("keep".into(), "1".into())];
        append_payload_to_query(&mut q, &serde_json::json!({"b": "x", "a": 2})).unwrap();
        assert_eq!(
            q,
            vec![
                ("keep".into(), "1".into()),
                ("a".into(), "2".into()),
                ("b".into(), "x".into()),
            ]
        );
    }

    #[test]
    fn get_never_has_body() {
        let body = outbound_body("GET", BodyEncoding::Json, &serde_json::json!({"a": 1})).unwrap();
        assert!(body.is_none());
    }

    #[test]
    fn post_none_has_no_body() {
        let body = outbound_body("POST", BodyEncoding::None, &Value::Null).unwrap();
        assert!(body.is_none());
    }

    #[test]
    fn post_json_null_payload_is_empty_object() {
        let body = outbound_body("POST", BodyEncoding::Json, &Value::Null).unwrap();
        match body {
            Some(BodyKind::Json(s)) => assert_eq!(s, "{}"),
            _ => panic!("expected json {{}}"),
        }
    }

    #[test]
    fn post_json_empty_object() {
        let body = outbound_body("POST", BodyEncoding::Json, &serde_json::json!({})).unwrap();
        match body {
            Some(BodyKind::Json(s)) => assert_eq!(s, "{}"),
            _ => panic!("expected json {{}}"),
        }
    }

    #[test]
    fn form_encodes_sorted_scalars() {
        let raw = encode_form_object(&serde_json::json!({"b": "x", "a": 1, "c": true})).unwrap();
        assert_eq!(raw, "a=1&b=x&c=true");
    }

    #[test]
    fn form_skips_null_keys() {
        let raw = encode_form_object(&serde_json::json!({"a": 1, "b": null})).unwrap();
        assert_eq!(raw, "a=1");
    }

    #[test]
    fn form_rejects_nested() {
        let err = encode_form_object(&serde_json::json!({"a": {"b": 1}})).unwrap_err();
        assert!(err.to_string().contains("嵌套"));
    }

    #[test]
    fn form_rejects_non_object() {
        let err = encode_form_object(&serde_json::json!([1, 2])).unwrap_err();
        assert!(err.to_string().contains("JSON 对象"));
    }
}
