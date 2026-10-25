//! 管理端 URL SSRF 预检：与数据面 `mazbri_upstream_runtime::SsrfChecker` 统一。

use mazbri_upstream_runtime::SsrfChecker;

/// 同步 DNS + 私网拦截（实现委托给共享 runtime checker）。
///
/// `ssrf` 来自 mgmt `[ssrf]` 配置（`AppState::connector_runtime.ssrf()`），
/// 与数据面 bypass 使用同一语义：默认拦截私网/回环；开启后放行私网/回环，
/// 链路本地与云元数据端点始终拦截。
///
/// # Errors
///
/// URL 非法、DNS 解析失败或目标地址被 SSRF 策略拦截时返回错误。
pub async fn validate_public_http_url(ssrf: &SsrfChecker, raw_url: &str) -> Result<(), String> {
    ssrf.validate_url(raw_url).await.map_err(|e| {
        // Keep admin-facing Chinese-friendly messages for common cases.
        if e.contains("SSRF") || e.contains("private") || e.contains("internal") {
            e.replace("SSRF Violation:", "SSRF 拦截:")
                .replace("Target IP", "目标 IP")
                .replace(
                    "resides in internal private subnet",
                    "属于内网/链路本地/未指定地址",
                )
                .replace("forbidden host", "禁止访问主机")
                .replace("Invalid URL structure:", "URL 无效:")
                .replace(
                    "URL scheme must be http/https, got",
                    "URL scheme 必须是 http/https，当前为",
                )
                .replace("Missing host string in URL", "URL 缺少 host")
                .replace("DNS resolution failed for", "DNS 解析失败")
        } else {
            e
        }
    })
}

#[cfg(test)]
mod tests {
    use super::validate_public_http_url;
    use mazbri_upstream_runtime::{SsrfChecker, SsrfConfig, is_blocked_ip};

    #[tokio::test]
    async fn default_blocks_private_and_loopback() {
        let checker = SsrfConfig::default().checker();
        for url in [
            "http://127.0.0.1:8080/api",
            "http://10.0.0.8/api",
            "http://192.168.10.2:9000/",
            "http://localhost/",
        ] {
            let err = validate_public_http_url(&checker, url)
                .await
                .expect_err(url);
            assert!(err.contains("SSRF 拦截"), "{url}: {err}");
        }
        assert!(
            validate_public_http_url(&checker, "http://8.8.8.8/")
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn allow_private_network_permits_private_and_loopback() {
        let checker = SsrfConfig {
            allow_private_network: true,
        }
        .checker();
        for url in [
            "http://127.0.0.1:8080/api",
            "http://10.0.0.8/api",
            "http://192.168.10.2:9000/",
            "http://[::1]:8080/",
        ] {
            assert!(
                validate_public_http_url(&checker, url).await.is_ok(),
                "{url} should be allowed"
            );
        }
    }

    #[tokio::test]
    async fn allow_private_network_still_blocks_metadata() {
        let checker = SsrfChecker::with_allow_private(true);
        for url in [
            "http://169.254.169.254/latest/meta-data/",
            "http://100.100.100.200/latest/meta-data/",
            "http://metadata.google.internal/computeMetadata/v1/",
        ] {
            let err = validate_public_http_url(&checker, url)
                .await
                .expect_err(url);
            assert!(err.contains("SSRF 拦截"), "{url}: {err}");
        }
    }

    #[test]
    fn blocks_loopback_literal() {
        assert!(is_blocked_ip("127.0.0.1".parse().unwrap()));
        assert!(is_blocked_ip("10.0.0.1".parse().unwrap()));
        assert!(is_blocked_ip("192.168.1.1".parse().unwrap()));
        assert!(!is_blocked_ip("8.8.8.8".parse().unwrap()));
    }
}
