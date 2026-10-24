//! 当前未编译、未生效（未声明 mod）。自动续期为规划功能。约束：续期时按证书 subject（规范服务名）用 SanPolicy 重算 SAN；不查预置、不沿用旧 dns_names。
use crate::ca::CaManager;
use crate::san::SanPolicy;
use crate::db::{CertificateRecord, CertificateRepository};
use chrono::{DateTime, Duration, Utc};
use mazbri_tls::CertMetricsRecorder;
use sqlx::FromRow;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::time;
use tracing::{error, info, warn};
use uuid::Uuid;
const AUTO_RENEWAL_LOCK_ID: i64 = 0x5A7B8C9D_0E1F2A3B;
#[derive(Debug, Clone)]
pub struct TaskConfig {
    pub check_interval_secs: u64,
}
impl Default for TaskConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 3600,
        }
    }
}
#[derive(Debug, Clone)]
pub struct RenewalConfig {
    pub threshold_days: i32,
    pub validity_days: i32,
}
pub struct CertificateDueForRenewal {
    pub certificate: CertificateRecord,
    pub config: RenewalConfig,
}
pub fn spawn_auto_renewal_task(
    repo: Arc<CertificateRepository>,
    ca_manager: Arc<CaManager>,
    config: TaskConfig,
) -> (tokio::task::JoinHandle<()>, oneshot::Sender<()>) {
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let handle = tokio::spawn(async move {
        let mut interval = time::interval(time::Duration::from_secs(config.check_interval_secs));
        interval.tick().await;
        info!(
            interval_secs = config.check_interval_secs,
            lock_id = AUTO_RENEWAL_LOCK_ID,
            "Auto-renewal background task started with transaction-level advisory lock / \
             自动续期后台任务已启动（事务级 advisory lock）"
        );
        loop {
            tokio::select! {
                _ = & mut shutdown_rx => {
                info!("Auto-renewal task received shutdown signal"); break; } _ =
                interval.tick() => { if let Err(e) = check_and_renew_in_transaction(&
                repo, & ca_manager). await { error!(error = % e,
                "自动续期任务失败"); } }
            }
        }
        info!("Auto-renewal task stopped");
    });
    (handle, shutdown_tx)
}
async fn check_and_renew_in_transaction(
    repo: &CertificateRepository,
    ca_manager: &CaManager,
    san_policy: &SanPolicy,
) -> anyhow::Result<()> {
    let mut tx = repo.get_pool().begin().await?;
    let _lock: i64 = sqlx::query_scalar("SELECT pg_advisory_xact_lock($1)")
        .bind(AUTO_RENEWAL_LOCK_ID)
        .fetch_one(&mut *tx)
        .await?;
    info!("已获取事务级自动续期分布式锁");
    let certificates = get_certificates_due_for_renewal_in_tx(&mut tx).await?;
    if certificates.is_empty() {
        info!("没有需要续期的证书");
        tx.commit().await?;
        return Ok(());
    }
    info!(count = certificates.len(), "处理需要续期的证书");
    for item in certificates {
        let metrics = CertMetricsRecorder::new("auto_renew");
        let _timer = metrics.start_timer();
        let cert = &item.certificate;
        let config = &item.config;
        let dns_names = match san_policy.sans_for_subject(&cert.subject) {
            Ok(v) => v,
            Err(e) => {
                metrics.record_renewal_failed();
                warn!(subject = %cert.subject, error = %e, "续期跳过：subject 非规范服务名或不需要 mTLS");
                continue;
            }
        };
        let (new_cert_pem, new_key_encrypted) =
            match ca_manager.issue_certificate(&cert.subject, &dns_names) {
                Ok(issued) => issued,
                Err(error) => {
                    metrics.record_renewal_failed();
                    return Err(error.into());
                }
            };
        let new_expires_at = Utc::now() + Duration::days(i64::from(config.validity_days));
        sqlx::query(
            r#"UPDATE certificates
               SET cert_pem = $2, key_pem = $3, expires_at = $4, dns_names = $5,
                   renewal_count = renewal_count + 1, updated_at = NOW()
               WHERE id = $1 AND revoked = false"#,
        )
        .bind(cert.id)
        .bind(&new_cert_pem)
        .bind(&new_key_encrypted)
        .bind(new_expires_at)
        .bind(&dns_names)
        .execute(&mut *tx)
        .await?;
        let now = Utc::now();
        let next_renewal = new_expires_at - Duration::days(i64::from(config.threshold_days));
        sqlx::query(
            r#"UPDATE auto_renewal_config
               SET last_renewal_at = $2, next_renewal_at = $3, updated_at = NOW()
               WHERE certificate_id = $1"#,
        )
        .bind(cert.id)
        .bind(now)
        .bind(next_renewal)
        .execute(&mut *tx)
        .await?;
        info!(
            subject = % cert.subject, cert_id = % cert.id, new_expires_at = %
            new_expires_at, "证书已自动续期"
        );
        metrics.record_renewal_success();
    }
    tx.commit().await?;
    Ok(())
}
async fn get_certificates_due_for_renewal_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> anyhow::Result<Vec<CertificateDueForRenewal>> {
    #[derive(FromRow)]
    struct CertificateWithConfig {
        id: Uuid,
        subject: String,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        revoked: bool,
        revoked_at: Option<DateTime<Utc>>,
        cert_pem: String,
        key_pem: String,
        dns_names: Vec<String>,
        created_by_ip: Option<String>,
        renewal_count: i32,
        fingerprint: Option<Vec<u8>>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        config_threshold_days: i32,
        config_validity_days: i32,
    }
    let records = sqlx::query_as::<_, CertificateWithConfig>(
        r#"
        SELECT
            c.id, c.subject, c.issued_at, c.expires_at, c.revoked, c.revoked_at,
            c.cert_pem, c.key_pem, c.dns_names, c.created_by_ip,
            c.renewal_count, c.fingerprint, c.created_at, c.updated_at,
            arc.threshold_days as config_threshold_days,
            arc.validity_days as config_validity_days
        FROM certificates c
        INNER JOIN auto_renewal_config arc ON c.id = arc.certificate_id
        WHERE arc.enabled = true
        AND c.revoked = false
        AND c.expires_at > NOW()
        AND c.expires_at <= NOW() + INTERVAL '1 day' * arc.threshold_days
        "#,
    )
    .fetch_all(tx.as_mut())
    .await?;
    let result: Vec<CertificateDueForRenewal> = records
        .into_iter()
        .map(|r| CertificateDueForRenewal {
            certificate: CertificateRecord {
                id: r.id,
                subject: r.subject,
                issued_at: r.issued_at,
                expires_at: r.expires_at,
                revoked: r.revoked,
                revoked_at: r.revoked_at,
                cert_pem: r.cert_pem,
                key_pem: r.key_pem,
                dns_names: r.dns_names,
                created_by_ip: r
                    .created_by_ip
                    .and_then(|ip| ip.parse::<std::net::IpAddr>().ok()),
                renewal_count: r.renewal_count,
                fingerprint: r.fingerprint,
                created_at: r.created_at,
                updated_at: r.updated_at,
            },
            config: RenewalConfig {
                threshold_days: r.config_threshold_days,
                validity_days: r.config_validity_days,
            },
        })
        .collect();
    Ok(result)
}
