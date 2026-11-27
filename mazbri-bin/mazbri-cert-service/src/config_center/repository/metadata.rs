use super::super::error::{ConfigError, ConfigResult};
use super::super::models::{
    ConfigCategory, ConfigEntry, ConfigEntryWithMeta, ConfigSearchPage, ConfigSearchQuery,
    ConfigTag, CreateCategoryRequest, CreateTagRequest, UpdateCategoryRequest, UpdateTagRequest,
};
use sqlx::Row;
use uuid::Uuid;
impl super::super::ConfigRepository {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn create_tag(&self, req: &CreateTagRequest) -> ConfigResult<ConfigTag> {
        let result = sqlx::query_as::<_, ConfigTag>(
            r"
            INSERT INTO config_tags (name, description, color)
            VALUES ($1, $2, $3)
            RETURNING *
            ",
        )
        .bind(&req.name)
        .bind(&req.description)
        .bind(&req.color)
        .fetch_one(&*self.db)
        .await;
        match result {
            Ok(tag) => Ok(tag),
            Err(sqlx::Error::Database(e)) if e.constraint() == Some("config_tags_name_key") => {
                Err(ConfigError::AlreadyExists {
                    service_name: req.name.clone(),
                })
            }
            Err(e) => Err(e.into()),
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn list_tags(&self) -> ConfigResult<Vec<ConfigTag>> {
        let tags = sqlx::query_as::<_, ConfigTag>(
            r"
            SELECT * FROM config_tags ORDER BY name
            ",
        )
        .fetch_all(&*self.db)
        .await?;
        Ok(tags)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_tag(&self, id: Uuid) -> ConfigResult<ConfigTag> {
        let tag = sqlx::query_as::<_, ConfigTag>(
            r"
            SELECT * FROM config_tags WHERE id = $1
            ",
        )
        .bind(id)
        .fetch_optional(&*self.db)
        .await?
        .ok_or_else(|| ConfigError::NotFound {
            service_name: id.to_string(),
        })?;
        Ok(tag)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn update_tag(&self, id: Uuid, req: &UpdateTagRequest) -> ConfigResult<ConfigTag> {
        let result = sqlx::query_as::<_, ConfigTag>(
            r"
            UPDATE config_tags
            SET description = $1, color = $2
            WHERE id = $3
            RETURNING *
            ",
        )
        .bind(&req.description)
        .bind(&req.color)
        .bind(id)
        .fetch_optional(&*self.db)
        .await?;
        result.ok_or_else(|| ConfigError::NotFound {
            service_name: id.to_string(),
        })
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn delete_tag(&self, id: Uuid) -> ConfigResult<()> {
        let result = sqlx::query(
            r"
            DELETE FROM config_tags WHERE id = $1
            ",
        )
        .bind(id)
        .execute(&*self.db)
        .await?;
        if result.rows_affected() == 0 {
            return Err(ConfigError::NotFound {
                service_name: id.to_string(),
            });
        }
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn create_category(
        &self,
        req: &CreateCategoryRequest,
    ) -> ConfigResult<ConfigCategory> {
        let result = sqlx::query_as::<_, ConfigCategory>(
            r"
            INSERT INTO config_categories (name, parent_id, description, sort_order)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            ",
        )
        .bind(&req.name)
        .bind(req.parent_id)
        .bind(&req.description)
        .bind(req.sort_order)
        .fetch_one(&*self.db)
        .await;
        match result {
            Ok(category) => Ok(category),
            Err(sqlx::Error::Database(e))
                if e.constraint() == Some("config_categories_name_key") =>
            {
                Err(ConfigError::AlreadyExists {
                    service_name: req.name.clone(),
                })
            }
            Err(e) => Err(e.into()),
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn list_categories(&self) -> ConfigResult<Vec<ConfigCategory>> {
        let categories = sqlx::query_as::<_, ConfigCategory>(
            r"
            SELECT * FROM config_categories ORDER BY sort_order, name
            ",
        )
        .fetch_all(&*self.db)
        .await?;
        Ok(categories)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn list_categories_tree(&self) -> ConfigResult<Vec<ConfigCategory>> {
        let categories = self.list_categories().await?;
        build_category_tree(&categories, None)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_category(&self, id: Uuid) -> ConfigResult<ConfigCategory> {
        let category = sqlx::query_as::<_, ConfigCategory>(
            r"
            SELECT * FROM config_categories WHERE id = $1
            ",
        )
        .bind(id)
        .fetch_optional(&*self.db)
        .await?
        .ok_or_else(|| ConfigError::NotFound {
            service_name: id.to_string(),
        })?;
        Ok(category)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn update_category(
        &self,
        id: Uuid,
        req: &UpdateCategoryRequest,
    ) -> ConfigResult<ConfigCategory> {
        let result = sqlx::query_as::<_, ConfigCategory>(
            r"
            UPDATE config_categories
            SET name = $1, parent_id = $2, description = $3, sort_order = $4
            WHERE id = $5
            RETURNING *
            ",
        )
        .bind(&req.name)
        .bind(req.parent_id)
        .bind(&req.description)
        .bind(req.sort_order)
        .bind(id)
        .fetch_optional(&*self.db)
        .await?;
        result.ok_or_else(|| ConfigError::NotFound {
            service_name: id.to_string(),
        })
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn delete_category(&self, id: Uuid) -> ConfigResult<()> {
        let result = sqlx::query(
            r"
            DELETE FROM config_categories WHERE id = $1
            ",
        )
        .bind(id)
        .execute(&*self.db)
        .await?;
        if result.rows_affected() == 0 {
            return Err(ConfigError::NotFound {
                service_name: id.to_string(),
            });
        }
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn add_tags_to_config(
        &self,
        entry_id: Uuid,
        tag_names: &[String],
    ) -> ConfigResult<()> {
        for tag_name in tag_names {
            sqlx::query(
                r"
                INSERT INTO config_entry_tags (config_entry_id, tag_id)
                SELECT $1, id FROM config_tags WHERE name = $2
                ON CONFLICT DO NOTHING
                ",
            )
            .bind(entry_id)
            .bind(tag_name)
            .execute(&*self.db)
            .await?;
        }
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn remove_tags_from_config(
        &self,
        entry_id: Uuid,
        tag_names: &[String],
    ) -> ConfigResult<()> {
        for tag_name in tag_names {
            sqlx::query(
                r"
                DELETE FROM config_entry_tags
                WHERE config_entry_id = $1
                AND tag_id = (SELECT id FROM config_tags WHERE name = $2)
                ",
            )
            .bind(entry_id)
            .bind(tag_name)
            .execute(&*self.db)
            .await?;
        }
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn set_config_category(
        &self,
        entry_id: Uuid,
        category_id: Option<Uuid>,
    ) -> ConfigResult<()> {
        sqlx::query(
            r"
            UPDATE config_entries SET category_id = $1 WHERE id = $2
            ",
        )
        .bind(category_id)
        .bind(entry_id)
        .execute(&*self.db)
        .await?;
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn search_configs(
        &self,
        query: &ConfigSearchQuery,
    ) -> ConfigResult<ConfigSearchPage> {
        let mut count_builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            r"
            SELECT COUNT(*)
            FROM config_entries ce
            LEFT JOIN config_categories cc ON ce.category_id = cc.id
            WHERE 1 = 1",
        );
        push_search_filters(&mut count_builder, query);
        let total: i64 = count_builder
            .build_query_scalar()
            .fetch_one(&*self.db)
            .await?;

        let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            r"
            WITH matched_entries AS (
                SELECT ce.id, ce.service_name, ce.content,
                       ce.content_hash, ce.is_sensitive, ce.updated_by,
                       ce.created_at, ce.updated_at, ce.category_id,
                       COALESCE(
                           (SELECT MAX(ch.version_number)
                            FROM config_history ch
                            WHERE ch.service_name = ce.service_name),
                           1
                       ) AS version
                FROM config_entries ce
                LEFT JOIN config_categories cc ON ce.category_id = cc.id
                WHERE 1 = 1",
        );
        push_search_filters(&mut builder, query);
        builder.push(" ORDER BY ce.service_name LIMIT ");
        builder.push_bind(query.limit);
        builder.push(" OFFSET ");
        builder.push_bind(query.offset);
        builder.push(
            r"
            )
            SELECT me.*,
                   COALESCE(json_agg(DISTINCT jsonb_build_object(
                       'id', ct.id, 'name', ct.name,
                       'description', ct.description, 'color', ct.color,
                       'created_at', ct.created_at
                   )) FILTER (WHERE ct.id IS NOT NULL), '[]'::json) AS tags_json,
                   (SELECT row_to_json(cc2.*) FROM config_categories cc2 WHERE cc2.id = me.category_id) AS category_json
            FROM matched_entries me
            LEFT JOIN config_entry_tags cet ON cet.config_entry_id = me.id
            LEFT JOIN config_tags ct ON ct.id = cet.tag_id
            GROUP BY me.id, me.service_name, me.content,
                     me.content_hash, me.is_sensitive, me.updated_by,
                     me.created_at, me.updated_at, me.category_id, me.version
            ORDER BY me.service_name
            ",
        );
        let rows = builder.build().fetch_all(&*self.db).await?;
        Ok(ConfigSearchPage {
            items: rows.iter().map(row_to_entry_with_meta).collect(),
            total,
        })
    }
}

fn push_search_filters(
    builder: &mut sqlx::QueryBuilder<sqlx::Postgres>,
    query: &ConfigSearchQuery,
) {
    if let Some(ref service_name) = query.service_name {
        builder.push(" AND ce.service_name ILIKE ");
        builder.push_bind(contains_pattern(service_name));
        builder.push(" ESCAPE '\\'");
    }
    if let Some(ref service_name_eq) = query.service_name_eq {
        builder.push(" AND ce.service_name = ");
        builder.push_bind(service_name_eq);
    }
    if let Some(ref updated_by) = query.updated_by {
        builder.push(" AND ce.updated_by ILIKE ");
        builder.push_bind(contains_pattern(updated_by));
        builder.push(" ESCAPE '\\'");
    }
    if let Some(ref gte) = query.updated_at_gte {
        builder.push(" AND ce.updated_at >= ");
        builder.push_bind(gte);
        builder.push("::timestamptz");
    }
    if let Some(ref lte) = query.updated_at_lte {
        builder.push(" AND ce.updated_at <= ");
        builder.push_bind(lte);
        builder.push("::timestamptz");
    }
    if let Some(ref q) = query.q {
        let pat = contains_pattern(q);
        builder.push(" AND (ce.service_name ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" ESCAPE '\\' OR ce.updated_by ILIKE ");
        builder.push_bind(pat);
        builder.push(" ESCAPE '\\')");
    }
    if let Some(ref category_name) = query.category {
        builder.push(" AND cc.name = ");
        builder.push_bind(category_name);
    }
    if let Some(ref tags) = query.tags
        && !tags.is_empty()
    {
        builder.push(
            " AND ce.id IN (SELECT config_entry_id FROM config_entry_tags \
             WHERE tag_id IN (SELECT id FROM config_tags WHERE name = ANY(",
        );
        builder.push_bind(tags);
        builder.push(")))");
    }
}

fn row_to_entry_with_meta(row: &sqlx::postgres::PgRow) -> ConfigEntryWithMeta {
    let entry = ConfigEntry {
        id: row.get("id"),
        service_name: row.get("service_name"),
        content: row.get("content"),
        content_hash: row.get("content_hash"),
        is_sensitive: row.get("is_sensitive"),
        updated_by: row.get("updated_by"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };
    let version: i32 = row.try_get("version").unwrap_or(1);
    let tags_json: Option<serde_json::Value> = row.try_get("tags_json").ok();
    let tags: Vec<ConfigTag> = match tags_json {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect(),
        _ => vec![],
    };
    let category: Option<ConfigCategory> = row
        .try_get("category_json")
        .ok()
        .and_then(|v: serde_json::Value| serde_json::from_value(v).ok());
    ConfigEntryWithMeta {
        entry,
        version,
        tags,
        category,
    }
}

/// 转义通配符并包成 `%value%`（ILIKE 包含匹配，检索词最长 128 字符）。
fn contains_pattern(value: &str) -> String {
    mazbri_core::config::contains_ilike_pattern(value)
}

const MAX_CATEGORY_DEPTH: usize = 10;
fn build_category_tree(
    categories: &[ConfigCategory],
    parent_id: Option<Uuid>,
) -> ConfigResult<Vec<ConfigCategory>> {
    let mut children_map: std::collections::HashMap<Option<Uuid>, Vec<ConfigCategory>> =
        std::collections::HashMap::new();
    for cat in categories {
        children_map
            .entry(cat.parent_id)
            .or_default()
            .push(cat.clone());
    }
    let mut visited = std::collections::HashSet::new();
    build_category_tree_recursive(&children_map, parent_id, &mut visited, 0)
}
fn build_category_tree_recursive(
    children_map: &std::collections::HashMap<Option<Uuid>, Vec<ConfigCategory>>,
    parent_id: Option<Uuid>,
    visited: &mut std::collections::HashSet<Uuid>,
    depth: usize,
) -> ConfigResult<Vec<ConfigCategory>> {
    if depth > MAX_CATEGORY_DEPTH {
        return Err(ConfigError::Validation(format!(
            "Category tree exceeds maximum depth of {MAX_CATEGORY_DEPTH}"
        )));
    }
    let mut result = Vec::new();
    if let Some(children) = children_map.get(&parent_id) {
        for mut cat in children.clone() {
            if !visited.insert(cat.id) {
                return Err(ConfigError::Validation(format!(
                    "Circular reference detected in category tree at category {}",
                    cat.id
                )));
            }
            cat.children = Some(build_category_tree_recursive(
                children_map,
                Some(cat.id),
                visited,
                depth + 1,
            )?);
            visited.remove(&cat.id);
            result.push(cat);
        }
    }
    Ok(result)
}
