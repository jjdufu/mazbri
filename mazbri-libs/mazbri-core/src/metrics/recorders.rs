use prometheus::{CounterVec, HistogramVec};
pub trait MetricsRecorder {
    fn record_success(&self);
    fn record_failure(&self);
    fn observe_duration(&self, duration_secs: f64);
}
pub struct CounterRecorder {
    counter: &'static CounterVec,
    label_values: Vec<&'static str>,
}
impl CounterRecorder {
    #[must_use]
    pub fn new(counter: &'static CounterVec, label_values: &[&'static str]) -> Self {
        Self {
            counter,
            label_values: label_values.to_vec(),
        }
    }
    pub fn inc(&self) {
        self.counter.with_label_values(&self.label_values).inc();
    }
    pub fn inc_by(&self, value: f64) {
        self.counter
            .with_label_values(&self.label_values)
            .inc_by(value);
    }
}
impl MetricsRecorder for CounterRecorder {
    fn record_success(&self) {
        self.inc();
    }
    fn record_failure(&self) {
        self.inc();
    }
    fn observe_duration(&self, _duration_secs: f64) {}
}
pub struct HistogramRecorder {
    histogram: &'static HistogramVec,
    label_values: Vec<&'static str>,
}
impl HistogramRecorder {
    #[must_use]
    pub fn new(histogram: &'static HistogramVec, label_values: &[&'static str]) -> Self {
        Self {
            histogram,
            label_values: label_values.to_vec(),
        }
    }
    pub fn observe(&self, value: f64) {
        self.histogram
            .with_label_values(&self.label_values)
            .observe(value);
    }
    pub fn start_timer(&self) -> prometheus::HistogramTimer {
        self.histogram
            .with_label_values(&self.label_values)
            .start_timer()
    }
}
impl MetricsRecorder for HistogramRecorder {
    fn record_success(&self) {}
    fn record_failure(&self) {}
    fn observe_duration(&self, duration_secs: f64) {
        self.observe(duration_secs);
    }
}
