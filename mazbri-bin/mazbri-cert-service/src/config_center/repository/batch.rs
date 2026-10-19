use super::super::error::ConfigResult;
use super::super::models::{
    BatchError, BatchFilters, BatchOperation, BatchResult, ConfigEntry, ConfigEntrySummary,
};
use super::super::validator::{compute_batch_content_hash, validate_batch_toml};
use uuid::Uuid;
impl super::super::ConfigRepository {
    pub async fn query_by_filters(&self, filters: &BatchFilters) -> ConfigResult<Vec<ConfigEntry>> {
        let mut query = String::from("SELECT * FROM config_entries");
        let mut param_count = 0;
        if let Some(ref service_names) = filters.service_names {
            if !service_names.is_empty() {
                param_count += 1;
                query.push_str(&format!(" WHERE service_name = ANY(${param_count})"));
            }
        }
        query.push_str(" ORDER BY service_name");
        let mut sql_query = sqlx::query_as::<_, ConfigEntry>(&query);
        if let Some(ref service_names) = filters.service_names {
            if !service_names.is_empty() {
                sql_query = sql_query.bind(service_names);
            }
        }
        let entries = sql_query.fetch_all(&*self.db).await?;
        Ok(entries)
    }
    pub async fn batch_delete(
        &self,
        filters: &BatchFilters,
        dry_run: bool,
    ) -> ConfigResult<BatchResult> {
        let entries = self.query_by_filters(filters).await?;
        if entries.is_empty() {
            return Ok(BatchResult {
                success: true,
                affected_count: 0,
                affected_entries: vec![],
                errors: vec![],
            });
        }
        let entry_ids: Vec<Uuid> = entries.iter().map(|e| e.id).collect();
        if dry_run {
            return Ok(BatchResult {
                success: true,
                affected_count: entries.len(),
                affected_entries: entries.iter().map(ConfigEntrySummary::from).collect(),
                errors: vec![],
            });
        }
        let deleted_count = sqlx::query(
            r#"
            DELETE FROM config_entries
            WHERE id = ANY($1)
            "#,
        )
        .bind(&entry_ids)
        .execute(&*self.db)
        .await?
        .rows_affected();
        Ok(BatchResult {
            success: true,
            affected_count: deleted_count as usize,
            affected_entries: entries.iter().map(ConfigEntrySummary::from).collect(),
            errors: vec![],
        })
    }
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
        {
            let enc = self.encryption.lock().await;
            for entry in &mut entries {
                if entry.is_sensitive {
                    match enc.decrypt(&entry.content).await {
                        Ok(decrypted) => entry.content = decrypted,
                        Err(e) => {
                            return Err(e.into());
                        }
                    }
                }
            }
        }
        let mut errors: Vec<BatchError> = Vec::new();
        let mut successful_entries: Vec<ConfigEntry> = Vec::new();
        'entry_loop: for entry in &entries {
            let mut modified_content = entry.content.clone();
            let mut is_sensitive = entry.is_sensitive;
            for op in operations {
                match op {
                    BatchOperation::ReplaceValue { path, value } => {
                        match apply_toml_value_replace(&modified_content, path, value) {
                            Ok(new_content) => modified_content = new_content,
                            Err(e) => {
                                errors.push(BatchError {
                                    entry_key: entry.service_name.clone(),
                                    error: e,
                                });
                                continue 'entry_loop;
                            }
                        }
                    }
                    BatchOperation::SetSensitive { value } => {
                        is_sensitive = *value;
                    }
                    BatchOperation::AddTag | BatchOperation::RemoveTag => {}
                    BatchOperation::MergeContent { content } => {
                        match merge_toml_contents(&modified_content, content) {
                            Ok(new_content) => modified_content = new_content,
                            Err(e) => {
                                errors.push(BatchError {
                                    entry_key: entry.service_name.clone(),
                                    error: e,
                                });
                                continue 'entry_loop;
                            }
                        }
                    }
                }
            }
            if let Err(e) = validate_batch_toml(&modified_content) {
                errors.push(BatchError {
                    entry_key: entry.service_name.clone(),
                    error: e.to_string(),
                });
                continue;
            }
            successful_entries.push(ConfigEntry {
                content: modified_content,
                is_sensitive,
                ..entry.clone()
            });
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
        let mut updated_count = 0usize;
        for entry in &successful_entries {
            let (stored_content, content_hash) = if entry.is_sensitive {
                let enc = self.encryption.lock().await;
                let encrypted = enc.encrypt(&entry.content).await?;
                (encrypted, compute_batch_content_hash(&entry.content))
            } else {
                (
                    entry.content.clone(),
                    compute_batch_content_hash(&entry.content),
                )
            };
            let result = sqlx::query(
                    r#"
                UPDATE config_entries
                SET content = $1, content_hash = $2, is_sensitive = $3, updated_by = $4, updated_at = NOW()
                WHERE id = $5
                "#,
                )
                .bind(&stored_content)
                .bind(&content_hash)
                .bind(entry.is_sensitive)
                .bind(updated_by)
                .bind(entry.id)
                .execute(&*self.db)
                .await;
            match result {
                Ok(_) => updated_count += 1,
                Err(e) => {
                    errors.push(BatchError {
                        entry_key: entry.service_name.clone(),
                        error: e.to_string(),
                    });
                }
            }
        }
        Ok(BatchResult {
            success: errors.is_empty(),
            affected_count: updated_count,
            affected_entries: successful_entries
                .iter()
                .map(ConfigEntrySummary::from)
                .collect(),
            errors,
        })
    }
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
