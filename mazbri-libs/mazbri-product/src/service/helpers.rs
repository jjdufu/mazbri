use super::{
    AuthorizationDetail, MAX_PRODUCT_ID_NUMBER, MIN_PRODUCT_ID_NUMBER, PgRow, Product,
    ProductWriteError, Row, TenantProductConfig, TenantProductDetail,
};

pub(crate) fn next_product_id_from_max(
    max_product_id: Option<i64>,
) -> std::result::Result<String, ProductWriteError> {
    let next_product_id = max_product_id.unwrap_or(MIN_PRODUCT_ID_NUMBER - 1) + 1;
    if next_product_id > MAX_PRODUCT_ID_NUMBER {
        return Err(ProductWriteError::ExhaustedProductIdSpace);
    }
    Ok(next_product_id.to_string())
}
pub(crate) fn tenant_product_config_from_value(value: serde_json::Value) -> TenantProductConfig {
    serde_json::from_value(value).unwrap_or_default()
}
pub(crate) fn tenant_product_detail_from_row(row: &PgRow) -> TenantProductDetail {
    let config = tenant_product_config_from_value(row.get("custom_config"));
    TenantProductDetail {
        id: row.get("id"),
        product_id: row.get("product_id"),
        product_code: row.get("product_code"),
        product_name: row.get("product_name"),
        description: row.get("description"),
        timeout_secs: row.get("timeout_secs"),
        enabled: row.get("enabled"),
        cb_pro_enabled: row.get("cb_pro_enabled"),
        cb_failure_threshold: row.get("cb_failure_threshold"),
        cb_success_threshold: row.get("cb_success_threshold"),
        cb_cooldown_secs: row.get("cb_cooldown_secs"),
        unit_price: row.get("unit_price"),
        currency: row.get("currency"),
        billing_type: row.get("billing_type"),
        cb_pro_fallback: row.get("cb_pro_fallback"),
        config,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
pub(crate) fn authorization_detail_from_row(row: &PgRow) -> AuthorizationDetail {
    let config = tenant_product_config_from_value(row.get("custom_config"));
    AuthorizationDetail {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        tenant_code: row.get("tenant_code"),
        tenant_name: row.get("tenant_name"),
        product_id: row.get("product_id"),
        product_code: row.get("product_code"),
        product_name: row.get("product_name"),
        enabled: row.get("enabled"),
        unit_price: row.get("unit_price"),
        currency: row.get("currency"),
        billing_type: row.get("billing_type"),
        cb_pro_fallback: row.get("cb_pro_fallback"),
        config,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
pub(crate) fn product_from_row(row: &PgRow) -> Product {
    Product {
        id: row.get("id"),
        product_id: row.get("product_id"),
        name: row.get("name"),
        description: row.get("description"),
        timeout_secs: row.get("timeout_secs"),
        enabled: row.get("enabled"),
        cb_pro_enabled: row.get("cb_pro_enabled"),
        cb_failure_threshold: row.get("cb_failure_threshold"),
        cb_success_threshold: row.get("cb_success_threshold"),
        cb_cooldown_secs: row.get("cb_cooldown_secs"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
pub(crate) fn classify_product_write_error(
    error: sqlx::Error,
    product_id: &str,
) -> ProductWriteError {
    match error {
        sqlx::Error::Database(database_error)
            if is_duplicate_product_id_error(database_error.as_ref()) =>
        {
            ProductWriteError::DuplicateProductId(product_id.to_owned())
        }
        other => ProductWriteError::Database(other),
    }
}
pub(crate) fn is_duplicate_product_id_error(
    database_error: &(dyn sqlx::error::DatabaseError + 'static),
) -> bool {
    if !database_error.is_unique_violation() {
        return false;
    }
    matches!(database_error.constraint(), Some("products_product_id_key"))
        || (database_error.table() == Some("products")
            && database_error.message().contains("(product_id)"))
}
