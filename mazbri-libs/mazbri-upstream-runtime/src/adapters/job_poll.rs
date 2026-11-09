//! `job_poll`：**submit-only**（或请求级一次性 `mode=query`）。
//!
//! **禁止**在一次 `invoke` 内 `sleep` + 轮询直至终态。无 `sync_wait`、无双模兼容开关。
//!
//! ## 请求约定（租户 JSON 对象）
//! - 默认，或 `job_poll_mode` 缺省 / `"submit"`：POST `submit_path`（或缺省用 `request.path`），
//!   按 `job_id_jsonpath` 解析 id，立即返回 `{"job_id":"<id>"}`（响应字段名固定为 [`JOB_ID_FIELD`]）。
//! - `job_poll_mode` = `"query"`：要求同对象含字符串 [`JOB_ID_FIELD`]；对 `query_path`
//!   （替换 `{job_id}`）发 **一次** GET，返回当次上游 JSON；**不循环**。
//!
//! 控制字段 [`MODE_FIELD`] 在 submit 组包前从业务体剥离，避免泄漏给上游。

use mazbri_core::channel_egress::JobPollSpec;

use crate::http_call::{HttpCall, append_query, join_url};
use crate::types::{BodyKind, ExecuteError, ExecuteMode};
use mazbri_core::json_path::extract_by_path;

use super::{AdapterCtx, AdapterOutcome, parse_json_body, wrap_method_biz};

/// 规范化 submit 响应 / query 请求中的 job 标识字段名（避免魔法字符串散落）。
pub const JOB_ID_FIELD: &str = "job_id";

/// 租户请求体中的模式字段：`submit`（默认）| `query`。
pub const MODE_FIELD: &str = "job_poll_mode";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobPollMode {
    Submit,
    Query,
}

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &ctx.input.spec;
    let jp = spec.job_poll.as_ref().ok_or_else(|| {
        ExecuteError::SpecInvalid("job_poll adapter requires job_poll object".into())
    })?;

    let mode = resolve_mode(&ctx.input.payload)?;
    match mode {
        JobPollMode::Submit => invoke_submit(ctx, jp).await,
        JobPollMode::Query => invoke_query(ctx, jp).await,
    }
}

fn resolve_mode(payload: &serde_json::Value) -> Result<JobPollMode, ExecuteError> {
    let Some(obj) = payload.as_object() else {
        return Ok(JobPollMode::Submit);
    };
    let Some(raw) = obj.get(MODE_FIELD) else {
        return Ok(JobPollMode::Submit);
    };
    let s = raw
        .as_str()
        .ok_or_else(|| ExecuteError::SpecInvalid(format!("{MODE_FIELD} must be a string")))?;
    match s {
        "submit" => Ok(JobPollMode::Submit),
        "query" => Ok(JobPollMode::Query),
        other => Err(ExecuteError::SpecInvalid(format!(
            "{MODE_FIELD} must be \"submit\" or \"query\", got {other:?}"
        ))),
    }
}

/// 从业务体去掉控制字段后再组包，避免把 `job_poll_mode` 打给上游。
fn payload_for_submit(payload: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = payload.as_object() else {
        return payload.clone();
    };
    if !obj.contains_key(MODE_FIELD) {
        return payload.clone();
    }
    let mut cleaned = obj.clone();
    cleaned.remove(MODE_FIELD);
    serde_json::Value::Object(cleaned)
}

fn job_id_from_value(val: &serde_json::Value) -> Result<String, ExecuteError> {
    let job_id = match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    };
    if job_id.is_empty() || job_id == "null" {
        return Err(ExecuteError::AdapterFailed("empty job_id".into()));
    }
    Ok(job_id)
}

