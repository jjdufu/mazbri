use sqlx::PgPool;
pub async fn tenant_exists(pool: &PgPool, tenant_id: uuid::Uuid) -> bool {
    match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
    {
        Ok(count) => count > 0,
        Err(e) => {
            tracing::error!(error = % e, "Failed to check tenant existence");
            false
        }
    }
}
