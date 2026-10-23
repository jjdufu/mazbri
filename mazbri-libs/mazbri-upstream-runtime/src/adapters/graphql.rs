use crate::http_call::{HttpCall, append_query, join_url};
use crate::types::{BizError, BodyKind, ExecuteError};

use super::{AdapterCtx, AdapterOutcome, parse_json_body, require_object, snake_to_camel};

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    require_object(&ctx.input.payload)?;
    let spec = &ctx.input.spec;
    let gql = spec.graphql.as_ref().ok_or_else(|| {
        ExecuteError::SpecInvalid("graphql adapter requires graphql.document".into())
    })?;
    if gql.document.trim().is_empty() {
        return Err(ExecuteError::SpecInvalid(
            "graphql.document is empty".into(),
        ));
    }

    let mut variables = serde_json::Map::new();
    if let Some(obj) = ctx.input.payload.as_object() {
        for (k, v) in obj {
            let mapped_name = gql
                .variables_map
                .as_ref()
                .and_then(|m| m.get(k))
                .cloned()
                .unwrap_or_else(|| snake_to_camel(k));
            let mapped_val = map_enum(v, gql.enum_map.as_ref());
            variables.insert(mapped_name, mapped_val);
        }
    }

    let mut body_obj = serde_json::json!({
        "query": gql.document,
        "variables": serde_json::Value::Object(variables),
    });
    if let Some(ref name) = gql.operation_name
        && !name.is_empty()
        && let Some(obj) = body_obj.as_object_mut()
    {
        obj.insert(
            "operationName".into(),
            serde_json::Value::String(name.clone()),
        );
    }

    let path = spec.request.path.as_str();
    let path = if path == "/" { "/graphql" } else { path };
    let url = append_query(
        &join_url(ctx.base_url, path),
        &super::merge_request_query(ctx.auth, spec),
    );
    let body = serde_json::to_string(&body_obj)
        .map_err(|e| ExecuteError::SpecInvalid(format!("graphql body: {e}")))?;
    let response = super::dispatch_http(
        ctx,
        HttpCall {
            method: "POST",
            url: &url,
            headers: super::merge_request_headers(ctx.auth, spec),
            body: Some(BodyKind::Json(body)),
            timeout: ctx.timeout,
        },
    )
    .await?;
    let json = parse_json_body(&response.body)?;
    if let Some(errors) = json.get("errors").and_then(|v| v.as_array())
        && let Some(first) = errors.first()
    {
        let code = first
            .pointer("/extensions/code")
            .and_then(|v| v.as_str())
            .unwrap_or("UPSTREAM_BIZ");
        let message = first
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("graphql error")
            .to_string();
        return Ok(AdapterOutcome {
            payload: serde_json::Value::Null,
            biz_error: Some(BizError {
                code: code.into(),
                message,
                details: Some(json.clone()),
            }),
            response,
        });
    }
    let payload = json.get("data").cloned().unwrap_or(json);
    Ok(AdapterOutcome {
        response,
        payload,
        biz_error: None,
    })
}

fn map_enum(
    v: &serde_json::Value,
    enum_map: Option<&std::collections::HashMap<String, String>>,
) -> serde_json::Value {
    let Some(map) = enum_map else {
        return v.clone();
    };
    match v {
        serde_json::Value::String(s) => {
            if let Some(m) = map.get(s) {
                serde_json::Value::String(m.clone())
            } else {
                v.clone()
            }
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|x| map_enum(x, enum_map)).collect())
        }
        serde_json::Value::Object(obj) => {
            let mut out = serde_json::Map::new();
            for (k, val) in obj {
                out.insert(k.clone(), map_enum(val, enum_map));
            }
            serde_json::Value::Object(out)
        }
        _ => v.clone(),
    }
}
