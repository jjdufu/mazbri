//! 启动期下游连接预热：后台任务、指数退避、直到成功。
//!
//! 下游（ratelimit/auth/billing/cache/cbpro/bypass 及直连上游）可能晚于 gateway
//! 就绪，甚至在另一台主机上；预热不能依赖启动顺序，也不能阻塞 gateway 启动。
use crate::config::ServicesConfig;
use crate::http2_upstream_client::UpstreamHttp2ClientPool;
use crate::tarpc_client::TarpcClientPool;
use mazbri_core::{CoreError, ServiceName};
use std::future::Future;
use std::io::ErrorKind;
use std::time::Duration;

/// 退避策略：0.5s 起步、逐次翻倍、上限 30s；前 `warn_after` 次失败记 DEBUG，之后记 WARN。
#[derive(Debug, Clone, Copy)]
pub struct Backoff {
    pub initial: Duration,
    pub max: Duration,
    pub warn_after: u32,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(500),
            max: Duration::from_secs(30),
            warn_after: 5,
        }
    }
}

impl Backoff {
    /// 第 `attempt` 次失败（从 0 开始）后的等待时长。
    #[must_use]
    pub fn delay(&self, attempt: u32) -> Duration {
        let factor = 1u32.checked_shl(attempt.min(16)).unwrap_or(u32::MAX);
        self.initial.saturating_mul(factor).min(self.max)
    }
}

/// 下游暂不可达（DNS 未注册、端口未监听、连接被重置等）视为可重试；配置类错误不重试。
#[must_use]
pub fn is_transient(error: &CoreError) -> bool {
    match error {
        CoreError::Io(io_error) => matches!(
            io_error.kind(),
            ErrorKind::ConnectionRefused
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
                | ErrorKind::BrokenPipe
                | ErrorKind::NotConnected
                | ErrorKind::TimedOut
                | ErrorKind::WouldBlock
                | ErrorKind::Interrupted
                | ErrorKind::UnexpectedEof
        ),
        // DNS 解析失败经 `TlsError::into_core` 映射为 Network。
        CoreError::Network(_) | CoreError::ServiceUnavailable { .. } => true,
        // TCP 建连/握手阶段失败只以字符串形式上抛。
        CoreError::Tls(message) => {
            let message = message.to_ascii_lowercase();
            [
                "connection refused",
                "connection reset",
                "broken pipe",
                "timed out",
                "temporary failure",
                "i/o timeout",
                "tcp connect failed",
            ]
            .iter()
            .any(|needle| message.contains(needle))
        }
        _ => false,
    }
}

/// 反复执行 `op` 直到成功；不可重试错误记 WARN 后放弃。返回最终是否成功。
pub async fn warmup_until_ready<F, Fut>(label: &str, addr: &str, backoff: Backoff, op: F) -> bool
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<(), CoreError>>,
{
    let mut attempt: u32 = 0;
    loop {
        match op().await {
            Ok(()) => {
                tracing::info!(service = %label, addr = %addr, attempts = attempt + 1, "Warmup succeeded");
                return true;
            }
            Err(error) if is_transient(&error) => {
                let delay = backoff.delay(attempt);
                let delay_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX);
                if attempt < backoff.warn_after {
                    tracing::debug!(service = %label, addr = %addr, attempt = attempt + 1, delay_ms, error = %error, "Warmup failed; retrying");
                } else {
                    tracing::warn!(service = %label, addr = %addr, attempt = attempt + 1, delay_ms, error = %error, "Warmup still failing; retrying");
                }
                tokio::time::sleep(delay).await;
                attempt = attempt.saturating_add(1);
            }
            Err(error) => {
                tracing::warn!(service = %label, addr = %addr, error = %error, "Warmup failed with non-retryable error; giving up");
                return false;
            }
        }
    }
}

