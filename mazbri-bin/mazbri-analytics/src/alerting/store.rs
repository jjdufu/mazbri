//! 告警规则与事件的 PostgreSQL（`mazbri_admin` 库）读写。
use super::rule::{ActiveEvent, AlertRule, CompareOp, NotifyConfig, ResolvedEvent, RuleKind};
use chrono::{DateTime, Utc};
use mazbri_core::sqlx::{self, PgPool, Postgres, Row, pool::PoolConnection, postgres::PgRow};
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

/// 告警评估轮次的 advisory lock 键（"mzbralrt" 的 ASCII 字节），所有实例共用。
pub const EVALUATION_LOCK_KEY: i64 = 0x6D7A_6272_616C_7274;

/// `insert_firing` 参数包，避免参数过多。
pub struct FireInsert<'a> {
    pub rule: &'a AlertRule,
    pub fingerprint: &'a str,
    pub labels: &'a BTreeMap<String, String>,
    pub value: f64,
    pub started_at: DateTime<Utc>,
    pub trace_ids: &'a [String],
    pub silenced: bool,
    pub inhibited: bool,
}

/// 告警存储。
#[derive(Clone)]
pub struct AlertStore {
    pool: Arc<PgPool>,
}

impl AlertStore {
    #[must_use]
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// 取一条连接并尝试获取评估锁（`pg_try_advisory_lock`，不阻塞）。
    ///
    /// 返回 `None` 表示其他实例正在评估本轮。本轮全部读写都在持锁的这一条
    /// 连接上执行，不额外占用连接。
    ///
    /// # Errors
    ///
    /// 连接或查询失败时返回错误。
    pub async fn try_begin_evaluation(&self) -> Result<Option<EvaluationSession>, sqlx::Error> {
        EvaluationSession::try_acquire(self.pool.acquire().await?).await
    }
}

/// 持有评估锁的单条连接。
///
/// - 正常结束调用 [`EvaluationSession::release`] 解锁并归还连接；
/// - 未解锁就被丢弃（`?` 提前返回、panic）时关闭该连接，PostgreSQL 随会话结束释放锁，
///   不会把带锁连接还回连接池；
/// - 实例崩溃或网络断开同样由会话结束释放锁，其他实例下一轮接手。
pub struct EvaluationSession {
    conn: PoolConnection<Postgres>,
    released: bool,
}

impl Drop for EvaluationSession {
    fn drop(&mut self) {
        if !self.released {
            self.conn.close_on_drop();
        }
    }
}

impl EvaluationSession {
    async fn try_acquire(mut conn: PoolConnection<Postgres>) -> Result<Option<Self>, sqlx::Error> {
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(EVALUATION_LOCK_KEY)
            .fetch_one(&mut *conn)
            .await?;
        if !acquired {
            // 未抢到锁：立刻归还连接，避免空占池槽（与 mgmt 共用 mazbri_admin 时尤其重要）。
            drop(conn);
            return Ok(None);
        }
        Ok(Some(Self {
            conn,
            released: false,
        }))
    }

    /// 解锁并归还连接；解锁失败时关闭连接以确保锁释放。
    pub async fn release(mut self) {
        let unlocked: Result<bool, sqlx::Error> =
            sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
                .bind(EVALUATION_LOCK_KEY)
                .fetch_one(&mut *self.conn)
                .await;
        match unlocked {
            Ok(true) => self.released = true,
            Ok(false) => {
                tracing::warn!(target: "alerting", "评估锁已不在本会话持有，关闭连接");
            }
            Err(error) => {
                tracing::warn!(target: "alerting", %error, "释放评估锁失败，关闭连接");
            }
        }
    }

