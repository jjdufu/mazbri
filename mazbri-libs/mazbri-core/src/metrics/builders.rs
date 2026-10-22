use prometheus::{CounterVec, HistogramOpts, HistogramVec, IntGauge, IntGaugeVec, Opts};
use std::sync::atomic::{AtomicBool, Ordering};
static FALLBACK_WARNED: AtomicBool = AtomicBool::new(false);
pub struct MetricsBuilder {
    prefix: Option<String>,
}
impl MetricsBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self { prefix: None }
    }
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }
    #[must_use]
    pub fn counter(&self, name: &str, help: &str, labels: &[&str]) -> CounterVec {
        let full_name = self.full_name(name);
        let fallback_name = format!("{full_name}_fallback");
        CounterVec::new(Opts::new(&full_name, help), labels).unwrap_or_else(|e| {
            if !FALLBACK_WARNED.load(Ordering::Relaxed) {
                tracing::warn!(
                    "Failed to create counter '{}', using fallback: {}",
                    full_name,
                    e
                );
            }
            CounterVec::new(Opts::new(&fallback_name, help), labels)
                .expect("Fallback metric creation should never fail")
        })
    }
    #[must_use]
    pub fn gauge_vec(&self, name: &str, help: &str, labels: &[&str]) -> IntGaugeVec {
        let full_name = self.full_name(name);
        let fallback_name = format!("{full_name}_fallback");
        IntGaugeVec::new(Opts::new(&full_name, help), labels).unwrap_or_else(|e| {
            if !FALLBACK_WARNED.load(Ordering::Relaxed) {
                tracing::warn!(
                    "Failed to create gauge '{}', using fallback: {}",
                    full_name,
                    e
                );
            }
            IntGaugeVec::new(Opts::new(&fallback_name, help), labels)
                .expect("Fallback metric creation should never fail")
        })
    }
    #[must_use]
    pub fn gauge(&self, name: &str, help: &str) -> IntGauge {
        let full_name = self.full_name(name);
        let fallback_name = format!("{full_name}_fallback");
        IntGauge::new(&full_name, help).unwrap_or_else(|e| {
            if !FALLBACK_WARNED.load(Ordering::Relaxed) {
                tracing::warn!(
                    "Failed to create gauge '{}', using fallback: {}",
                    full_name,
                    e
                );
            }
            IntGauge::new(&fallback_name, help).expect("Fallback metric creation should never fail")
        })
    }
    #[must_use]
    pub fn histogram(
        &self,
        name: &str,
        help: &str,
        buckets: Vec<f64>,
        labels: &[&str],
    ) -> HistogramVec {
        let full_name = self.full_name(name);
        let fallback_name = format!("{full_name}_fallback");
        HistogramVec::new(
            HistogramOpts::new(&full_name, help).buckets(buckets),
            labels,
        )
        .unwrap_or_else(|e| {
            if !FALLBACK_WARNED.load(Ordering::Relaxed) {
                tracing::warn!(
                    "Failed to create histogram '{}', using fallback: {}",
                    full_name,
                    e
                );
            }
            HistogramVec::new(HistogramOpts::new(&fallback_name, help), labels)
                .expect("Fallback metric creation should never fail")
        })
    }
    fn full_name(&self, name: &str) -> String {
        match &self.prefix {
            Some(p) => format!("{p}_{name}"),
            None => name.to_string(),
        }
    }
}
impl Default for MetricsBuilder {
    fn default() -> Self {
        Self::new()
    }
}
