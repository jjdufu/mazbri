use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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

const fn default_true() -> bool {
    true
}

/// 通道级跨租户总限流覆盖模式（未声明时回退至网关全局默认值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteRateLimitMode {
    Reject,
    Queue,
}

impl RouteRateLimitMode {
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        if raw.eq_ignore_ascii_case("reject") {
            Some(Self::Reject)
        } else if raw.eq_ignore_ascii_case("queue") {
            Some(Self::Queue)
        } else {
            None
        }
    }
}

/// 从 `channels.config` 预提取的轻量 `Copy` 限流参数覆盖项。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelRateLimitOverrides {
    pub mode: Option<RouteRateLimitMode>,
    pub max_wait_ms: Option<u64>,
    pub max_queue_depth: Option<u64>,
}

impl ChannelRateLimitOverrides {
    #[must_use]
    pub fn from_channel_config(channel_config: &serde_json::Value) -> Self {
        Self {
            mode: channel_config
                .get("rate_limit_mode")
                .and_then(serde_json::Value::as_str)
                .and_then(RouteRateLimitMode::parse),
            max_wait_ms: channel_config
                .get("rate_limit_max_wait_ms")
                .and_then(serde_json::Value::as_u64)
                .filter(|v| *v > 0),
            max_queue_depth: channel_config
                .get("rate_limit_max_queue_depth")
                .and_then(serde_json::Value::as_u64)
                .filter(|v| *v > 0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateChannel<P = ()> {
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
    /// Upstream cost price per call (`channels.cost_price`, decimal string).
    pub cost_price: String,
    /// Pre-parsed channel rate-limit overrides (`Copy`).
    #[serde(default)]
    pub rate_limit: ChannelRateLimitOverrides,
    /// Optional pre-parsed runtime payload populated at route load time.
    #[serde(skip, default)]
    pub prepared: P,
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
pub struct RouteEngine<P = ()> {
    routes: HashMap<RouteKey, Arc<[Arc<CandidateChannel<P>>]>>,
}

impl<P> RouteEngine<P> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_routes(routes: HashMap<String, Vec<CandidateChannel<P>>>) -> Self {
        let routes = routes
            .into_iter()
            .map(|(product_id, channels)| {
                (
                    RouteKey::product_default(product_id),
                    freeze_sorted_channels(channels),
                )
            })
            .collect();
        Self { routes }
    }

    #[must_use]
    pub fn with_scoped_routes(routes: HashMap<RouteKey, Vec<CandidateChannel<P>>>) -> Self {
        let routes = routes
            .into_iter()
            .map(|(key, channels)| (key, freeze_sorted_channels(channels)))
            .collect();
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
        keep: impl Fn(&CandidateChannel<P>) -> bool,
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
    pub fn match_route(
        &self,
        ctx: &RouteMatchContext,
    ) -> Option<Cow<'_, [Arc<CandidateChannel<P>>]>> {
        let tenant_route = RouteKey::tenant_product(&ctx.tenant_id, &ctx.product_id);
        let default_route = RouteKey::product_default(&ctx.product_id);
        let base = self
            .routes
            .get(&tenant_route)
            .or_else(|| self.routes.get(&default_route))?;

        let mut channels: Cow<'_, [Arc<CandidateChannel<P>>]> = Cow::Borrowed(base.as_ref());

        if let Some(ref cv) = ctx.client_version {
            let filtered: Vec<Arc<CandidateChannel<P>>> = channels
                .iter()
                .filter(|c| c.version_tag.as_deref() == Some(cv.as_str()))
                .cloned()
                .collect();
            if !filtered.is_empty() {
                channels = Cow::Owned(filtered);
            }
        }

        let total_weight: u32 = channels.iter().map(|c| c.weight.unwrap_or(0)).sum();
        if total_weight > 0
            && let Some(idx) = select_weighted_index(&channels, &ctx.tenant_id, total_weight)
            && idx > 0
        {
            let vec = channels.to_mut();
            let selected = vec.remove(idx);
            vec.insert(0, selected);
        }

        Some(channels)
    }
}

impl<P> Default for RouteEngine<P> {
    fn default() -> Self {
        Self::new()
    }
}

fn freeze_sorted_channels<P>(mut channels: Vec<CandidateChannel<P>>) -> Arc<[Arc<CandidateChannel<P>>]> {
    channels.sort_by_key(|c| c.order);
    let boxed: Vec<Arc<CandidateChannel<P>>> = channels.into_iter().map(Arc::new).collect();
    Arc::from(boxed)
}

fn select_weighted_index<P>(
    channels: &[Arc<CandidateChannel<P>>],
    tenant_id: &str,
    total_weight: u32,
) -> Option<usize> {
    let roll = if tenant_id.is_empty() {
        rand::random_range(0..100)
    } else {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(tenant_id, &mut hasher);
        u32::try_from(std::hash::Hasher::finish(&hasher) % 100).unwrap_or(0)
    };
    let scaled_roll = if total_weight == 100 {
        roll
    } else {
        (roll * total_weight) / 100
    };
    let mut cumulative = 0;
    for (idx, channel) in channels.iter().enumerate() {
        let w = channel.weight.unwrap_or(0);
        if w > 0 {
            cumulative += w;
            if scaled_roll < cumulative {
                return Some(idx);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_channel(id: &str, order: u32, weight: Option<u32>, version_tag: Option<&str>) -> CandidateChannel<u32> {
        CandidateChannel {
            channel_id: id.into(),
            channel_code: id.into(),
            channel_name: id.into(),
            supplier_id: "s".into(),
            supplier_code: "sc".into(),
            supplier_name: "sn".into(),
            supplier_upstream_url: "http://127.0.0.1:8080".into(),
            path_prefix: "/".into(),
            switch_condition: SwitchCondition::default(),
            order,
            version_tag: version_tag.map(str::to_string),
            weight,
            supplier_config: serde_json::Value::Null,
            channel_config: serde_json::Value::Null,
            qps_limit: 0,
            cost_price: "0".into(),
            rate_limit: ChannelRateLimitOverrides::default(),
            prepared: order,
        }
    }

    #[test]
    fn unweighted_match_presorts_by_order_and_borrows_zero_alloc() {
        let mut map = HashMap::new();
        map.insert(
            "p1".to_string(),
            vec![
                sample_channel("c2", 20, None, None),
                sample_channel("c1", 10, None, None),
            ],
        );
        let engine = RouteEngine::with_routes(map);
        let matched = engine
            .match_route(&RouteMatchContext {
                tenant_id: "t1".into(),
                product_id: "p1".into(),
                client_version: None,
            })
            .expect("matched");
        assert!(matches!(matched, Cow::Borrowed(_)));
        assert_eq!(matched[0].channel_id, "c1");
        assert_eq!(matched[1].channel_id, "c2");
        assert_eq!(matched[0].prepared, 10);
    }

    #[test]
    fn version_filter_preserves_order_and_falls_back_when_unmatched() {
        let mut map = HashMap::new();
        map.insert(
            "p1".to_string(),
            vec![
                sample_channel("v2-b", 2, None, Some("v2")),
                sample_channel("v1-a", 1, None, Some("v1")),
                sample_channel("v2-a", 0, None, Some("v2")),
            ],
        );
        let engine = RouteEngine::with_routes(map);
        let v2 = engine
            .match_route(&RouteMatchContext {
                tenant_id: "t1".into(),
                product_id: "p1".into(),
                client_version: Some("v2".into()),
            })
            .expect("v2");
        assert_eq!(v2.len(), 2);
        assert_eq!(v2[0].channel_id, "v2-a");
        assert_eq!(v2[1].channel_id, "v2-b");

        let fallback = engine
            .match_route(&RouteMatchContext {
                tenant_id: "t1".into(),
                product_id: "p1".into(),
                client_version: Some("v999".into()),
            })
            .expect("fallback");
        assert!(matches!(fallback, Cow::Borrowed(_)));
        assert_eq!(fallback.len(), 3);
    }

    #[test]
    fn rate_limit_overrides_parse_cleanly() {
        let cfg = serde_json::json!({
            "rate_limit_mode": "queue",
            "rate_limit_max_wait_ms": 1500,
            "rate_limit_max_queue_depth": 32
        });
        let parsed = ChannelRateLimitOverrides::from_channel_config(&cfg);
        assert_eq!(parsed.mode, Some(RouteRateLimitMode::Queue));
        assert_eq!(parsed.max_wait_ms, Some(1500));
        assert_eq!(parsed.max_queue_depth, Some(32));
    }
}
