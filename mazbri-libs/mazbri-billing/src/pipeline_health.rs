//! 计费链路健康自查：定时按 `status`（`idx_*_pending` 前缀）查 PG，刷新积压、最老记录年龄与死信 gauge。

use crate::metrics::{
    BILLING_DEAD_LETTERS, BILLING_OLDEST_PENDING_AGE_SECONDS, BILLING_OUTBOX_PENDING,
};
use serde::Deserialize;
use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{info, warn};

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PipelineHealthConfig {
    pub enabled: bool,
    pub interval_secs: u64,
}

impl Default for PipelineHealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 15,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Outbox,
    Queue,
}

impl Stage {
    const ALL: [Self; 2] = [Self::Outbox, Self::Queue];

    const fn label(self) -> &'static str {
        match self {
            Self::Outbox => "outbox",
            Self::Queue => "queue",
        }
    }

    const fn sql(self) -> &'static str {
        match self {
            Self::Outbox => STAGE_SQL_OUTBOX,
            Self::Queue => STAGE_SQL_QUEUE,
        }
    }
}

const STAGE_SQL_OUTBOX: &str = "SELECT \
     count(*) FILTER (WHERE status IN ('pending', 'processing', 'failed')) AS pending, \
     COALESCE(EXTRACT(EPOCH FROM now() - min(created_at) \
         FILTER (WHERE status IN ('pending', 'processing', 'failed')))::bigint, 0) AS oldest_age, \
     count(*) FILTER (WHERE status = 'dead_letter') AS dead_letters \
     FROM usage_outbox WHERE status IN ('pending', 'processing', 'failed', 'dead_letter')";

const STAGE_SQL_QUEUE: &str = "SELECT \
     count(*) FILTER (WHERE status IN ('pending', 'processing', 'failed')) AS pending, \
     COALESCE(EXTRACT(EPOCH FROM now() - min(created_at) \
         FILTER (WHERE status IN ('pending', 'processing', 'failed')))::bigint, 0) AS oldest_age, \
     count(*) FILTER (WHERE status = 'dead_letter') AS dead_letters \
     FROM usage_charge_queue WHERE status IN ('pending', 'processing', 'failed', 'dead_letter')";

#[derive(Debug, sqlx::FromRow)]
struct StageSnapshot {
    pending: i64,
    oldest_age: i64,
    dead_letters: i64,
}

pub struct PipelineHealthJob {
    cfg: PipelineHealthConfig,
    db: PgPool,
}

impl PipelineHealthJob {
    #[must_use]
    pub const fn new(cfg: PipelineHealthConfig, db: PgPool) -> Self {
        Self { cfg, db }
    }

    #[must_use]
    pub fn spawn(self, mut shutdown: watch::Receiver<bool>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            info!(
                interval_secs = self.cfg.interval_secs,
                "billing pipeline health job started"
            );
            loop {
                self.tick().await;
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_secs(self.cfg.interval_secs.max(1))) => {}
                    _ = shutdown.changed() => break,
                }
            }
        })
    }

    async fn tick(&self) {
        for stage in Stage::ALL {
            match sqlx::query_as::<_, StageSnapshot>(stage.sql())
                .fetch_one(&self.db)
                .await
            {
                Ok(snap) => {
                    let label = [stage.label()];
                    BILLING_OUTBOX_PENDING
                        .with_label_values(&label)
                        .set(snap.pending);
                    BILLING_OLDEST_PENDING_AGE_SECONDS
                        .with_label_values(&label)
                        .set(snap.oldest_age.max(0));
                    BILLING_DEAD_LETTERS
                        .with_label_values(&label)
                        .set(snap.dead_letters);
                }
                Err(e) => {
                    warn!(stage = stage.label(), error = %e, "billing pipeline health query failed");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_queries_are_status_bounded() {
        for stage in Stage::ALL {
            let sql = stage.sql();
            assert!(
                sql.contains("WHERE status IN ('pending', 'processing', 'failed', 'dead_letter')")
            );
        }
        assert!(STAGE_SQL_OUTBOX.contains("FROM usage_outbox "));
        assert!(STAGE_SQL_QUEUE.contains("FROM usage_charge_queue "));
    }
}