    /// 加载全部规则；无法解析的行记录告警日志后跳过。
    ///
    /// # Errors
    ///
    /// 数据库查询失败时返回错误。
    pub async fn load_rules(&mut self) -> Result<Vec<AlertRule>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, name, kind, expr, threshold, op, for_seconds, severity, labels, notify, enabled \
             FROM alert_rules ORDER BY name",
        )
        .fetch_all(&mut *self.conn)
        .await?;
        let mut rules = Vec::with_capacity(rows.len());
        for row in &rows {
            match parse_rule(row) {
                Ok(rule) => rules.push(rule),
                Err(reason) => {
                    tracing::warn!(target: "alerting", %reason, "跳过无法解析的告警规则");
                }
            }
        }
        Ok(rules)
    }

    /// 加载全部活跃（firing / acked）事件。
    ///
    /// # Errors
    ///
    /// 数据库查询失败时返回错误。
    pub async fn active_events(&mut self) -> Result<Vec<ActiveEvent>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, rule_id, fingerprint, value FROM alert_events \
             WHERE status IN ('firing', 'acked')",
        )
        .fetch_all(&mut *self.conn)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(ActiveEvent {
                    id: row.try_get("id")?,
                    rule_id: row.try_get("rule_id")?,
                    fingerprint: row.try_get("fingerprint")?,
                    value: row.try_get("value")?,
                })
            })
            .collect()
    }

    /// 写入 firing 事件；同指纹已有活跃事件时不写入并返回 `None`。
    ///
    /// # Errors
    ///
    /// 数据库写入失败时返回错误。
    pub async fn insert_firing(
        &mut self,
        params: FireInsert<'_>,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let labels = serde_json::to_value(params.labels).unwrap_or_else(|_| serde_json::json!({}));
        let last_notified = if params.silenced || params.inhibited {
            None
        } else {
            Some(Utc::now())
        };
        let row = sqlx::query(
            "INSERT INTO alert_events (rule_id, fingerprint, status, severity, labels, value, started_at, trace_ids, silenced, inhibited, escalation_level, last_notified_at)              VALUES ($1, $2, 'firing', $3, $4, $5, $6, $7, $8, $9, 0, $10)              ON CONFLICT (fingerprint) WHERE status IN ('firing', 'acked') DO NOTHING              RETURNING id",
        )
        .bind(params.rule.id)
        .bind(params.fingerprint)
        .bind(&params.rule.severity)
        .bind(labels)
        .bind(params.value)
        .bind(params.started_at)
        .bind(params.trace_ids)
        .bind(params.silenced)
        .bind(params.inhibited)
        .bind(last_notified)
        .fetch_optional(&mut *self.conn)
        .await?;
        row.map(|r| r.try_get("id")).transpose()
    }
    /// 刷新活跃事件的当前值。
    ///
    /// # Errors
    ///
    /// 数据库写入失败时返回错误。
    pub async fn refresh_value(&mut self, event_id: Uuid, value: f64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE alert_events SET value = $2, updated_at = now() \
             WHERE id = $1 AND status IN ('firing', 'acked')",
        )
        .bind(event_id)
        .bind(value)
        .execute(&mut *self.conn)
        .await?;
        Ok(())
    }

    /// 将活跃事件置为 resolved；事件已不处于活跃状态时返回 `None`。
    ///
    /// # Errors
    ///
    /// 数据库写入失败时返回错误。
    pub async fn resolve(&mut self, event_id: Uuid) -> Result<Option<ResolvedEvent>, sqlx::Error> {
        let row = sqlx::query(
            "UPDATE alert_events SET status = 'resolved', resolved_at = now(), updated_at = now() \
             WHERE id = $1 AND status IN ('firing', 'acked') \
             RETURNING id, rule_id, labels, value, started_at, resolved_at",
        )
        .bind(event_id)
        .fetch_optional(&mut *self.conn)
        .await?;
        row.map(|row| {
            let labels: serde_json::Value = row.try_get("labels")?;
            Ok(ResolvedEvent {
                id: row.try_get("id")?,
                rule_id: row.try_get("rule_id")?,
                labels: string_map(&labels),
                value: row.try_get("value")?,
                started_at: row.try_get("started_at")?,
                resolved_at: row
                    .try_get::<Option<DateTime<Utc>>, _>("resolved_at")?
                    .unwrap_or_else(Utc::now),
            })
        })
        .transpose()
    }

    /// 加载当前有效的静默窗（`ends_at` 晚于现在）。
    ///
    /// # Errors
    ///
    /// 数据库查询失败时返回错误。
    pub async fn load_silences(&mut self) -> Result<Vec<crate::alerting::filter::Silence>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT matchers, starts_at, ends_at FROM alert_silences WHERE ends_at > now()",
        )
        .fetch_all(&mut *self.conn)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(crate::alerting::filter::Silence {
                    matchers: row.try_get("matchers")?,
                    starts_at: row.try_get("starts_at")?,
                    ends_at: row.try_get("ends_at")?,
                })
            })
            .collect()
    }

    /// 加载启用的抑制规则。
    ///
    /// # Errors
    ///
    /// 数据库查询失败时返回错误。
    pub async fn load_inhibit_rules(
        &mut self,
    ) -> Result<Vec<crate::alerting::filter::InhibitRule>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT source_matchers, target_matchers, equal_labels FROM alert_inhibit_rules WHERE enabled",
        )
        .fetch_all(&mut *self.conn)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(crate::alerting::filter::InhibitRule {
                    source_matchers: row.try_get("source_matchers")?,
                    target_matchers: row.try_get("target_matchers")?,
                    equal_labels: row.try_get("equal_labels")?,
                })
            })
            .collect()
    }

    /// 加载全部活跃事件的标签（用于抑制源匹配）。
    ///
    /// # Errors
    ///
    /// 数据库查询失败时返回错误。
    pub async fn active_event_labels(
        &mut self,
    ) -> Result<Vec<crate::alerting::filter::ActiveLabels>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT labels FROM alert_events WHERE status IN ('firing', 'acked')",
        )
        .fetch_all(&mut *self.conn)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let labels: serde_json::Value = row.try_get("labels")?;
            out.push(crate::alerting::filter::ActiveLabels {
                labels: string_map(&labels),
            });
        }
        Ok(out)
    }

    /// 加载启用的值班升级阶梯（按名称取首张启用表）。
    ///
    /// # Errors
    ///
    /// 数据库查询失败时返回错误。
    pub async fn load_primary_escalation(
        &mut self,
    ) -> Result<Vec<crate::alerting::filter::EscalationStep>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT escalation FROM oncall_schedules WHERE enabled ORDER BY name LIMIT 1",
        )
        .fetch_optional(&mut *self.conn)
        .await?;
        let Some(row) = row else {
            return Ok(Vec::new());
        };
        let escalation: serde_json::Value = row.try_get("escalation")?;
        Ok(parse_escalation(&escalation))
    }

    /// 加载启用的通知渠道摘要。
    ///
    /// # Errors
    ///
    /// 数据库查询失败时返回错误。
    pub async fn load_notify_channels(&mut self) -> Result<Vec<NotifyChannelRow>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, name, type, config, (secret_enc IS NOT NULL) AS has_secret              FROM notify_channels WHERE enabled",
        )
        .fetch_all(&mut *self.conn)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            out.push(NotifyChannelRow {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                channel_type: row.try_get("type")?,
                config: row.try_get("config")?,
                has_secret: row.try_get("has_secret")?,
            });
        }
        Ok(out)
    }

    /// 标记事件为已抑制（不外发通知）。
    ///
    /// # Errors
    ///
    /// 数据库写入失败时返回错误。
    pub async fn set_inhibited(&mut self, event_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE alert_events SET inhibited = TRUE, updated_at = now() WHERE id = $1",
        )
        .bind(event_id)
        .execute(&mut *self.conn)
        .await?;
        Ok(())
    }

    /// 标记已通知并推进升级层级。
    ///
    /// # Errors
    ///
    /// 数据库写入失败时返回错误。
    pub async fn mark_notified(
        &mut self,
        event_id: Uuid,
        escalation_level: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE alert_events SET escalation_level = $2, last_notified_at = now(), updated_at = now()              WHERE id = $1",
        )
        .bind(event_id)
        .bind(escalation_level)
        .execute(&mut *self.conn)
        .await?;
        Ok(())
    }

    /// 待升级的未确认 firing 事件。
    ///
    /// # Errors
    ///
    /// 数据库查询失败时返回错误。
    pub async fn unacked_firing_for_escalation(
        &mut self,
    ) -> Result<Vec<EscalationCandidate>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, rule_id, labels, value, started_at, escalation_level              FROM alert_events WHERE status = 'firing' AND NOT silenced AND NOT inhibited",
        )
        .fetch_all(&mut *self.conn)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let labels: serde_json::Value = row.try_get("labels")?;
            out.push(EscalationCandidate {
                id: row.try_get("id")?,
                rule_id: row.try_get("rule_id")?,
                labels: string_map(&labels),
                value: row.try_get("value")?,
                started_at: row.try_get("started_at")?,
                escalation_level: row.try_get("escalation_level")?,
            });
        }
        Ok(out)
    }
}

