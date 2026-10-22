use super::{
    Context, DateTime, PermissionCache, ProductService, Result, Row, TenantProduct,
    TenantProductConfig, UpdateTenantProductConfigResult, Utc, Uuid, info,
};

impl ProductService {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn grant_tenant_access(
        &self,
        tenant_id: Uuid,
        product_id: &str,
        config: Option<&TenantProductConfig>,
    ) -> Result<TenantProduct> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let product_uuid: Uuid =
            sqlx::query_scalar("SELECT id FROM products WHERE product_id = $1")
                .bind(product_id)
                .fetch_one(&self.db)
                .await?;
        let config = config.cloned().unwrap_or_default();
        config.validate().map_err(anyhow::Error::msg)?;
        let custom_config = serde_json::to_value(&config).with_context(|| {
            format!(
                "Failed to serialize tenant product config: {config:?} / 无法序列化租户产品配置"
            )
        })?;
        let row = sqlx::query(
                r"
            INSERT INTO tenant_products (id, tenant_id, product_id, enabled, custom_config, created_at)
            VALUES ($1, $2, $3, true, $4, $5)
            ON CONFLICT (tenant_id, product_id) DO UPDATE SET
                enabled = true,
                custom_config = COALESCE($4, tenant_products.custom_config),
                updated_at = $5
            RETURNING id, tenant_id, product_id, enabled, custom_config, created_at
            ",
            )
            .bind(id)
            .bind(tenant_id)
            .bind(product_uuid)
            .bind(&custom_config)
            .bind(now)
            .fetch_one(&self.db)
            .await?;
        info!(
            "Granted tenant {} access to product {} with config {:?}",
            tenant_id, product_id, config
        );
        self.invalidate_permission_cache(tenant_id, product_id)
            .await;
        self.invalidate_tenant_product_detail_cache(tenant_id, product_id)
            .await;
        Ok(TenantProduct {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            product_id: row.get("product_id"),
            enabled: row.get("enabled"),
            custom_config: row.get("custom_config"),
            created_at: row.get("created_at"),
        })
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn revoke_tenant_access(&self, tenant_id: Uuid, product_id: &str) -> Result<bool> {
        info!(
            "revoke_tenant_access: tenant_id={}, product_id={}",
            tenant_id, product_id
        );
        let product_uuid: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM products WHERE product_id = $1")
                .bind(product_id)
                .fetch_optional(&self.db)
                .await?;
        let product_uuid = if let Some(uuid) = product_uuid {
            info!("revoke_tenant_access: found product_uuid={}", uuid);
            uuid
        } else {
            info!(
                "revoke_tenant_access: product not found for product_id={}",
                product_id
            );
            return Ok(false);
        };
        let result = sqlx::query(
                "UPDATE tenant_products SET enabled = false, updated_at = NOW() WHERE tenant_id = $1 AND product_id = $2 AND enabled = true",
            )
            .bind(tenant_id)
            .bind(product_uuid)
            .execute(&self.db)
            .await?;
        let affected = result.rows_affected() > 0;
        info!(
            "revoke_tenant_access: UPDATE affected {} rows",
            result.rows_affected()
        );
        if affected {
            self.invalidate_permission_cache(tenant_id, product_id)
                .await;
            self.invalidate_tenant_product_detail_cache(tenant_id, product_id)
                .await;
            info!(
                "Revoked tenant {} access to product {}",
                tenant_id, product_id
            );
        }
        Ok(affected)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn update_tenant_product_config(
        &self,
        tenant_id: Uuid,
        product_id: &str,
        config: &TenantProductConfig,
        expected_updated_at: Option<DateTime<Utc>>,
    ) -> Result<UpdateTenantProductConfigResult> {
        let product_uuid: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM products WHERE product_id = $1")
                .bind(product_id)
                .fetch_optional(&self.db)
                .await?;
        let Some(product_uuid) = product_uuid else {
            return Ok(UpdateTenantProductConfigResult::NotFound);
        };
        config.validate().map_err(anyhow::Error::msg)?;
        let custom_config = serde_json::to_value(config).with_context(|| {
            format!(
                "Failed to serialize tenant product config: {config:?} / 无法序列化租户产品配置"
            )
        })?;
        let now = Utc::now();
        let row = match expected_updated_at {
            Some(expected_updated_at) => {
                sqlx::query(
                    r"
                    UPDATE tenant_products
                    SET custom_config = $3, updated_at = $4
                    WHERE tenant_id = $1
                      AND product_id = $2
                      AND enabled = true
                      AND COALESCE(updated_at, created_at) = $5
                    RETURNING id, tenant_id, product_id, enabled, custom_config, created_at
                    ",
                )
                .bind(tenant_id)
                .bind(product_uuid)
                .bind(&custom_config)
                .bind(now)
                .bind(expected_updated_at)
                .fetch_optional(&self.db)
                .await?
            }
            None => {
                sqlx::query(
                    r"
                    UPDATE tenant_products
                    SET custom_config = $3, updated_at = $4
                    WHERE tenant_id = $1 AND product_id = $2 AND enabled = true
                    RETURNING id, tenant_id, product_id, enabled, custom_config, created_at
                    ",
                )
                .bind(tenant_id)
                .bind(product_uuid)
                .bind(&custom_config)
                .bind(now)
                .fetch_optional(&self.db)
                .await?
            }
        };
        if let Some(row) = row {
            self.invalidate_permission_cache(tenant_id, product_id)
                .await;
            self.invalidate_tenant_product_detail_cache(tenant_id, product_id)
                .await;
            return Ok(UpdateTenantProductConfigResult::Updated(TenantProduct {
                id: row.get("id"),
                tenant_id: row.get("tenant_id"),
                product_id: row.get("product_id"),
                enabled: row.get("enabled"),
                custom_config: row.get("custom_config"),
                created_at: row.get("created_at"),
            }));
        }
        let latest_updated_at = sqlx::query_scalar(
            r"
            SELECT COALESCE(updated_at, created_at) AS updated_at
            FROM tenant_products
            WHERE tenant_id = $1 AND product_id = $2 AND enabled = true
            ",
        )
        .bind(tenant_id)
        .bind(product_uuid)
        .fetch_optional(&self.db)
        .await?;
        if let Some(latest_updated_at) = latest_updated_at {
            return Ok(UpdateTenantProductConfigResult::Conflict(latest_updated_at));
        }
        Ok(UpdateTenantProductConfigResult::NotFound)
    }
}
