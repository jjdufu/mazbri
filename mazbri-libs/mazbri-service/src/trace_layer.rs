//! 统一的 HTTP `TraceLayer`：探针接口（`/health`、`/ready`）返回 503 是预期的状态上报，
//! 不归类为失败，因此 tower-http 不会为其打 ERROR 级 "response failed"；
//! 其它接口的 5xx 仍按 `ServerErrorsAsFailures` 记为失败（ERROR）。

use axum::http::{Request, Response, StatusCode};
use std::fmt::Display;
use tower_http::classify::{
    ClassifiedResponse, ClassifyResponse, MakeClassifier, NeverClassifyEos, ServerErrorsAsFailures,
    ServerErrorsFailureClass,
};
use tower_http::trace::TraceLayer;

/// 探针接口返回 503 属于预期的状态上报，不是服务端故障。
pub(crate) fn is_probe_unavailable(path: &str, status: StatusCode) -> bool {
    status == StatusCode::SERVICE_UNAVAILABLE && matches!(path, "/health" | "/ready")
}

/// 按请求路径生成分类器：探针路径的 503 视为成功，其余沿用 `ServerErrorsAsFailures`。
#[derive(Clone, Copy, Debug, Default)]
pub struct ProbeAwareMakeClassifier;

/// 单个请求的分类器，见 [`ProbeAwareMakeClassifier`]。
#[derive(Clone, Copy, Debug)]
pub struct ProbeAwareClassifier {
    probe: bool,
}

impl MakeClassifier for ProbeAwareMakeClassifier {
    type Classifier = ProbeAwareClassifier;
    type FailureClass = ServerErrorsFailureClass;
    type ClassifyEos = NeverClassifyEos<ServerErrorsFailureClass>;

    fn make_classifier<B>(&self, req: &Request<B>) -> Self::Classifier {
        ProbeAwareClassifier {
            probe: matches!(req.uri().path(), "/health" | "/ready"),
        }
    }
}

impl ClassifyResponse for ProbeAwareClassifier {
    type FailureClass = ServerErrorsFailureClass;
    type ClassifyEos = NeverClassifyEos<ServerErrorsFailureClass>;

    fn classify_response<B>(
        self,
        res: &Response<B>,
    ) -> ClassifiedResponse<Self::FailureClass, Self::ClassifyEos> {
        if self.probe && res.status() == StatusCode::SERVICE_UNAVAILABLE {
            return ClassifiedResponse::Ready(Ok(()));
        }
        ServerErrorsAsFailures::new().classify_response(res)
    }

    fn classify_error<E: Display + 'static>(self, error: &E) -> Self::FailureClass {
        ServerErrorsAsFailures::new().classify_error(error)
    }
}

/// 替代 `TraceLayer::new_for_http()`，所有 HTTP 服务统一使用。
#[must_use]
pub fn http_trace_layer() -> TraceLayer<ProbeAwareMakeClassifier> {
    TraceLayer::new(ProbeAwareMakeClassifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(path: &str, status: StatusCode) -> bool {
        let req = Request::builder().uri(path).body(()).unwrap();
        let res = Response::builder().status(status).body(()).unwrap();
        match ProbeAwareMakeClassifier
            .make_classifier(&req)
            .classify_response(&res)
        {
            ClassifiedResponse::Ready(r) => r.is_ok(),
            ClassifiedResponse::RequiresEos(_) => unreachable!(),
        }
    }

    #[test]
    fn probe_503_is_not_failure() {
        assert!(classify("/health", StatusCode::SERVICE_UNAVAILABLE));
        assert!(classify("/ready", StatusCode::SERVICE_UNAVAILABLE));
        assert!(classify("/health?x=1", StatusCode::SERVICE_UNAVAILABLE));
    }

    #[test]
    fn other_5xx_still_failure() {
        assert!(!classify("/health", StatusCode::INTERNAL_SERVER_ERROR));
        assert!(!classify("/v1/chat", StatusCode::SERVICE_UNAVAILABLE));
        assert!(!classify("/healthz", StatusCode::SERVICE_UNAVAILABLE));
        assert!(classify("/v1/chat", StatusCode::OK));
        assert!(classify("/v1/chat", StatusCode::NOT_FOUND));
    }

    #[test]
    fn probe_predicate() {
        assert!(is_probe_unavailable(
            "/ready",
            StatusCode::SERVICE_UNAVAILABLE
        ));
        assert!(!is_probe_unavailable("/ready", StatusCode::BAD_GATEWAY));
    }
}
