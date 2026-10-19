//! 管理端 URL SSRF 预检：与数据面 `mazbri_upstream_runtime::SsrfChecker` 统一。

use mazbri_upstream_runtime::SsrfChecker;

/// 同步 DNS + 私网拦截（实现委托给共享 runtime checker）。
pub async fn validate_public_http_url(raw_url: &str) -> Result<(), String> {
    SsrfChecker::new().validate_url(raw_url).await.map_err(|e| {
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
    use mazbri_upstream_runtime::is_blocked_ip;

    #[test]
    fn blocks_loopback_literal() {
        assert!(is_blocked_ip("127.0.0.1".parse().unwrap()));
        assert!(is_blocked_ip("10.0.0.1".parse().unwrap()));
        assert!(is_blocked_ip("192.168.1.1".parse().unwrap()));
        assert!(!is_blocked_ip("8.8.8.8".parse().unwrap()));
    }
}
