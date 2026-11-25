use crate::upstream_contract::{BYPASS_CONTRACT_ID, grpc_path_for_contract};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::{SupplierConfig, parse_channel_config_egress};
use mazbri_routing::{
    CandidateChannel, ChannelRateLimitOverrides, RouteEngine, RouteKey, SwitchCondition,
};
use serde::Deserialize as _;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub type GatewayCandidateChannel = CandidateChannel<PreparedChannel>;

#[derive(Clone, Default)]
pub struct GatewayRouteEngine {
    inner: RouteEngine<PreparedChannel>,
    tenant_overrides: HashMap<String, HashSet<String>>,
}

impl GatewayRouteEngine {
    #[must_use]
    pub fn from_scoped_routes(
        routes: HashMap<RouteKey, Vec<GatewayCandidateChannel>>,
        tenant_overrides: HashMap<String, HashSet<String>>,
    ) -> Self {
        Self {
            inner: RouteEngine::with_scoped_routes(routes),
            tenant_overrides,
        }
    }

    #[must_use]
    pub fn has_tenant_override(&self, tenant_id: &str, product_id: &str) -> bool {
        self.tenant_overrides
            .get(tenant_id)
            .is_some_and(|products| products.contains(product_id))
    }
}

impl std::ops::Deref for GatewayRouteEngine {
    type Target = RouteEngine<PreparedChannel>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// 路由加载期预解析与预计算的通道执行配置（请求热路径零反序列化、零 SHA-256）。
#[derive(Debug, Clone)]
pub enum PreparedChannel {
    Transparent {
        adapter: Arc<str>,
        timeout_ms: u64,
        supplier_json: Arc<str>,
        supplier_hash: Arc<str>,
        egress_json: Arc<str>,
        egress_hash: Arc<str>,
        supplier_sync_key: Arc<str>,
        egress_sync_key: Arc<str>,
    },
    Microservice {
        contract_id: Arc<str>,
        timeout_secs: u64,
    },
    Invalid {
        error: Arc<str>,
        unknown_contract_id: Option<Arc<str>>,
        mode_hint: Arc<str>,
        contract_hint: Arc<str>,
    },
}

impl Default for PreparedChannel {
    fn default() -> Self {
        Self::Invalid {
            error: Arc::from("unprepared channel"),
            unknown_contract_id: None,
            mode_hint: Arc::from("unknown"),
            contract_hint: Arc::from("unknown"),
        }
    }
}

impl PreparedChannel {
    #[must_use]
    pub fn mode_and_contract_hint(&self) -> (&str, &str) {
        match self {
            Self::Transparent { adapter, .. } => (adapter.as_ref(), BYPASS_CONTRACT_ID),
            Self::Microservice { contract_id, .. } => ("microservice", contract_id.as_ref()),
            Self::Invalid {
                mode_hint,
                contract_hint,
                ..
            } => (mode_hint.as_ref(), contract_hint.as_ref()),
        }
    }
}

fn sha256_hex(json: &str) -> Arc<str> {
    let digest = Sha256::digest(json.as_bytes());
    Arc::from(hex::encode(digest))
}

#[must_use]
pub fn prepare_channel(
    channel_id: &str,
    supplier_code: &str,
    supplier_config: &Value,
    channel_config: &Value,
) -> PreparedChannel {
    let cfg = match SupplierConfig::deserialize(supplier_config) {
        Ok(cfg) => cfg,
        Err(e) => {
            return PreparedChannel::Invalid {
                error: Arc::from(format!("invalid supplier_config JSON: {e}")),
                unknown_contract_id: None,
                mode_hint: Arc::from("unknown"),
                contract_hint: Arc::from("unknown"),
            };
        }
    };
    let timeout_secs = cfg
        .timeouts
        .as_ref()
        .and_then(|t| t.total_ms)
        .map_or(0, |ms| ms.div_ceil(1000).max(1));

    if cfg.is_transparent() {
        let spec = match parse_channel_config_egress(channel_config) {
            Ok(spec) => spec,
            Err(e) => {
                return PreparedChannel::Invalid {
                    error: Arc::from(e.to_string()),
                    unknown_contract_id: None,
                    mode_hint: Arc::from("transparent"),
                    contract_hint: Arc::from(BYPASS_CONTRACT_ID),
                };
            }
        };
        let egress_json_str = serde_json::to_string(&spec).unwrap_or_else(|_| "{}".into());
        let supplier_json_str = serde_json::to_string(&cfg).unwrap_or_else(|_| "{}".into());
        let egress_hash = sha256_hex(&egress_json_str);
        let supplier_hash = sha256_hex(&supplier_json_str);
        let timeout_ms = spec
            .timeout_ms
            .or_else(|| cfg.timeouts.as_ref().and_then(|t| t.total_ms))
            .unwrap_or(timeout_secs.saturating_mul(1000));
        let adapter: Arc<str> = Arc::from(spec.adapter.as_str());
        return PreparedChannel::Transparent {
            adapter,
            timeout_ms,
            supplier_json: Arc::from(supplier_json_str),
            supplier_hash,
            egress_json: Arc::from(egress_json_str),
            egress_hash,
            supplier_sync_key: Arc::from(format!("supplier:{supplier_code}")),
            egress_sync_key: Arc::from(format!("egress:{channel_id}")),
        };
    }

    if cfg.is_microservice() {
        let Some(id) = cfg
            .contract_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return PreparedChannel::Invalid {
                error: Arc::from("microservice supplier missing config.contract_id"),
                unknown_contract_id: None,
                mode_hint: Arc::from("microservice"),
                contract_hint: Arc::from("unknown"),
            };
        };
        if let Err(e) = grpc_path_for_contract(id) {
            return PreparedChannel::Invalid {
                error: Arc::from(format!("upstream contract error: {e}")),
                unknown_contract_id: Some(Arc::from(id)),
                mode_hint: Arc::from("microservice"),
                contract_hint: Arc::from(id),
            };
        }
        let contract_id: Arc<str> = Arc::from(id);
        return PreparedChannel::Microservice {
            contract_id,
            timeout_secs,
        };
    }

