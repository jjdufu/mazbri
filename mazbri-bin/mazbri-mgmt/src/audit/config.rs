//! `[audit]` 配置段：可信代理、异步写入参数与下游操作人声明签名密钥。

use serde::Deserialize;

const fn default_queue_capacity() -> usize {
    10_000
}
const fn default_batch_size() -> usize {
    100
}
const fn default_flush_interval_ms() -> u64 {
    200
}
const fn default_settings_cache_ttl_secs() -> u64 {
    30
}
const fn default_shutdown_drain_secs() -> u64 {
    5
}
const fn default_actor_token_ttl_secs() -> i64 {
    30
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditConfig {
    /// 可信反向代理（IP 或 CIDR）。仅当 TCP 对端命中时才解析 `X-Forwarded-For`；缺省为空。
    #[serde(default)]
    pub trusted_proxies: Vec<String>,
    /// 异步写入队列容量；队列满时记录写入兜底文件。
    #[serde(default = "default_queue_capacity")]
    pub queue_capacity: usize,
    /// 单批落库最大条数。
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// 攒批最长等待（毫秒）。
    #[serde(default = "default_flush_interval_ms")]
    pub flush_interval_ms: u64,
    /// 落库失败时的兜底 JSONL 文件；缺省为 `<tracing.log_dir>/mgmt-audit-fallback.jsonl`。
    #[serde(default)]
    pub fallback_file: Option<String>,
    /// `enable_audit_log` 开关缓存 TTL（秒），保存系统设置时会主动失效。
    #[serde(default = "default_settings_cache_ttl_secs")]
    pub settings_cache_ttl_secs: u64,
    /// 优雅停机时等待队列排空的最长时间（秒）。
    #[serde(default = "default_shutdown_drain_secs")]
    pub shutdown_drain_secs: u64,
    /// 已废弃。配置了只告警并忽略，v2 声明改由主令牌派生，不再读取该文件。
    #[serde(default)]
    pub actor_signing_key_file: Option<String>,
    /// 操作人声明有效期（秒），范围 [1, 60]。
    #[serde(default = "default_actor_token_ttl_secs")]
    pub actor_token_ttl_secs: i64,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            trusted_proxies: Vec::new(),
            queue_capacity: default_queue_capacity(),
            batch_size: default_batch_size(),
            flush_interval_ms: default_flush_interval_ms(),
            fallback_file: None,
            settings_cache_ttl_secs: default_settings_cache_ttl_secs(),
            shutdown_drain_secs: default_shutdown_drain_secs(),
            actor_signing_key_file: None,
            actor_token_ttl_secs: default_actor_token_ttl_secs(),
        }
    }
}

impl mazbri_core::config::ValidateConfig for AuditConfig {
    fn validate(&self) -> anyhow::Result<()> {
        super::ip::TrustedProxies::parse(&self.trusted_proxies)
            .map_err(|e| anyhow::anyhow!("audit.trusted_proxies: {e}"))?;
        if self.queue_capacity == 0 {
            anyhow::bail!("audit.queue_capacity must be greater than 0");
        }
        if self.batch_size == 0 || self.batch_size > 1000 {
            anyhow::bail!("audit.batch_size must be in [1, 1000]");
        }
        if self.flush_interval_ms == 0 {
            anyhow::bail!("audit.flush_interval_ms must be greater than 0");
        }
        if !(1..=3600).contains(&self.settings_cache_ttl_secs) {
            anyhow::bail!("audit.settings_cache_ttl_secs must be in [1, 3600]");
        }
        if !(1..=mazbri_audit::actor::MAX_TTL_SECS).contains(&self.actor_token_ttl_secs) {
            anyhow::bail!("audit.actor_token_ttl_secs must be in [1, 60]");
        }
        Ok(())
    }
}
