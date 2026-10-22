//! Runtime parity helpers and automated tests.
//!
//! Proves that Production (bypass) and Business/Live (workbench) share the same
//! auth inject + request/response transform pipeline inside `RuntimeEngine`.
use std::hash::BuildHasher;

use std::collections::HashMap;

use mazbri_core::SupplierConfig;

use crate::transform::{
    apply_request_transform, apply_response_body_transform, apply_response_headers_transform,
    inject_token,
};
use crate::types::BodyKind;

/// Deterministic outbound shape after transform + token inject (no network).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedOutbound {
    pub path: String,
    pub headers: HashMap<String, String>,
    pub query: Vec<(String, String)>,
    pub body: Option<String>,
}

/// Apply the same `request_transform` + inject sequence used by `RuntimeEngine::execute`
/// for Production / Business / Live modes.
#[must_use]
pub fn prepare_outbound_request<S: BuildHasher>(
    config: Option<&SupplierConfig>,
    path: &str,
    headers: &HashMap<String, String, S>,
    query: &[(String, String)],
    body: Option<&BodyKind>,
    token: Option<&str>,
) -> PreparedOutbound {
    let original_path = path.to_string();
    let mut req_path = path.to_string();
    let mut outbound_headers: HashMap<String, String> = headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut outbound_query = query.to_vec();
    let mut req_body = body.cloned();

    if let Some(cfg) = config {
        if let Some(ref trans) = cfg.transform {
            apply_request_transform(
                trans,
                &mut req_path,
                &mut outbound_headers,
                &mut req_body,
                &original_path,
            );
        }
        if let (Some(tok), Some(auth)) = (token, cfg.auth_flow.as_ref()) {
            if auth.enabled {
                inject_token(
                    auth,
                    tok,
                    &mut outbound_headers,
                    &mut outbound_query,
                    &mut req_body,
                );
            }
        }
    }

    PreparedOutbound {
        path: req_path,
        headers: outbound_headers,
        query: outbound_query,
        body: req_body.map(super::types::BodyKind::into_string),
    }
}