    PreparedChannel::Invalid {
        error: Arc::from(format!(
            "invalid upstream_type: '{}' (expect transparent|microservice)",
            cfg.upstream_type
        )),
        unknown_contract_id: None,
        mode_hint: Arc::from("unknown"),
        contract_hint: Arc::from("unknown"),
    }
}

#[derive(Debug, Clone)]
struct RouteSourceRow {
    tenant_route_key: Option<String>,
    product_id: String,
    channel_id: String,
    channel_code: String,
    channel_name: String,
    supplier_id: String,
    supplier_code: String,
    supplier_name: String,
    supplier_upstream_url: String,
    supplier_config: Value,
    channel_config: Value,
    path_prefix: String,
    qps_limit: i32,
    cost_price: String,
    channel_order: i32,
    switch_condition: Value,
    version_tag: Option<String>,
    weight: Option<i32>,
}

const LOAD_ROUTES_SQL: &str = r"
    WITH ranked_rules AS (
        SELECT
            rr.id,
            rr.tenant_id,
            rr.product_id,
            ROW_NUMBER() OVER (
                PARTITION BY rr.tenant_id, rr.product_id
                ORDER BY rr.priority ASC, rr.created_at DESC
            ) AS rule_rank
        FROM route_rules rr
        WHERE rr.status = 'active'
    )
    SELECT
        rr.tenant_id::text AS tenant_route_key,
        p.product_external_id,
        c.channel_id,
        c.channel_code,
        c.channel_name,
        s.supplier_id,
        s.supplier_code,
        s.supplier_name,
        s.supplier_upstream_url,
        s.supplier_config,
        c.channel_config,
        c.path_prefix,
        c.qps_limit,
        c.cost_price,
        rcb.channel_order,
        rcb.switch_condition,
        rcb.version_tag,
        rcb.weight
    FROM ranked_rules rr
    JOIN (
        SELECT id, product_id AS product_external_id, enabled
        FROM products
    ) p ON p.id = rr.product_id
    JOIN route_channel_bindings rcb ON rcb.route_rule_id = rr.id
    JOIN (
        SELECT
            id,
            id::text AS channel_id,
            channel_code,
            name AS channel_name,
            supplier_id,
            config AS channel_config,
            path_prefix,
            qps_limit,
            cost_price::text AS cost_price,
            status
        FROM channels
    ) c ON c.id = rcb.channel_id
    JOIN (
        SELECT
            id,
            id::text AS supplier_id,
            supplier_code,
            name AS supplier_name,
            upstream_url AS supplier_upstream_url,
            config AS supplier_config,
            status
        FROM suppliers
    ) s ON s.id = c.supplier_id
    WHERE rr.rule_rank = 1
      AND p.enabled = true
      AND c.status = 'active'
      AND s.status = 'active'
    ORDER BY rr.tenant_id NULLS LAST, p.product_external_id, rcb.channel_order ASC
";

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn load_route_engine(pool: &PgPool) -> anyhow::Result<GatewayRouteEngine> {
    let rows = sqlx::query(LOAD_ROUTES_SQL).fetch_all(pool).await?;
    let rows = rows
        .iter()
        .map(map_route_source_row)
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let (routes, tenant_overrides) = build_route_map(rows);
    Ok(GatewayRouteEngine::from_scoped_routes(
        routes,
        tenant_overrides,
    ))
}

fn map_route_source_row(row: &sqlx::postgres::PgRow) -> Result<RouteSourceRow, sqlx::Error> {
    Ok(RouteSourceRow {
        tenant_route_key: row.try_get("tenant_route_key")?,
        product_id: row.try_get("product_external_id")?,
        channel_id: row.try_get("channel_id")?,
        channel_code: row.try_get("channel_code")?,
        channel_name: row.try_get("channel_name")?,
        supplier_id: row.try_get("supplier_id")?,
        supplier_code: row.try_get("supplier_code")?,
        supplier_name: row.try_get("supplier_name")?,
        supplier_upstream_url: row.try_get("supplier_upstream_url")?,
        supplier_config: row.try_get("supplier_config").unwrap_or(Value::Null),
        channel_config: row.try_get("channel_config").unwrap_or(Value::Null),
        path_prefix: row.try_get("path_prefix")?,
        qps_limit: row.try_get("qps_limit")?,
        cost_price: row.try_get("cost_price")?,
        channel_order: row.try_get("channel_order")?,
        switch_condition: row.try_get("switch_condition")?,
        version_tag: row.try_get("version_tag")?,
        weight: row.try_get("weight")?,
    })
}

