use super::{
    AuthorizationDetail, Postgres, ProductService, ProductTenantAccess, QueryBuilder, Result, Row,
    TenantProductDetail, Uuid, authorization_detail_from_row, tenant_product_detail_from_row,
};

fn escape_ilike_pattern(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len() + 2);
    escaped.push('%');
    for ch in raw.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped.push('%');
    escaped
}

fn normalized_ilike_pattern(search: Option<&str>) -> Option<String> {
    search
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(escape_ilike_pattern)
}

fn push_product_tenants_scope(
    builder: &mut QueryBuilder<'_, Postgres>,
    product_code: &str,
    pattern: Option<&str>,
) {
    builder.push(
        " FROM tenant_products tp \
         JOIN products p ON p.id = tp.product_id \
         JOIN tenants t ON t.id = tp.tenant_id \
         WHERE p.product_id = ",
    );
    builder.push_bind(product_code.to_owned());
    builder.push(" AND tp.enabled = TRUE");
    if let Some(pat) = pattern {
        let pat_owned = pat.to_owned();
        builder.push(" AND (t.name ILIKE ");
        builder.push_bind(pat_owned.clone());
        builder.push(" ESCAPE '\\' OR t.tenant_code ILIKE ");
        builder.push_bind(pat_owned.clone());
        builder.push(" ESCAPE '\\' OR t.tenant_id ILIKE ");
        builder.push_bind(pat_owned);
        builder.push(" ESCAPE '\\')");
    }
}

fn build_product_tenants_count_query(
    product_code: &str,
    pattern: Option<&str>,
) -> QueryBuilder<'static, Postgres> {
    let mut builder = QueryBuilder::new("SELECT COUNT(*)");
    push_product_tenants_scope(&mut builder, product_code, pattern);
    builder
}

fn build_product_tenants_list_query(
    product_code: &str,
    pattern: Option<&str>,
    limit: i64,
    offset: i64,
) -> QueryBuilder<'static, Postgres> {
    let mut builder = QueryBuilder::new(
        "SELECT \
            tp.id, \
            tp.tenant_id AS tenant_uuid, \
            t.tenant_code, \
            t.tenant_id, \
            t.name AS tenant_name, \
            t.enabled AS tenant_enabled, \
            tp.enabled, \
            tp.created_at",
    );
    push_product_tenants_scope(&mut builder, product_code, pattern);
    builder.push(" ORDER BY tp.created_at DESC, tp.id ASC LIMIT ");
    builder.push_bind(limit);
    builder.push(" OFFSET ");
    builder.push_bind(offset);
    builder
}

impl ProductService {
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
        let search_pattern = normalized_ilike_pattern(search);

