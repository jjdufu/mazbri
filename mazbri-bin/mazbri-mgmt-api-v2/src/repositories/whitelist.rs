use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};
#[derive(Debug, Serialize)]
pub struct WhitelistEntry {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub ip_address: String,
    pub cidr: i32,
    pub description: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Serialize)]
pub struct InsertedEntry {
    pub id: uuid::Uuid,
    pub ip_address: String,
    pub cidr: i32,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}
pub async fn fetch_entries(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<Vec<WhitelistEntry>, sqlx::Error> {
    let query = r#"
        SELECT id, tenant_id, ip_address, cidr, description, is_active, created_at, updated_at
        FROM ip_whitelist
        WHERE tenant_id = $1
          AND is_active = true
        ORDER BY created_at DESC
    "#;
    let rows = sqlx::query(query).bind(tenant_id).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let ip_network: ipnetwork::IpNetwork = row.get("ip_address");
            WhitelistEntry {
                id: row.get("id"),
                tenant_id: row.get("tenant_id"),
                ip_address: ip_network.to_string(),
                cidr: row.get("cidr"),
                description: row.get("description"),
                is_active: row.get("is_active"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            }
        })
        .collect())
}
pub use super::tenant_helpers::tenant_exists;
pub async fn insert_entry(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    ip_address: std::net::IpAddr,
    cidr: i32,
    description: &Option<String>,
) -> Result<InsertedEntry, sqlx::Error> {
    let query = r#"
        INSERT INTO ip_whitelist (tenant_id, ip_address, cidr, description, is_active)
        VALUES ($1, $2, $3, $4, true)
        ON CONFLICT (tenant_id, ip_address, cidr) DO UPDATE
        SET description = COALESCE($4, ip_whitelist.description),
            is_active = true,
            updated_at = NOW()
        RETURNING id, ip_address, cidr, description, created_at
    "#;
    let row = sqlx::query(query)
        .bind(tenant_id)
        .bind(ip_address)
        .bind(cidr)
        .bind(description)
        .fetch_one(pool)
        .await?;
    let ip_network: ipnetwork::IpNetwork = row.get("ip_address");
    Ok(InsertedEntry {
        id: row.get("id"),
        ip_address: ip_network.to_string(),
        cidr: row.get("cidr"),
        description: row.get("description"),
        created_at: row.get("created_at"),
    })
}
pub async fn delete_entry(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    ip_address: std::net::IpAddr,
    cidr: i32,
) -> Result<bool, sqlx::Error> {
    let query = r#"
        UPDATE ip_whitelist
        SET is_active = false, updated_at = NOW()
        WHERE tenant_id = $1 AND ip_address = $2 AND cidr = $3 AND is_active = true
        RETURNING ip_address
    "#;
    Ok(sqlx::query(query)
        .bind(tenant_id)
        .bind(ip_address)
        .bind(cidr)
        .fetch_optional(pool)
        .await?
        .is_some())
}
