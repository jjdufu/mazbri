//! mgmt ↔ bypass 隔离管理探针 HTTP JSON 契约（`POST /v1/mgmt-probe/invoke`）。
//!
//! - mgmt 客户端序列化 [`ProbeInvokeRequest`]、反序列化 [`ProbeInvokeResponse`]；
//!   bypass handler 反序列化请求、序列化响应。两侧只用这里的类型，不各自定义。
//! - 响应由 proto [`InvokeResponse`] 经 `From` 转换得到，转换对 proto 消息做穷尽解构：
//!   proto 字段增删或改名时这里编译失败，JSON 契约不会再与 gRPC 契约悄悄分叉。
//! - 全部类型 `deny_unknown_fields`、无 `default`：缺字段或多字段都按解码失败处理。

use crate::shared::{
    InvokeBizError, InvokeRequest, InvokeResponse, InvokeUpstreamMeta, TraceContext,
};
use serde::{Deserialize, Serialize};

/// 管理探针唯一路由。
pub const MGMT_PROBE_INVOKE_PATH: &str = "/v1/mgmt-probe/invoke";

/// 探针请求体；字段与 proto [`InvokeRequest`] 一一对应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeInvokeRequest {
    pub product_id: String,
    pub tenant_id: String,
    pub channel_id: String,
    pub request_id: String,
    pub trace: ProbeTrace,
    pub timeout_ms: u64,
    pub payload_json: String,
    pub egress_spec_hash: String,
    pub egress_spec_json: String,
    pub supplier_id: String,
    pub supplier_code: String,
    pub supplier_config_hash: String,
    pub supplier_config_json: String,
}

/// 链路上下文；字段与 proto [`TraceContext`] 一一对应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeTrace {
    pub request_id: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: String,
}

/// 探针响应体；由 proto [`InvokeResponse`] 转换而来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeInvokeResponse {
    pub success: bool,
    pub error: String,
    pub biz_error: Option<ProbeBizError>,
    pub payload_json: String,
    pub upstream_meta: Option<ProbeUpstreamMeta>,
    /// bypass 内部重试次数（认证换票、WS 换新连接）。
    pub internal_retries: u32,
    /// 每次内部重试的原因标签。
    pub retry_reasons: Vec<String>,
}

/// 业务错误；由 proto [`InvokeBizError`] 转换而来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeBizError {
    pub code: String,
    pub message: String,
    pub details_json: String,
}

/// 上游元信息；由 proto [`InvokeUpstreamMeta`] 转换而来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeUpstreamMeta {
    pub status: u32,
    pub url: String,
    pub adapter: String,
    /// 纯上游耗时（reqwest 发送 + 读 body），不含 bypass 内部处理。
    pub upstream_wire_ms: u64,
}

impl From<ProbeTrace> for TraceContext {
    fn from(t: ProbeTrace) -> Self {
        let ProbeTrace {
            request_id,
            trace_id,
            span_id,
            parent_span_id,
        } = t;
        Self {
            request_id,
            trace_id,
            span_id,
            parent_span_id,
        }
    }
}

impl From<ProbeInvokeRequest> for InvokeRequest {
    fn from(r: ProbeInvokeRequest) -> Self {
        let ProbeInvokeRequest {
            product_id,
            tenant_id,
            channel_id,
            request_id,
            trace,
            timeout_ms,
            payload_json,
            egress_spec_hash,
            egress_spec_json,
            supplier_id,
            supplier_code,
            supplier_config_hash,
            supplier_config_json,
        } = r;
        Self {
            product_id,
            tenant_id,
            channel_id,
            request_id,
            trace: Some(trace.into()),
            timeout_ms,
            payload_json,
            egress_spec_hash,
            egress_spec_json,
            supplier_id,
            supplier_code,
            supplier_config_hash,
            supplier_config_json,
        }
    }
}

impl From<InvokeBizError> for ProbeBizError {
    fn from(b: InvokeBizError) -> Self {
        let InvokeBizError {
            code,
            message,
            details_json,
        } = b;
        Self {
            code,
            message,
            details_json,
        }
    }
}

impl From<InvokeUpstreamMeta> for ProbeUpstreamMeta {
    fn from(m: InvokeUpstreamMeta) -> Self {
        let InvokeUpstreamMeta {
            status,
            url,
            adapter,
            upstream_wire_ms,
        } = m;
        Self {
            status,
            url,
            adapter,
            upstream_wire_ms,
        }
    }
}