fn build_route_map(
    rows: Vec<RouteSourceRow>,
) -> (
    HashMap<RouteKey, Vec<GatewayCandidateChannel>>,
    HashMap<String, HashSet<String>>,
) {
    let mut routes: HashMap<RouteKey, Vec<GatewayCandidateChannel>> = HashMap::new();
    let mut tenant_overrides: HashMap<String, HashSet<String>> = HashMap::new();
    for row in rows {
        let key = match row.tenant_route_key {
            Some(tenant_route_key) => {
                tenant_overrides
                    .entry(tenant_route_key.clone())
                    .or_default()
                    .insert(row.product_id.clone());
                RouteKey::tenant_product(tenant_route_key, row.product_id)
            }
            None => RouteKey::product_default(row.product_id),
        };
        let rate_limit = ChannelRateLimitOverrides::from_channel_config(&row.channel_config);
        let prepared = prepare_channel(
            &row.channel_id,
            &row.supplier_code,
            &row.supplier_config,
            &row.channel_config,
        );
        routes.entry(key).or_default().push(CandidateChannel {
            channel_id: row.channel_id,
            channel_code: row.channel_code,
            channel_name: row.channel_name,
            supplier_id: row.supplier_id,
            supplier_code: row.supplier_code,
            supplier_name: row.supplier_name,
            supplier_upstream_url: row.supplier_upstream_url,
            supplier_config: row.supplier_config,
            channel_config: row.channel_config,
            path_prefix: row.path_prefix,
            switch_condition: parse_switch_condition(row.switch_condition),
            order: u32::saturating_from(row.channel_order.max(0)),
            version_tag: row.version_tag,
            weight: row.weight.map(|w| u32::saturating_from(w.max(0))),
            qps_limit: row.qps_limit.max(0),
            cost_price: row.cost_price,
            rate_limit,
            prepared,
        });
    }
    (routes, tenant_overrides)
}

fn parse_switch_condition(value: Value) -> SwitchCondition {
    serde_json::from_value(value).unwrap_or(SwitchCondition {
        max_retries: 1,
        timeout_ms: 3000,
        switch_on_timeout: true,
        switch_on_bad_gateway: true,
        switch_on_internal_error: false,
        switch_on_status_codes: Vec::new(),
    })
}

/// 路由全局版本 + 更新时间（用于 lag 指标与 admin status）。
#[derive(Debug, Clone)]
pub struct RouteVersionInfo {
    pub version: i64,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn check_route_version_info(pool: &PgPool) -> Result<RouteVersionInfo, sqlx::Error> {
    let row = sqlx::query(
        "SELECT version, updated_at FROM route_sync_metadata WHERE key = 'global_route_version'",
    )
    .fetch_one(pool)
    .await?;
    Ok(RouteVersionInfo {
        version: row.get("version"),
        updated_at: row.get("updated_at"),
    })
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn check_tenants_count(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM tenants")
        .fetch_one(pool)
        .await?;
    let count: i64 = row.get("count");
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prepare_channel_isolates_invalid_channel_without_failing_others() {
        let good = prepare_channel(
            "ch-1",
            "SUP-1",
            &json!({"upstream_type": "transparent"}),
            &json!({"egress": {"adapter": "http_json", "base_url": "https://api.example.com", "request": {"method": "POST", "path": "/v1", "body_encoding": "json"}}}),
        );
        match good {
            PreparedChannel::Transparent {
                adapter,
                egress_hash,
                supplier_hash,
                egress_sync_key,
                supplier_sync_key,
                ..
            } => {
                assert_eq!(adapter.as_ref(), "http_json");
                assert!(!egress_hash.is_empty());
                assert!(!supplier_hash.is_empty());
                assert_eq!(egress_sync_key.as_ref(), "egress:ch-1");
                assert_eq!(supplier_sync_key.as_ref(), "supplier:SUP-1");
            }
            other => panic!("expected Transparent, got {other:?}"),
        }

        let bad = prepare_channel(
            "ch-bad",
            "SUP-BAD",
            &json!({"upstream_type": "microservice", "contract_id": "NO_SUCH_CONTRACT"}),
            &json!({}),
        );
        match bad {
            PreparedChannel::Invalid {
                unknown_contract_id,
                mode_hint,
                ..
            } => {
                assert_eq!(unknown_contract_id.as_deref(), Some("NO_SUCH_CONTRACT"));
                assert_eq!(mode_hint.as_ref(), "microservice");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}
