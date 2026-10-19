use sqlx::PgPool;
pub async fn service_name_exists(pool: &PgPool, service_name: &str) -> bool {
    match sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM services WHERE service_name = $1 AND status IN ('active', 'deprecated'))",
        )
        .bind(service_name)
        .fetch_one(pool)
        .await
    {
        Ok(exists) => exists,
        Err(e) => {
            tracing::error!(error = % e, "Failed to check service name existence");
            false
        }
    }
}
pub struct ServiceRegistryEntry<'a> {
    pub service_name: &'a str,
    pub display_name: &'a str,
    pub description: Option<&'a str>,
    pub category: &'a str,
    pub status: &'a str,
    pub source: &'a str,
    pub dns_names: &'a [String],
    pub metadata: &'a serde_json::Value,
    pub updated_by: &'a str,
}
pub async fn upsert_service_registry_entry(
    pool: &PgPool,
    service: &ServiceRegistryEntry<'_>,
) -> Result<(), sqlx::Error> {
    let query = r#"
        INSERT INTO services (
            service_name,
            display_name,
            description,
            category,
            status,
            source,
            dns_names,
            metadata,
            created_by,
            updated_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
        ON CONFLICT (service_name) DO UPDATE SET
            display_name = EXCLUDED.display_name,
            description = EXCLUDED.description,
            category = EXCLUDED.category,
            status = EXCLUDED.status,
            source = EXCLUDED.source,
            dns_names = EXCLUDED.dns_names,
            metadata = EXCLUDED.metadata,
            updated_by = EXCLUDED.updated_by,
            updated_at = NOW()
    "#;
    sqlx::query(query)
        .bind(service.service_name)
        .bind(service.display_name)
        .bind(service.description)
        .bind(service.category)
        .bind(service.status)
        .bind(service.source)
        .bind(service.dns_names)
        .bind(service.metadata)
        .bind(service.updated_by)
        .execute(pool)
        .await?;
    Ok(())
}
