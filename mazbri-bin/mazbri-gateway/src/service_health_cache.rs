use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, interval};
use tracing::warn;
#[path = "service_health_metrics_parser.rs"]
mod service_health_metrics_parser;
const HEALTH_SCRAPE_INTERVAL_SECS: u64 = 15;
const HEALTH_SCRAPE_TIMEOUT_SECS: u64 = 5;
const HEALTH_SCRAPE_BATCH_SIZE: usize = 2;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ServiceHealthState {
    #[default]
    Unknown,
    Healthy,
    Unhealthy,
}
#[derive(Debug, Clone, Default)]
pub struct CachedServiceHealth {
    pub state: ServiceHealthState,
    pub healthy: bool,
    pub last_scrape_error: Option<String>,
}
#[derive(Clone)]
pub struct ServiceHealthCache {
    inner: Arc<RwLock<HashMap<String, CachedServiceHealth>>>,
}
impl Default for ServiceHealthCache {
    fn default() -> Self {
        Self::new()
    }
}
impl ServiceHealthCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    pub async fn get(&self, service_name: &str) -> CachedServiceHealth {
        self.inner
            .read()
            .await
            .get(service_name)
            .cloned()
            .unwrap_or_default()
    }

    pub fn spawn_dynamic(
        self,
        static_endpoints: HashMap<String, String>,
        cert_service_url: String,
        token: Option<String>,
    ) -> tokio::task::JoinHandle<()> {
        let cache = self.inner.clone();
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(HEALTH_SCRAPE_INTERVAL_SECS));
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(HEALTH_SCRAPE_TIMEOUT_SECS))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new());
            let mut next_batch_start = 0usize;

            #[derive(serde::Deserialize)]
            struct RegisteredNode {
                service_name: String,
                health_check_url: Option<String>,
            }

            loop {
                tick.tick().await;

                let mut endpoints = static_endpoints.clone();
                let url = format!("{}/nodes", cert_service_url.trim_end_matches('/'));
                let mut req = client.get(&url);
                if let Some(ref t) = token {
                    req = req.bearer_auth(t);
                }

                match req.send().await {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            match resp.json::<Vec<RegisteredNode>>().await {
                                Ok(nodes) => {
                                    for node in nodes {
                                        if node.service_name != "mazbri-gateway" {
                                            if let Some(hc_url) = node.health_check_url {
                                                let adjusted_url = adjust_health_check_url(
                                                    &node.service_name,
                                                    &hc_url,
                                                    &static_endpoints,
                                                );
                                                endpoints.insert(node.service_name, adjusted_url);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!(error = %e, "Failed to parse registered nodes JSON from registry");
                                }
                            }
                        } else {
                            warn!(status = %resp.status(), "Registry /nodes request returned error status");
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to fetch registered nodes from registry");
                    }
                }

                let endpoint_list: Vec<(String, String)> = endpoints.into_iter().collect();
                let endpoint_count = endpoint_list.len();
                if endpoint_count == 0 {
                    continue;
                }

                let batch_size = endpoint_count.min(HEALTH_SCRAPE_BATCH_SIZE);
                let mut results = Vec::with_capacity(batch_size);
                for offset in 0..batch_size {
                    let index = (next_batch_start + offset) % endpoint_count;
                    let (name, url) = &endpoint_list[index];
                    let result = scrape_service_health(&client, url).await;
                    results.push((name.clone(), result));
                }
                next_batch_start = (next_batch_start + batch_size) % endpoint_count;

                let mut map = cache.write().await;
                for (name, result) in results {
                    match result {
                        Ok(healthy) => {
                            map.insert(
                                name,
                                CachedServiceHealth {
                                    state: if healthy {
                                        ServiceHealthState::Healthy
                                    } else {
                                        ServiceHealthState::Unhealthy
                                    },
                                    healthy,
                                    last_scrape_error: None,
                                },
                            );
                        }
                        Err(e) => {
                            warn!(
                                service = % name, error = % e,
                                "Failed to scrape service health metrics"
                            );
                            let entry = map.entry(name).or_default();
                            entry.state = ServiceHealthState::Unknown;
                            entry.healthy = false;
                            entry.last_scrape_error = Some(e.to_string());
                        }
                    }
                }
            }
        })
    }
}
fn adjust_health_check_url(
    service_name: &str,
    hc_url: &str,
    static_endpoints: &HashMap<String, String>,
) -> String {
    if let Some(static_url) = static_endpoints.get(service_name) {
        if let Ok(static_parsed) = reqwest::Url::parse(static_url) {
            if let Some(static_port) = static_parsed.port() {
                if let Ok(mut hc_parsed) = reqwest::Url::parse(hc_url) {
                    if hc_parsed.set_port(Some(static_port)).is_ok() {
                        return hc_parsed.to_string();
                    }
                }
            }
        }
    }
    hc_url.to_string()
}

async fn scrape_service_health(client: &reqwest::Client, admin_url: &str) -> anyhow::Result<bool> {
    let url = if admin_url.contains("/metrics") || admin_url.contains("/health") {
        admin_url.to_string()
    } else {
        format!("{}/metrics", admin_url.trim_end_matches('/'))
    };
    let resp = client.get(&url).send().await?.error_for_status()?;
    if url.contains("/health") {
        let body = resp.text().await?;
        #[derive(serde::Deserialize)]
        struct HealthResponse {
            status: String,
        }
        if let Ok(res) = serde_json::from_str::<HealthResponse>(&body) {
            let s = res.status.to_lowercase();
            return Ok(s == "healthy" || s == "ok" || s == "ready");
        }
        let trimmed = body.trim().to_lowercase();
        return Ok(trimmed == "ok" || trimmed == "healthy" || trimmed == "success");
    }
    let body = resp.text().await?;
    service_health_metrics_parser::parse_health_metrics_body(&body)
}
