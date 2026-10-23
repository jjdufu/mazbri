use mazbri_core::ServiceName;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, interval};
use tracing::warn;
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

    #[must_use]
    pub fn spawn_dynamic(
        self,
        cert_service_url: String,
        token: String,
    ) -> tokio::task::JoinHandle<()> {
        let cache = self.inner.clone();
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(HEALTH_SCRAPE_INTERVAL_SECS));
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(HEALTH_SCRAPE_TIMEOUT_SECS))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new());
            let mut next_batch_start = 0usize;
            let nodes_url = format!("{}/nodes", cert_service_url.trim_end_matches('/'));

            loop {
                tick.tick().await;

                let endpoints = fetch_registered_endpoints(&client, &nodes_url, &token)
                    .await
                    .unwrap_or_default();
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
                                "Failed to scrape service health"
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

/// cert `/nodes` 返回的节点（只取需要的字段）。
#[derive(Debug, serde::Deserialize)]
struct RegisteredNode {
    service_name: String,
    health_check_url: Option<String>,
}

/// 从 cert 注册中心取各服务的健康检查地址（admin 端口 `/health`）。
async fn fetch_registered_endpoints(
    client: &reqwest::Client,
    nodes_url: &str,
    token: &str,
) -> Option<HashMap<String, String>> {
    let resp = match client.get(nodes_url).bearer_auth(token).send().await {
        Ok(resp) => resp,
        Err(e) => {
            warn!(error = %e, "Failed to fetch registered nodes from registry");
            return None;
        }
    };
    if !resp.status().is_success() {
        warn!(status = %resp.status(), "Registry /nodes request returned error status");
        return None;
    }
    match resp.json::<Vec<RegisteredNode>>().await {
        Ok(nodes) => Some(collect_endpoints(nodes)),
        Err(e) => {
            warn!(error = %e, "Failed to parse registered nodes JSON from registry");
            None
        }
    }
}

/// 跳过网关自身与未上报健康地址的节点；同名多副本以最后一个为准。
fn collect_endpoints(nodes: Vec<RegisteredNode>) -> HashMap<String, String> {
    nodes
        .into_iter()
        .filter(|node| node.service_name != ServiceName::Gateway.as_str())
        .filter_map(|node| node.health_check_url.map(|url| (node.service_name, url)))
        .collect()
}

/// 直接请求注册的健康检查地址，2xx 视为健康。
async fn scrape_service_health(client: &reqwest::Client, url: &str) -> anyhow::Result<bool> {
    let resp = client.get(url).send().await?;
    Ok(resp.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, http::StatusCode, routing::get};

    fn node(name: &str, url: Option<&str>) -> RegisteredNode {
        RegisteredNode {
            service_name: name.to_owned(),
            health_check_url: url.map(str::to_owned),
        }
    }

    #[test]
    fn collect_skips_gateway_and_nodes_without_url() {
        let map = collect_endpoints(vec![
            node("mazbri-gateway", Some("http://10.0.0.1:9000/health")),
            node("mazbri-auth", Some("http://10.0.0.2:9000/health")),
            node("mazbri-cache", None),
        ]);
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get("mazbri-auth").map(String::as_str),
            Some("http://10.0.0.2:9000/health")
        );
    }

    async fn serve(router: Router) -> anyhow::Result<String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(format!("http://{addr}/health"))
    }

    #[tokio::test]
    async fn scrape_admin_health_ok_and_unavailable() -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let ok = serve(Router::new().route("/health", get(|| async { "OK" }))).await?;
        assert!(scrape_service_health(&client, &ok).await?);
        let bad = serve(Router::new().route(
            "/health",
            get(|| async { (StatusCode::SERVICE_UNAVAILABLE, "Unhealthy") }),
        ))
        .await?;
        assert!(!scrape_service_health(&client, &bad).await?);
        Ok(())
    }
}
