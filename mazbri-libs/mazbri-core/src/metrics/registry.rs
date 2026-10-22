use prometheus::core::Collector;
use prometheus::{Error as PrometheusError, Registry};
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
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
