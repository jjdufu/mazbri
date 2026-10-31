//! OTLP span export health: spans this process failed to hand to the collector.
use super::{MetricsBuilder, register_collector_once};
use crate::REGISTRY;
use prometheus::CounterVec;
use std::sync::LazyLock;

/// Why a span was dropped before reaching the collector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanDropReason {
    /// Export queue at `max_queue_size`.
    QueueFull,
    /// Exporter returned an error for the batch.
    ExportFailed,
}

impl SpanDropReason {
    const ALL: [Self; 2] = [Self::QueueFull, Self::ExportFailed];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::QueueFull => "queue_full",
            Self::ExportFailed => "export_failed",
        }
    }
}

pub static OTLP_SPANS_DROPPED_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "mazbri_otlp_spans_dropped_total",
        "Spans dropped before OTLP export (queue full or export error)",
        &["service", "reason"],
    )
});

/// Register the OTLP drop counter and publish zero series (idempotent).
pub fn register_otlp_metrics(service: &'static str) {
    if let Err(error) =
        register_collector_once(&REGISTRY, Box::new(OTLP_SPANS_DROPPED_TOTAL.clone()))
    {
        tracing::warn!(%error, "mazbri_otlp_spans_dropped_total registration failed");
    }
    for reason in SpanDropReason::ALL {
        OTLP_SPANS_DROPPED_TOTAL.with_label_values(&[service, reason.as_str()]);
    }
}

/// Count `spans` dropped for `reason`.
pub fn record_spans_dropped(reason: SpanDropReason, spans: usize) {
    if spans == 0 {
        return;
    }
    let count = u32::try_from(spans).map_or(f64::from(u32::MAX), f64::from);
    OTLP_SPANS_DROPPED_TOTAL
        .with_label_values(&[super::dep_client::current_service(), reason.as_str()])
        .inc_by(count);
}
