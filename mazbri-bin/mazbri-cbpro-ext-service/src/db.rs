use anyhow::Result;
use chrono::TimeZone;
use mazbri_core::num::SaturatingFrom;
use mazbri_rpc::{RuntimeDecision, ValidationEnqueueRequest, ValidationResultRequest};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CbProExtRepository {
    db: Option<Arc<PgPool>>,
    platform_db: Option<Arc<PgPool>>,
    node_id: String,
}

#[derive(Debug, Clone)]
pub struct ValidationJobRecord {
    pub id: Uuid,
    pub cache_key: String,
    pub tenant_id: String,
    pub product_id: String,
    pub method: String,
    pub path: String,
}

impl CbProExtRepository {
    #[must_use]
    pub fn new(db: Option<Arc<PgPool>>, platform_db: Option<Arc<PgPool>>) -> Self {
        let node_id = hostname::get().map_or_else(
            |_| "unknown".to_string(),
            |value| value.to_string_lossy().to_string(),
        );
        Self {
            db,
            platform_db,
            node_id,
        }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn upsert_runtime_decision(&self, decision: &RuntimeDecision) -> Result<()> {
        let Some(db) = self.db.as_ref() else {
            return Ok(());
        };
        sqlx::query(
            r"
            INSERT INTO mazbri_cbpro.runtime_decisions (
                cache_key, tenant_id, product_id, method, path, mode, policy_version,
                latency_baseline_ms, latency_p50_ms, latency_p75_ms, latency_p95_ms,
                jitter_min_ms, jitter_max_ms, max_simulated_delay_ms, current_ttl_secs,
                hot_level, hit_count, hit_count_since_validate, validate_every_hits,
                max_validate_interval_secs, last_hit_at, last_validated_at,
                last_validation_status, stale_risk_level, generated_at, valid_until
            )
            VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,
                $22,$23,$24,$25,$26
            )
            ON CONFLICT (cache_key) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                product_id = EXCLUDED.product_id,
                method = EXCLUDED.method,
                path = EXCLUDED.path,
                mode = EXCLUDED.mode,
                policy_version = EXCLUDED.policy_version,
                latency_baseline_ms = EXCLUDED.latency_baseline_ms,
                latency_p50_ms = EXCLUDED.latency_p50_ms,
                latency_p75_ms = EXCLUDED.latency_p75_ms,
                latency_p95_ms = EXCLUDED.latency_p95_ms,
                jitter_min_ms = EXCLUDED.jitter_min_ms,
                jitter_max_ms = EXCLUDED.jitter_max_ms,
                max_simulated_delay_ms = EXCLUDED.max_simulated_delay_ms,
                current_ttl_secs = EXCLUDED.current_ttl_secs,
                hot_level = EXCLUDED.hot_level,
                hit_count = EXCLUDED.hit_count,
                hit_count_since_validate = EXCLUDED.hit_count_since_validate,
                validate_every_hits = EXCLUDED.validate_every_hits,
                max_validate_interval_secs = EXCLUDED.max_validate_interval_secs,
                last_hit_at = EXCLUDED.last_hit_at,
                last_validated_at = EXCLUDED.last_validated_at,
                last_validation_status = EXCLUDED.last_validation_status,
                stale_risk_level = EXCLUDED.stale_risk_level,
                generated_at = EXCLUDED.generated_at,
                valid_until = EXCLUDED.valid_until,
                updated_at = NOW()
            ",
        )
        .bind(&decision.cache_key)
        .bind(&decision.tenant_id)
        .bind(&decision.product_id)
        .bind(&decision.method)
        .bind(&decision.path)
        .bind(&decision.mode)
        .bind(i64::try_from(decision.policy_version).unwrap_or(i64::MAX))
        .bind(mazbri_core::num::u64_to_f64(decision.latency_baseline_ms))
        .bind(mazbri_core::num::u64_to_f64(decision.latency_p50_ms))
        .bind(mazbri_core::num::u64_to_f64(decision.latency_p75_ms))
        .bind(mazbri_core::num::u64_to_f64(decision.latency_p95_ms))
        .bind(i64::try_from(decision.jitter_min_ms).unwrap_or(i64::MAX))
        .bind(i64::try_from(decision.jitter_max_ms).unwrap_or(i64::MAX))
        .bind(i64::try_from(decision.max_simulated_delay_ms).unwrap_or(i64::MAX))
        .bind(i64::try_from(decision.current_ttl_secs).unwrap_or(i64::MAX))
        .bind(i32::try_from(decision.hot_level).unwrap_or(i32::MAX))
        .bind(0_i64)
        .bind(0_i64)
        .bind(10_i64)
        .bind(300_i64)
        .bind(Option::<chrono::DateTime<chrono::Utc>>::None)
        .bind(Option::<chrono::DateTime<chrono::Utc>>::None)
        .bind(&decision.last_validation_status)
        .bind(0_i32)
        .bind(
            chrono::Utc
                .timestamp_millis_opt(decision.generated_at_unix_ms)
                .single(),
        )
        .bind(
            chrono::Utc
                .timestamp_millis_opt(decision.valid_until_unix_ms)
                .single(),
        )
        .execute(&**db)
        .await?;
        Ok(())
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_runtime_decision(&self, cache_key: &str) -> Result<Option<RuntimeDecision>> {
        let Some(db) = self.db.as_ref() else {
            return Ok(None);
        };
        let row = sqlx::query(
            r"
            SELECT cache_key, tenant_id, product_id, method, path, mode, policy_version,
                   latency_baseline_ms, latency_p50_ms, latency_p75_ms, latency_p95_ms,
                   jitter_min_ms, jitter_max_ms, max_simulated_delay_ms, current_ttl_secs,
                   hot_level, hit_count_since_validate, validate_every_hits, max_validate_interval_secs,
                   last_validated_at, last_validation_status, generated_at, valid_until
            FROM mazbri_cbpro.runtime_decisions
            WHERE cache_key = $1
            ",
        )
        .bind(cache_key)
        .fetch_optional(&**db)
        .await?;
        Ok(row.map(|row| Self::runtime_decision_from_row(&row)))
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn list_runtime_decisions(&self, limit: i64) -> Result<Vec<RuntimeDecision>> {
        let Some(db) = self.db.as_ref() else {
            return Ok(Vec::new());
        };
        let rows = sqlx::query(
            r"
            SELECT cache_key, tenant_id, product_id, method, path, mode, policy_version,
                   latency_baseline_ms, latency_p50_ms, latency_p75_ms, latency_p95_ms,
                   jitter_min_ms, jitter_max_ms, max_simulated_delay_ms, current_ttl_secs,
                   hot_level, hit_count_since_validate, validate_every_hits, max_validate_interval_secs,
                   last_validated_at, last_validation_status, generated_at, valid_until
            FROM mazbri_cbpro.runtime_decisions
            ORDER BY updated_at DESC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&**db)
        .await?;
        Ok(rows.iter().map(Self::runtime_decision_from_row).collect())
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn enqueue_validation_job(
        &self,
        request: &ValidationEnqueueRequest,
    ) -> Result<Option<Uuid>> {
        let Some(db) = self.db.as_ref() else {
            return Ok(None);
        };
        let row = sqlx::query(
            r"
            INSERT INTO mazbri_cbpro.validation_jobs (
                cache_key, tenant_id, product_id, method, path, reason, priority
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7)
            ON CONFLICT (cache_key) WHERE status IN ('pending', 'running')
            DO UPDATE SET
                reason = EXCLUDED.reason,
                priority = LEAST(mazbri_cbpro.validation_jobs.priority, EXCLUDED.priority),
                updated_at = NOW()
            RETURNING id
            ",
        )
        .bind(&request.cache_key)
        .bind(&request.tenant_id)
        .bind(&request.product_id)
        .bind(&request.method)
        .bind(&request.path)
        .bind(&request.reason)
        .bind(request.priority)
        .fetch_optional(&**db)
        .await?;
        if let Some(row) = row {
            let id: Uuid = row.try_get("id")?;
            Ok(Some(id))
        } else {
            let existing = sqlx::query(
                r"
                SELECT id
                FROM mazbri_cbpro.validation_jobs
                WHERE cache_key = $1 AND status IN ('pending', 'running')
                ORDER BY updated_at DESC
                LIMIT 1
                ",
            )
            .bind(&request.cache_key)
            .fetch_optional(&**db)
            .await?;
            Ok(existing.map(|row| row.get("id")))
        }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn claim_validation_jobs(
        &self,
        limit: i64,
        lock_ttl_secs: i64,
    ) -> Result<Vec<ValidationJobRecord>> {
        let Some(db) = self.db.as_ref() else {
            return Ok(Vec::new());
        };
        let rows = sqlx::query(
            r"
            WITH picked AS (
                SELECT id
                FROM mazbri_cbpro.validation_jobs
                WHERE status = 'pending'
                  AND (locked_until IS NULL OR locked_until < NOW())
                ORDER BY priority ASC, scheduled_at ASC
                LIMIT $1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE mazbri_cbpro.validation_jobs AS jobs
            SET status = 'running',
                attempts = attempts + 1,
                locked_by = $2,
                locked_until = NOW() + ($3 || ' seconds')::interval,
                started_at = COALESCE(started_at, NOW()),
                updated_at = NOW()
            FROM picked
            WHERE jobs.id = picked.id
            RETURNING jobs.id, jobs.cache_key, jobs.tenant_id, jobs.product_id,
                      jobs.method, jobs.path
            ",
        )
        .bind(limit)
        .bind(&self.node_id)
        .bind(lock_ttl_secs)
        .fetch_all(&**db)
        .await?;
        let mut jobs = Vec::with_capacity(rows.len());
        for row in rows {
            jobs.push(ValidationJobRecord {
                id: row.get("id"),
                cache_key: row.get("cache_key"),
                tenant_id: row.get("tenant_id"),
                product_id: row.get("product_id"),
                method: row.get("method"),
                path: row.get("path"),
            });
        }
        Ok(jobs)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn mark_validation_result(
        &self,
        request: &ValidationResultRequest,
        status: &str,
    ) -> Result<()> {
        let Some(db) = self.db.as_ref() else {
            return Ok(());
        };
        sqlx::query(
            r"
            UPDATE mazbri_cbpro.validation_jobs
            SET status = $2,
                finished_at = NOW(),
                last_error = $3,
                updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(Uuid::parse_str(&request.job_id).unwrap_or_else(|_| Uuid::nil()))
        .bind(status)
        .bind(request.message.as_deref())
        .execute(&**db)
        .await?;
        Ok(())
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn restore_runtime_decisions(&self) -> Result<Vec<RuntimeDecision>> {
        self.list_runtime_decisions(10_000).await
    }

    #[must_use]
    pub fn platform_db(&self) -> Option<&Arc<PgPool>> {
        self.platform_db.as_ref()
    }

    #[must_use]
    pub fn is_missing_cbpro_schema_error(error: &anyhow::Error) -> bool {
        error.chain().any(|cause| {
            cause
                .downcast_ref::<sqlx::Error>()
                .is_some_and(|sqlx_error| {
                    matches!(
                        sqlx_error,
                        sqlx::Error::Database(database_error)
                            if matches!(database_error.code().as_deref(), Some("42P01" | "3F000"))
                    )
                })
        })
    }

    fn runtime_decision_from_row(row: &sqlx::postgres::PgRow) -> RuntimeDecision {
        let valid_until = row
            .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("valid_until")
            .ok()
            .flatten()
            .map(|dt| dt.timestamp_millis())
            .unwrap_or_default();
        let generated_at = row
            .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("generated_at")
            .ok()
            .flatten()
            .map(|dt| dt.timestamp_millis())
            .unwrap_or_default();
        RuntimeDecision {
            cache_key: row.get("cache_key"),
            tenant_id: row.get("tenant_id"),
            product_id: row.get("product_id"),
            method: row.get("method"),
            path: row.get("path"),
            mode: row.get("mode"),
            policy_version: u64::saturating_from(row.get::<i64, _>("policy_version").max(0)),
            latency_baseline_ms: mazbri_core::num::f64_to_u64(
                row.get::<Option<f64>, _>("latency_baseline_ms")
                    .unwrap_or(120.0),
            ),
            latency_p50_ms: mazbri_core::num::f64_to_u64(
                row.get::<Option<f64>, _>("latency_p50_ms").unwrap_or(120.0),
            ),
            latency_p75_ms: mazbri_core::num::f64_to_u64(
                row.get::<Option<f64>, _>("latency_p75_ms").unwrap_or(132.0),
            ),
            latency_p95_ms: mazbri_core::num::f64_to_u64(
                row.get::<Option<f64>, _>("latency_p95_ms").unwrap_or(150.0),
            ),
            jitter_min_ms: u64::saturating_from(row.get::<i64, _>("jitter_min_ms").max(0)),
            jitter_max_ms: u64::saturating_from(row.get::<i64, _>("jitter_max_ms").max(0)),
            max_simulated_delay_ms: u64::saturating_from(
                row.get::<i64, _>("max_simulated_delay_ms").max(0),
            ),
            current_ttl_secs: u64::saturating_from(row.get::<i64, _>("current_ttl_secs").max(0)),
            validate_due: row.get::<i64, _>("hit_count_since_validate")
                >= row.get::<i64, _>("validate_every_hits")
                || row
                    .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_validated_at")
                    .ok()
                    .flatten()
                    .is_none_or(|ts| {
                        (chrono::Utc::now() - ts).num_seconds()
                            >= row.get::<i64, _>("max_validate_interval_secs")
                    }),
            hot_level: u32::saturating_from(row.get::<i32, _>("hot_level").max(0)),
            last_validation_status: row
                .try_get::<Option<String>, _>("last_validation_status")
                .ok()
                .flatten(),
            valid_until_unix_ms: valid_until,
            generated_at_unix_ms: generated_at,
        }
    }
}
