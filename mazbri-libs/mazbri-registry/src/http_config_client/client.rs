use crate::config_client::ConfigCache;
use crate::http_config_client::error::HttpConfigClientError;
use crate::http_config_client::types::{ConfigResponse, HeartbeatResponse, RegisterNodeResponse};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;
#[path = "heartbeat.rs"]
mod heartbeat;
#[path = "registration.rs"]
mod registration;
pub struct HttpConfigClient {
    http_client: Client,
    service_name: String,
    cert_service_url: String,
    cert_service_token: String,
    cache: Arc<RwLock<ConfigCache>>,
    _heartbeat_interval: Duration,
    request_timeout: Duration,
}
pub struct HttpConfigClientBuilder {
    service_name: String,
    cert_service_url: String,
    http_client: Option<Client>,
    cert_service_token: String,
    heartbeat_interval: Duration,
    request_timeout: Duration,
}
impl HttpConfigClientBuilder {
    pub fn new(
        service_name: impl Into<String>,
        cert_service_url: impl Into<String>,
        cert_service_token: impl Into<String>,
    ) -> Self {
        Self {
            service_name: service_name.into(),
            cert_service_url: cert_service_url.into(),
            http_client: None,
            cert_service_token: cert_service_token.into(),
            heartbeat_interval: Duration::from_secs(30),
            request_timeout: Duration::from_secs(10),
        }
    }
    #[must_use]
    pub fn heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }
    #[must_use]
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }
    #[must_use]
    pub fn http_client(mut self, client: Client) -> Self {
        self.http_client = Some(client);
        self
    }
    pub async fn build(self) -> Result<HttpConfigClient, HttpConfigClientError> {
        let http_client = match self.http_client {
            Some(client) => client,
            None => Client::builder()
                .timeout(self.request_timeout)
                .build()
                .map_err(|e| {
                    HttpConfigClientError::RequestFailed(format!("无法创建 HTTP 客户端: {e}"))
                })?,
        };
        let client = HttpConfigClient {
            http_client,
            service_name: self.service_name,
            cert_service_url: self.cert_service_url,
            cert_service_token: self.cert_service_token,
            cache: Arc::new(RwLock::new(ConfigCache::default())),
            _heartbeat_interval: self.heartbeat_interval,
            request_timeout: self.request_timeout,
        };
        client.refresh().await?;
        Ok(client)
    }
}
impl HttpConfigClient {
    pub async fn get_content(&self) -> String {
        let cache = self.cache.read().await;
        cache.content.clone()
    }
    pub async fn get_content_with_metadata(&self) -> (String, i32, Option<std::time::Instant>) {
        let cache = self.cache.read().await;
        (cache.content.clone(), cache.version, cache.last_updated)
    }
    pub async fn fetch_config(&self) -> Result<ConfigResponse, HttpConfigClientError> {
        let url = format!("{}/configs/{}", self.cert_service_url, self.service_name);
        let request_builder = self
            .http_client
            .get(&url)
            .bearer_auth(&self.cert_service_token);
        let response = request_builder
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|e| HttpConfigClientError::RequestFailed(e.to_string()))?;
        if !response.status().is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(HttpConfigClientError::ConfigNotFound {
                service_name: self.service_name.clone(),
                message: error_body,
            });
        }
        let content = response
            .text()
            .await
            .map_err(|e| HttpConfigClientError::RequestFailed(e.to_string()))?;
        let content_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
        Ok(ConfigResponse {
            content,
            content_hash,
            version: 0,
        })
    }
    pub async fn refresh(&self) -> Result<(), HttpConfigClientError> {
        let config = self.fetch_config().await?;
        let mut cache = self.cache.write().await;
        cache.content = config.content;
        cache.content_hash = config.content_hash;
        cache.version = config.version;
        cache.last_updated = Some(std::time::Instant::now());
        info!(
            service = % self.service_name, content_len = cache.content.len(), "配置已刷新"
        );
        Ok(())
    }
    pub async fn register_node(
        &self,
        ip: String,
        port: u16,
        health_check_url: Option<String>,
        started_at: i64,
    ) -> Result<RegisterNodeResponse, HttpConfigClientError> {
        let url = format!("{}/nodes/register", self.cert_service_url);
        let cache = self.cache.read().await;
        let request = registration::build_register_node_request(
            &self.service_name,
            ip,
            port,
            started_at,
            &cache,
            health_check_url,
        );
        drop(cache);
        let request_builder = self
            .http_client
            .post(&url)
            .json(&request)
            .bearer_auth(&self.cert_service_token);
        let response = request_builder
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|e| HttpConfigClientError::RequestFailed(e.to_string()))?;
        if !response.status().is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(HttpConfigClientError::RegistrationFailed(error_body));
        }
        response
            .json::<RegisterNodeResponse>()
            .await
            .map_err(|e| HttpConfigClientError::ResponseParseFailed(e.to_string()))
    }
    pub async fn send_heartbeat(
        &self,
        ip: &str,
        port: u16,
    ) -> Result<HeartbeatResponse, HttpConfigClientError> {
        let url = format!("{}/nodes/heartbeat", self.cert_service_url);
        let cache = self.cache.read().await;
        let request = heartbeat::build_heartbeat_request(&self.service_name, ip, port, &cache);
        drop(cache);
        let request_builder = self
            .http_client
            .post(&url)
            .json(&request)
            .bearer_auth(&self.cert_service_token);
        let response = request_builder
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|e| HttpConfigClientError::RequestFailed(e.to_string()))?;
        response
            .json::<HeartbeatResponse>()
            .await
            .map_err(|e| HttpConfigClientError::ResponseParseFailed(e.to_string()))
    }
}
