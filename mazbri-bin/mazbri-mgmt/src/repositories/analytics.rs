use sqlx::{PgPool, QueryBuilder};
type Qb<'a> = QueryBuilder<'a, sqlx::Postgres>;
pub async fn get_total_tenants(db: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM tenants")
        .fetch_one(db)
        .await
        .unwrap_or_default()
}
pub async fn get_active_tenants(db: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM tenants WHERE status = 'active'")
        .fetch_one(db)
        .await
        .unwrap_or_default()
}
pub async fn get_total_products(db: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE enabled = true")
        .fetch_one(db)
        .await
        .unwrap_or_default()
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn tenant_exists(db: &PgPool, tenant_id: uuid::Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_one(db)
        .await
}
#[must_use]
pub fn build_usage_analytics_query(
    sql_template: &str,
    tenant_id: uuid::Uuid,
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Qb<'static> {
    let mut builder = QueryBuilder::new(sql_template);
    builder.push_bind(tenant_id);
    if let Some(start) = start_time {
        builder.push(" AND recorded_at >= TO_TIMESTAMP(");
        builder.push_bind(start);
        builder.push(")");
    }
    if let Some(end) = end_time {
        builder.push(" AND recorded_at <= TO_TIMESTAMP(");
        builder.push_bind(end);
        builder.push(")");
    }
    builder.push(" GROUP BY period ORDER BY period DESC LIMIT 100");
    builder
}
#[must_use]
pub fn build_top_endpoints_query(
    tenant_id: uuid::Uuid,
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Qb<'static> {
    let mut builder = QueryBuilder::new(
        "SELECT api_endpoint, http_method, COUNT(*) AS request_count, AVG(billing_service_duration_ms) AS avg_billing_service_duration_ms, SUM(billable_amount) AS total_cost \
         FROM usage_records WHERE tenant_id = ",
    );
    builder.push_bind(tenant_id);
    if let Some(start) = start_time {
        builder.push(" AND recorded_at >= TO_TIMESTAMP(");
        builder.push_bind(start);
        builder.push(")");
    }
    if let Some(end) = end_time {
        builder.push(" AND recorded_at <= TO_TIMESTAMP(");
        builder.push_bind(end);
        builder.push(")");
    }
    builder.push(" GROUP BY api_endpoint, http_method ORDER BY request_count DESC LIMIT 10");
    builder
}
#[must_use]
pub fn build_status_distribution_query(
    tenant_id: uuid::Uuid,
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Qb<'static> {
    let mut builder = QueryBuilder::new(
        "SELECT response_status, COUNT(*) AS count FROM usage_records WHERE tenant_id = ",
    );
    builder.push_bind(tenant_id);
    if let Some(start) = start_time {
        builder.push(" AND recorded_at >= TO_TIMESTAMP(");
        builder.push_bind(start);
        builder.push(")");
    }
    if let Some(end) = end_time {
        builder.push(" AND recorded_at <= TO_TIMESTAMP(");
        builder.push_bind(end);
        builder.push(")");
    }
    builder.push(" GROUP BY response_status ORDER BY count DESC");
    builder
}
#[must_use]
pub fn build_ratelimit_analytics_query(
    sql_template: &str,
    tenant_id: uuid::Uuid,
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Qb<'static> {
    let mut builder = QueryBuilder::new(sql_template);
    builder.push_bind(tenant_id);
    if let Some(start) = start_time {
        builder.push(" AND created_at >= TO_TIMESTAMP(");
        builder.push_bind(start);
        builder.push(")");
    }
    if let Some(end) = end_time {
        builder.push(" AND created_at <= TO_TIMESTAMP(");
        builder.push_bind(end);
        builder.push(")");
    }
    builder.push(" GROUP BY period ORDER BY period DESC LIMIT 100");
    builder
}
#[must_use]
pub fn build_denial_reasons_query(
    tenant_id: uuid::Uuid,
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Qb<'static> {
    let mut builder = QueryBuilder::new(
        "SELECT reason, COUNT(*) AS count, COUNT(*) * 100.0 / NULLIF(SUM(COUNT(*)) OVER (), 0) AS percentage \
         FROM rate_limit_events WHERE tenant_id = ",
    );
    builder.push_bind(tenant_id);
    builder.push(" AND allowed = false");
    if let Some(start) = start_time {
        builder.push(" AND created_at >= TO_TIMESTAMP(");
        builder.push_bind(start);
        builder.push(")");
    }
    if let Some(end) = end_time {
        builder.push(" AND created_at <= TO_TIMESTAMP(");
        builder.push_bind(end);
        builder.push(")");
    }
    builder.push(" GROUP BY reason ORDER BY count DESC");
    builder
}
#[must_use]
pub fn build_scope_breakdown_query(
    tenant_id: uuid::Uuid,
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Qb<'static> {
    let mut builder = QueryBuilder::new(
        "SELECT scope, counter_type, COUNT(*) AS total_checks, \
         COUNT(*) FILTER (WHERE allowed = true) AS allowed_count, \
         COUNT(*) FILTER (WHERE allowed = false) AS denied_count \
         FROM rate_limit_events WHERE tenant_id = ",
    );
    builder.push_bind(tenant_id);
    if let Some(start) = start_time {
        builder.push(" AND created_at >= TO_TIMESTAMP(");
        builder.push_bind(start);
        builder.push(")");
    }
    if let Some(end) = end_time {
        builder.push(" AND created_at <= TO_TIMESTAMP(");
        builder.push_bind(end);
        builder.push(")");
    }
    builder.push(" GROUP BY scope, counter_type ORDER BY total_checks DESC");
    builder
}
#[must_use]
pub fn build_top_denied_apis_query(
    tenant_id: uuid::Uuid,
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Qb<'static> {
    let mut builder = QueryBuilder::new(
        "SELECT api_endpoint, COUNT(*) AS total_checks, \
         COUNT(*) FILTER (WHERE allowed = false) AS denied_count, \
         CASE WHEN COUNT(*) > 0 THEN COUNT(*) FILTER (WHERE allowed = false) * 100.0 / COUNT(*) ELSE 0 END AS denial_rate \
         FROM rate_limit_events WHERE tenant_id = ",
    );
    builder.push_bind(tenant_id);
    builder.push(" AND api_endpoint IS NOT NULL");
    if let Some(start) = start_time {
        builder.push(" AND created_at >= TO_TIMESTAMP(");
        builder.push_bind(start);
        builder.push(")");
    }
    if let Some(end) = end_time {
        builder.push(" AND created_at <= TO_TIMESTAMP(");
        builder.push_bind(end);
        builder.push(")");
    }
    builder.push(" GROUP BY api_endpoint ORDER BY denied_count DESC LIMIT 10");
    builder
}
#[must_use]
pub fn build_ratelimit_platform_overall_query(
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Qb<'static> {
    let mut builder = QueryBuilder::new(
        "SELECT COUNT(*) AS total_checks, COUNT(*) FILTER (WHERE allowed = true) AS total_allowed, \
         COUNT(*) FILTER (WHERE allowed = false) AS total_denied, COUNT(DISTINCT tenant_id) AS active_tenants \
         FROM rate_limit_events WHERE 1=1",
    );
    if let Some(start) = start_time {
        builder.push(" AND created_at >= TO_TIMESTAMP(");
        builder.push_bind(start);
        builder.push(")");
    }
    if let Some(end) = end_time {
        builder.push(" AND created_at <= TO_TIMESTAMP(");
        builder.push_bind(end);
        builder.push(")");
    }
    builder
}
#[must_use]
pub fn build_ratelimit_platform_top_tenants_query(
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Qb<'static> {
    let mut builder = QueryBuilder::new(
        "SELECT rle.tenant_id, t.name AS tenant_name, COUNT(*) AS total_checks, \
         COUNT(*) FILTER (WHERE rle.allowed = false) AS denied_count \
         FROM rate_limit_events rle LEFT JOIN tenants t ON rle.tenant_id = t.id WHERE 1=1",
    );
    if let Some(start) = start_time {
        builder.push(" AND rle.created_at >= TO_TIMESTAMP(");
        builder.push_bind(start);
        builder.push(")");
    }
    if let Some(end) = end_time {
        builder.push(" AND rle.created_at <= TO_TIMESTAMP(");
        builder.push_bind(end);
        builder.push(")");
    }
    builder.push(" GROUP BY rle.tenant_id, t.name ORDER BY denied_count DESC LIMIT 10");
    builder
}
#[must_use]
pub fn build_ratelimit_platform_scope_dist_query(
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Qb<'static> {
    let mut builder = QueryBuilder::new(
        "SELECT scope, counter_type, COUNT(*) AS total_checks, COUNT(*) FILTER (WHERE allowed = false) AS denied_count \
         FROM rate_limit_events WHERE 1=1",
    );
    if let Some(start) = start_time {
        builder.push(" AND created_at >= TO_TIMESTAMP(");
        builder.push_bind(start);
        builder.push(")");
    }
    if let Some(end) = end_time {
        builder.push(" AND created_at <= TO_TIMESTAMP(");
        builder.push_bind(end);
        builder.push(")");
    }
    builder.push(" GROUP BY scope, counter_type ORDER BY total_checks DESC");
    builder
}
pub async fn get_active_rate_limit_configs(db: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM rate_limit_tenant_policies WHERE enabled = true")
        .fetch_one(db)
        .await
        .unwrap_or_default()
}
