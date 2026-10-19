use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{PgPool, QueryBuilder, Row};
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ChannelRow {
    pub id: uuid::Uuid,
    pub supplier_id: uuid::Uuid,
    pub channel_code: String,
    pub name: String,
    pub path_prefix: String,
    pub cost_price: Decimal,
    pub qps_limit: i32,
    pub status: String,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ChannelWithSupplierRow {
    pub id: uuid::Uuid,
    pub supplier_id: uuid::Uuid,
    pub supplier_code: String,
    pub supplier_name: String,
    pub channel_code: String,
    pub name: String,
    pub path_prefix: String,
    pub cost_price: Decimal,
    pub qps_limit: i32,
    pub status: String,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RouteRuleRow {
    pub id: uuid::Uuid,
    pub tenant_id: Option<uuid::Uuid>,
    pub product_id: uuid::Uuid,
    pub priority: i32,
    pub status: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RouteChannelBindingRow {
    pub id: uuid::Uuid,
    pub route_rule_id: uuid::Uuid,
    pub channel_id: uuid::Uuid,
    pub channel_order: i32,
    pub switch_condition: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
pub struct ChannelInsert<'a> {
    pub supplier_id: uuid::Uuid,
    pub channel_code: &'a str,
    pub name: &'a str,
    pub path_prefix: &'a str,
    pub cost_price: Decimal,
    pub qps_limit: i32,
    pub config: &'a serde_json::Value,
}
pub struct ChannelUpdate<'a> {
    pub supplier_id: Option<uuid::Uuid>,
    pub channel_code: Option<&'a str>,
    pub name: Option<&'a str>,
    pub path_prefix: Option<&'a str>,
    pub cost_price: Option<Decimal>,
    pub qps_limit: Option<i32>,
    pub status: Option<&'a str>,
    pub config: Option<&'a serde_json::Value>,
}
pub struct RouteRuleInsert<'a> {
    pub tenant_id: Option<uuid::Uuid>,
    pub product_id: uuid::Uuid,
    pub priority: i32,
    pub status: &'a str,
    pub metadata: &'a serde_json::Value,
}
pub struct RouteRuleUpdate<'a> {
    pub tenant_id: Option<uuid::Uuid>,
    pub product_id: Option<uuid::Uuid>,
    pub priority: Option<i32>,
    pub status: Option<&'a str>,
    pub metadata: Option<&'a serde_json::Value>,
}
pub struct RouteBindingInsert<'a> {
    pub route_rule_id: uuid::Uuid,
    pub channel_id: uuid::Uuid,
    pub channel_order: i32,
    pub switch_condition: &'a serde_json::Value,
}
pub struct RouteBindingUpdate<'a> {
    pub channel_id: Option<uuid::Uuid>,
    pub channel_order: Option<i32>,
    pub switch_condition: Option<&'a serde_json::Value>,
}
const CHANNEL_COLUMNS: &str = "id, supplier_id, channel_code, name, path_prefix, cost_price, qps_limit, status, config, created_at, updated_at";
const ROUTE_RULE_COLUMNS: &str =
    "id, tenant_id, product_id, priority, status, metadata, created_at, updated_at";
const ROUTE_BINDING_COLUMNS: &str =
    "id, route_rule_id, channel_id, channel_order, switch_condition, created_at, updated_at";
pub const ROUTE_RULE_COLUMNS_FOR_SERVICE: &str = ROUTE_RULE_COLUMNS;
pub const ROUTE_BINDING_COLUMNS_FOR_SERVICE: &str = ROUTE_BINDING_COLUMNS;
fn push_channel_scope<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Postgres>,
    status: Option<&str>,
    search_pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<(), crate::list_query::ListQueryError> {
    if let Some(status) = status {
        builder.push(" AND c.status = ");
        builder.push_bind(status.to_string());
    }
    if let Some(pattern) = search_pattern {
        let pat = pattern.to_string();
        builder.push(" AND (c.name ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR c.channel_code ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR c.path_prefix ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR s.name ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR s.supplier_code ILIKE ");
        builder.push_bind(pat);
        builder.push(")");
    }
    crate::list_query::apply_pg_filters(builder, filters)
}