/// 通知渠道行（analytics 侧只读公开配置）。
#[derive(Debug, Clone)]
pub struct NotifyChannelRow {
    pub id: Uuid,
    pub name: String,
    pub channel_type: String,
    pub config: serde_json::Value,
    pub has_secret: bool,
}

/// 升级候选事件。
#[derive(Debug, Clone)]
pub struct EscalationCandidate {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub labels: BTreeMap<String, String>,
    pub value: f64,
    pub started_at: DateTime<Utc>,
    pub escalation_level: i32,
}

fn parse_escalation(value: &serde_json::Value) -> Vec<crate::alerting::filter::EscalationStep> {
    let Some(arr) = value.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let after = item.get("after_minutes")?.as_i64()?;
            let users = item
                .get("users")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let channel_ids = item
                .get("channel_ids")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Some(crate::alerting::filter::EscalationStep {
                after_minutes: after,
                users,
                channel_ids,
            })
        })
        .collect()
}

fn parse_rule(row: &PgRow) -> Result<AlertRule, String> {
    let get_err = |e: sqlx::Error| e.to_string();
    let id: Uuid = row.try_get("id").map_err(get_err)?;
    let kind_raw: String = row.try_get("kind").map_err(get_err)?;
    let op_raw: String = row.try_get("op").map_err(get_err)?;
    let labels: serde_json::Value = row.try_get("labels").map_err(get_err)?;
    let notify: serde_json::Value = row.try_get("notify").map_err(get_err)?;
    let for_seconds: i32 = row.try_get("for_seconds").map_err(get_err)?;
    Ok(AlertRule {
        id,
        name: row.try_get("name").map_err(get_err)?,
        kind: RuleKind::parse(&kind_raw)
            .ok_or_else(|| format!("规则 {id} kind 非法: {kind_raw}"))?,
        expr: row.try_get("expr").map_err(get_err)?,
        threshold: row.try_get("threshold").map_err(get_err)?,
        op: CompareOp::parse(&op_raw).ok_or_else(|| format!("规则 {id} op 非法: {op_raw}"))?,
        for_seconds: i64::from(for_seconds),
        severity: row.try_get("severity").map_err(get_err)?,
        labels: string_map(&labels),
        notify: serde_json::from_value::<NotifyConfig>(notify)
            .map_err(|e| format!("规则 {id} notify 非法: {e}"))?,
        enabled: row.try_get("enabled").map_err(get_err)?,
    })
}

