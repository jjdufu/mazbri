use super::*;

impl ProductService {
    pub fn clear_cache(&self) {
        self.tenant_product_detail_cache.invalidate_all();
        info!("Product cache cleared");
    }
    pub async fn list_tenant_products(&self, tenant_id: Uuid) -> Result<Vec<TenantProductDetail>> {
        let rows = sqlx::query(
            r#"
            SELECT
                tp.id,
                tp.product_id,
                p.product_id AS product_code,
                p.name AS product_name,
                p.description,
                p.timeout_secs,
                tp.enabled,
                p.cb_pro_enabled,
                p.cb_failure_threshold,
                p.cb_success_threshold,
                p.cb_cooldown_secs,
                tp.custom_config,
                COALESCE((tp.custom_config->>'unit_price')::TEXT, '0.001') AS unit_price,
                COALESCE(tp.custom_config->>'currency', 'CNY') AS currency,
                COALESCE(tp.custom_config->>'billing_type', 'call') AS billing_type,
                COALESCE((tp.custom_config->>'cb_pro_fallback')::BOOLEAN, false) AS cb_pro_fallback,
                tp.created_at,
                COALESCE(tp.updated_at, tp.created_at) AS updated_at
            FROM tenant_products tp
            JOIN products p ON tp.product_id = p.id
            WHERE tp.tenant_id = $1 AND tp.enabled = true AND p.enabled = true
            ORDER BY tp.created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.db)
        .await?;
        let products = rows.iter().map(tenant_product_detail_from_row).collect();
        Ok(products)
    }
    pub async fn get_tenant_product_config(
        &self,
        tenant_id: Uuid,
        product_id: &str,
    ) -> Result<Option<TenantProductDetail>> {
        let cache_key = Self::tenant_product_detail_cache_key(tenant_id, product_id);
        if let Some(detail) = self.tenant_product_detail_cache.get(&cache_key) {
            debug!(
                tenant_id = % tenant_id, product_id = % product_id,
                "Cache hit for tenant product detail"
            );
            return Ok(detail.clone());
        }
        let row = sqlx::query(
            r#"
            SELECT
                tp.id,
                tp.product_id,
                p.product_id AS product_code,
                p.name AS product_name,
                p.description,
                p.timeout_secs,
                tp.enabled,
                p.cb_pro_enabled,
                p.cb_failure_threshold,
                p.cb_success_threshold,
                p.cb_cooldown_secs,
                tp.custom_config,
                COALESCE((tp.custom_config->>'unit_price')::TEXT, '0.001') AS unit_price,
                COALESCE(tp.custom_config->>'currency', 'CNY') AS currency,
                COALESCE(tp.custom_config->>'billing_type', 'call') AS billing_type,
                COALESCE((tp.custom_config->>'cb_pro_fallback')::BOOLEAN, false) AS cb_pro_fallback,
                tp.created_at,
                COALESCE(tp.updated_at, tp.created_at) AS updated_at
            FROM tenant_products tp
            JOIN products p ON tp.product_id = p.id
            WHERE tp.tenant_id = $1
              AND p.product_id = $2
              AND tp.enabled = true
              AND p.enabled = true
            "#,
        )
        .bind(tenant_id)
        .bind(product_id)
        .fetch_optional(&self.db)
        .await?;
        let detail = row.as_ref().map(tenant_product_detail_from_row);
        self.tenant_product_detail_cache
            .insert(cache_key, detail.clone());
        Ok(detail)
    }
    pub async fn list_product_tenants(&self, product_id: &str) -> Result<Vec<ProductTenantAccess>> {
        let product_uuid: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM products WHERE product_id = $1")
                .bind(product_id)
                .fetch_optional(&self.db)
                .await?;
        let product_uuid = match product_uuid {
            Some(uuid) => uuid,
            None => return Ok(vec![]),
        };
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, product_id, enabled, created_at
            FROM tenant_products
            WHERE product_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(product_uuid)
        .fetch_all(&self.db)
        .await?;
        let tenants = rows
            .iter()
            .map(|row| ProductTenantAccess {
                id: row.get("id"),
                tenant_id: row.get("tenant_id"),
                product_id: row.get("product_id"),
                enabled: row.get("enabled"),
                created_at: row.get("created_at"),
            })
            .collect();
        Ok(tenants)
    }
}
