use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::http_call::{append_query, join_url};
use crate::types::{BizError, ExecuteError, OutboundResponse};

use super::{AdapterCtx, AdapterOutcome, wrap_by_name};

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &ctx.input.spec;
    let operation = spec
        .operation
        .as_deref()
        .ok_or_else(|| ExecuteError::SpecInvalid("ws_unary operation missing".into()))?;
    let path = spec
        .ws
        .as_ref()
        .and_then(|w| w.path.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| spec.request_path(&ctx.input.path_prefix));
    let mut url = join_url(ctx.base_url, path);
    if url.starts_with("http://") {
        url = format!("ws://{}", url.trim_start_matches("http://"));
    } else if url.starts_with("https://") {
        url = format!("wss://{}", url.trim_start_matches("https://"));
    }
    url = append_query(&url, &super::merge_request_query(ctx.auth, spec));
    if let Err(e) = ctx
        .ssrf
        .validate_url(
            &url.replace("ws://", "http://")
                .replace("wss://", "https://"),
        )
        .await
    {
        return Err(ExecuteError::SsrfBlocked(e));
    }

    let mut req = url
        .as_str()
        .into_client_request()
        .map_err(|e| ExecuteError::AdapterFailed(format!("ws request: {e}")))?;
    let mut headers = super::merge_request_headers(ctx.auth, spec);
    crate::http_call::inject_x_request_id(&mut headers, &ctx.input.request_id);
    for (k, v) in headers {
        if let (Ok(name), Ok(val)) = (
            tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(k.as_bytes()),
            tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&v),
        ) {
            req.headers_mut().insert(name, val);
        }
    }

    let connect = tokio_tungstenite::connect_async(req);
    let (mut ws, _resp) = tokio::time::timeout(ctx.ws_handshake_timeout, connect)
        .await
        .map_err(|_| ExecuteError::UpstreamTransport("ws handshake timeout".into()))?
        .map_err(|e| ExecuteError::UpstreamTransport(format!("ws connect: {e}")))?;

    let mut frame = wrap_by_name(spec, &ctx.input.payload)?;
    if let Some(obj) = frame.as_object_mut() {
        obj.entry("id")
            .or_insert_with(|| serde_json::Value::String(ctx.input.request_id.clone()));
        obj.entry("method")
            .or_insert_with(|| serde_json::Value::String(operation.to_string()));
    }
    let text = serde_json::to_string(&frame)
        .map_err(|e| ExecuteError::SpecInvalid(format!("ws frame: {e}")))?;
    ws.send(Message::Text(text.into()))
        .await
        .map_err(|e| ExecuteError::UpstreamTransport(format!("ws send: {e}")))?;

    let wait = tokio::time::timeout(ctx.timeout, async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(t))) => {
                    let v: serde_json::Value = serde_json::from_str(&t)
                        .map_err(|e| ExecuteError::AdapterFailed(format!("ws frame json: {e}")))?;
                    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
                    if id == ctx.input.request_id || id.is_empty() {
                        return Ok(v);
                    }
                }
                Some(Ok(Message::Binary(_))) => {
                    return Err(ExecuteError::AdapterFailed(
                        "binary ws frame not supported".into(),
                    ));
                }
                Some(Ok(Message::Close(_))) | None => {
                    return Err(ExecuteError::UpstreamTransport(
                        "ws closed before reply".into(),
                    ));
                }
                Some(Ok(_)) => continue,
                Some(Err(e)) => {
                    return Err(ExecuteError::UpstreamTransport(format!("ws: {e}")));
                }
            }
        }
    })
    .await
    .map_err(|_| ExecuteError::UpstreamTransport("ws wait exceeded timeout_ms".into()))?;

    let _ = ws.close(None).await;
    let json = wait?;
    let code = json
        .get("code")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    if code != 0 {
        let message = json
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("ws business error")
            .to_string();
        return Ok(AdapterOutcome {
            payload: serde_json::Value::Null,
            biz_error: Some(BizError {
                code: "UPSTREAM_BIZ".into(),
                message,
                details: Some(json.clone()),
            }),
            response: synthetic_response(&url, &json),
        });
    }
    let payload = json.get("data").cloned().unwrap_or(json.clone());
    Ok(AdapterOutcome {
        payload,
        biz_error: None,
        response: synthetic_response(&url, &json),
    })
}

fn synthetic_response(url: &str, json: &serde_json::Value) -> OutboundResponse {
    OutboundResponse {
        status: 200,
        headers: Default::default(),
        body: serde_json::to_vec(json).unwrap_or_default(),
        final_url: url.to_string(),
        method: "WS".into(),
        path: url.to_string(),
    }
}