/// 将 JSON 对象中的字符串值收集为有序标签表（非字符串值忽略）。
fn string_map(value: &serde_json::Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .map(|object| {
            object
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{EVALUATION_LOCK_KEY, EvaluationSession};
    use mazbri_core::sqlx::{self, Connection, PgConnection, PgPool, postgres::PgPoolOptions};

    #[test]
    fn lock_key_is_stable() {
        assert_eq!(
            EVALUATION_LOCK_KEY.to_be_bytes(),
            *b"mzbralrt",
            "多实例必须使用同一个锁键"
        );
    }

    fn url() -> String {
        std::env::var("MAZBRI_ALERTING_TEST_DATABASE_URL")
            .expect("MAZBRI_ALERTING_TEST_DATABASE_URL must point to a PostgreSQL database")
    }

    async fn pool() -> PgPool {
        PgPoolOptions::new()
            .max_connections(1)
            .connect(&url())
            .await
            .expect("connect")
    }

    async fn try_lock(pool: &PgPool) -> Option<EvaluationSession> {
        let conn = pool.acquire().await.expect("acquire");
        EvaluationSession::try_acquire(conn).await.expect("query")
    }

    async fn eventually_free(pool: &PgPool) -> bool {
        for _ in 0..50 {
            if let Some(session) = try_lock(pool).await {
                session.release().await;
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        false
    }

    /// 运行：`MAZBRI_ALERTING_TEST_DATABASE_URL=postgres://... cargo test -p mazbri-analytics -- --ignored alerting_pg`
    /// 每个池只用 1 条连接，可在连接数受限的角色上运行。
    #[tokio::test]
    #[ignore = "requires PostgreSQL via MAZBRI_ALERTING_TEST_DATABASE_URL"]
    async fn alerting_pg_evaluation_lock_is_exclusive_and_released() {
        let first = pool().await;
        let second = pool().await;

        // 互斥：持锁期间另一实例拿不到。
        let held = try_lock(&first).await.expect("first acquires");
        assert!(
            try_lock(&second).await.is_none(),
            "持锁期间其他实例拿不到锁"
        );

        // 正常解锁后连接归还池，且不再带锁。
        held.release().await;
        assert!(eventually_free(&second).await, "release 后锁释放");
        let reused = try_lock(&first).await.expect("归还的连接可再次加锁");
        reused.release().await;

        // 未解锁即丢弃：连接被关闭，锁随会话结束释放。
        let dropped = try_lock(&first).await.expect("acquire");
        drop(dropped);
        assert!(eventually_free(&second).await, "丢弃会话后锁释放");

        // 持锁连接直接断开（模拟实例崩溃）同样释放锁。
        drop(first);
        drop(second);
        let observer = pool().await;
        let mut raw = PgConnection::connect(&url()).await.expect("connect");
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(EVALUATION_LOCK_KEY)
            .fetch_one(&mut raw)
            .await
            .expect("lock");
        assert!(acquired);
        assert!(try_lock(&observer).await.is_none());
        drop(raw);
        assert!(eventually_free(&observer).await, "持锁连接断开后锁释放");
    }
}
