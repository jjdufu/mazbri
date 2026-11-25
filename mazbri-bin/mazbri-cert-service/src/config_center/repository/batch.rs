use super::super::error::ConfigResult;
use super::super::models::{
    BatchError, BatchFilters, BatchOperation, BatchResult, ConfigEntry, ConfigEntrySummary,
};
use super::super::validator::{compute_batch_content_hash, validate_batch_toml};
impl super::super::ConfigRepository {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn query_by_filters(&self, filters: &BatchFilters) -> ConfigResult<Vec<ConfigEntry>> {
        let mut builder =
            sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT * FROM config_entries");
        if let Some(ref service_names) = filters.service_names
            && !service_names.is_empty()
        {
            builder.push(" WHERE service_name = ANY(");
            builder.push_bind(service_names);
            builder.push(")");
        }
        builder.push(" ORDER BY service_name");
        let entries = builder
            .build_query_as::<ConfigEntry>()
            .fetch_all(&*self.db)
            .await?;
        Ok(entries)
    }
    async fn decrypt_batch_entries(&self, entries: &mut [ConfigEntry]) -> ConfigResult<()> {
        let enc = self.encryption.lock().await;
        for entry in entries {
            if entry.is_sensitive {
                entry.content = enc.decrypt(&entry.service_name, &entry.content).await?;
            }
        }
        Ok(())
    }

    async fn prepare_batch_writes(
        &self,
        entries: &[ConfigEntry],
    ) -> ConfigResult<Vec<(uuid::Uuid, String, String, bool)>> {
        let mut prepared = Vec::with_capacity(entries.len());
        let enc = self.encryption.lock().await;
        for entry in entries {
            let content_hash = compute_batch_content_hash(&entry.content);
            let stored_content = if entry.is_sensitive {
                enc.encrypt(&entry.service_name, &entry.content).await?
            } else {
                entry.content.clone()
            };
            prepared.push((entry.id, stored_content, content_hash, entry.is_sensitive));
        }
        Ok(prepared)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn batch_update(
        &self,
        filters: &BatchFilters,
        operations: &[BatchOperation],
        updated_by: &str,
        dry_run: bool,
    ) -> ConfigResult<BatchResult> {
        let mut entries = self.query_by_filters(filters).await?;
        if entries.is_empty() {
            return Ok(BatchResult {
                success: true,
                affected_count: 0,
                affected_entries: vec![],
                errors: vec![],
            });
        }
        self.decrypt_batch_entries(&mut entries).await?;
        let mut errors: Vec<BatchError> = Vec::new();
        let mut successful_entries: Vec<ConfigEntry> = Vec::new();
        for entry in &entries {
            match apply_batch_operations(entry, operations) {
                Ok(updated) => successful_entries.push(updated),
                Err(error) => errors.push(BatchError {
                    entry_key: entry.service_name.clone(),
                    error,
                }),
            }
        }
        if dry_run {
            return Ok(BatchResult {
                success: errors.is_empty(),
                affected_count: successful_entries.len(),
                affected_entries: successful_entries
                    .iter()
                    .map(ConfigEntrySummary::from)
                    .collect(),
                errors,
            });
        }
        if !errors.is_empty() {
            return Ok(BatchResult {
                success: false,
                affected_count: 0,
                affected_entries: vec![],
                errors,
            });
        }
        let prepared_writes = self.prepare_batch_writes(&successful_entries).await?;
        let mut tx = self.db.begin().await?;
        for (id, stored_content, content_hash, is_sensitive) in &prepared_writes {
            if let Err(e) = sqlx::query(
                r"
                UPDATE config_entries
                SET content = $1, content_hash = $2, is_sensitive = $3, updated_by = $4, updated_at = NOW()
                WHERE id = $5
                ",
            )
            .bind(stored_content)
            .bind(content_hash)
            .bind(*is_sensitive)
            .bind(updated_by)
            .bind(*id)
            .execute(&mut *tx)
            .await
            {
                let _ = tx.rollback().await;
                return Err(e.into());
            }
        }
        tx.commit().await?;
        Ok(BatchResult {
            success: true,
            affected_count: prepared_writes.len(),
            affected_entries: successful_entries
                .iter()
                .map(ConfigEntrySummary::from)
                .collect(),
            errors: vec![],
        })
    }
}
/// 按顺序对单个条目应用批量操作并校验结果 TOML。
fn apply_batch_operations(
    entry: &ConfigEntry,
    operations: &[BatchOperation],
) -> Result<ConfigEntry, String> {
    let mut modified_content = entry.content.clone();
    let mut is_sensitive = entry.is_sensitive;
    for op in operations {
        match op {
            BatchOperation::ReplaceValue { path, value } => {
                modified_content = apply_toml_value_replace(&modified_content, path, value)?;
            }
            BatchOperation::SetSensitive { value } => is_sensitive = *value,
            BatchOperation::AddTag | BatchOperation::RemoveTag => {}
            BatchOperation::MergeContent { content } => {
                modified_content = merge_toml_contents(&modified_content, content)?;
            }
        }
    }
    validate_batch_toml(&modified_content).map_err(|e| e.to_string())?;
    Ok(ConfigEntry {
        content: modified_content,
        is_sensitive,
        ..entry.clone()
    })
}

fn apply_toml_value_replace(
    content: &str,
    path: &str,
    value: &serde_json::Value,
) -> Result<String, String> {
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("Failed to parse TOML: {e}"))?;
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return Err("Empty path".to_string());
    }
    let mut current = doc.as_table_mut();
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            let toml_value = json_to_toml_value(value)?;
            current[*part] = toml_value;
        } else {
            current = current
                .get_mut(part)
                .and_then(|item| item.as_table_mut())
                .ok_or_else(|| format!("Path not found: {}", parts[..=i].join(".")))?;
        }
    }
    Ok(doc.to_string())
}
fn merge_toml_contents(base: &str, overlay: &str) -> Result<String, String> {
    let mut base_doc = base
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("Failed to parse base TOML: {e}"))?;
    let overlay_doc = overlay
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("Failed to parse overlay TOML: {e}"))?;
    for (key, value) in overlay_doc.iter() {
        base_doc[key] = value.clone();
    }
    Ok(base_doc.to_string())
}
fn json_to_toml_value(value: &serde_json::Value) -> Result<toml_edit::Item, String> {
    match value {
        serde_json::Value::Null => Ok(toml_edit::Item::None),
        serde_json::Value::Bool(b) => Ok(toml_edit::value(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(toml_edit::value(i))
            } else if let Some(f) = n.as_f64() {
                Ok(toml_edit::value(f))
            } else {
                Err("Invalid number".to_string())
            }
        }
        serde_json::Value::String(s) => Ok(toml_edit::value(s.as_str())),
        serde_json::Value::Array(_) => {
            Err("Array values not supported in TOML path replacement".to_string())
        }
        serde_json::Value::Object(_) => {
            Err("Object values not supported in TOML path replacement".to_string())
        }
    }
}
