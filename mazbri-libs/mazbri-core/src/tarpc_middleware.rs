use uuid::Uuid;
#[derive(Debug, thiserror::Error)]
pub enum TenantValidationError {
    #[error("tenant_id is empty or blank")]
    Empty,
    #[error("tenant_id is not a valid UUID: {0}")]
    InvalidFormat(String),
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_tenant_id(tenant_id: &str) -> Result<Uuid, TenantValidationError> {
    let trimmed = tenant_id.trim();
    if trimmed.is_empty() {
        return Err(TenantValidationError::Empty);
    }
    Uuid::parse_str(trimmed)
        .map_err(|e| TenantValidationError::InvalidFormat(format!("{tenant_id}: {e}")))
}

/// tarpc 链路透传。
///
/// tarpc 0.35 依赖的 OpenTelemetry 版本与本仓库不同，它自己的 `Context::try_from(&Span)`
/// 永远失败并退回 `new_child()`（随机 `span_id`）。这里在传输层修正：
/// - 客户端：发送请求时 tarpc 已进入调用方 RPC span，用该 span 的 ID 覆盖 `trace_context`；
/// - 服务端：收到请求时按 `(trace_id, deadline)` 记下远端父 span（`new_child` 只改 `span_id`，
///   这两个字段原样到达处理函数），在服务端 RPC span 内取回并设为父节点。
pub mod trace {
    use crate::trace_propagation::{
        current_span_context, set_remote_parent, span_context_from_tarpc, tarpc_trace_context,
    };
    use futures::{Sink, Stream};
    use opentelemetry::trace::SpanContext;
    use std::collections::{HashMap, VecDeque};
    use std::pin::Pin;
    use std::sync::{Arc, Mutex, PoisonError};
    use std::task::{Context, Poll};
    use std::time::Instant;
    use tarpc::server::{BaseChannel, Channel, Serve};
    use tarpc::{ClientMessage, Response, ServerError, context};

    pin_project_lite::pin_project! {
        /// 客户端传输包装：把当前 RPC span 写入出站请求的 tarpc 上下文。
        pub struct ClientTraceTransport<T> {
            #[pin]
            inner: T,
        }
    }

    impl<T> ClientTraceTransport<T> {
        pub fn new(inner: T) -> Self {
            Self { inner }
        }
    }

    impl<T: Stream> Stream for ClientTraceTransport<T> {
        type Item = T::Item;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.project().inner.poll_next(cx)
        }
    }

    impl<Req, T: Sink<ClientMessage<Req>>> Sink<ClientMessage<Req>> for ClientTraceTransport<T> {
        type Error = T::Error;

        fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), T::Error>> {
            self.project().inner.poll_ready(cx)
        }

        fn start_send(self: Pin<&mut Self>, mut item: ClientMessage<Req>) -> Result<(), T::Error> {
            if let ClientMessage::Request(request) = &mut item
                && let Some(span_context) = current_span_context()
            {
                request.context.trace_context = tarpc_trace_context(&span_context);
            }
            self.project().inner.start_send(item)
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), T::Error>> {
            self.project().inner.poll_flush(cx)
        }

        fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), T::Error>> {
            self.project().inner.poll_close(cx)
        }
    }

    type ParentKey = (u128, Instant);

    /// 单连接内尚未被处理函数取走的远端父 span；容量有限，超出时丢弃最早的记录。
    #[derive(Clone, Default)]
    pub struct RemoteParents {
        inner: Arc<Mutex<ParentSlots>>,
    }

    #[derive(Default)]
    struct ParentSlots {
        by_key: HashMap<ParentKey, SpanContext>,
        order: VecDeque<ParentKey>,
    }

    const MAX_PENDING_PARENTS: usize = 4096;

    fn key_of(ctx: &context::Context) -> ParentKey {
        (u128::from(ctx.trace_context.trace_id), ctx.deadline)
    }

    impl RemoteParents {
        fn remember(&self, ctx: &context::Context) {
            let Some(parent) = span_context_from_tarpc(&ctx.trace_context) else {
                return;
            };
            let key = key_of(ctx);
            let mut slots = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            if slots.by_key.insert(key, parent).is_none() {
                slots.order.push_back(key);
            }
            while slots.order.len() > MAX_PENDING_PARENTS {
                if let Some(oldest) = slots.order.pop_front() {
                    slots.by_key.remove(&oldest);
                }
            }
        }

        fn take(&self, ctx: &context::Context) -> Option<SpanContext> {
            let key = key_of(ctx);
            let mut slots = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            let parent = slots.by_key.remove(&key)?;
            slots.order.retain(|pending| *pending != key);
            Some(parent)
        }
    }

    pin_project_lite::pin_project! {
        /// 服务端传输包装：在 tarpc 改写 `span_id` 之前记下入站请求的远端父 span。
        pub struct ServerTraceTransport<T> {
            #[pin]
            inner: T,
            parents: RemoteParents,
        }
    }

    impl<T> ServerTraceTransport<T> {
        pub fn new(inner: T, parents: RemoteParents) -> Self {
            Self { inner, parents }
        }
    }

    impl<Req, E, T> Stream for ServerTraceTransport<T>
    where
        T: Stream<Item = Result<ClientMessage<Req>, E>>,
    {
        type Item = T::Item;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let this = self.project();
            let polled = this.inner.poll_next(cx);
            if let Poll::Ready(Some(Ok(ClientMessage::Request(request)))) = &polled {
                this.parents.remember(&request.context);
            }
            polled
        }
    }

    impl<Item, T: Sink<Item>> Sink<Item> for ServerTraceTransport<T> {
        type Error = T::Error;

        fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), T::Error>> {
            self.project().inner.poll_ready(cx)
        }

        fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), T::Error> {
            self.project().inner.start_send(item)
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), T::Error>> {
            self.project().inner.poll_flush(cx)
        }

        fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), T::Error>> {
            self.project().inner.poll_close(cx)
        }
    }

    /// 在服务端 RPC span 内把远端父 span 接回去，再交给真正的服务实现。
    #[derive(Clone)]
    pub struct TracedServe<S> {
        inner: S,
        parents: RemoteParents,
    }

    impl<S: Serve> Serve for TracedServe<S> {
        type Req = S::Req;
        type Resp = S::Resp;

        async fn serve(self, ctx: context::Context, req: S::Req) -> Result<S::Resp, ServerError> {
            if let Some(parent) = self.parents.take(&ctx) {
                set_remote_parent(&tracing::Span::current(), parent);
            }
            self.inner.serve(ctx, req).await
        }
    }

    /// 以链路透传方式服务一条 tarpc 连接（替代 `BaseChannel::with_defaults(t).execute(s)`）。
    pub fn execute_traced<T, S>(
        transport: T,
        serve: S,
    ) -> impl Stream<Item = impl Future<Output = ()>>
    where
        S: Serve + Clone,
        T: tarpc::Transport<Response<S::Resp>, ClientMessage<S::Req>>,
    {
        let parents = RemoteParents::default();
        BaseChannel::with_defaults(ServerTraceTransport::new(transport, parents.clone())).execute(
            TracedServe {
                inner: serve,
                parents,
            },
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::time::Duration;

        fn ctx(trace: u128, span: u64, deadline: Instant) -> context::Context {
            let mut ctx = context::current();
            ctx.deadline = deadline;
            ctx.trace_context = tarpc::trace::Context {
                trace_id: trace.into(),
                span_id: span.into(),
                sampling_decision: tarpc::trace::SamplingDecision::Sampled,
            };
            ctx
        }

        #[test]
        fn parent_survives_tarpc_new_child() {
            let parents = RemoteParents::default();
            let deadline = Instant::now() + Duration::from_secs(5);
            let incoming = ctx(7, 42, deadline);
            parents.remember(&incoming);
            // tarpc 服务端 start_request 只把 `span_id` 换成随机值。
            let mut handler_ctx = incoming;
            handler_ctx.trace_context.span_id = 9_999.into();
            let parent = parents.take(&handler_ctx).expect("parent recorded");
            assert_eq!(u64::from_be_bytes(parent.span_id().to_bytes()), 42);
            assert!(parent.is_remote());
            assert!(parents.take(&handler_ctx).is_none());
        }

        #[test]
        fn pending_parents_are_bounded() {
            let parents = RemoteParents::default();
            let base = Instant::now();
            let total = MAX_PENDING_PARENTS + 10;
            for i in 0..total {
                let offset = Duration::from_millis(u64::try_from(i).expect("small index"));
                parents.remember(&ctx(1, 1, base + offset));
            }
            let slots = parents.inner.lock().expect("lock");
            assert_eq!(slots.by_key.len(), MAX_PENDING_PARENTS);
            assert_eq!(slots.order.len(), MAX_PENDING_PARENTS);
        }
    }
}
