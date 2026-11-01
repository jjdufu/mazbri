use mazbri_core::metrics::{MetricsBuilder, REGISTRY, register_collector_once};
use prometheus::{CounterVec, Error as PrometheusError, GaugeVec, Opts};
use std::sync::LazyLock;
pub static ANALYTICS_LOG_INGESTION_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "analytics_log_ingestion_total",
        "Total number of logs ingested by source",
        &["source"],
    )
});

/// 当前 SLI（0..1），标签 `slo` 为定义名。
pub static MAZBRI_SLO_SLI: LazyLock<GaugeVec> = LazyLock::new(|| {
    GaugeVec::new(
        Opts::new("mazbri_slo_sli", "Current SLI ratio for an SLO definition"),
        &["slo"],
    )
    .expect("metric")
});

/// 剩余错误预算比例（可为负），标签 `slo`。
pub static MAZBRI_SLO_ERROR_BUDGET_REMAINING: LazyLock<GaugeVec> = LazyLock::new(|| {
    GaugeVec::new(
        Opts::new(
            "mazbri_slo_error_budget_remaining",
            "Remaining error budget ratio for an SLO (1=full)",
        ),
        &["slo"],
    )
    .expect("metric")
});

/// 快烧（page）条件是否成立：1/0。
pub static MAZBRI_SLO_BURN_PAGE: LazyLock<GaugeVec> = LazyLock::new(|| {
    GaugeVec::new(
        Opts::new(
            "mazbri_slo_burn_page",
            "Fast-burn (page) multi-window condition (1=firing)",
        ),
        &["slo"],
    )
    .expect("metric")
});

/// 慢烧（ticket）条件是否成立：1/0。
pub static MAZBRI_SLO_BURN_TICKET: LazyLock<GaugeVec> = LazyLock::new(|| {
    GaugeVec::new(
        Opts::new(
            "mazbri_slo_burn_ticket",
            "Slow-burn (ticket) multi-window condition (1=firing)",
        ),
        &["slo"],
    )
    .expect("metric")
});

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_analytics_metrics() -> Result<(), PrometheusError> {
    register_collector_once(&REGISTRY, Box::new(ANALYTICS_LOG_INGESTION_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(MAZBRI_SLO_SLI.clone()))?;
    register_collector_once(&REGISTRY, Box::new(MAZBRI_SLO_ERROR_BUDGET_REMAINING.clone()))?;
    register_collector_once(&REGISTRY, Box::new(MAZBRI_SLO_BURN_PAGE.clone()))?;
    register_collector_once(&REGISTRY, Box::new(MAZBRI_SLO_BURN_TICKET.clone()))?;
    Ok(())
}
