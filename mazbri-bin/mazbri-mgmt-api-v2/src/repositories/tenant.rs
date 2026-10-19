use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, QueryBuilder, Row};
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TenantRow {
    #[serde(skip_serializing)]
    pub id: uuid::Uuid,
    pub tenant_code: String,
    pub tenant_id: String,
    pub name: String,
    pub contact_person: Option<String>,
    pub contact_phone: Option<String>,
    pub contact_email: Option<String>,
    pub status: String,
    pub token_expiration_secs: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TenantCreateRow {
    #[serde(skip_serializing)]
    pub id: uuid::Uuid,
    pub tenant_code: String,
    pub tenant_id: String,
    pub name: String,
    pub contact_person: Option<String>,
    pub contact_phone: Option<String>,
    pub contact_email: Option<String>,
    pub status: String,
    pub token_expiration_secs: Option<i32>,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TenantUpdateRow {
    #[serde(skip_serializing)]
    pub id: uuid::Uuid,
    pub tenant_code: String,
    pub tenant_id: String,
    pub name: String,
    pub status: String,
    pub contact_email: Option<String>,
    pub token_expiration_secs: Option<i32>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TenantStatusRow {
    #[serde(skip_serializing)]
    pub id: uuid::Uuid,
    pub tenant_code: String,
    pub name: String,
    pub status: String,
}
fn push_tenant_scope<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Postgres>,
    status: Option<&str>,
    search_pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<(), crate::list_query::ListQueryError> {
    if let Some(s) = status {
        builder.push(" AND status = ");
        builder.push_bind(s.to_string());
    }
    if let Some(p) = search_pattern {
        // q covers catalog in_q fields: tenant_id, name, tenant_code, contact_*
        let pat = p.to_string();
        builder.push(" AND (tenant_id ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR name ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR tenant_code ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR contact_email ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR contact_person ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR contact_phone ILIKE ");
        builder.push_bind(pat);
        builder.push(")");
    }
    crate::list_query::apply_pg_filters(builder, filters)
}

pub async fn count_tenants(
    pool: &PgPool,
    status: Option<&str>,
    search_pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<i64, sqlx::Error> {
    let mut builder: QueryBuilder<_> = QueryBuilder::new("SELECT COUNT(*) FROM tenants WHERE 1=1");
    push_tenant_scope(&mut builder, status, search_pattern, filters)
        .map_err(|e| sqlx::Error::Protocol(e.message))?;
    let total: i64 = builder.build().fetch_one(pool).await?.get(0);
    Ok(total)
}
pub async fn list_tenants(
    pool: &PgPool,
    status: Option<&str>,
    search_pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
    page_size: i64,
    offset: i64,
) -> Result<Vec<TenantRow>, sqlx::Error> {
    let mut builder: QueryBuilder<_> = QueryBuilder::new(
        "SELECT id, tenant_code, tenant_id, name, contact_person, contact_phone, contact_email, status, token_expiration_secs, created_at, updated_at FROM tenants WHERE 1=1",
    );
    push_tenant_scope(&mut builder, status, search_pattern, filters)
        .map_err(|e| sqlx::Error::Protocol(e.message))?;
    builder.push(" ORDER BY created_at DESC LIMIT ");
    builder.push_bind(page_size);
    builder.push(" OFFSET ");
    builder.push_bind(offset);
    let rows = builder.build().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| TenantRow {
            id: row.get("id"),
            tenant_code: row.get("tenant_code"),
            tenant_id: row.get("tenant_id"),
            name: row.get("name"),
            contact_person: row.get("contact_person"),
            contact_phone: row.get("contact_phone"),
            contact_email: row.get("contact_email"),
            status: row.get("status"),
            token_expiration_secs: row.get("token_expiration_secs"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect())
}
pub async fn count_by_tenant_code(pool: &PgPool, tenant_code: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tenants WHERE tenant_code = $1")
        .bind(tenant_code)
        .fetch_one(pool)
        .await
}
pub async fn count_by_tenant_id(pool: &PgPool, tenant_id: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tenants WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
}
pub async fn get_id_by_code(
    pool: &PgPool,
    tenant_code: &str,
) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM tenants WHERE tenant_code = $1")
        .bind(tenant_code)
        .fetch_optional(pool)
        .await
}
pub struct InsertTenantParams<'a> {
    pub tenant_code: &'a str,
    pub tenant_id: &'a str,
    pub name: &'a str,
    pub contact_person: Option<&'a str>,
    pub contact_phone: Option<&'a str>,
    pub contact_email: Option<&'a str>,
    pub token_expiration_secs: Option<i32>,
}

pub async fn insert_tenant(
    pool: &PgPool,
    params: InsertTenantParams<'_>,
) -> Result<TenantCreateRow, sqlx::Error> {
    sqlx::query_as::<
        _,
        TenantCreateRow,
    >(
            r#"
        INSERT INTO tenants (tenant_code, tenant_id, name, contact_person, contact_phone, contact_email, status, token_expiration_secs)
        VALUES ($1, $2, $3, $4, $5, $6, 'active', $7)
        RETURNING id, tenant_code, tenant_id, name, contact_person, contact_phone, contact_email, status, token_expiration_secs, created_at
    "#,
        )
        .bind(params.tenant_code)
        .bind(params.tenant_id)
        .bind(params.name)
        .bind(params.contact_person)
        .bind(params.contact_phone)
        .bind(params.contact_email)
        .bind(params.token_expiration_secs)
        .fetch_one(pool)
        .await
}
pub async fn find_by_id(pool: &PgPool, id: uuid::Uuid) -> Result<Option<TenantRow>, sqlx::Error> {
    sqlx::query_as::<
        _,
        TenantRow,
    >(
            "SELECT id, tenant_code, tenant_id, name, contact_person, contact_phone, contact_email, status, token_expiration_secs, created_at, updated_at FROM tenants WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
}
pub async fn update_tenant(
    pool: &PgPool,
    id: uuid::Uuid,
    name: Option<&str>,
    contact_person: Option<&str>,
    contact_phone: Option<&str>,
    contact_email: Option<&str>,
    token_expiration_secs: Option<i32>,
) -> Result<Option<TenantUpdateRow>, sqlx::Error> {
    let mut builder: QueryBuilder<_> = QueryBuilder::new("UPDATE tenants SET updated_at = NOW()");
    if let Some(v) = name {
        builder.push(", name = ");
        builder.push_bind(v);
    }
    if let Some(v) = contact_person {
        builder.push(", contact_person = ");
        builder.push_bind(v);
    }
    if let Some(v) = contact_phone {
        builder.push(", contact_phone = ");
        builder.push_bind(v);
    }
    if let Some(v) = contact_email {
        builder.push(", contact_email = ");
        builder.push_bind(v);
    }
    if let Some(v) = token_expiration_secs {
        builder.push(", token_expiration_secs = ");
        builder.push_bind(v);
    }
    builder.push(" WHERE id = ");
    builder.push_bind(id);
    builder
        .push(
            " RETURNING id, tenant_code, tenant_id, name, status, contact_email, token_expiration_secs, updated_at",
        );
    let row = builder.build().fetch_optional(pool).await?;
    Ok(row.map(|row| TenantUpdateRow {
        id: row.get("id"),
        tenant_code: row.get("tenant_code"),
        tenant_id: row.get("tenant_id"),
        name: row.get("name"),
        status: row.get("status"),
        contact_email: row.get("contact_email"),
        token_expiration_secs: row.get("token_expiration_secs"),
        updated_at: row.get("updated_at"),
    }))
}
/// Load identity fields before hard-delete (for audit).
pub async fn get_status_row(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<Option<TenantStatusRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantStatusRow>(
        "SELECT id, tenant_code, name, status FROM tenants WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Physical delete. Platform child tables use ON DELETE CASCADE.
pub async fn delete_by_id(pool: &PgPool, id: uuid::Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
pub async fn set_status(
    pool: &PgPool,
    id: uuid::Uuid,
    status: &str,
) -> Result<Option<TenantStatusRow>, sqlx::Error> {
    sqlx::query_as::<
        _,
        TenantStatusRow,
    >(
            "UPDATE tenants SET status = $1, updated_at = NOW() WHERE id = $2 RETURNING id, tenant_code, name, status",
        )
        .bind(status)
        .bind(id)
        .fetch_optional(pool)
        .await
}
pub async fn get_tenant_identities(
    pool: &PgPool,
    ids: &[uuid::Uuid],
) -> Result<std::collections::HashMap<uuid::Uuid, (String, String)>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let rows = sqlx::query("SELECT id, tenant_code, tenant_id FROM tenants WHERE id = ANY($1)")
        .bind(ids)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get("id"),
                (row.get("tenant_code"), row.get("tenant_id")),
            )
        })
        .collect())
}
#[derive(Debug, Clone)]
pub struct TenantDisplayIdentity {
    pub tenant_code: String,
    pub name: String,
}
pub async fn get_tenant_display_identities(
    pool: &PgPool,
    ids: &[uuid::Uuid],
) -> Result<std::collections::HashMap<uuid::Uuid, TenantDisplayIdentity>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let rows = sqlx::query("SELECT id, tenant_code, name FROM tenants WHERE id = ANY($1)")
        .bind(ids)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get("id"),
                TenantDisplayIdentity {
                    tenant_code: row.get::<String, _>("tenant_code").trim().to_string(),
                    name: row.get("name"),
                },
            )
        })
        .collect())
}

/// Resolve tenant UUIDs for free-text search (name / tenant_code / tenant_id).
/// Used by cross-db list search (e.g. bills in billing DB).
pub async fn search_tenant_ids(
    pool: &PgPool,
    pattern: &str,
) -> Result<Vec<uuid::Uuid>, sqlx::Error> {
    let needle = pattern.trim();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let like = format!("%{needle}%");
    sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM tenants
         WHERE name ILIKE $1
            OR tenant_code ILIKE $1
            OR tenant_id ILIKE $1
         LIMIT 500",
    )
    .bind(like)
    .fetch_all(pool)
    .await
}