pub async fn list_channels(
    pool: &PgPool,
    status: Option<&str>,
    search_pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
    page_size: i64,
    offset: i64,
) -> Result<Vec<ChannelWithSupplierRow>, sqlx::Error> {
    let mut builder: QueryBuilder<_> = QueryBuilder::new(
        "SELECT c.id, c.supplier_id, s.supplier_code, s.name AS supplier_name, c.channel_code, c.name, c.path_prefix, c.cost_price, c.qps_limit, c.status, c.config, c.created_at, c.updated_at FROM channels c JOIN suppliers s ON s.id = c.supplier_id WHERE 1=1",
    );
    push_channel_scope(&mut builder, status, search_pattern, filters)
        .map_err(|e| sqlx::Error::Protocol(e.message))?;
    builder.push(" ORDER BY c.created_at DESC LIMIT ");
    builder.push_bind(page_size);
    builder.push(" OFFSET ");
    builder.push_bind(offset);
    builder.build_query_as().fetch_all(pool).await
}
pub async fn count_channels(
    pool: &PgPool,
    status: Option<&str>,
    search_pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<i64, sqlx::Error> {
    let mut builder: QueryBuilder<_> = QueryBuilder::new(
        "SELECT COUNT(*) FROM channels c JOIN suppliers s ON s.id = c.supplier_id WHERE 1=1",
    );
    push_channel_scope(&mut builder, status, search_pattern, filters)
        .map_err(|e| sqlx::Error::Protocol(e.message))?;
    Ok(builder.build().fetch_one(pool).await?.get(0))
}
pub async fn find_channel_with_supplier_by_id(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<Option<ChannelWithSupplierRow>, sqlx::Error> {
    sqlx::query_as::<
        _,
        ChannelWithSupplierRow,
    >(
            "SELECT c.id, c.supplier_id, s.supplier_code, s.name AS supplier_name, c.channel_code, c.name, c.path_prefix, c.cost_price, c.qps_limit, c.status, c.config, c.created_at, c.updated_at
         FROM channels c
         JOIN suppliers s ON s.id = c.supplier_id
         WHERE c.id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
}
pub async fn find_channel_by_id(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<Option<ChannelRow>, sqlx::Error> {
    sqlx::query_as::<_, ChannelRow>(&format!(
        "SELECT {} FROM channels WHERE id = $1",
        CHANNEL_COLUMNS
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}
pub async fn insert_channel(
    pool: &PgPool,
    input: ChannelInsert<'_>,
) -> Result<ChannelRow, sqlx::Error> {
    sqlx::query_as::<
        _,
        ChannelRow,
    >(
            &format!(
                "INSERT INTO channels (supplier_id, channel_code, name, path_prefix, cost_price, qps_limit, config)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {}",
                CHANNEL_COLUMNS
            ),
        )
        .bind(input.supplier_id)
        .bind(input.channel_code)
        .bind(input.name)
        .bind(input.path_prefix)
        .bind(input.cost_price)
        .bind(input.qps_limit)
        .bind(input.config)
        .fetch_one(pool)
        .await
}
pub async fn update_channel(
    pool: &PgPool,
    id: uuid::Uuid,
    input: ChannelUpdate<'_>,
) -> Result<Option<ChannelRow>, sqlx::Error> {
    let mut builder: QueryBuilder<_> = QueryBuilder::new("UPDATE channels SET updated_at = NOW()");
    if let Some(value) = input.supplier_id {
        builder.push(", supplier_id = ");
        builder.push_bind(value);
    }
    if let Some(value) = input.channel_code {
        builder.push(", channel_code = ");
        builder.push_bind(value);
    }
    if let Some(value) = input.name {
        builder.push(", name = ");
        builder.push_bind(value);
    }
    if let Some(value) = input.path_prefix {
        builder.push(", path_prefix = ");
        builder.push_bind(value);
    }
    if let Some(value) = input.cost_price {
        builder.push(", cost_price = ");
        builder.push_bind(value);
    }
    if let Some(value) = input.qps_limit {
        builder.push(", qps_limit = ");
        builder.push_bind(value);
    }
    if let Some(value) = input.status {
        builder.push(", status = ");
        builder.push_bind(value);
    }
    if let Some(value) = input.config {
        builder.push(", config = ");
        builder.push_bind(value);
    }
    builder.push(" WHERE id = ");
    builder.push_bind(id);
    builder.push(" RETURNING ");
    builder.push(CHANNEL_COLUMNS);
    builder.build_query_as().fetch_optional(pool).await
}
pub async fn delete_channel(pool: &PgPool, id: uuid::Uuid) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query("DELETE FROM channels WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected())
}
pub async fn count_route_bindings_for_channel(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM route_channel_bindings WHERE channel_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
}
fn push_route_rule_scope<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Postgres>,
    tenant_id: Option<uuid::Uuid>,
    product_id: Option<uuid::Uuid>,
    search: Option<&str>,
    status: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<(), crate::list_query::ListQueryError> {
    if let Some(value) = tenant_id {
        builder.push(" AND tenant_id = ");
        builder.push_bind(value);
    }
    if let Some(value) = product_id {
        builder.push(" AND product_id = ");
        builder.push_bind(value);
    }
    if let Some(value) = search.map(str::trim).filter(|value| !value.is_empty()) {
        let pattern = format!("%{value}%");
        builder.push(" AND (metadata #>> '{route_config,product_name}' ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" OR metadata #>> '{route_config,product_code}' ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" OR metadata #>> '{route_config,tenant_name}' ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" OR metadata #>> '{route_config,tenant_code}' ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" OR product_id::text ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" OR tenant_id::text ILIKE ");
        builder.push_bind(pattern);
        builder.push(")");
    }
    if let Some(value) = status {
        builder.push(" AND status = ");
        builder.push_bind(value.to_string());
    }
    crate::list_query::apply_pg_filters(builder, filters)
}

pub async fn list_route_rules(
    pool: &PgPool,
    tenant_id: Option<uuid::Uuid>,
    product_id: Option<uuid::Uuid>,
    search: Option<&str>,
    status: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
    page_size: i64,
    offset: i64,
) -> Result<Vec<RouteRuleRow>, sqlx::Error> {
    let mut builder: QueryBuilder<_> = QueryBuilder::new(format!(
        "SELECT {} FROM route_rules WHERE 1=1",
        ROUTE_RULE_COLUMNS
    ));
    push_route_rule_scope(&mut builder, tenant_id, product_id, search, status, filters)
        .map_err(|e| sqlx::Error::Protocol(e.message))?;
    builder.push(" ORDER BY priority ASC, created_at DESC LIMIT ");
    builder.push_bind(page_size);
    builder.push(" OFFSET ");
    builder.push_bind(offset);
    builder.build_query_as().fetch_all(pool).await
}
pub async fn count_route_rules(
    pool: &PgPool,
    tenant_id: Option<uuid::Uuid>,
    product_id: Option<uuid::Uuid>,
    search: Option<&str>,
    status: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<i64, sqlx::Error> {
    let mut builder: QueryBuilder<_> =
        QueryBuilder::new("SELECT COUNT(*) FROM route_rules WHERE 1=1");
    push_route_rule_scope(&mut builder, tenant_id, product_id, search, status, filters)
        .map_err(|e| sqlx::Error::Protocol(e.message))?;
    Ok(builder.build().fetch_one(pool).await?.get(0))
}
pub async fn find_route_rule_by_id(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<Option<RouteRuleRow>, sqlx::Error> {
    sqlx::query_as::<_, RouteRuleRow>(&format!(
        "SELECT {} FROM route_rules WHERE id = $1",
        ROUTE_RULE_COLUMNS
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}
pub async fn insert_route_rule(
    pool: &PgPool,
    input: RouteRuleInsert<'_>,
) -> Result<RouteRuleRow, sqlx::Error> {
    sqlx::query_as::<_, RouteRuleRow>(&format!(
        "INSERT INTO route_rules (tenant_id, product_id, priority, status, metadata)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {}",
        ROUTE_RULE_COLUMNS
    ))
    .bind(input.tenant_id)
    .bind(input.product_id)
    .bind(input.priority)
    .bind(input.status)
    .bind(input.metadata)
    .fetch_one(pool)
    .await
}
pub async fn update_route_rule(
    pool: &PgPool,
    id: uuid::Uuid,
    input: RouteRuleUpdate<'_>,
) -> Result<Option<RouteRuleRow>, sqlx::Error> {
    let mut builder: QueryBuilder<_> =
        QueryBuilder::new("UPDATE route_rules SET updated_at = NOW()");
    if let Some(value) = input.tenant_id {
        builder.push(", tenant_id = ");
        builder.push_bind(value);
    }
    if let Some(value) = input.product_id {
        builder.push(", product_id = ");
        builder.push_bind(value);
    }
    if let Some(value) = input.priority {
        builder.push(", priority = ");
        builder.push_bind(value);
    }
    if let Some(value) = input.status {
        builder.push(", status = ");
        builder.push_bind(value);
    }
    if let Some(value) = input.metadata {
        builder.push(", metadata = ");
        builder.push_bind(value);
    }
    builder.push(" WHERE id = ");
    builder.push_bind(id);
    builder.push(" RETURNING ");
    builder.push(ROUTE_RULE_COLUMNS);
    builder.build_query_as().fetch_optional(pool).await
}
pub async fn delete_route_rule(pool: &PgPool, id: uuid::Uuid) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query("DELETE FROM route_rules WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected())
}
pub async fn list_route_bindings(
    pool: &PgPool,
    route_rule_id: uuid::Uuid,
) -> Result<Vec<RouteChannelBindingRow>, sqlx::Error> {
    sqlx::query_as::<_, RouteChannelBindingRow>(&format!(
        "SELECT {} FROM route_channel_bindings WHERE route_rule_id = $1 ORDER BY channel_order ASC",
        ROUTE_BINDING_COLUMNS
    ))
    .bind(route_rule_id)
    .fetch_all(pool)
    .await
}
pub async fn find_route_binding_by_id(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<Option<RouteChannelBindingRow>, sqlx::Error> {
    sqlx::query_as::<_, RouteChannelBindingRow>(&format!(
        "SELECT {} FROM route_channel_bindings WHERE id = $1",
        ROUTE_BINDING_COLUMNS
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}
pub async fn insert_route_binding(
    pool: &PgPool,
    input: RouteBindingInsert<'_>,
) -> Result<RouteChannelBindingRow, sqlx::Error> {
    sqlx::query_as::<
        _,
        RouteChannelBindingRow,
    >(
            &format!(
                "INSERT INTO route_channel_bindings (route_rule_id, channel_id, channel_order, switch_condition)
         VALUES ($1, $2, $3, $4)
         RETURNING {}",
                ROUTE_BINDING_COLUMNS
            ),
        )
        .bind(input.route_rule_id)
        .bind(input.channel_id)
        .bind(input.channel_order)
        .bind(input.switch_condition)
        .fetch_one(pool)
        .await
}
pub async fn update_route_binding(
    pool: &PgPool,
    id: uuid::Uuid,
    input: RouteBindingUpdate<'_>,
) -> Result<Option<RouteChannelBindingRow>, sqlx::Error> {
    let mut builder: QueryBuilder<_> =
        QueryBuilder::new("UPDATE route_channel_bindings SET updated_at = NOW()");
    if let Some(value) = input.channel_id {
        builder.push(", channel_id = ");
        builder.push_bind(value);
    }
    if let Some(value) = input.channel_order {
        builder.push(", channel_order = ");
        builder.push_bind(value);
    }
    if let Some(value) = input.switch_condition {
        builder.push(", switch_condition = ");
        builder.push_bind(value);
    }
    builder.push(" WHERE id = ");
    builder.push_bind(id);
    builder.push(" RETURNING ");
    builder.push(ROUTE_BINDING_COLUMNS);
    builder.build_query_as().fetch_optional(pool).await
}
pub async fn delete_route_binding(pool: &PgPool, id: uuid::Uuid) -> Result<u64, sqlx::Error> {
    Ok(
        sqlx::query("DELETE FROM route_channel_bindings WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

pub async fn get_max_channel_code_matching_pattern(
    pool: &PgPool,
) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT channel_code FROM channels WHERE channel_code ~ '^8[0-9]{8}$' ORDER BY channel_code DESC LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(code,)| code))
}
