use rand::RngExt;
use std::time::Duration;
use tracing::warn;
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub initial_delay_ms: u64,
    pub max_retries: u32,
    pub multiplier: u32,
}
impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            initial_delay_ms: 100,
            max_retries: 3,
            multiplier: 2,
        }
    }
}
pub const DEFAULT_RETRY_CONFIG: RetryConfig = RetryConfig {
    initial_delay_ms: 100,
    max_retries: 3,
    multiplier: 2,
};
pub trait IsTransient {
    fn is_transient(&self) -> bool;
}
impl IsTransient for tarpc::client::RpcError {
    fn is_transient(&self) -> bool {
        !matches!(self, tarpc::client::RpcError::Server(_))
    }
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn retry_rpc<F, Fut, T, E>(config: &RetryConfig, f: F) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display + IsTransient,
{
    let mut attempt: u32 = 0;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                if e.is_transient() && attempt < config.max_retries {
                    let cap = config.initial_delay_ms * u64::from(config.multiplier).pow(attempt);
                    let delay_ms = rand::rng().random_range(0..=cap);
                    warn!(
                        attempt = attempt + 1, max_retries = config.max_retries,
                        delay_ms, error = % e, "Transient RPC error, retrying"
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    attempt += 1;
                } else {
                    return Err(e);
                }
            }
        }
    }
}
