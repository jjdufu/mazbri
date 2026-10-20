use crate::config::GreptimeConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct GreptimeClient {
    base_url: String,
    username: String,
    password: String,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LatencySnapshot {
    #[serde(default)]
    pub p50_ms: Option<f64>,
    #[serde(default)]
    pub p75_ms: Option<f64>,
    #[serde(default)]
    pub p95_ms: Option<f64>,
    #[serde(default)]
    pub qps: Option<f64>,
    #[serde(default)]
    pub error_rate: Option<f64>,
}

impl GreptimeClient {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn new(config: &GreptimeConfig) -> Result<Option<Self>> {
        if !config.enabled {
            return Ok(None);
        }
        Ok(Some(Self {
            base_url: config.http_url.trim_end_matches('/').to_string(),
            username: config.username.clone(),
            password: config.password.clone(),
            client: reqwest::Client::builder().build()?,
        }))
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn ping(&self) -> Result<bool> {
        let response = self
            .client
            .get(format!("{}/", self.base_url))
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?;
        Ok(response.status().is_success())
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn query_latency_snapshot(&self) -> Result<Option<LatencySnapshot>> {
        if !self.ping().await.unwrap_or(false) {
            return Ok(None);
        }
        Ok(Some(LatencySnapshot::default()))
    }
}