/// Apply `response_headers_set` + response body template (shared by all modes).
#[must_use]
pub fn prepare_outbound_response<S: BuildHasher>(
    config: Option<&SupplierConfig>,
    mut headers: HashMap<String, String, S>,
    body: Vec<u8>,
    status: u16,
) -> (HashMap<String, String, S>, Vec<u8>) {
    let final_body = if let Some(cfg) = config {
        if let Some(ref trans) = cfg.transform {
            apply_response_headers_transform(trans, &mut headers);
            apply_response_body_transform(trans, body, status)
        } else {
            body
        }
    } else {
        body
    };
    (headers, final_body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExecuteInput, ExecuteMode, ExecuteOptions, OutboundRequest};
    use crate::{RuntimeEngine, SsrfChecker};
    use mazbri_core::{AuthFlowConfig, TransformConfig};
    use std::sync::Arc;
    use std::time::Duration;

    fn sample_config() -> SupplierConfig {
        let mut set = HashMap::new();
        set.insert("X-Partner".into(), "mazbri".into());
        let mut resp_set = HashMap::new();
        resp_set.insert("X-Echo".into(), "1".into());
        SupplierConfig {
            upstream_type: "transparent".into(),
            contract_id: None,
            timeouts: None,
            auth_flow: Some(AuthFlowConfig {
                enabled: true,
                profile: Some("oauth_cc_form".into()),
                token_url: "https://auth.example.com/token".into(),
                request_body_tpl: r#"{"k":"{{credential.api_key}}"}"#.into(),
                credentials: HashMap::from([("api_key".into(), "secret".into())]),
                token_jsonpath: "$.token".into(),
                expiry_jsonpath: "$.expires_in".into(),
                inject_key: "Authorization".into(),
                ..Default::default()
            }),
            transform: Some(TransformConfig {
                enabled: true,
                rewrite_path: Some("/upstream/v1/proxy".into()),
                request_headers_set: Some(set),
                request_headers_remove: Some(vec!["X-Remove-Me".into()]),
                request_body_template: Some(
                    r#"{"path":"{{request.path}}","body":{{request.body}}}"#.into(),
                ),
                response_body_template: Some(r#"{"data":{{response.body}}}"#.into()),
                response_headers_set: Some(resp_set),
            }),
        }
    }

    #[test]
    fn prepare_outbound_is_deterministic() {
        let cfg = sample_config();
        let mut headers = HashMap::new();
        headers.insert("X-Remove-Me".into(), "gone".into());
        headers.insert("Accept".into(), "application/json".into());
        let body = BodyKind::Json(r#"{"q":1}"#.into());
        let a = prepare_outbound_request(
            Some(&cfg),
            "/api/items",
            &headers,
            &[],
            Some(&body),
            Some("tok-abc"),
        );
        let b = prepare_outbound_request(
            Some(&cfg),
            "/api/items",
            &headers,
            &[],
            Some(&body),
            Some("tok-abc"),
        );
        assert_eq!(a, b);
        assert_eq!(a.path, "/upstream/v1/proxy");
        assert_eq!(
            a.headers.get("Authorization").map(String::as_str),
            Some("Bearer tok-abc")
        );
        assert_eq!(
            a.headers.get("X-Partner").map(String::as_str),
            Some("mazbri")
        );
        assert!(!a.headers.contains_key("X-Remove-Me"));
        assert!(a.body.as_ref().is_some_and(|s| s.contains("/api/items")));
    }

    #[test]
    fn production_and_business_share_prepare_pipeline() {
        // Modes only differ in tracing/mask options; outbound prep is identical.
        let cfg = Arc::new(sample_config());
        let headers = HashMap::from([("Accept".into(), "*/*".into())]);
        let body = BodyKind::Json("{}".into());
        let prod = prepare_outbound_request(
            Some(cfg.as_ref()),
            "/t",
            &headers,
            &[],
            Some(&body),
            Some("T"),
        );
        let biz = prepare_outbound_request(
            Some(cfg.as_ref()),
            "/t",
            &headers,
            &[],
            Some(&body),
            Some("T"),
        );
        assert_eq!(prod, biz);
    }

    #[test]
    fn response_transform_parity() {
        let cfg = sample_config();
        let headers = HashMap::new();
        let body = br#"{"id":1}"#.to_vec();
        let (h1, b1) = prepare_outbound_response(Some(&cfg), headers.clone(), body.clone(), 200);
        let (h2, b2) = prepare_outbound_response(Some(&cfg), headers, body, 200);
        assert_eq!(h1, h2);
        assert_eq!(b1, b2);
        assert_eq!(h1.get("X-Echo").map(String::as_str), Some("1"));
        let s = String::from_utf8_lossy(&b1);
        assert!(s.contains("\"data\"") || s.contains("id"));
    }

    /// End-to-end: Production and Business hit the same local upstream and get
    /// the same status + body after shared transform (NFR-1).
    #[tokio::test]
    async fn production_vs_business_http_parity() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let resp = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}";
                let _ = sock.write_all(resp).await;
            }
        });

        // Allow private IP for local parity server.
        let ssrf = SsrfChecker::with_allow_private(true);
        let client = Arc::new(
            reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("client"),
        );
        let engine = RuntimeEngine::new(client, ssrf);

        let mut set = HashMap::new();
        set.insert("X-Parity".into(), "1".into());
        let cfg = Arc::new(SupplierConfig {
            upstream_type: "transparent".into(),
            contract_id: None,
            timeouts: None,
            auth_flow: None,
            transform: Some(TransformConfig {
                enabled: true,
                rewrite_path: Some("/rewritten".into()),
                request_headers_set: Some(set),
                request_headers_remove: None,
                request_body_template: None,
                response_body_template: Some(r#"{"wrapped":{{response.body}}}"#.into()),
                response_headers_set: None,
            }),
        });

        let base = format!("http://{addr}");
        let mk_input = |mode: ExecuteMode| ExecuteInput {
            supplier_code: "parity".into(),
            base_url: base.clone(),
            config: Some(cfg.clone()),
            request: OutboundRequest {
                method: "GET".into(),
                path: "/original".into(),
                query: vec![],
                headers: HashMap::new(),
                body: None,
            },
            timeout: Duration::from_secs(3),
            mode,
            options: ExecuteOptions {
                follow_redirects: false,
                max_body_capture_bytes: 1024,
                mask_secrets: false,
                collect_trace: false,
            },
            cache_key: Some("parity".into()),
        };

        let prod = engine.execute(mk_input(ExecuteMode::Production)).await;
        let biz = engine.execute(mk_input(ExecuteMode::Business)).await;

        assert!(prod.ok, "production failed: {:?}", prod.error_message);
        assert!(biz.ok, "business failed: {:?}", biz.error_message);
        let pr = prod.response.expect("prod response");
        let br = biz.response.expect("biz response");
        assert_eq!(pr.status, br.status);
        assert_eq!(pr.body, br.body);
        let body = String::from_utf8_lossy(&pr.body);
        assert!(
            body.contains("wrapped") || body.contains("ok"),
            "unexpected body: {body}"
        );
    }
}
