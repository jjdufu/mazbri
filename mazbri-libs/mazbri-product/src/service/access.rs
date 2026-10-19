use super::*;

impl ProductService {
    pub async fn check_tenant_access(&self, tenant_id: Uuid, product_id: &str) -> Result<bool> {
        if let Ok(Some(entry)) = self.permission_cache.get(&tenant_id, product_id).await {
            debug!("Permission cache hit for: {}:{}", tenant_id, product_id);
            return Ok(entry.enabled);
        }
        let row = sqlx::query(
            r#"
            SELECT tp.enabled
            FROM tenant_products tp
            JOIN products p ON tp.product_id = p.id
            WHERE tp.tenant_id = $1 AND p.product_id = $2
                AND p.enabled = true
            "#,
        )
        .bind(tenant_id)
        .bind(product_id)
        .fetch_optional(&self.db)
        .await?;
        let result = row.map(|r| r.get::<bool, _>("enabled")).unwrap_or(false);
        let entry = PermissionEntry { enabled: result };
        let _ = self
            .permission_cache
            .set(
                &tenant_id,
                product_id,
                entry,
                Duration::from_secs(self.config.cache_ttl_secs),
            )
            .await;
        debug!(
            "Permission check: tenant {} -> product {} = {}",
            tenant_id, product_id, result
        );
        Ok(result)
    }
    pub(crate) async fn invalidate_permission_cache(&self, tenant_id: Uuid, product_id: &str) {
        let _ = self
            .permission_cache
            .invalidate(&tenant_id, product_id)
            .await;
        debug!(
            "Invalidated permission cache for: {}:{}",
            tenant_id, product_id
        );
    }
    pub(crate) fn tenant_product_detail_cache_key(tenant_id: Uuid, product_id: &str) -> String {
        format!("{tenant_id}|{product_id}")
    }
    pub(crate) async fn invalidate_tenant_product_detail_cache(
        &self,
        tenant_id: Uuid,
        product_id: &str,
    ) {
        let cache_key = Self::tenant_product_detail_cache_key(tenant_id, product_id);
        self.tenant_product_detail_cache.invalidate(&cache_key);
        debug!(
            "Invalidated tenant product detail cache for: {}:{}",
            tenant_id, product_id
        );
    }
    pub async fn warm_tenant_product_detail_cache(&self) -> Result<usize> {
        let rows = sqlx::query(
            r#"
            SELECT
                tp.id,
                tp.tenant_id,
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
            WHERE tp.enabled = true
              AND p.enabled = true
            "#,
        )
        .fetch_all(&self.db)
        .await?;
        let mut warmed = 0usize;
        for row in rows {
            let detail = tenant_product_detail_from_row(&row);
            let tenant_id: Uuid = row.get("tenant_id");
            let product_code = detail.product_code.clone();
            let cache_key = Self::tenant_product_detail_cache_key(tenant_id, &product_code);
            self.tenant_product_detail_cache
                .insert(cache_key, Some(detail));
            warmed += 1;
        }
        info!(count = warmed, "Warm tenant product detail cache completed");
        Ok(warmed)
    }
}
