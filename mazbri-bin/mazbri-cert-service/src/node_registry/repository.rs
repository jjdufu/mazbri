use super::types::{HeartbeatData, RegisterNodeData, RegisteredNode};
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::debug;
const HEARTBEAT_TIMEOUT_SECONDS: i64 = 300;
#[derive(Clone)]
pub struct NodeRegistryRepository {
    db: Arc<PgPool>,
}
impl NodeRegistryRepository {
    pub fn new(db: Arc<PgPool>) -> Self {
        Self { db }
    }
    pub async fn register(&self, data: &RegisterNodeData) -> Result<RegisteredNode, sqlx::Error> {
        let started_at = DateTime::from_timestamp(data.started_at, 0).unwrap_or_else(Utc::now);
        sqlx::query_as::<
            _,
            RegisteredNode,
        >(
                r#"
            INSERT INTO registered_nodes
                (service_name, ip, port, started_at, config_version, config_md5, health_check_url, last_heartbeat_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
            ON CONFLICT (service_name, ip, port) DO UPDATE SET
                started_at = EXCLUDED.started_at,
                config_version = EXCLUDED.config_version,
                config_md5 = EXCLUDED.config_md5,
                health_check_url = EXCLUDED.health_check_url,
                last_heartbeat_at = NOW()
            RETURNING *
            "#,
            )
            .bind(&data.service_name)
            .bind(&data.ip)
            .bind(data.port as i32)
            .bind(started_at)
            .bind(data.config_version)
            .bind(&data.config_md5)
            .bind(&data.health_check_url)
            .fetch_one(&*self.db)
            .await
    }
    pub async fn heartbeat(
        &self,
        data: &HeartbeatData,
        ip: &str,
        port: u16,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE registered_nodes
            SET last_heartbeat_at = NOW(), config_version = $4
            WHERE service_name = $1 AND ip = $2 AND port = $3
            "#,
        )
        .bind(&data.service_name)
        .bind(ip)
        .bind(port as i32)
        .bind(data.config_version)
        .execute(&*self.db)
        .await?;
        Ok(result.rows_affected() > 0)
    }
    pub async fn list_online(&self) -> Result<Vec<RegisteredNode>, sqlx::Error> {
        let cutoff = Utc::now() - Duration::seconds(HEARTBEAT_TIMEOUT_SECONDS);
        sqlx::query_as::<_, RegisteredNode>(
            r#"
            SELECT * FROM registered_nodes
            WHERE last_heartbeat_at > $1
            ORDER BY service_name, ip, port
            "#,
        )
        .bind(cutoff)
        .fetch_all(&*self.db)
        .await
    }
    pub async fn cleanup_offline(&self) -> Result<u64, sqlx::Error> {
        let cutoff = Utc::now() - Duration::seconds(HEARTBEAT_TIMEOUT_SECONDS);
        let result = sqlx::query("DELETE FROM registered_nodes WHERE last_heartbeat_at < $1")
            .bind(cutoff)
            .execute(&*self.db)
            .await?;
        let deleted = result.rows_affected();
        if deleted > 0 {
            debug!(deleted_count = deleted, "已清理离线节点");
        }
        Ok(deleted)
    }
}
