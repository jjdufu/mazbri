use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteMatchContext {
    pub tenant_id: String,
    pub product_id: String,
    pub client_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SwitchCondition {
    pub max_retries: u32,
    pub timeout_ms: u64,
    #[serde(default = "default_true")]
    pub switch_on_timeout: bool,
    #[serde(default = "default_true")]
    pub switch_on_bad_gateway: bool,
    #[serde(default)]
    pub switch_on_internal_error: bool,
    #[serde(default)]
    pub switch_on_status_codes: Vec<u16>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateChannel {
    pub channel_id: String,
    pub channel_code: String,
    pub channel_name: String,
    pub supplier_id: String,
    pub supplier_code: String,
    pub supplier_name: String,
    pub supplier_upstream_url: String,
    pub path_prefix: String,
    pub switch_condition: SwitchCondition,
    pub order: u32,
    pub version_tag: Option<String>,
    pub weight: Option<u32>,
    pub supplier_config: serde_json::Value,
    /// Channel JSON config (`channels.config`). May include `rate_limit`_* overrides.
    #[serde(default)]
    pub channel_config: serde_json::Value,
    /// Cross-tenant total QPS for this channel (`channels.qps_limit`).
    /// `0` means unlimited. Enforced by gateway independently of tenant ratelimit.
    #[serde(default)]
    pub qps_limit: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RouteKey {
    tenant_id: Option<String>,
    product_id: String,
}

impl RouteKey {
    pub fn product_default(product_id: impl Into<String>) -> Self {
        Self {
            tenant_id: None,
            product_id: product_id.into(),
        }
    }
    pub fn tenant_product(tenant_id: impl Into<String>, product_id: impl Into<String>) -> Self {
        Self {
            tenant_id: Some(tenant_id.into()),
            product_id: product_id.into(),
        }
    }
}

#[derive(Clone)]
pub struct RouteEngine {
    routes: HashMap<RouteKey, Vec<CandidateChannel>>,
}

impl RouteEngine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }
    #[must_use]
    pub fn with_routes(routes: HashMap<String, Vec<CandidateChannel>>) -> Self {
        let routes = routes
            .into_iter()
            .map(|(product_id, channels)| (RouteKey::product_default(product_id), channels))
            .collect();
        Self { routes }
    }
    #[must_use]
    pub fn with_scoped_routes(routes: HashMap<RouteKey, Vec<CandidateChannel>>) -> Self {
        Self { routes }
    }
    #[must_use]
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }
    #[must_use]
    pub fn unique_service_urls(&self) -> Vec<String> {
        self.unique_service_urls_where(|_| true)
    }
    /// Distinct supplier upstream URLs of channels accepted by `keep`.
    #[must_use]
    pub fn unique_service_urls_where(
        &self,
        keep: impl Fn(&CandidateChannel) -> bool,
    ) -> Vec<String> {
        let mut urls = HashSet::new();
        for channels in self.routes.values() {
            for channel in channels.iter().filter(|c| keep(c)) {
                urls.insert(channel.supplier_upstream_url.clone());
            }
        }
        urls.into_iter().collect()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
    #[must_use]
    pub fn match_route(&self, ctx: &RouteMatchContext) -> Option<Vec<CandidateChannel>> {
        let tenant_route = RouteKey::tenant_product(&ctx.tenant_id, &ctx.product_id);
        let default_route = RouteKey::product_default(&ctx.product_id);
        let mut channels = self
            .routes
            .get(&tenant_route)
            .or_else(|| self.routes.get(&default_route))
            .cloned()?;

        if let Some(ref cv) = ctx.client_version {
            let filtered: Vec<_> = channels
                .iter()
                .filter(|c| c.version_tag.as_ref() == Some(cv))
                .cloned()
                .collect();
            if !filtered.is_empty() {
                channels = filtered;
            }
        }

        let total_weight: u32 = channels.iter().map(|c| c.weight.unwrap_or(0)).sum();
        if total_weight > 0 {
            let roll = if ctx.tenant_id.is_empty() {
                rand::random_range(0..100)
            } else {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                std::hash::Hash::hash(&ctx.tenant_id, &mut hasher);
                (std::hash::Hasher::finish(&hasher) % 100) as u32
            };

            let scaled_roll = if total_weight == 100 {
                roll
            } else {
                (roll * total_weight) / 100
            };

            let mut cumulative = 0;
            let mut selected_idx = None;
            for (idx, channel) in channels.iter().enumerate() {
                let w = channel.weight.unwrap_or(0);
                if w > 0 {
                    cumulative += w;
                    if scaled_roll < cumulative {
                        selected_idx = Some(idx);
                        break;
                    }
                }
            }

            if let Some(idx) = selected_idx {
                let selected = channels.remove(idx);
                channels.insert(0, selected);
            }
        } else {
            channels = sort_channels(channels);
        }

        Some(channels)
    }
}

impl Default for RouteEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn sort_channels(mut channels: Vec<CandidateChannel>) -> Vec<CandidateChannel> {
    channels.sort_by_key(|c| c.order);
    channels
}