async fn invoke_submit(
    ctx: &AdapterCtx<'_>,
    jp: &JobPollSpec,
) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &ctx.input.spec;
    let operation = spec
        .operation
        .as_deref()
        .ok_or_else(|| ExecuteError::SpecInvalid("job_poll operation missing".into()))?;
    let submit_path = jp
        .submit_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(spec.request.path.as_str());
    let biz = payload_for_submit(&ctx.input.payload);
    let wrapped = wrap_method_biz(operation, &biz);
    let body = serde_json::to_string(&wrapped)
        .map_err(|e| ExecuteError::SpecInvalid(format!("job submit: {e}")))?;
    let headers = super::merge_request_headers(ctx.auth, spec);
    let query = super::merge_request_query(ctx.auth, spec);
    let submit_url = append_query(&join_url(ctx.base_url, submit_path), &query);
    let submit = super::dispatch_http(
        ctx,
        HttpCall {
            method: "POST",
            url: &submit_url,
            headers,
            body: Some(BodyKind::Json(body)),
            timeout: ctx.timeout,
        },
    )
    .await?;
    if matches!(ctx.input.mode, ExecuteMode::AuthOnly) || ctx.is_reject_status(submit.status) {
        return Ok(AdapterOutcome {
            auth_rejection: None,
            payload: serde_json::Value::Null,
            biz_error: None,
            response: submit,
        });
    }
    let submit_json = parse_json_body(&submit.body)?;
    let job_id_val = extract_by_path(&submit_json, &jp.job_id_jsonpath).ok_or_else(|| {
        ExecuteError::AdapterFailed(format!("job_id not found at {}", jp.job_id_jsonpath))
    })?;
    let job_id = job_id_from_value(&job_id_val)?;
    // 立即返回；终态由租户侧轮询 / 回调 / 产品查询 API 完成——不在此 sleep。
    Ok(AdapterOutcome {
        auth_rejection: None,
        payload: serde_json::json!({ JOB_ID_FIELD: job_id }),
        biz_error: None,
        response: submit,
    })
}

