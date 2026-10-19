use super::super::error::{ConfigError, ConfigResult};
use super::super::models::ConfigHistory;
impl super::super::repository::ConfigRepository {
    pub async fn get_history(
        &self,
        service_name: &str,
        limit: i64,
        offset: i64,
    ) -> ConfigResult<Vec<ConfigHistory>> {
        let records = sqlx::query_as::<_, ConfigHistory>(
            r#"
            SELECT * FROM config_history
            WHERE service_name = $1
            ORDER BY version_number DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(service_name)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.db)
        .await?;
        Ok(records)
    }
    pub async fn get_history_version(
        &self,
        service_name: &str,
        version_number: i32,
    ) -> ConfigResult<ConfigHistory> {
        let record = sqlx::query_as::<_, ConfigHistory>(
            r#"
            SELECT * FROM config_history
            WHERE service_name = $1 AND version_number = $2
            "#,
        )
        .bind(service_name)
        .bind(version_number)
        .fetch_optional(&*self.db)
        .await?
        .ok_or_else(|| ConfigError::NotFound {
            service_name: service_name.to_string(),
        })?;
        Ok(record)
    }
    pub async fn get_service_version(&self, service_name: &str) -> ConfigResult<i32> {
        let version: Option<i32> = sqlx::query_scalar(
            r#"
            SELECT MAX(ch.version_number)
            FROM config_history ch
            WHERE ch.service_name = $1
            "#,
        )
        .bind(service_name)
        .fetch_one(&*self.db)
        .await?;
        Ok(version.unwrap_or(0))
    }
}
