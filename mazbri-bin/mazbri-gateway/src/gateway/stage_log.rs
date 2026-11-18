//! Gateway stage 日志采样：热路径降频，慢请求/失败/摘要全量保留。

use std::sync::atomic::{AtomicU64, Ordering};

/// 默认每 N 条 info 阶段日志打 1 条（失败/慢请求仍全量）。
const DEFAULT_SAMPLE_N: u64 = 20;
const SLOW_STAGE_MS: u64 = 10;

static STAGE_SEQ: AtomicU64 = AtomicU64::new(0);

#[inline]
pub fn should_log_stage_info(stage_ms: u64) -> bool {
    if stage_ms >= SLOW_STAGE_MS {
        return true;
    }
    let n = STAGE_SEQ.fetch_add(1, Ordering::Relaxed);
    n.is_multiple_of(DEFAULT_SAMPLE_N)
}

/// 平台摘要、余额拒绝等始终打。
#[inline]
#[must_use]
pub fn always_log_stage() -> bool {
    true
}

/// 阶段 span 的 ID，写进同一阶段日志的 `trace_id` / `span_id`；未启用导出时为空串。
#[derive(Debug, Default, Clone)]
pub struct StageSpanIds {
    pub trace_id: String,
    pub span_id: String,
}

/// 网关阶段 span：名称即 `stage_name`，耗时与阶段日志的 `stage_ms` 同源。
///
/// 阶段日志有采样，span 不采样：每个请求的每个阶段都补记一个已结束的 span。
#[must_use]
pub struct StageSpan {
    span: mazbri_core::trace_spans::CompletedSpan,
    stage: &'static str,
    failed: bool,
}

pub fn stage_span(stage: &'static str, stage_ms: u64) -> StageSpan {
    StageSpan {
        span: mazbri_core::trace_spans::CompletedSpan::from_millis(stage, stage_ms),
        stage,
        failed: false,
    }
}

impl StageSpan {
    pub fn product(mut self, product_id: &str) -> Self {
        self.span = self.span.attr("product_id", product_id.to_string());
        self
    }

    pub fn tenant(mut self, tenant_code: &str) -> Self {
        self.span = self.span.attr("tenant_code", tenant_code.to_string());
        self
    }

    pub fn mode(mut self, mode: &str) -> Self {
        self.span = self.span.attr("mode", mode.to_string());
        self
    }

    /// 本阶段出错：span status=error，`error_stage` 属性取阶段名（与访问日志 `error_stage` 一致）。
    pub fn failed(mut self) -> Self {
        self.failed = true;
        self
    }

    /// 以当前 span 为父节点记录。
    #[must_use]
    pub fn record(self) -> StageSpanIds {
        let parent = mazbri_core::trace_spans::current_context();
        self.record_with_parent(&parent)
    }

    /// 以给定上下文为父节点记录（脱离请求 span 的异步任务用）。
    #[must_use]
    pub fn record_with_parent(
        self,
        parent: &mazbri_core::trace_spans::OtelContext,
    ) -> StageSpanIds {
        let span = if self.failed {
            self.span
                .attr("error_stage", self.stage)
                .error(format!("{} failed", self.stage))
        } else {
            self.span
        };
        span.record(parent)
            .map_or_default(|span_context| StageSpanIds {
                trace_id: span_context.trace_id().to_string(),
                span_id: span_context.span_id().to_string(),
            })
    }
}

/// 阶段日志的公共维度：每条 `gateway.stage` / `gateway.postpath` 行都带同一组租户、产品与路由字段，
/// 未知的（如路由前的阶段没有渠道）记空串 / 0，不再各处手写、各处缺字段。
#[derive(Debug, Clone, Default)]
pub(crate) struct StageCtx {
    pub request_id: String,
    pub tenant_id: String,
    pub tenant_code: String,
    pub product_id: String,
    pub supplier_code: String,
    pub channel_id: String,
    pub channel_code: String,
    pub attempt_order: u32,
}

impl StageCtx {
    #[must_use]
    pub(crate) fn new(
        request_id: &impl std::fmt::Display,
        claims: &crate::auth::claims::MazbriClaims,
        product_id: &str,
    ) -> Self {
        Self {
            request_id: request_id.to_string(),
            tenant_id: claims.tenant_id.clone(),
            tenant_code: claims.tenant_code.clone(),
            product_id: product_id.to_string(),
            ..Self::default()
        }
    }

    /// 从请求扩展取（鉴权之后的中间件用）：JWT claims 与当前请求 ID；缺 claims 时只带租户 ID。
    #[must_use]
    pub(crate) fn from_extensions(
        extensions: &axum::http::Extensions,
        request_id: &impl std::fmt::Display,
        tenant_id: &impl std::fmt::Display,
        product_id: Option<&str>,
    ) -> Self {
        let product_id = product_id.unwrap_or_default();
        extensions
            .get::<crate::auth::claims::MazbriClaims>()
            .map_or_else(
                || Self {
                    request_id: request_id.to_string(),
                    tenant_id: tenant_id.to_string(),
                    product_id: product_id.to_string(),
                    ..Self::default()
                },
                |claims| Self::new(request_id, claims, product_id),
            )
    }

    /// 带上已选定的上游路由（供应商、渠道、第几个候选渠道）。
    #[must_use]
    pub(crate) fn route(mut self, route: Option<&super::execution::UpstreamRouteMetadata>) -> Self {
        if let Some(route) = route {
            self.supplier_code.clone_from(&route.supplier_code);
            self.channel_id.clone_from(&route.channel_id);
            self.channel_code.clone_from(&route.channel_code);
            self.attempt_order = route.attempt_order;
        }
        self
    }
}

/// 统一输出阶段日志：注入 `service`、`stage_name`、`stage_ms`、请求 / 链路 ID 与 [`StageCtx`] 的维度，
/// 调用方只写本阶段特有的字段（不得再重复写上述字段）。
///
/// `stage_log!(info, "gateway.stage", &ctx, &stage_ids, "upstream", ms, result, message = "...")`
macro_rules! stage_log {
    ($level:ident, $target:literal, $ctx:expr, $ids:expr, $name:expr, $ms:expr, $($rest:tt)+) => {{
        let ctx: &$crate::gateway::stage_log::StageCtx = $ctx;
        let ids: &$crate::gateway::stage_log::StageSpanIds = $ids;
        tracing::$level!(
            target: $target,
            service = "mazbri-gateway",
            stage_name = $name,
            stage_ms = $ms,
            request_id = %ctx.request_id,
            trace_id = %ids.trace_id,
            span_id = %ids.span_id,
            tenant_id = %ctx.tenant_id,
            tenant_code = %ctx.tenant_code,
            product_id = %ctx.product_id,
            supplier_code = %ctx.supplier_code,
            channel_id = %ctx.channel_id,
            channel_code = %ctx.channel_code,
            attempt_order = ctx.attempt_order,
            $($rest)+
        )
    }};
}
pub(crate) use stage_log;
