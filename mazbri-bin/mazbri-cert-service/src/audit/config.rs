//! `[audit]` 配置段。整段可省略：旧配置照常启动（只写本地兜底文件并告警）。

use mazbri_core::DatabaseConfig;
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
const fn default_shutdown_drain_secs() -> u64 {
    5
}

/// 操作人声明的处理模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActorMode {
    /// 缺失/无效声明只记录（`details.actor_verification`），不拒绝。上线初期使用。
    #[default]
    Lenient,
    /// mgmt 专属写接口必须携带有效声明；任何接口携带无效声明都返回 401。
    Strict,
}

impl ActorMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lenient => "lenient",
            Self::Strict => "strict",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CertAuditConfig {
    #[serde(default)]
    pub actor_mode: ActorMode,
    /// 已废弃。配置了就加载 v1 Ed25519 公钥；v2 HMAC 由主令牌派生，不依赖此文件。
    #[serde(default)]
    pub actor_verify_key_file: Option<String>,
    /// 落库失败时的兜底 JSONL；缺省 `<tracing.log_dir>/cert-audit-fallback.jsonl`。
    #[serde(default)]
    pub fallback_file: Option<String>,
    #[serde(default = "default_queue_capacity")]
    pub queue_capacity: usize,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_flush_interval_ms")]
    pub flush_interval_ms: u64,
    #[serde(default = "default_shutdown_drain_secs")]
    pub shutdown_drain_secs: u64,
    /// 统一审计库（`mazbri_admin`，账号 `mazbri_audit_writer`，只有 `audit_logs` 的 INSERT 权限）。
    /// 惰性连接：审计库不可用不影响 cert 启动。
    #[serde(default)]
    pub database: Option<DatabaseConfig>,
}

impl Default for CertAuditConfig {
    fn default() -> Self {
        Self {
            actor_mode: ActorMode::default(),
            actor_verify_key_file: None,
            fallback_file: None,
            queue_capacity: default_queue_capacity(),
            batch_size: default_batch_size(),
            flush_interval_ms: default_flush_interval_ms(),
            shutdown_drain_secs: default_shutdown_drain_secs(),
            database: None,
        }
    }
}

impl CertAuditConfig {
    /// 去掉空白后的公钥路径。
    #[must_use]
    pub fn verify_key_file(&self) -> Option<&str> {
        self.actor_verify_key_file
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
    }

    /// # Errors
    ///
    /// 参数越界时返回错误。严格模式不再要求 v1 公钥文件。
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.queue_capacity == 0 {
            anyhow::bail!("audit.queue_capacity must be greater than 0");
        }
        if self.batch_size == 0 || self.batch_size > 1000 {
            anyhow::bail!("audit.batch_size must be in [1, 1000]");
        }
        if self.flush_interval_ms == 0 {
            anyhow::bail!("audit.flush_interval_ms must be greater than 0");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_lenient_and_valid() {
        let config: CertAuditConfig = toml::from_str("").expect("empty");
        assert_eq!(config.actor_mode, ActorMode::Lenient);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn strict_does_not_require_verify_key() {
        let config: CertAuditConfig = toml::from_str("actor_mode = \"strict\"").expect("strict");
        assert!(config.validate().is_ok());
        let config: CertAuditConfig = toml::from_str(
            "actor_mode = \"strict\"\nactor_verify_key_file = \"/etc/mazbri/keys/a.pem\"",
        )
        .expect("strict with key");
        assert!(config.validate().is_ok());
        assert_eq!(config.verify_key_file(), Some("/etc/mazbri/keys/a.pem"));
        assert!(toml::from_str::<CertAuditConfig>("actor_mode = \"off\"").is_err());
        assert!(toml::from_str::<CertAuditConfig>("unknown = 1").is_err());
    }
}