async fn invoke_query(
    ctx: &AdapterCtx<'_>,
    jp: &JobPollSpec,
) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &ctx.input.spec;
    let job_id = ctx
        .input
        .payload
        .as_object()
        .and_then(|o| o.get(JOB_ID_FIELD))
        .ok_or_else(|| {
            ExecuteError::SpecInvalid(format!(
                "job_poll {MODE_FIELD}=query requires string field {JOB_ID_FIELD}"
            ))
        })?;
    let job_id = job_id.as_str().ok_or_else(|| {
        ExecuteError::SpecInvalid(format!(
            "job_poll {MODE_FIELD}=query requires string field {JOB_ID_FIELD}"
        ))
    })?;
    if job_id.is_empty() {
        return Err(ExecuteError::SpecInvalid(format!(
            "job_poll {MODE_FIELD}=query: {JOB_ID_FIELD} must not be empty"
        )));
    }

    let headers = super::merge_request_headers(ctx.auth, spec);
    let query = super::merge_request_query(ctx.auth, spec);
    let query_path = jp.query_path.replace("{job_id}", job_id);
    let qurl = append_query(&join_url(ctx.base_url, &query_path), &query);
    let last = super::dispatch_http(
        ctx,
        HttpCall {
            method: "GET",
            url: &qurl,
            headers,
            body: None,
            timeout: ctx.timeout,
        },
    )
    .await?;
    if matches!(ctx.input.mode, ExecuteMode::AuthOnly) || ctx.is_reject_status(last.status) {
        return Ok(AdapterOutcome {
            auth_rejection: None,
            payload: serde_json::Value::Null,
            biz_error: None,
            response: last,
        });
    }
    if last.status == 404 {
        return Ok(AdapterOutcome {
            auth_rejection: None,
            payload: serde_json::Value::Null,
            biz_error: Some(crate::types::BizError {
                code: "NOT_FOUND".into(),
                message: "job not found".into(),
                details: None,
            }),
            response: last,
        });
    }
    let json = parse_json_body(&last.body)?;
    Ok(AdapterOutcome {
        auth_rejection: None,
        payload: json,
        biz_error: None,
        response: last,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AppliedAuth;
    use crate::ssrf::SsrfChecker;
    use crate::types::{ExecuteMode, ExecuteOptions, InvokeInput};
    use mazbri_core::channel_egress::{ADAPTER_JOB_POLL, ChannelEgressSpec, JobPollSpec};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn auth_empty() -> AppliedAuth {
        AppliedAuth {
            headers: HashMap::new(),
            query: vec![],
            metadata: HashMap::new(),
            token: None,
            stale_evicted: None,
        }
    }

    fn base_spec(jp: JobPollSpec) -> ChannelEgressSpec {
        ChannelEgressSpec {
            adapter: ADAPTER_JOB_POLL.into(),
            base_url: "http://127.0.0.1".into(),
            operation: Some("upupt.posts.search".into()),
            job_poll: Some(jp),
            request: mazbri_core::channel_egress::EgressRequestSpec {
                method: "POST".into(),
                path: "/jobs".into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn invoke_input(payload: serde_json::Value, spec: ChannelEgressSpec) -> InvokeInput {
        InvokeInput {
            supplier_id: "s".into(),
            supplier_code: "s".into(),
            channel_id: "c".into(),
            request_id: "rid".into(),
            payload,
            spec,
            config: None,
            timeout: Duration::from_secs(2),
            mode: ExecuteMode::Business,
            options: ExecuteOptions::default(),
            credentials: HashMap::new(),
            pipeline_keys: None,
            artifact_bytes: None,
            cache_key: None,
        }
    }

    /// 极简上游：`POST /jobs` → `job_id`；`GET /jobs/{id}` → 永远 pending。
    async fn spawn_counting_upstream(hits: Arc<AtomicUsize>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let hits = hits.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    hits.fetch_add(1, Ordering::SeqCst);
                    let (status, body) = if req.starts_with("POST ") {
                        (200, r#"{"job_id":"j-1","status":"pending"}"#)
                    } else if req.starts_with("GET ") {
                        // 故意一直 pending：旧实现会在此循环；新实现最多打一次。
                        (200, r#"{"job_id":"j-1","status":"pending"}"#)
                    } else {
                        (404, r#"{"error":"no"}"#)
                    };
                    let resp = format!(
                        "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}")
    }

    async fn run_invoke(payload: serde_json::Value) -> (AdapterOutcome, Arc<AtomicUsize>) {
        let hits = Arc::new(AtomicUsize::new(0));
        let base = spawn_counting_upstream(hits.clone()).await;
        let spec = base_spec(JobPollSpec {
            submit_path: Some("/jobs".into()),
            query_path: "/jobs/{job_id}".into(),
            job_id_jsonpath: "$.job_id".into(),
        });
        let mut input = invoke_input(payload, spec);
        input.spec.base_url = base.clone();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let ssrf = SsrfChecker::with_allow_private(true);
        let auth = auth_empty();
        let pipeline_keys = crate::crypto_pipeline::PipelineKeys::empty();
        let registry = mazbri_crypto::AlgorithmRegistry::default_registry();
        let ws_pool = crate::ws_pool::WsPool::new(crate::ws_pool::WsPoolConfig::default());
        let invalidation = mazbri_core::AuthInvalidationSpec::default();
        let ctx = AdapterCtx {
            client: &client,
            ssrf: &ssrf,
            input: &input,
            base_url: base.as_str(),
            auth: &auth,
            timeout: Duration::from_secs(2),
            ws_pool: &ws_pool,
            pipeline_keys: &pipeline_keys,
            registry: &registry,
            invalidation: &invalidation,
            ws_stale_retry: None,
            observer: &crate::observe::NoopObserver,
            retry_log: &crate::observe::RetryLog::default(),
        };
        let out = invoke(&ctx).await.expect("invoke");
        (out, hits)
    }

    #[tokio::test]
    async fn submit_only_makes_exactly_one_http_call() {
        let (out, hits) = run_invoke(serde_json::json!({"keyword": "x"})).await;
        assert_eq!(hits.load(Ordering::SeqCst), 1, "submit must be single HTTP");
        assert_eq!(out.payload, serde_json::json!({"job_id": "j-1"}));
        assert!(out.biz_error.is_none());
    }

    #[tokio::test]
    async fn query_mode_makes_exactly_one_http_call() {
        let (out, hits) = run_invoke(serde_json::json!({
            "job_poll_mode": "query",
            "job_id": "j-1"
        }))
        .await;
        assert_eq!(hits.load(Ordering::SeqCst), 1, "query must be single HTTP");
        assert_eq!(
            out.payload.get("status").and_then(|v| v.as_str()),
            Some("pending")
        );
    }

    #[tokio::test]
    async fn submit_does_not_poll_even_when_upstream_stays_pending() {
        // 若误留 sleep 轮询，timeout 内会对 pending 多次 GET。
        let (out, hits) = run_invoke(serde_json::json!({})).await;
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(out.payload["job_id"], "j-1");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }
}
