//! Client disconnect observation (step 1 of transport disconnect governance).
//!
//! When a request future is cancelled (client gone before handler returns) or the
//! response body is dropped before it is fully written, we emit metrics + stage logs.

use super::metrics::record_client_disconnect;
use axum::body::{Body, Bytes, HttpBody};
use axum::extract::ConnectInfo;
use axum::http::{Request, header};
use axum::middleware::Next;
use axum::response::Response;
use http_body::Frame;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

/// Tracks whether the handler completed; Drop without complete → handler-phase disconnect.
struct HandlerDisconnectGuard {
    completed: Arc<AtomicBool>,
    peer: String,
    request_id: String,
}

impl HandlerDisconnectGuard {
    fn new(peer: String, request_id: String) -> Self {
        Self {
            completed: Arc::new(AtomicBool::new(false)),
            peer,
            request_id,
        }
    }

    fn mark_completed(&self) {
        self.completed.store(true, Ordering::SeqCst);
    }
}

impl Drop for HandlerDisconnectGuard {
    fn drop(&mut self) {
        if !self.completed.load(Ordering::SeqCst) {
            record_client_disconnect(
                "handler",
                "canceled",
                Some(self.peer.as_str()),
                Some(self.request_id.as_str()),
                Some("request future cancelled before handler completed"),
            );
        }
    }
}

/// Body wrapper: Drop before end-of-stream → response-phase disconnect.
struct TrackedBody {
    inner: Body,
    finished: bool,
    peer: String,
    request_id: String,
}

impl TrackedBody {
    fn new(inner: Body, peer: String, request_id: String) -> Self {
        Self {
            inner,
            finished: false,
            peer,
            request_id,
        }
    }
}

impl HttpBody for TrackedBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        // Body is Unpin; project via get_mut.
        let this = self.as_mut().get_mut();
        match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(None) => {
                this.finished = true;
                Poll::Ready(None)
            }
            Poll::Ready(Some(Ok(frame))) => Poll::Ready(Some(Ok(frame))),
            Poll::Ready(Some(Err(err))) => {
                this.finished = true;
                let reason = classify_body_error(&err);
                let err_msg = err.to_string();
                record_client_disconnect(
                    "response",
                    reason,
                    Some(this.peer.as_str()),
                    Some(this.request_id.as_str()),
                    Some(&err_msg),
                );
                Poll::Ready(Some(Err(err)))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.finished || self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for TrackedBody {
    fn drop(&mut self) {
        if !self.finished && !self.inner.is_end_stream() {
            record_client_disconnect(
                "response",
                "canceled",
                Some(self.peer.as_str()),
                Some(self.request_id.as_str()),
                Some("response body dropped before end of stream"),
            );
        }
    }
}

fn classify_body_error(err: &axum::Error) -> &'static str {
    let msg = err.to_string().to_ascii_lowercase();
    if msg.contains("reset") {
        "reset"
    } else if msg.contains("timed out") || msg.contains("timeout") {
        "timeout"
    } else if msg.contains("broken pipe") || msg.contains("connection abort") {
        "eof"
    } else if msg.contains("canceled") || msg.contains("cancelled") {
        "canceled"
    } else {
        "unknown"
    }
}

fn extract_request_id(req: &Request<Body>) -> String {
    req.headers()
        .get(header::HeaderName::from_static("x-request-id"))
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .unwrap_or_default()
}

/// Middleware: observe client disconnect during handler and response write.
pub async fn client_disconnect_middleware(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let peer_str = peer.to_string();
    let request_id = extract_request_id(&req);
    let guard = HandlerDisconnectGuard::new(peer_str.clone(), request_id.clone());
    let response = next.run(req).await;
    guard.mark_completed();

    let (parts, body) = response.into_parts();
    let tracked = TrackedBody::new(body, peer_str, request_id);
    Response::from_parts(parts, Body::new(tracked))
}
