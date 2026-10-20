use serde::Deserialize;

/// 路由热加载配置。
///
/// 最坏生效时间 ≈ `interval_secs` + 全量加载耗时；
/// 启用 `listen_notify` 时可在变更后立即唤醒，轮询仍为保底。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteReloadConfig {
    /// 轮询间隔（秒），默认 5。
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u64,
    /// 失败后的最大退避（秒），默认 60。
    #[serde(default = "default_max_backoff_secs")]
    pub max_backoff_secs: u64,
    /// 是否启用 PostgreSQL LISTEN/NOTIFY 加速，默认 false。
    #[serde(default)]
    pub listen_notify: bool,
    /// NOTIFY 频道名，需与 DB `increment_route_version` 中一致。
    #[serde(default = "default_notify_channel")]
    pub notify_channel: String,
}

fn default_interval_secs() -> u64 {
    5
}

fn default_max_backoff_secs() -> u64 {
    60
}

fn default_notify_channel() -> String {
    "mazbri_route_changed".to_string()
}

impl Default for RouteReloadConfig {
    fn default() -> Self {
        Self {
            interval_secs: default_interval_secs(),
            max_backoff_secs: default_max_backoff_secs(),
            listen_notify: false,
            notify_channel: default_notify_channel(),
        }
    }
}

impl RouteReloadConfig {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.interval_secs == 0 {
            anyhow::bail!("route_reload.interval_secs 必须 ≥ 1");
        }
        if self.interval_secs > 300 {
            anyhow::bail!("route_reload.interval_secs 不得超过 300");
        }
        if self.max_backoff_secs < self.interval_secs {
            anyhow::bail!("route_reload.max_backoff_secs 不得小于 interval_secs");
        }
        if self.notify_channel.trim().is_empty() {
            anyhow::bail!("route_reload.notify_channel 不能为空");
        }
        Ok(())
    }
}
