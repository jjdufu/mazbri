use super::{
    AuthorizationDetail, Postgres, ProductService, QueryBuilder, Result, Row,
    authorization_detail_from_row,
};

impl ProductService {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn list_authorizations(&self) -> Result<Vec<AuthorizationDetail>> {
        let rows = sqlx::query(
            r"
            SELECT
                tp.id,
                tp.tenant_id,
                t.tenant_code,
                t.name AS tenant_name,
                tp.product_id,
                p.product_id AS product_code,
                p.name AS product_name,
                tp.enabled,
                tp.custom_config,
                COALESCE((tp.custom_config->>'unit_price')::TEXT, '0.001') AS unit_price,
                COALESCE(tp.custom_config->>'currency', 'CNY') AS currency,
                COALESCE(tp.custom_config->>'billing_type', 'call') AS billing_type,
                COALESCE((tp.custom_config->>'cb_pro_fallback')::BOOLEAN, false) AS cb_pro_fallback,
                tp.created_at,
                COALESCE(tp.updated_at, tp.created_at) AS updated_at
            FROM tenant_products tp
            JOIN products p ON tp.product_id = p.id
            JOIN tenants t ON tp.tenant_id = t.id
            WHERE tp.enabled = true AND p.enabled = true
            ORDER BY tp.created_at DESC
            ",
        )
        .fetch_all(&self.db)
        .await?;
        let authorizations = rows.iter().map(authorization_detail_from_row).collect();
        Ok(authorizations)
    }
    /// Paginated authorizations with free-text search and optional extra AND filters.
    ///
    /// Default scope: only enabled `tenant_products` on enabled products.
    /// `apply_extra` runs for both COUNT and SELECT (caller validates filters).
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn list_authorizations_page<F>(
        &self,
        search: Option<&str>,
        limit: i64,
        offset: i64,
        mut apply_extra: F,
    ) -> Result<(Vec<AuthorizationDetail>, i64)>
    where
        F: FnMut(&mut QueryBuilder<'_, Postgres>) -> Result<(), String>,
    {
        const BASE_FROM: &str = "\
            FROM tenant_products tp \
            JOIN products p ON tp.product_id = p.id \
            JOIN tenants t ON tp.tenant_id = t.id \
            WHERE tp.enabled = true AND p.enabled = true";
        let search_pattern = search.map(|value| format!("%{value}%"));

        let mut count_builder: QueryBuilder<'_, Postgres> =
            QueryBuilder::new(format!("SELECT COUNT(*) {BASE_FROM}"));
        if let Some(ref p) = search_pattern {
            count_builder.push(" AND (t.name ILIKE ");
            count_builder.push_bind(p.clone());
            count_builder.push(" OR t.tenant_code ILIKE ");
            count_builder.push_bind(p.clone());
            count_builder.push(" OR p.name ILIKE ");
            count_builder.push_bind(p.clone());
            count_builder.push(" OR p.product_id ILIKE ");
            count_builder.push_bind(p.clone());
            count_builder.push(")");
        }
        apply_extra(&mut count_builder).map_err(|e| anyhow::anyhow!(e))?;
        let total: i64 = count_builder.build().fetch_one(&self.db).await?.get(0);

        let mut list_builder: QueryBuilder<'_, Postgres> = QueryBuilder::new(format!(
            "SELECT \
                tp.id, \
                tp.tenant_id, \
                t.tenant_code, \
                t.name AS tenant_name, \
                tp.product_id, \
                p.product_id AS product_code, \
                p.name AS product_name, \
                tp.enabled, \
                tp.custom_config, \
                COALESCE((tp.custom_config->>'unit_price')::TEXT, '0.001') AS unit_price, \
                COALESCE(tp.custom_config->>'currency', 'CNY') AS currency, \
                COALESCE(tp.custom_config->>'billing_type', 'call') AS billing_type, \
                COALESCE((tp.custom_config->>'cb_pro_fallback')::BOOLEAN, false) AS cb_pro_fallback, \
                tp.created_at, \
                COALESCE(tp.updated_at, tp.created_at) AS updated_at \
             {BASE_FROM}"
        ));
        if let Some(ref p) = search_pattern {
            list_builder.push(" AND (t.name ILIKE ");
            list_builder.push_bind(p.clone());
            list_builder.push(" OR t.tenant_code ILIKE ");
            list_builder.push_bind(p.clone());
            list_builder.push(" OR p.name ILIKE ");
            list_builder.push_bind(p.clone());
            list_builder.push(" OR p.product_id ILIKE ");
            list_builder.push_bind(p.clone());
            list_builder.push(")");
        }
        apply_extra(&mut list_builder).map_err(|e| anyhow::anyhow!(e))?;
        list_builder.push(" ORDER BY tp.created_at DESC LIMIT ");
        list_builder.push_bind(limit);
        list_builder.push(" OFFSET ");
        list_builder.push_bind(offset);
        let rows = list_builder.build().fetch_all(&self.db).await?;
        let authorizations = rows.iter().map(authorization_detail_from_row).collect();
        Ok((authorizations, total))
    }
}
