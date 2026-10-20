use super::super::error::{ConfigError, ConfigResult};
use super::super::models::ConfigEntry;
use super::super::validator::{compute_content_hash, validate_toml};
impl super::super::repository::ConfigRepository {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn create(
        &self,
        req: &super::super::models::CreateConfigRequest,
    ) -> ConfigResult<ConfigEntry> {
        validate_toml(&req.content)?;
        let (stored_content, content_hash) = if req.is_sensitive {
            let enc = self.encryption.lock().await;
            let encrypted = enc.encrypt(&req.content).await?;
            (encrypted, compute_content_hash(&req.content))
        } else {
            (req.content.clone(), compute_content_hash(&req.content))
        };
        let result = sqlx::query_as::<
            _,
            ConfigEntry,
        >(
                r"
            INSERT INTO config_entries (service_name, content, content_hash, is_sensitive, updated_by)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            ",
            )
            .bind(&req.service_name)
            .bind(&stored_content)
            .bind(&content_hash)
            .bind(req.is_sensitive)
            .bind(&req.updated_by)
            .fetch_one(&*self.db)
        .await;
        match result {
            Ok(entry) => Ok(entry),
            Err(sqlx::Error::Database(e)) if e.constraint() == Some("unique_service_config") => {
                Err(ConfigError::AlreadyExists {
                    service_name: req.service_name.clone(),
                })
            }
            Err(e) => Err(e.into()),
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn update(
        &self,
        service_name: &str,
        content: &str,
        updated_by: &str,
        is_sensitive: bool,
    ) -> ConfigResult<ConfigEntry> {
        validate_toml(content)?;
        let (stored_content, content_hash) = if is_sensitive {
            let enc = self.encryption.lock().await;
            let encrypted = enc.encrypt(content).await?;
            (encrypted, compute_content_hash(content))
        } else {
            (content.to_string(), compute_content_hash(content))
        };
        let result = sqlx::query_as::<_, ConfigEntry>(
            r"
            UPDATE config_entries
            SET content = $2, content_hash = $3, is_sensitive = $4, updated_by = $5
            WHERE service_name = $1
            RETURNING *
            ",
        )
        .bind(service_name)
        .bind(&stored_content)
        .bind(&content_hash)
        .bind(is_sensitive)
        .bind(updated_by)
        .fetch_optional(&*self.db)
        .await?;
        match result {
            Some(entry) => Ok(entry),
            None => Err(ConfigError::NotFound {
                service_name: service_name.to_string(),
            }),
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn rollback(
        &self,
        service_name: &str,
        content: &str,
        content_hash: Option<&str>,
        updated_by: &str,
    ) -> ConfigResult<ConfigEntry> {
        let result = sqlx::query_as::<_, ConfigEntry>(
            r"
            UPDATE config_entries
            SET content = $2, content_hash = $3, updated_by = $4, updated_at = NOW()
            WHERE service_name = $1
            RETURNING *
            ",
        )
        .bind(service_name)
        .bind(content)
        .bind(content_hash)
        .bind(updated_by)
        .fetch_optional(&*self.db)
        .await?;
        match result {
            Some(entry) => Ok(entry),
            None => Err(ConfigError::NotFound {
                service_name: service_name.to_string(),
            }),
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn delete(&self, service_name: &str) -> ConfigResult<()> {
        let result = sqlx::query(
            r"
            DELETE FROM config_entries
            WHERE service_name = $1
            ",
        )
        .bind(service_name)
        .execute(&*self.db)
        .await?;
        if result.rows_affected() == 0 {
            return Err(ConfigError::NotFound {
                service_name: service_name.to_string(),
            });
        }
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_by_service(
        &self,
        service_name: &str,
        decrypt: bool,
    ) -> ConfigResult<ConfigEntry> {
        let mut entry = sqlx::query_as::<_, ConfigEntry>(
            r"
            SELECT * FROM config_entries
            WHERE service_name = $1
            ",
        )
        .bind(service_name)
        .fetch_optional(&*self.db)
        .await?
        .ok_or_else(|| ConfigError::NotFound {
            service_name: service_name.to_string(),
        })?;
        if decrypt && entry.is_sensitive {
            let enc = self.encryption.lock().await;
            entry.content = enc.decrypt(&entry.content).await?;
        }
        Ok(entry)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn list(&self, limit: i64, offset: i64) -> ConfigResult<Vec<ConfigEntry>> {
        let entries = sqlx::query_as::<_, ConfigEntry>(
            r"
            SELECT * FROM config_entries
            ORDER BY service_name
            LIMIT $1 OFFSET $2
            ",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.db)
        .await?;
        Ok(entries)
    }
}
