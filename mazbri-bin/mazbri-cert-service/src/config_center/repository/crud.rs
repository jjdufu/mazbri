use super::super::encryption::ConfigEncryption;
use super::super::error::{ConfigError, ConfigResult};
use super::super::models::{ConfigEntry, ConfigHistory};
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
            let encrypted = enc.encrypt(&req.service_name, &req.content).await?;
            (encrypted, compute_content_hash(&req.content))
        } else {
            (req.content.clone(), compute_content_hash(&req.content))
        };
        let result = sqlx::query_as::<_, ConfigEntry>(
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
            Ok(mut entry) => {
                if req.is_sensitive {
                    entry.content.clone_from(&req.content);
                }
                Ok(entry)
            }
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
        expected_content_hash: Option<&str>,
    ) -> ConfigResult<ConfigEntry> {
        validate_toml(content)?;
        let existing = self.get_by_service(service_name, true).await?;
        let existing_hash = existing
            .content_hash
            .clone()
            .unwrap_or_else(|| compute_content_hash(&existing.content));
        if let Some(expected) = expected_content_hash
            && expected != existing_hash
        {
            return Err(ConfigError::Conflict {
                service_name: service_name.to_string(),
                expected_hash: expected.to_string(),
                current_hash: Some(existing_hash),
            });
        }
        if existing.content == content && existing.is_sensitive == is_sensitive {
            return Ok(existing);
        }
        let (stored_content, content_hash) = if is_sensitive {
            let enc = self.encryption.lock().await;
            let encrypted = enc.encrypt(service_name, content).await?;
            (encrypted, compute_content_hash(content))
        } else {
            (content.to_string(), compute_content_hash(content))
        };
        let result = sqlx::query_as::<_, ConfigEntry>(
            r"
            UPDATE config_entries
            SET content = $2, content_hash = $3, is_sensitive = $4, updated_by = $5, updated_at = NOW()
            WHERE service_name = $1
              AND ($6::text IS NULL OR content_hash = $6)
            RETURNING *
            ",
        )
        .bind(service_name)
        .bind(&stored_content)
        .bind(&content_hash)
        .bind(is_sensitive)
        .bind(updated_by)
        .bind(expected_content_hash)
        .fetch_optional(&*self.db)
        .await?;
        match result {
            Some(mut entry) => {
                if is_sensitive {
                    content.clone_into(&mut entry.content);
                }
                Ok(entry)
            }
            None => {
                if let Some(expected) = expected_content_hash {
                    match self.get_by_service(service_name, false).await {
                        Ok(curr) => Err(ConfigError::Conflict {
                            service_name: service_name.to_string(),
                            expected_hash: expected.to_string(),
                            current_hash: curr.content_hash,
                        }),
                        Err(ConfigError::NotFound { .. }) => Err(ConfigError::NotFound {
                            service_name: service_name.to_string(),
                        }),
                        Err(e) => Err(e),
                    }
                } else {
                    Err(ConfigError::NotFound {
                        service_name: service_name.to_string(),
                    })
                }
            }
        }
    }

    /// 将历史快照解密还原为明文 TOML（供回滚前执行 TOML 与业务规则校验）。
    ///
    /// # Errors
    ///
    /// 历史密文损坏、AAD 归属不匹配或格式不受支持时返回错误。
    pub async fn decrypt_history_content(&self, history: &ConfigHistory) -> ConfigResult<String> {
        if history.is_sensitive || ConfigEncryption::is_encrypted(&history.content) {
            let enc = self.encryption.lock().await;
            Ok(enc.decrypt(&history.service_name, &history.content).await?)
        } else {
            Ok(history.content.clone())
        }
    }

    /// 使用已校验的明文 TOML 与历史 `is_sensitive` 标记执行回滚写回（敏感配置重新生成随机 Nonce 密封）。
    ///
    /// # Errors
    ///
    /// TOML 语法非法、加密失败或数据库更新失败时返回错误。
    pub async fn rollback(
        &self,
        service_name: &str,
        plaintext_content: &str,
        is_sensitive: bool,
        updated_by: &str,
    ) -> ConfigResult<ConfigEntry> {
        validate_toml(plaintext_content)?;
        let content_hash = compute_content_hash(plaintext_content);
        let stored_content = if is_sensitive {
            let enc = self.encryption.lock().await;
            enc.encrypt(service_name, plaintext_content).await?
        } else {
            plaintext_content.to_string()
        };
        let result = sqlx::query_as::<_, ConfigEntry>(
            r"
            UPDATE config_entries
            SET content = $2, content_hash = $3, is_sensitive = $4, updated_by = $5, updated_at = NOW()
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
            Some(mut entry) => {
                if is_sensitive {
                    plaintext_content.clone_into(&mut entry.content);
                }
                Ok(entry)
            }
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
            entry.content = enc.decrypt(&entry.service_name, &entry.content).await?;
        }
        Ok(entry)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn list(&self, limit: i64, offset: i64) -> ConfigResult<Vec<ConfigEntry>> {
        let mut entries = sqlx::query_as::<_, ConfigEntry>(
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
        if entries.iter().any(|e| e.is_sensitive) {
            let enc = self.encryption.lock().await;
            for entry in &mut entries {
                if entry.is_sensitive {
                    entry.content = enc.decrypt(&entry.service_name, &entry.content).await?;
                }
            }
        }
        Ok(entries)
    }
}