/// 为每个下游各起一个后台预热任务，立即返回，不阻塞启动。
pub fn spawn_warmups(
    tarpc_pool: &TarpcClientPool,
    upstream_pool: &UpstreamHttp2ClientPool,
    services: &ServicesConfig,
    direct_upstream_urls: Vec<String>,
) {
    let backoff = Backoff::default();
    macro_rules! tarpc_warmup {
        ($svc:expr, $url:expr, $method:ident) => {{
            let pool = tarpc_pool.clone();
            let url = $url.to_owned();
            tokio::spawn(async move {
                warmup_until_ready($svc.as_str(), &url, backoff, || async {
                    pool.$method(&url).await.map(|_| ())
                })
                .await;
            });
        }};
    }
    tarpc_warmup!(
        ServiceName::Ratelimit,
        services.ratelimit.url(),
        get_ratelimit_client
    );
    tarpc_warmup!(ServiceName::Auth, services.auth.url(), get_auth_client);
    tarpc_warmup!(
        ServiceName::Billing,
        services.billing.url(),
        get_billing_client
    );
    tarpc_warmup!(ServiceName::Cache, services.cache.url(), get_cache_client);
    if let Some(cbpro) = services.cbpro.as_ref() {
        tarpc_warmup!(ServiceName::Cbpro, cbpro.url(), get_http_upstream_client);
    }

    let mut http2_targets: Vec<(String, String)> = direct_upstream_urls
        .into_iter()
        .map(|url| ("route-upstream".to_owned(), url))
        .collect();
    if let Some(bypass) = services.bypass.as_ref() {
        // transparent 出站走 bypass：预热与 execute 相同的 endpoint 形式，避免首包冷握手。
        http2_targets.push((
            ServiceName::Bypass.as_str().to_owned(),
            format!("https://{}", bypass.url()),
        ));
    } else {
        tracing::info!("No bypass_url configured; skip bypass HTTP/2 warmup");
    }
    for (label, endpoint) in http2_targets {
        let pool = upstream_pool.clone();
        tokio::spawn(async move {
            warmup_until_ready(&label, &endpoint, backoff, || {
                pool.warmup_endpoint(&endpoint)
            })
            .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn backoff_doubles_and_caps() {
        let b = Backoff::default();
        let secs: Vec<f64> = (0..8).map(|i| b.delay(i).as_secs_f64()).collect();
        assert_eq!(secs, vec![0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 30.0, 30.0]);
        assert_eq!(b.delay(u32::MAX), Duration::from_secs(30));
    }

    #[test]
    fn dns_and_connect_failures_are_transient() {
        let dns = mazbri_tls::TlsError::Resolve("Failed to resolve 'mazbri-ratelimit:8888'".into());
        assert!(is_transient(&dns.into_core()));
        assert!(is_transient(&CoreError::Tls(
            "TCP connect failed: Connection refused".into()
        )));
        assert!(is_transient(&CoreError::Io(std::io::Error::from(
            ErrorKind::ConnectionRefused
        ))));
        assert!(!is_transient(&CoreError::Config("missing port".into())));
        assert!(!is_transient(&CoreError::Tls(
            "invalid peer certificate".into()
        )));
    }

    fn fast() -> Backoff {
        Backoff {
            initial: Duration::from_millis(1),
            max: Duration::from_millis(2),
            warn_after: 1,
        }
    }

    #[tokio::test]
    async fn retries_until_success() {
        let calls = Arc::new(AtomicU32::new(0));
        let ok = warmup_until_ready("svc", "x:1", fast(), || {
            let calls = Arc::clone(&calls);
            async move {
                if calls.fetch_add(1, Ordering::SeqCst) < 4 {
                    Err(CoreError::Network("not yet".into()))
                } else {
                    Ok(())
                }
            }
        })
        .await;
        assert!(ok);
        assert_eq!(calls.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn gives_up_on_non_transient() {
        let calls = Arc::new(AtomicU32::new(0));
        let ok = warmup_until_ready("svc", "x:1", fast(), || {
            let calls = Arc::clone(&calls);
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Err(CoreError::Config("bad".into()))
            }
        })
        .await;
        assert!(!ok);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
