use super::{ProductService, Result, TenantProductDetail, Uuid, debug, info, tenant_product_detail_from_row};

impl ProductService {
    pub fn clear_cache(&self) {
        self.tenant_product_detail_cache.invalidate_all();
        self.cache_enabled_lookup.invalidate_all();
        info!("Product cache cleared");
    }

    /// Overlay `response_cache.enabled` from cache DB (30s mem / negative cache).
    async fn apply_cache_enabled(&self, tenant_id: Uuid, detail: &mut TenantProductDetail) {
        detail.response_cache.enabled = self.cache_enabled_lookup.is_enabled(tenant_id).await;
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
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
            let mut detail = detail.clone();
            if let Some(ref mut d) = detail {
                self.apply_cache_enabled(tenant_id, d).await;
            }
            return Ok(detail);
        }
        let row = sqlx::query(
            r"
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
                COALESCE((tp.custom_config->>'cb_pro_fallback')::BOOLEAN, false) AS cb_pro_fallback,
                tp.created_at,
                COALESCE(tp.updated_at, tp.created_at) AS updated_at
            FROM tenant_products tp
            JOIN products p ON tp.product_id = p.id
                        WHERE tp.tenant_id = $1
              AND p.product_id = $2
              AND tp.enabled = true
              AND p.enabled = true
            ",
        )
        .bind(tenant_id)
        .bind(product_id)
        .fetch_optional(&self.db)
        .await?;
        let mut detail = row.as_ref().map(tenant_product_detail_from_row);
        if let Some(ref mut d) = detail {
            self.apply_cache_enabled(tenant_id, d).await;
        }
        self.tenant_product_detail_cache
            .insert(cache_key, detail.clone());
        Ok(detail)
    }
}
