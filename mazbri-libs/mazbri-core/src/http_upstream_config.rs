use anyhow::Result;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpUpstreamConfig {
    pub base_url: String,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
    #[serde(default)]
    pub response_body: ResponseBodyConfig,
    pub channels: Vec<HttpUpstreamChannelConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseBodyConfig {
    #[serde(default = "default_json_raw_enabled")]
    pub json_raw_enabled: bool,
    #[serde(default = "default_json_raw_min_bytes")]
    pub json_raw_min_bytes: usize,
    #[serde(default)]
    pub validate_json_raw: bool,
}

impl Default for ResponseBodyConfig {
    fn default() -> Self {
        Self {
            json_raw_enabled: default_json_raw_enabled(),
            json_raw_min_bytes: default_json_raw_min_bytes(),
            validate_json_raw: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpUpstreamChannelConfig {
    pub channel_code: String,
    pub path_prefix: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[must_use]
pub fn default_timeout_secs() -> u64 {
    15
}

#[must_use]
pub fn default_connect_timeout_secs() -> u64 {
    3
}

#[must_use]
pub fn default_json_raw_enabled() -> bool {
    true
}

#[must_use]
pub fn default_json_raw_min_bytes() -> usize {
    32 * 1024
}

impl HttpUpstreamConfig {
    #[must_use]
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }

    #[must_use]
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_secs)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<()> {
        validate_base_config(&self.base_url, self.timeout_secs, self.connect_timeout_secs)?;
        validate_channels(&self.channels)
    }
}

impl HttpUpstreamChannelConfig {
    #[must_use]
    pub fn effective_base_url<'a>(&'a self, default_base_url: &'a str) -> &'a str {
        self.base_url.as_deref().unwrap_or(default_base_url)
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_base_config(
    base_url: &str,
    timeout_secs: u64,
    connect_timeout_secs: u64,
) -> Result<()> {
    if base_url.trim().is_empty() {
        anyhow::bail!("upstream.base_url 不能为空");
    }
    if !(1..=300).contains(&timeout_secs) {
        anyhow::bail!("upstream.timeout_secs must be between 1 and 300, got {timeout_secs}");
    }
    if !(1..=30).contains(&connect_timeout_secs) {
        anyhow::bail!(
            "upstream.connect_timeout_secs must be between 1 and 30, got {connect_timeout_secs}"
        );
    }
    Ok(())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_channels(channels: &[HttpUpstreamChannelConfig]) -> Result<()> {
    if channels.is_empty() {
        anyhow::bail!("upstream.channels must contain at least one channel");
    }

    let mut seen_prefixes = std::collections::HashSet::new();
    for channel in channels {
        validate_channel_config(channel)?;
        let normalized_prefix = normalize_path_prefix(&channel.path_prefix)?;
        if !seen_prefixes.insert(normalized_prefix) {
            anyhow::bail!(
                "upstream.channels contains duplicate path_prefix: {}",
                channel.path_prefix
            );
        }
    }
    Ok(())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_channel_config(channel: &HttpUpstreamChannelConfig) -> Result<()> {
    validate_channel_code(&channel.channel_code)?;
    let _ = normalize_path_prefix(&channel.path_prefix)?;
    if let Some(base_url) = channel.base_url.as_deref()
        && base_url.trim().is_empty()
    {
        anyhow::bail!(
            "upstream.channels[{}].base_url cannot be empty",
            channel.channel_code
        );
    }
    Ok(())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_channel_code(channel_code: &str) -> Result<()> {
    if channel_code.is_empty() || channel_code.len() > 100 {
        anyhow::bail!(
            "channel_code must be between 1 and 100 characters, got {}",
            channel_code.len()
        );
    }
    if !channel_code
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        anyhow::bail!("channel_code may only contain letters, numbers, underscore, and hyphen");
    }
    Ok(())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn normalize_path_prefix(prefix: &str) -> Result<String> {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        anyhow::bail!("path_prefix 不能为空");
    }
    if !prefix.starts_with('/') {
        anyhow::bail!("path_prefix 必须以 / 开头");
    }
    let normalized = if prefix == "/" {
        "/".to_string()
    } else {
        prefix.trim_end_matches('/').to_string()
    };
    if normalized.len() > 2048 {
        anyhow::bail!("path_prefix 长度超过限制");
    }
    Ok(normalized)
}
