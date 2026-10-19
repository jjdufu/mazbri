use prometheus::core::Collector;
use prometheus::{Error as PrometheusError, Registry};
pub fn register_collector_once(
    registry: &Registry,
    collector: Box<dyn Collector>,
) -> Result<(), PrometheusError> {
    match registry.register(collector) {
        Ok(()) => Ok(()),
        Err(PrometheusError::AlreadyReg) => Ok(()),
        Err(error) => Err(error),
    }
}
