use mazbri_routing::{CandidateChannel, RouteEngine, RouteKey, SwitchCondition};
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
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
    channel_order: i32,
    switch_condition: Value,
    version_tag: Option<String>,
    weight: Option<i32>,
}
pub async fn load_route_engine(pool: &PgPool) -> anyhow::Result<RouteEngine> {
    let rows = sqlx::query(
        r#"
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
            p.product_id AS product_external_id,
            c.id::text AS channel_id,
            c.channel_code,
            c.name AS channel_name,
            s.id::text AS supplier_id,
            s.supplier_code,
            s.name AS supplier_name,
            s.upstream_url AS supplier_upstream_url,
            s.config AS supplier_config,
            c.config AS channel_config,
            c.path_prefix,
            c.qps_limit,
            rcb.channel_order,
            rcb.switch_condition,
            rcb.version_tag,
            rcb.weight
        FROM ranked_rules rr
        JOIN products p ON p.id = rr.product_id
        JOIN route_channel_bindings rcb ON rcb.route_rule_id = rr.id
        JOIN channels c ON c.id = rcb.channel_id
        JOIN suppliers s ON s.id = c.supplier_id
        WHERE rr.rule_rank = 1
          AND p.enabled = true
          AND c.status = 'active'
          AND s.status = 'active'
        ORDER BY rr.tenant_id NULLS LAST, p.product_id, rcb.channel_order ASC
        "#,
    )
    .fetch_all(pool)
    .await?;
    let rows = rows
        .into_iter()
        .map(|row| {
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
                supplier_config: row
                    .try_get("supplier_config")
                    .unwrap_or(serde_json::Value::Null),
                channel_config: row
                    .try_get("channel_config")
                    .unwrap_or(serde_json::Value::Null),
                path_prefix: row.try_get("path_prefix")?,
                qps_limit: row.try_get("qps_limit")?,
                channel_order: row.try_get("channel_order")?,
                switch_condition: row.try_get("switch_condition")?,
                version_tag: row.try_get("version_tag")?,
                weight: row.try_get("weight")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(RouteEngine::with_scoped_routes(build_route_map(rows)))
}
fn build_route_map(rows: Vec<RouteSourceRow>) -> HashMap<RouteKey, Vec<CandidateChannel>> {
    let mut routes: HashMap<RouteKey, Vec<CandidateChannel>> = HashMap::new();
    for row in rows {
        let key = match row.tenant_route_key {
            Some(tenant_route_key) => RouteKey::tenant_product(tenant_route_key, row.product_id),
            None => RouteKey::product_default(row.product_id),
        };
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
            order: row.channel_order.max(0) as u32,
            version_tag: row.version_tag,
            weight: row.weight.map(|w| w.max(0) as u32),
            qps_limit: row.qps_limit.max(0),
        });
    }
    routes
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

pub async fn check_tenants_count(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM tenants")
        .fetch_one(pool)
        .await?;
    let count: i64 = row.get("count");
    Ok(count)
}
