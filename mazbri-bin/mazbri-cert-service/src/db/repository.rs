use super::models::{AutoRenewalConfig, CaStorageRecord, CertificateRecord, CertificateSummary};
use crate::error::{CertError, CertResult};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Structured list filter (List Query JSON shape).
#[derive(Debug, Clone, Deserialize)]
pub struct CertListFilter {
    pub field: String,
    pub op: String,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
}
fn compute_cert_fingerprint(pem: &str) -> Result<Vec<u8>, String> {
    use x509_parser::pem::parse_x509_pem;
    let (_, pem_obj) = parse_x509_pem(pem.as_bytes()).map_err(|e| format!("invalid PEM: {e}"))?;
    Ok(Sha256::digest(&pem_obj.contents).to_vec())
}
pub struct CertificateRepository {
    db: Arc<PgPool>,
}
impl CertificateRepository {
    #[must_use]
    pub fn new(db: Arc<PgPool>) -> Self {
        Self { db }
    }
    #[must_use]
    pub fn get_pool(&self) -> &PgPool {
        &self.db
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn create_certificate(
        &self,
        subject: &str,
        cert_pem: &str,
        key_pem: &str,
        expires_at: DateTime<Utc>,
        dns_names: &[String],
        created_by_ip: Option<std::net::IpAddr>,
    ) -> CertResult<CertificateRecord> {
        let fingerprint = compute_cert_fingerprint(cert_pem)
            .map_err(|e| CertError::VerificationFailed(e.clone()))?;
        let record = sqlx::query_as::<
            _,
            CertificateRecord,
        >(
                r"
            INSERT INTO certificates (subject, cert_pem, key_pem, expires_at, dns_names, created_by_ip, fingerprint)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            ",
            )
            .bind(subject)
            .bind(cert_pem)
            .bind(key_pem)
            .bind(expires_at)
            .bind(dns_names)
            .bind(created_by_ip)
            .bind(&fingerprint)
            .fetch_one(&*self.db)
            .await?;
        Ok(record)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_by_id(&self, id: Uuid) -> CertResult<CertificateRecord> {
        let record =
            sqlx::query_as::<_, CertificateRecord>("SELECT * FROM certificates WHERE id = $1")
                .bind(id)
                .fetch_optional(&*self.db)
                .await?
                .ok_or_else(|| CertError::NotFound(id.to_string()))?;
        Ok(record)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_by_subject(&self, subject: &str) -> CertResult<Option<CertificateRecord>> {
        let record = sqlx::query_as::<
            _,
            CertificateRecord,
        >(
                "SELECT * FROM certificates WHERE subject = $1 AND revoked = false AND expires_at > NOW() ORDER BY created_at DESC LIMIT 1",
            )
            .bind(subject)
            .fetch_optional(&*self.db)
            .await?;
        Ok(record)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn subject_exists(&self, subject: &str) -> CertResult<bool> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM certificates WHERE subject = $1 AND revoked = false)",
        )
        .bind(subject)
        .fetch_one(&*self.db)
        .await?;
        Ok(exists)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn update_certificate(
        &self,
        id: Uuid,
        cert_pem: &str,
        key_pem: &str,
        expires_at: DateTime<Utc>,
        dns_names: &[String],
    ) -> CertResult<CertificateRecord> {
        let record = sqlx::query_as::<_, CertificateRecord>(
            r"
            UPDATE certificates
            SET cert_pem = $2, key_pem = $3, expires_at = $4, dns_names = $5,
                renewal_count = renewal_count + 1, updated_at = NOW()
            WHERE id = $1 AND revoked = false
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cert_pem)
        .bind(key_pem)
        .bind(expires_at)
        .bind(dns_names)
        .fetch_optional(&*self.db)
        .await?
        .ok_or_else(|| CertError::NotFound(id.to_string()))?;
        Ok(record)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn revoke_certificate(&self, id: Uuid) -> CertResult<CertificateRecord> {
        let existing = self.get_by_id(id).await?;
        if existing.revoked {
            return Err(CertError::AlreadyRevoked(id.to_string()));
        }
        let record = sqlx::query_as::<_, CertificateRecord>(
            r"
            UPDATE certificates
            SET revoked = true, revoked_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND revoked = false
            RETURNING *
            ",
        )
        .bind(id)
        .fetch_optional(&*self.db)
        .await?
        .ok_or_else(|| CertError::NotFound(id.to_string()))?;
        Ok(record)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn list_certificates(
        &self,
        include_revoked: bool,
        q: Option<&str>,
        filters: &[CertListFilter],
        limit: i64,
        offset: i64,
    ) -> CertResult<Vec<CertificateSummary>> {
        let mut builder: sqlx::QueryBuilder<'_, sqlx::Postgres> = sqlx::QueryBuilder::new(
            "SELECT id, subject, issued_at, expires_at, revoked, revoked_at, renewal_count, dns_names, fingerprint \
             FROM certificates WHERE 1=1",
        );
        push_cert_list_scope(&mut builder, include_revoked, q, filters)?;
        builder.push(" ORDER BY created_at DESC LIMIT ");
        builder.push_bind(limit);
        builder.push(" OFFSET ");
        builder.push_bind(offset);
        let records = builder
            .build_query_as::<CertificateSummary>()
            .fetch_all(&*self.db)
            .await?;
        Ok(records)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn find_by_fingerprint(
        &self,
        fingerprint: &[u8],
    ) -> CertResult<Option<CertificateSummary>> {
        let record = sqlx::query_as::<
            _,
            CertificateSummary,
        >(
                "SELECT id, subject, issued_at, expires_at, revoked, revoked_at, renewal_count, dns_names, fingerprint FROM certificates WHERE fingerprint = $1",
            )
            .bind(fingerprint)
            .fetch_optional(&*self.db)
            .await?;
        Ok(record)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn create_audit_log(
        &self,
        action: &str,
        actor_ip: Option<std::net::IpAddr>,
        target_subject: Option<&str>,
        target_cert_id: Option<Uuid>,
        result: &str,
        error_message: Option<&str>,
    ) -> CertResult<()> {
        sqlx::query(
                r"
            INSERT INTO audit_logs (action, actor_ip, target_subject, target_cert_id, result, error_message)
            VALUES ($1, $2, $3, $4, $5, $6)
            ",
            )
            .bind(action)
            .bind(actor_ip)
            .bind(target_subject)
            .bind(target_cert_id)
            .bind(result)
            .bind(error_message)
            .execute(&*self.db)
            .await?;
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_active_ca(&self) -> CertResult<Option<CaStorageRecord>> {
        let record = sqlx::query_as::<_, CaStorageRecord>(
            "SELECT * FROM ca_storage WHERE is_active = true ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_optional(&*self.db)
        .await?;
        Ok(record)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn store_ca(&self, ca_cert_pem: &str, ca_key_encrypted: &str) -> CertResult<()> {
        sqlx::query(
            r"
            INSERT INTO ca_storage (ca_cert_pem, ca_key_encrypted, is_active)
            VALUES ($1, $2, true)
            ",
        )
        .bind(ca_cert_pem)
        .bind(ca_key_encrypted)
        .execute(&*self.db)
        .await?;
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn count_certificates(
        &self,
        include_revoked: bool,
        q: Option<&str>,
        filters: &[CertListFilter],
    ) -> CertResult<i64> {
        let mut builder: sqlx::QueryBuilder<'_, sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) FROM certificates WHERE 1=1");
        push_cert_list_scope(&mut builder, include_revoked, q, filters)?;
        let count: i64 = builder.build_query_scalar().fetch_one(&*self.db).await?;
        Ok(count)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn count_active_certificates(&self) -> CertResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM certificates WHERE revoked = false AND expires_at > NOW()",
        )
        .fetch_one(&*self.db)
        .await?;
        Ok(count)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn count_expiring_certificates(&self, threshold_days: i64) -> CertResult<i64> {
        let count: i64 = sqlx::query_scalar(
            r"
            SELECT COUNT(*) FROM certificates
            WHERE revoked = false
            AND expires_at > NOW()
            AND expires_at < NOW() + INTERVAL '1 day' * $1
            ",
        )
        .bind(threshold_days)
        .fetch_one(&*self.db)
        .await?;
        Ok(count)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn min_active_expiry_days(&self) -> CertResult<Option<i64>> {
        let days: Option<i64> = sqlx::query_scalar(
            r"
            SELECT FLOOR(EXTRACT(EPOCH FROM (MIN(expires_at) - NOW())) / 86400)::BIGINT
            FROM certificates
            WHERE revoked = false AND expires_at > NOW()
            ",
        )
        .fetch_one(&*self.db)
        .await?;
        Ok(days)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_auto_renewal_config(
        &self,
        certificate_id: Uuid,
    ) -> CertResult<Option<AutoRenewalConfig>> {
        let config = sqlx::query_as::<_, AutoRenewalConfig>(
            "SELECT * FROM auto_renewal_config WHERE certificate_id = $1",
        )
        .bind(certificate_id)
        .fetch_optional(&*self.db)
        .await?;
        Ok(config)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn upsert_auto_renewal_config(
        &self,
        certificate_id: Uuid,
        enabled: bool,
        threshold_days: i32,
        validity_days: i32,
    ) -> CertResult<AutoRenewalConfig> {
        let config = sqlx::query_as::<_, AutoRenewalConfig>(
            r"
            INSERT INTO auto_renewal_config (certificate_id, enabled, threshold_days, validity_days)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (certificate_id) DO UPDATE SET
                enabled = EXCLUDED.enabled,
                threshold_days = EXCLUDED.threshold_days,
                validity_days = EXCLUDED.validity_days,
                updated_at = NOW()
            RETURNING *
            ",
        )
        .bind(certificate_id)
        .bind(enabled)
        .bind(threshold_days)
        .bind(validity_days)
        .fetch_one(&*self.db)
        .await?;
        Ok(config)
    }
}

const CERT_STATUS_SQL: &str = "CASE WHEN revoked THEN 'revoked' WHEN expires_at <= NOW() THEN 'expired' WHEN expires_at <= NOW() + INTERVAL '30 days' THEN 'expiring_soon' ELSE 'valid' END";

fn push_cert_list_scope(
    builder: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>,
    include_revoked: bool,
    q: Option<&str>,
    filters: &[CertListFilter],
) -> CertResult<()> {
    if !include_revoked {
        builder.push(" AND revoked = false");
    }
    if let Some(raw) = q.map(str::trim).filter(|s| !s.is_empty()) {
        let pat = format!("%{raw}%");
        builder.push(" AND (subject ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR encode(fingerprint, 'hex') ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR array_to_string(dns_names, ',') ILIKE ");
        builder.push_bind(pat);
        builder.push(")");
    }
    for f in filters {
        push_cert_filter(builder, f)?;
    }
    Ok(())
}

fn push_cert_filter(
    builder: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>,
    f: &CertListFilter,
) -> CertResult<()> {
    let op = f.op.as_str();
    match f.field.as_str() {
        "subject" => push_str_filter(builder, "subject", op, f.value.as_ref())?,
        "fingerprint" => {
            push_str_filter(builder, "encode(fingerprint, 'hex')", op, f.value.as_ref())?;
        }
        "dns_names" => {
            push_str_filter(
                builder,
                "array_to_string(dns_names, ',')",
                op,
                f.value.as_ref(),
            )?;
        }
        "expires_at" => push_datetime_filter(builder, "expires_at", op, f.value.as_ref())?,
        "status" => push_str_filter(builder, CERT_STATUS_SQL, op, f.value.as_ref())?,
        other => {
            return Err(CertError::InvalidRequest(format!(
                "Unknown filter field: {other}"
            )));
        }
    }
    Ok(())
}

fn filter_string(value: Option<&serde_json::Value>) -> CertResult<String> {
    match value {
        Some(serde_json::Value::String(s)) if !s.is_empty() => Ok(s.clone()),
        Some(serde_json::Value::Number(n)) => Ok(n.to_string()),
        Some(serde_json::Value::Bool(b)) => Ok(b.to_string()),
        _ => Err(CertError::InvalidRequest(
            "filter value required (string)".into(),
        )),
    }
}

fn push_str_filter(
    builder: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>,
    expr: &str,
    op: &str,
    value: Option<&serde_json::Value>,
) -> CertResult<()> {
    match op {
        "eq" | "neq" => {
            let s = filter_string(value)?;
            builder.push(" AND ");
            builder.push(expr);
            builder.push(if op == "eq" { " = " } else { " != " });
            builder.push_bind(s);
        }
        "contains" => {
            let s = filter_string(value)?;
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" ILIKE ");
            builder.push_bind(format!("%{s}%"));
        }
        "starts_with" => {
            let s = filter_string(value)?;
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" ILIKE ");
            builder.push_bind(format!("{s}%"));
        }
        "ends_with" => {
            let s = filter_string(value)?;
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" ILIKE ");
            builder.push_bind(format!("%{s}"));
        }
        "in" => {
            let items = match value {
                Some(serde_json::Value::Array(a)) => a
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect::<Vec<_>>(),
                Some(serde_json::Value::String(s)) => s
                    .split(',')
                    .map(str::trim)
                    .filter(|x| !x.is_empty())
                    .map(str::to_string)
                    .collect(),
                _ => {
                    return Err(CertError::InvalidRequest(
                        "in requires array or comma-separated string".into(),
                    ));
                }
            };
            if items.is_empty() {
                return Err(CertError::InvalidRequest("in requires values".into()));
            }
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" IN (");
            let mut first = true;
            for item in items {
                if !first {
                    builder.push(", ");
                }
                first = false;
                builder.push_bind(item);
            }
            builder.push(")");
        }
        other => {
            return Err(CertError::InvalidRequest(format!(
                "Operator {other} not allowed for string field"
            )));
        }
    }
    Ok(())
}

fn parse_dt(raw: &str) -> Option<DateTime<Utc>> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(n) = s.parse::<i64>() {
        let secs = if n.abs() > 1_000_000_000_000 {
            n / 1000
        } else {
            n
        };
        return DateTime::from_timestamp(secs, 0);
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
        if let Ok(date) = NaiveDate::parse_from_str(s, fmt) {
            if let Some(naive) = date.and_hms_opt(0, 0, 0) {
                return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
            }
        }
    }
    None
}

fn push_datetime_filter(
    builder: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>,
    expr: &str,
    op: &str,
    value: Option<&serde_json::Value>,
) -> CertResult<()> {
    match op {
        "eq" => {
            let s = filter_string(value)?;
            let start = parse_dt(&s)
                .ok_or_else(|| CertError::InvalidRequest(format!("invalid datetime: {s}")))?;
            let end = start + Duration::milliseconds(1);
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" >= ");
            builder.push_bind(start);
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" < ");
            builder.push_bind(end);
        }
        "gte" | "gt" | "lte" | "lt" => {
            let s = filter_string(value)?;
            let t = parse_dt(&s)
                .ok_or_else(|| CertError::InvalidRequest(format!("invalid datetime: {s}")))?;
            let sql_op = match op {
                "gte" => " >= ",
                "gt" => " > ",
                "lte" => " <= ",
                "lt" => " < ",
                _ => unreachable!(),
            };
            builder.push(" AND ");
            builder.push(expr);
            builder.push(sql_op);
            builder.push_bind(t);
        }
        "between" => {
            let (a, b) = match value {
                Some(serde_json::Value::Array(arr)) if arr.len() == 2 => {
                    let a = arr[0]
                        .as_str()
                        .ok_or_else(|| CertError::InvalidRequest("between needs strings".into()))?;
                    let b = arr[1]
                        .as_str()
                        .ok_or_else(|| CertError::InvalidRequest("between needs strings".into()))?;
                    (a.to_string(), b.to_string())
                }
                _ => {
                    return Err(CertError::InvalidRequest(
                        "between requires 2 datetime values".into(),
                    ));
                }
            };
            let mut ta = parse_dt(&a)
                .ok_or_else(|| CertError::InvalidRequest(format!("invalid datetime: {a}")))?;
            let mut tb = parse_dt(&b)
                .ok_or_else(|| CertError::InvalidRequest(format!("invalid datetime: {b}")))?;
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" >= ");
            builder.push_bind(ta);
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" <= ");
            builder.push_bind(tb);
        }
        other => {
            return Err(CertError::InvalidRequest(format!(
                "Operator {other} not allowed for datetime field"
            )));
        }
    }
    Ok(())
}
