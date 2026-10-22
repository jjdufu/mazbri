use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, QueryBuilder, Row};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SupplierRow {
    pub id: uuid::Uuid,
    pub supplier_code: String,
    pub name: String,
    pub upstream_url: String,
    pub status: String,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const SUPPLIER_COLUMNS: &str =
    "id, supplier_code, name, upstream_url, status, config, created_at, updated_at";

fn push_supplier_scope(
    builder: &mut QueryBuilder<'_, sqlx::Postgres>,
    status: Option<&str>,
    search_pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<(), crate::list_query::ListQueryError> {
    if let Some(s) = status {
        builder.push(" AND status = ");
        builder.push_bind(s.to_string());
    }
    if let Some(p) = search_pattern {
        let pat = p.to_string();
        builder.push(" AND (name ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR supplier_code ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR upstream_url ILIKE ");
        builder.push_bind(pat);
        builder.push(")");
    }
    crate::list_query::apply_pg_filters(builder, filters)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_suppliers(
    pool: &PgPool,
    status: Option<&str>,
    search_pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
    page_size: i64,
    offset: i64,
) -> Result<Vec<SupplierRow>, sqlx::Error> {
    let mut builder: QueryBuilder<_> = QueryBuilder::new(format!(
        "SELECT {SUPPLIER_COLUMNS} FROM suppliers WHERE 1=1"
    ));
    push_supplier_scope(&mut builder, status, search_pattern, filters)
        .map_err(|e| sqlx::Error::Protocol(e.message))?;
    builder.push(" ORDER BY created_at DESC LIMIT ");
    builder.push_bind(page_size);
    builder.push(" OFFSET ");
    builder.push_bind(offset);
    let rows = builder.build().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| SupplierRow {
            id: row.get("id"),
            supplier_code: row.get("supplier_code"),
            name: row.get("name"),
            upstream_url: row.get("upstream_url"),
            status: row.get("status"),
            config: row.get("config"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn count_suppliers(
    pool: &PgPool,
    status: Option<&str>,
    search_pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<i64, sqlx::Error> {
    let mut builder: QueryBuilder<_> =
        QueryBuilder::new("SELECT COUNT(*) FROM suppliers WHERE 1=1");
    push_supplier_scope(&mut builder, status, search_pattern, filters)
        .map_err(|e| sqlx::Error::Protocol(e.message))?;
    let total: i64 = builder.build().fetch_one(pool).await?.get(0);
    Ok(total)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn find_by_id(pool: &PgPool, id: uuid::Uuid) -> Result<Option<SupplierRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, supplier_code, name, upstream_url, status, config, created_at, updated_at
         FROM suppliers WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| SupplierRow {
        id: r.get("id"),
        supplier_code: r.get("supplier_code"),
        name: r.get("name"),
        upstream_url: r.get("upstream_url"),
        status: r.get("status"),
        config: r.get("config"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_supplier(
    pool: &PgPool,
    supplier_code: &str,
    name: &str,
    upstream_url: &str,
    config: serde_json::Value,
) -> Result<SupplierRow, sqlx::Error> {
    sqlx::query_as::<_, SupplierRow>(
        "INSERT INTO suppliers (supplier_code, name, upstream_url, status, config)
         VALUES ($1, $2, $3, 'active', $4)
         RETURNING id, supplier_code, name, upstream_url, status, config, created_at, updated_at",
    )
    .bind(supplier_code)
    .bind(name)
    .bind(upstream_url)
    .bind(config)
    .fetch_one(pool)
    .await
}

pub struct UpdateSupplierParams<'a> {
    pub id: uuid::Uuid,
    pub supplier_code: Option<&'a str>,
    pub name: Option<&'a str>,
    pub upstream_url: Option<&'a str>,
    pub status: Option<&'a str>,
    pub config: Option<serde_json::Value>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_supplier(
    pool: &PgPool,
    params: UpdateSupplierParams<'_>,
) -> Result<Option<SupplierRow>, sqlx::Error> {
    let mut builder: QueryBuilder<_> = QueryBuilder::new("UPDATE suppliers SET updated_at = NOW()");
    if let Some(v) = params.supplier_code {
        builder.push(", supplier_code = ");
        builder.push_bind(v);
    }
    if let Some(v) = params.name {
        builder.push(", name = ");
        builder.push_bind(v);
    }
    if let Some(v) = params.upstream_url {
        builder.push(", upstream_url = ");
        builder.push_bind(v);
    }
    if let Some(v) = params.status {
        builder.push(", status = ");
        builder.push_bind(v);
    }
    if let Some(v) = params.config {
        builder.push(", config = ");
        builder.push_bind(v);
    }
    builder.push(" WHERE id = ");
    builder.push_bind(params.id);
    builder.push(" RETURNING ");
    builder.push(SUPPLIER_COLUMNS);
    let row = builder.build().fetch_optional(pool).await?;
    Ok(row.map(|r| SupplierRow {
        id: r.get("id"),
        supplier_code: r.get("supplier_code"),
        name: r.get("name"),
        upstream_url: r.get("upstream_url"),
        status: r.get("status"),
        config: r.get("config"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_supplier(pool: &PgPool, id: uuid::Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM suppliers WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn count_channels_for_supplier(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM channels WHERE supplier_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
}