impl From<InvokeResponse> for ProbeInvokeResponse {
    fn from(r: InvokeResponse) -> Self {
        let InvokeResponse {
            success,
            error,
            biz_error,
            payload_json,
            upstream_meta,
            internal_retries,
            retry_reasons,
        } = r;
        Self {
            success,
            error,
            biz_error: biz_error.map(Into::into),
            payload_json,
            upstream_meta: upstream_meta.map(Into::into),
            internal_retries,
            retry_reasons,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> ProbeInvokeRequest {
        ProbeInvokeRequest {
            product_id: "p".into(),
            tenant_id: "t".into(),
            channel_id: "c".into(),
            request_id: "r".into(),
            trace: ProbeTrace {
                request_id: "r".into(),
                trace_id: "tr".into(),
                span_id: "sp".into(),
                parent_span_id: String::new(),
            },
            timeout_ms: 1500,
            payload_json: "{\"a\":1}".into(),
            egress_spec_hash: "eh".into(),
            egress_spec_json: "{}".into(),
            supplier_id: "sid".into(),
            supplier_code: "scode".into(),
            supplier_config_hash: "ch".into(),
            supplier_config_json: "{}".into(),
        }
    }

    fn sample_proto_response() -> InvokeResponse {
        InvokeResponse {
            success: true,
            error: String::new(),
            biz_error: Some(InvokeBizError {
                code: "E1".into(),
                message: "m".into(),
                details_json: "{\"k\":1}".into(),
            }),
            payload_json: "{\"items\":[]}".into(),
            upstream_meta: Some(InvokeUpstreamMeta {
                status: 200,
                url: "http://upstream.invalid/x".into(),
                adapter: "http_json".into(),
                upstream_wire_ms: 59,
            }),
            internal_retries: 1,
            retry_reasons: vec!["auth_rejected:jsonrpc_code".into()],
        }
    }

    #[test]
    fn request_round_trips_and_maps_to_proto() {
        let req = sample_request();
        let text = serde_json::to_string(&req).unwrap();
        let back: ProbeInvokeRequest = serde_json::from_str(&text).unwrap();
        assert_eq!(back, req);

        let proto = InvokeRequest::from(back);
        assert_eq!(proto.channel_id, "c");
        assert_eq!(proto.timeout_ms, 1500);
        let trace = proto.trace.expect("trace");
        assert_eq!(trace.trace_id, "tr");
        assert_eq!(trace.span_id, "sp");
    }

    #[test]
    fn response_from_proto_round_trips() {
        let resp = ProbeInvokeResponse::from(sample_proto_response());
        let meta = resp.upstream_meta.as_ref().expect("meta");
        assert_eq!(meta.upstream_wire_ms, 59);
        assert_eq!(resp.biz_error.as_ref().expect("biz").code, "E1");
        assert_eq!(resp.internal_retries, 1);
        assert_eq!(resp.retry_reasons, vec!["auth_rejected:jsonrpc_code"]);

        let text = serde_json::to_string(&resp).unwrap();
        assert!(text.contains("\"upstream_wire_ms\":59"));
        let back: ProbeInvokeResponse = serde_json::from_str(&text).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn response_without_meta_round_trips() {
        let resp = ProbeInvokeResponse::from(InvokeResponse {
            success: false,
            error: "SSRF blocked".into(),
            biz_error: None,
            payload_json: String::new(),
            upstream_meta: None,
            internal_retries: 0,
            retry_reasons: Vec::new(),
        });
        let text = serde_json::to_string(&resp).unwrap();
        let back: ProbeInvokeResponse = serde_json::from_str(&text).unwrap();
        assert_eq!(back, resp);
        assert!(back.upstream_meta.is_none());
    }

    #[test]
    fn legacy_latency_field_is_rejected() {
        let body = r#"{"success":true,"error":"","biz_error":null,"payload_json":"","upstream_meta":{"status":200,"url":"u","adapter":"a","latency_ms":5}}"#;
        let err = serde_json::from_str::<ProbeInvokeResponse>(body).unwrap_err();
        assert!(err.to_string().contains("latency_ms"), "{err}");
    }

    #[test]
    fn missing_retry_fields_are_rejected() {
        let body = r#"{"success":true,"error":"","biz_error":null,"payload_json":"","upstream_meta":null}"#;
        let err = serde_json::from_str::<ProbeInvokeResponse>(body).unwrap_err();
        assert!(err.to_string().contains("internal_retries"), "{err}");
    }

    #[test]
    fn missing_field_is_rejected() {
        let body = r#"{"success":true,"error":"","biz_error":null,"upstream_meta":null}"#;
        let err = serde_json::from_str::<ProbeInvokeResponse>(body).unwrap_err();
        assert!(err.to_string().contains("payload_json"), "{err}");
    }
}