        let mut count_builder: QueryBuilder<'_, Postgres> =
            QueryBuilder::new(format!("SELECT COUNT(*) {BASE_FROM}"));
        if let Some(ref p) = search_pattern {
            count_builder.push(" AND (t.name ILIKE ");
            count_builder.push_bind(p.clone());
            count_builder.push(" ESCAPE '\\' OR t.tenant_code ILIKE ");
            count_builder.push_bind(p.clone());
            count_builder.push(" ESCAPE '\\' OR p.name ILIKE ");
            count_builder.push_bind(p.clone());
            count_builder.push(" ESCAPE '\\' OR p.product_id ILIKE ");
            count_builder.push_bind(p.clone());
            count_builder.push(" ESCAPE '\\')");
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
                p.cb_pro_enabled, \
                tp.custom_config, \
                COALESCE((tp.custom_config->>'unit_price')::TEXT, '0.001') AS unit_price, \
                COALESCE(tp.custom_config->>'currency', 'CNY') AS currency, \
                COALESCE((tp.custom_config->>'cb_pro_fallback')::BOOLEAN, false) AS cb_pro_fallback, \
                tp.created_at, \
                COALESCE(tp.updated_at, tp.created_at) AS updated_at \
             {BASE_FROM}"
        ));
        if let Some(ref p) = search_pattern {
            list_builder.push(" AND (t.name ILIKE ");
            list_builder.push_bind(p.clone());
            list_builder.push(" ESCAPE '\\' OR t.tenant_code ILIKE ");
            list_builder.push_bind(p.clone());
            list_builder.push(" ESCAPE '\\' OR p.name ILIKE ");
            list_builder.push_bind(p.clone());
            list_builder.push(" ESCAPE '\\' OR p.product_id ILIKE ");
            list_builder.push_bind(p.clone());
            list_builder.push(" ESCAPE '\\')");
        }
        apply_extra(&mut list_builder).map_err(|e| anyhow::anyhow!(e))?;
        list_builder.push(" ORDER BY tp.created_at DESC, tp.id ASC LIMIT ");
        list_builder.push_bind(limit);
        list_builder.push(" OFFSET ");
        list_builder.push_bind(offset);
        let rows = list_builder.build().fetch_all(&self.db).await?;
        let authorizations = rows.iter().map(authorization_detail_from_row).collect();
        Ok((authorizations, total))
    }

    /// 按产品编码分页查询已授权租户（SQL 真 `COUNT(*) + LIMIT/OFFSET`，支持按租户名称/编码/账号模糊检索；不过滤 `p.enabled` 以便已停用产品仍可查看关联租户）。
    ///
    /// # Errors
    ///
    /// 数据库查询失败时返回错误。
    pub async fn list_product_tenants_page(
        &self,
        product_code: &str,
        search: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<ProductTenantAccess>, i64)> {
        let pattern = normalized_ilike_pattern(search);
        let mut count_builder = build_product_tenants_count_query(product_code, pattern.as_deref());
        let total: i64 = count_builder.build().fetch_one(&self.db).await?.get(0);

        let mut list_builder = build_product_tenants_list_query(
            product_code,
            pattern.as_deref(),
            limit.max(1),
            offset.max(0),
        );
        let rows = list_builder.build().fetch_all(&self.db).await?;
        let tenants = rows
            .iter()
            .map(|row| ProductTenantAccess {
                id: row.get("id"),
                tenant_uuid: row.get("tenant_uuid"),
                tenant_code: row.get("tenant_code"),
                tenant_id: row.get("tenant_id"),
                tenant_name: row.get("tenant_name"),
                tenant_enabled: row.get("tenant_enabled"),
                enabled: row.get("enabled"),
                created_at: row.get("created_at"),
            })
            .collect();
        Ok((tenants, total))
    }

    /// 按租户分页查询已开通产品详情（SQL 真 `COUNT(*) + LIMIT/OFFSET`，支持按产品名或编码模糊检索）。
    ///
    /// # Errors
    ///
    /// 数据库查询失败时返回错误。
    pub async fn list_tenant_products_page(
        &self,
        tenant_id: Uuid,
        search: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<TenantProductDetail>, i64)> {
        const BASE_FROM: &str = "\
            FROM tenant_products tp \
            JOIN products p ON tp.product_id = p.id \
            WHERE tp.tenant_id = ";
        let pattern = search
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(escape_ilike_pattern);

        let mut count_builder: QueryBuilder<'_, Postgres> =
            QueryBuilder::new(format!("SELECT COUNT(*) {BASE_FROM}"));
        count_builder.push_bind(tenant_id);
        count_builder.push(" AND tp.enabled = true AND p.enabled = true");
        if let Some(ref pat) = pattern {
            count_builder.push(" AND (p.name ILIKE ");
            count_builder.push_bind(pat.clone());
            count_builder.push(" ESCAPE '\\' OR p.product_id ILIKE ");
            count_builder.push_bind(pat.clone());
            count_builder.push(" ESCAPE '\\')");
        }
        let total: i64 = count_builder.build().fetch_one(&self.db).await?.get(0);

        let mut list_builder: QueryBuilder<'_, Postgres> = QueryBuilder::new(format!(
            "SELECT \
                tp.id, \
                tp.product_id, \
                p.product_id AS product_code, \
                p.name AS product_name, \
                p.description, \
                p.timeout_secs, \
                tp.enabled, \
                p.cb_pro_enabled, \
                p.cb_failure_threshold, \
                p.cb_success_threshold, \
                p.cb_cooldown_secs, \
                tp.custom_config, \
                COALESCE((tp.custom_config->>'unit_price')::TEXT, '0.001') AS unit_price, \
                COALESCE(tp.custom_config->>'currency', 'CNY') AS currency, \
                COALESCE((tp.custom_config->>'cb_pro_fallback')::BOOLEAN, false) AS cb_pro_fallback, \
                tp.created_at, \
                COALESCE(tp.updated_at, tp.created_at) AS updated_at \
             {BASE_FROM}"
        ));
        list_builder.push_bind(tenant_id);
        list_builder.push(" AND tp.enabled = true AND p.enabled = true");
        if let Some(ref pat) = pattern {
            list_builder.push(" AND (p.name ILIKE ");
            list_builder.push_bind(pat.clone());
            list_builder.push(" ESCAPE '\\' OR p.product_id ILIKE ");
            list_builder.push_bind(pat.clone());
            list_builder.push(" ESCAPE '\\')");
        }
        list_builder.push(" ORDER BY tp.created_at DESC, tp.id ASC LIMIT ");
        list_builder.push_bind(limit);
        list_builder.push(" OFFSET ");
        list_builder.push_bind(offset);

        let rows = list_builder.build().fetch_all(&self.db).await?;
        let cache_enabled = self.cache_enabled_lookup.is_enabled(tenant_id).await;
        let products = rows
            .iter()
            .map(|row| {
                let mut detail = tenant_product_detail_from_row(row);
                detail.response_cache.enabled = cache_enabled;
                detail
            })
            .collect();
        Ok((products, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_ilike_pattern_trims_and_escapes_wildcards() {
        assert_eq!(normalized_ilike_pattern(None), None);
        assert_eq!(normalized_ilike_pattern(Some("   ")), None);
        assert_eq!(
            normalized_ilike_pattern(Some("  JM5K_26%\\T6 ")),
            Some("%JM5K\\_26\\%\\\\T6%".to_string())
        );
    }

    #[test]
    fn product_tenants_page_builders_share_filters_and_bind_pagination() {
        let count_no_q = build_product_tenants_count_query("8000000001", None);
        let count_sql = count_no_q.sql();
        assert!(count_sql.contains("JOIN products p ON p.id = tp.product_id"));
        assert!(count_sql.contains("JOIN tenants t ON t.id = tp.tenant_id"));
        assert!(count_sql.contains("WHERE p.product_id = $1 AND tp.enabled = TRUE"));
        assert!(
            !count_sql.contains(" p.enabled") && !count_sql.contains("(p.enabled"),
            "disabled products must still list authorized tenants"
        );
        assert!(!count_sql.contains("ILIKE"));

        let pat = normalized_ilike_pattern(Some("JM5K26T6"));
        let count_with_q = build_product_tenants_count_query("8000000001", pat.as_deref());
        let list_with_q = build_product_tenants_list_query("8000000001", pat.as_deref(), 10, 20);
        assert!(count_with_q.sql().contains("t.name ILIKE $2"));
        assert!(count_with_q.sql().contains("t.tenant_code ILIKE $3"));
        assert!(count_with_q.sql().contains("t.tenant_id ILIKE $4"));
        assert!(list_with_q.sql().contains("t.name AS tenant_name"));
        assert!(list_with_q.sql().contains("t.enabled AS tenant_enabled"));
        assert!(
            list_with_q
                .sql()
                .contains("ORDER BY tp.created_at DESC, tp.id ASC LIMIT $5 OFFSET $6")
        );
    }
}
