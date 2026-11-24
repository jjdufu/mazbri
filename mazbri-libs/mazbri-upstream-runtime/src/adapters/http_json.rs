use crate::http_call::{HttpCall, append_query, join_url};
use crate::types::{BodyKind, ExecuteError};
use mazbri_core::BodyEncoding;
use serde_json::Value;

use super::crypto_wire::{apply_parts, maybe_open_json_body, profile_with_slots, seal_body};
use super::{
    AdapterCtx, AdapterOutcome, apply_ok_and_unwrap, can_passthrough_raw_json, parse_json_body,
    try_take_valid_raw_json,
};

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &*ctx.input.spec;
    let skip_validate = ctx.input.spec_validated;
    let method = spec.request.method.as_str();
    let path = spec.request.path.as_str();
    let mut query = super::merge_request_query(ctx.auth, spec);
    let mut headers = super::merge_request_headers(ctx.auth, spec);

    // GET/HEAD 无 body：把 JSON 对象标量字段摊进 query（通用，便于 posts/search 等）。
    if method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD") {
        append_payload_to_query(&mut query, &ctx.input.payload)?;
    }

    let body = build_request_body(ctx, &mut headers, &mut query)?;

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
            payload: Value::Null,
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
    let json = parse_json_body(body_slice).unwrap_or_else(|_| {
        serde_json::Value::String(String::from_utf8_lossy(&response.body).into_owned())
    });
    let (payload, biz_error) = apply_ok_and_unwrap(&json, spec)?;
    Ok(AdapterOutcome {
        auth_rejection: None,
        response,
        payload,
        raw_payload_json: None,
        biz_error,
    })
}

fn build_request_body(
    ctx: &AdapterCtx<'_>,
    headers: &mut std::collections::HashMap<String, String>,
    query: &mut Vec<(String, String)>,
) -> Result<Option<BodyKind>, ExecuteError> {
    let spec = &*ctx.input.spec;
    let skip_validate = ctx.input.spec_validated;
    let method = spec.request.method.as_str();
    let encoding = spec.request.body_encoding;
    if let Some(profile) = spec.crypto.as_ref() {
        if method.eq_ignore_ascii_case("GET")
            || method.eq_ignore_ascii_case("HEAD")
            || encoding == BodyEncoding::None
        {
            return Ok(None);
        }
        let raw = if ctx.input.payload.is_null() {
            b"{}".to_vec()
        } else {
            serde_json::to_vec(&ctx.input.payload)
                .map_err(|e| ExecuteError::SpecInvalid(format!("payload json: {e}")))?
        };
        let form_style = encoding == BodyEncoding::Form;
        let effective_profile = profile_with_slots(profile, form_style);
        let sealed = seal_body(
            &raw,
            &effective_profile,
            ctx.pipeline_keys,
            ctx.registry,
            &[],
            skip_validate,
        )?;
        let mut wire = apply_parts(headers, query, &sealed)?;
        if form_style
            && let Some(BodyKind::Json(s)) = wire
        {
            let v: Value = serde_json::from_str(&s)
                .map_err(|e| ExecuteError::AdapterFailed(format!("crypto form from json: {e}")))?;
            wire = Some(BodyKind::Form(encode_form_object(&v)?));
        }
        if matches!(wire, Some(BodyKind::Form(_))) {
            super::force_urlencoded_content_type(headers);
        } else {
            super::apply_json_content_type_override(headers, spec);
        }
        return Ok(wire);
    }
    let body = outbound_body(method, encoding, &ctx.input.payload)?;
    if body.is_some() {
        if encoding == BodyEncoding::Form {
            super::force_urlencoded_content_type(headers);
        } else {
            super::apply_json_content_type_override(headers, spec);
        }
    }
    Ok(body)
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

    #[tokio::test]
    async fn production_http_json_uses_raw_passthrough_and_records_wire_us() {
        use std::collections::HashMap;
        use std::sync::Arc;
        use std::time::Duration;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = sock.read(&mut buf).await.unwrap();
            let body = r#"{"ok":true,"items":[1,2,3]}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let _ = sock.write_all(resp.as_bytes()).await;
        });

        let spec = mazbri_core::ChannelEgressSpec {
            adapter: mazbri_core::ADAPTER_HTTP_JSON.into(),
            base_url: format!("http://{addr}"),
            request: mazbri_core::EgressRequestSpec {
                method: "POST".into(),
                path: "/echo".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let ssrf = crate::ssrf::SsrfChecker::with_allow_private(true);
        let client = Arc::new(reqwest::Client::builder().no_proxy().build().unwrap());
        let engine = crate::engine::RuntimeEngine::new(client, ssrf);
        let result = engine
            .invoke(crate::types::InvokeInput {
                supplier_id: "s".into(),
                supplier_code: "S".into(),
                channel_id: "c".into(),
                request_id: "r".into(),
                payload: serde_json::json!({"ping": 1}),
                spec: Arc::new(spec),
                spec_validated: true,
                config: None,
                timeout: Duration::from_secs(2),
                mode: crate::types::ExecuteMode::Production,
                options: crate::types::ExecuteOptions::default(),
                credentials: HashMap::new(),
                pipeline_keys: None,
                artifact_bytes: None,
                descriptor_pool: None,
                cache_key: None,
            })
            .await;
        assert!(result.ok);
        assert!(
            result.payload.is_none(),
            "production direct JSON must avoid Value DOM allocation"
        );
        assert_eq!(
            result.raw_payload_json.as_deref(),
            Some(r#"{"ok":true,"items":[1,2,3]}"#)
        );
        let meta = result.upstream_meta.expect("upstream_meta");
        assert!(meta.latency_us > 0);
    }
}
