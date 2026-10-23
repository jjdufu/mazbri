use super::{DeductAndRecordResult, PgBillingRepository};
use crate::service::AddFundsResult;
use chrono::{DateTime, Utc};
use mazbri_core::CoreError;
use rust_decimal::Decimal;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
struct LedgerEntry<'a> {
    tenant_id: Uuid,
    event_id: Option<Uuid>,
    entry_type: &'a str,
    amount: Decimal,
    direction: &'a str,
    balance_before: Decimal,
    balance_after: Decimal,
    description: Option<&'a str>,
}
async fn insert_ledger_entry(
    tx: &mut Transaction<'_, Postgres>,
    entry: LedgerEntry<'_>,
) -> Result<(), CoreError> {
    sqlx::query(
            r"
        INSERT INTO mazbri_billing.ledger_entries 
        (tenant_id, interaction_event_id, entry_type, amount, direction, balance_before, balance_after, description)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ",
        )
        .bind(entry.tenant_id)
        .bind(entry.event_id)
        .bind(entry.entry_type)
        .bind(entry.amount)
        .bind(entry.direction)
        .bind(entry.balance_before)
        .bind(entry.balance_after)
        .bind(entry.description)
        .execute(&mut **tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    Ok(())
}
pub(crate) async fn deduct_balance(
    repo: &PgBillingRepository,
    tenant_id: Uuid,
    amount: Decimal,
) -> Result<Decimal, CoreError> {
    if amount <= Decimal::ZERO {
        return Ok(Decimal::ZERO);
    }
    let mut tx = repo
        .pool
        .begin()
        .await
        .map_err(PgBillingRepository::map_error)?;
    let account_id: Uuid =
        sqlx::query_scalar("SELECT id FROM tenant_accounts WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(PgBillingRepository::map_error)?
            .ok_or_else(|| CoreError::NotFound {
                resource: "tenant_account".to_string(),
                id: tenant_id.to_string(),
            })?;
    let new_balance: Decimal = sqlx::query_scalar(
        "UPDATE tenant_accounts
         SET balance = balance - $1
         WHERE tenant_id = $2 AND balance + credit_limit >= $1
         RETURNING balance",
    )
    .bind(amount)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?
    .ok_or_else(|| {
        CoreError::Validation(format!(
            "Insufficient balance: required {amount}, tenant {tenant_id}"
        ))
    })?;
    let transaction_id = Uuid::new_v4();
    let balance_before = new_balance + amount;
    sqlx::query(
        r"INSERT INTO transactions
           (id, tenant_id, account_id, transaction_type, amount,
            balance_before, balance_after, status)
           VALUES ($1, $2, $3, 'charge', $4, $5, $6, 'completed')",
    )
    .bind(transaction_id)
    .bind(tenant_id)
    .bind(account_id)
    .bind(amount)
    .bind(balance_before)
    .bind(new_balance)
    .execute(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    insert_ledger_entry(
        &mut tx,
        LedgerEntry {
            tenant_id,
            event_id: None,
            entry_type: "CHARGE",
            amount,
            direction: "DEBIT",
            balance_before,
            balance_after: new_balance,
            description: Some("Direct deduction"),
        },
    )
    .await?;
    tx.commit().await.map_err(PgBillingRepository::map_error)?;
    Ok(new_balance)
}
pub(crate) async fn add_funds(
    repo: &PgBillingRepository,
    tenant_id: Uuid,
    amount: Decimal,
    payment_method: &str,
) -> Result<AddFundsResult, CoreError> {
    if amount <= Decimal::ZERO {
        return Err(CoreError::Validation("Amount must be positive".to_string()));
    }
    let mut tx = repo
        .pool
        .begin()
        .await
        .map_err(PgBillingRepository::map_error)?;
    let row = sqlx::query(
        "UPDATE tenant_accounts
         SET balance = balance + $1, updated_at = NOW()
         WHERE tenant_id = $2
         RETURNING id, balance",
    )
    .bind(amount)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?
    .ok_or_else(|| CoreError::NotFound {
        resource: "tenant_account".to_string(),
        id: tenant_id.to_string(),
    })?;
    let account_id: Uuid = row.get("id");
    let new_balance: Decimal = row.get("balance");
    let transaction_id = Uuid::new_v4();
    let now = Utc::now();
    let balance_before = new_balance - amount;
    sqlx::query(
            "INSERT INTO transactions (id, tenant_id, account_id, transaction_type, amount, balance_before, balance_after, payment_method, status)
         VALUES ($1, $2, $3, 'credit', $4, $5, $6, $7, 'completed')",
        )
        .bind(transaction_id)
        .bind(tenant_id)
        .bind(account_id)
        .bind(amount)
        .bind(balance_before)
        .bind(new_balance)
        .bind(payment_method)
        .execute(&mut *tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    insert_ledger_entry(
        &mut tx,
        LedgerEntry {
            tenant_id,
            event_id: None,
            entry_type: "RECHARGE",
            amount,
            direction: "CREDIT",
            balance_before,
            balance_after: new_balance,
            description: Some(payment_method),
        },
    )
    .await?;
    tx.commit().await.map_err(PgBillingRepository::map_error)?;
    Ok(AddFundsResult {
        transaction_id,
        new_balance,
        processed_at: now.timestamp(),
    })
}
/// 统计账期内的成功调用数、数据量与基础费用。
async fn usage_totals(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<(i64, i64, Decimal), CoreError> {
    let usage_row = sqlx::query(
        "SELECT
            COUNT(*) as total_calls,
            COALESCE(SUM(request_size + response_size), 0)::BIGINT as total_data_bytes,
            COALESCE(SUM(billable_amount), 0) as base_amount
         FROM usage_records
         WHERE tenant_id = $1
         AND recorded_at >= $2
         AND recorded_at <= $3
         AND response_status < 400",
    )
    .bind(tenant_id)
    .bind(period_start)
    .bind(period_end)
    .fetch_one(&mut **tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    let total_calls: i64 = usage_row.get("total_calls");
    let total_data_bytes: i64 = usage_row.get("total_data_bytes");
    let base_amount: Decimal = usage_row.get("base_amount");
    Ok((total_calls, total_data_bytes, base_amount))
}
pub(crate) async fn generate_bill(
    repo: &PgBillingRepository,
    tenant_id: Uuid,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<Uuid, CoreError> {
    if period_start >= period_end {
        return Err(CoreError::Validation(
            "Billing period start must be before end".to_string(),
        ));
    }
    let mut tx = repo
        .pool
        .begin()
        .await
        .map_err(PgBillingRepository::map_error)?;
    let existing_bill: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM bills
         WHERE tenant_id = $1
           AND billing_period_start = $2
           AND billing_period_end = $3
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(period_start)
    .bind(period_end)
    .fetch_optional(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    let account_id: Uuid =
        sqlx::query_scalar("SELECT id FROM tenant_accounts WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(PgBillingRepository::map_error)?
            .ok_or_else(|| CoreError::Validation("Tenant account not found".to_string()))?;
    let (total_calls, total_data_bytes, base_amount) =
        usage_totals(&mut tx, tenant_id, period_start, period_end).await?;
    if let Some(bill_id) = existing_bill {
        sqlx::query(
            "UPDATE bills
             SET account_id = $2,
                 total_calls = $3,
                 total_data_bytes = $4,
                 base_amount = $5,
                 discount_amount = 0,
                 tax_amount = 0,
                 total_amount = $6,
                 updated_at = NOW()
             WHERE id = $1",
        )
        .bind(bill_id)
        .bind(account_id)
        .bind(total_calls)
        .bind(total_data_bytes)
        .bind(base_amount)
        .bind(base_amount)
        .execute(&mut *tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
        tx.commit().await.map_err(PgBillingRepository::map_error)?;
        return Ok(bill_id);
    }
    let bill_id: Uuid = sqlx::query_scalar(
        "INSERT INTO bills (
            tenant_id, account_id,
            billing_period_start, billing_period_end,
            total_calls, total_data_bytes,
            base_amount, discount_amount, tax_amount, total_amount,
            status
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending')
        RETURNING id",
    )
    .bind(tenant_id)
    .bind(account_id)
    .bind(period_start)
    .bind(period_end)
    .bind(total_calls)
    .bind(total_data_bytes)
    .bind(base_amount)
    .bind(Decimal::ZERO)
    .bind(Decimal::ZERO)
    .bind(base_amount)
    .fetch_one(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    tx.commit().await.map_err(PgBillingRepository::map_error)?;
    Ok(bill_id)
}
async fn zero_charge_result(
    repo: &PgBillingRepository,
    tenant_id: Uuid,
) -> Result<DeductAndRecordResult, CoreError> {
    let row = sqlx::query("SELECT balance FROM tenant_accounts WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_optional(&*repo.pool)
        .await
        .map_err(PgBillingRepository::map_error)?;
    let balance: Decimal = row.map_or(Decimal::ZERO, |r| r.get("balance"));
    Ok(DeductAndRecordResult {
        transaction_id: Uuid::nil(),
        balance_before: balance,
        balance_after: balance,
    })
}

/// 扣减余额（含授信额度检查），返回 (扣前, 扣后)。
async fn debit_balance(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    amount: Decimal,
) -> Result<(Decimal, Decimal), CoreError> {
    let row = sqlx::query(
        "UPDATE tenant_accounts
         SET balance = balance - $1
         WHERE tenant_id = $2 AND balance + credit_limit >= $1
         RETURNING balance + $1 AS balance_before, balance AS balance_after",
    )
    .bind(amount)
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    let Some(r) = row else {
        return Err(CoreError::Validation(format!(
            "Insufficient balance: required {amount}, tenant {tenant_id}"
        )));
    };
    Ok((r.get("balance_before"), r.get("balance_after")))
}

/// 标记扣费队列项与对应用量记录为已扣费。
async fn mark_queue_charged(
    tx: &mut Transaction<'_, Postgres>,
    queue_id: Uuid,
    transaction_id: Option<Uuid>,
) -> Result<(), CoreError> {
    sqlx::query(
        r"UPDATE usage_charge_queue
           SET status = 'completed', transaction_id = COALESCE($2, transaction_id), processed_at = NOW()
           WHERE id = $1",
    )
    .bind(queue_id)
    .bind(transaction_id)
    .execute(&mut **tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    sqlx::query(
        "UPDATE usage_records SET charge_status = 'charged', charged_at = NOW() \
         WHERE charge_queue_id = $1",
    )
    .bind(queue_id)
    .execute(&mut **tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    Ok(())
}

struct UsageCharge {
    transaction_id: Uuid,
    tenant_id: Uuid,
    account_id: Uuid,
    amount: Decimal,
    balance_before: Decimal,
    balance_after: Decimal,
    usage_record_id: i64,
    queue_id: Uuid,
}

/// 写一笔用量扣费：交易记录、账本分录、队列与用量状态。
async fn record_usage_charge(
    tx: &mut Transaction<'_, Postgres>,
    charge: &UsageCharge,
) -> Result<(), CoreError> {
    let usage_record_id = charge.usage_record_id;
    let event_id: Option<Uuid> =
        sqlx::query_scalar("SELECT event_id FROM usage_records WHERE id = $1")
            .bind(usage_record_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(PgBillingRepository::map_error)?;
    sqlx::query(
        r"INSERT INTO transactions
           (id, tenant_id, account_id, transaction_type, amount,
            balance_before, balance_after, reference_type, reference_id,
            description, status)
           VALUES ($1, $2, $3, 'charge', $4, $5, $6, 'usage', $7, $8, 'completed')",
    )
    .bind(charge.transaction_id)
    .bind(charge.tenant_id)
    .bind(charge.account_id)
    .bind(charge.amount)
    .bind(charge.balance_before)
    .bind(charge.balance_after)
    .bind(usage_record_id.to_string())
    .bind(format!("Usage charge for record {usage_record_id}"))
    .execute(&mut **tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    insert_ledger_entry(
        tx,
        LedgerEntry {
            tenant_id: charge.tenant_id,
            event_id,
            entry_type: "CHARGE",
            amount: charge.amount,
            direction: "DEBIT",
            balance_before: charge.balance_before,
            balance_after: charge.balance_after,
            description: Some(&format!("Usage record {usage_record_id}")),
        },
    )
    .await?;
    mark_queue_charged(tx, charge.queue_id, Some(charge.transaction_id)).await
}

pub(crate) async fn deduct_and_record(
    repo: &PgBillingRepository,
    tenant_id: Uuid,
    amount: Decimal,
    account_id: Uuid,
    usage_record_id: i64,
    queue_id: Uuid,
) -> Result<DeductAndRecordResult, CoreError> {
    if amount <= Decimal::ZERO {
        return zero_charge_result(repo, tenant_id).await;
    }
    let mut tx = repo
        .pool
        .begin()
        .await
        .map_err(PgBillingRepository::map_error)?;
    let (balance_before, balance_after) = debit_balance(&mut tx, tenant_id, amount).await?;
    let transaction_id = Uuid::new_v4();
    record_usage_charge(
        &mut tx,
        &UsageCharge {
            transaction_id,
            tenant_id,
            account_id,
            amount,
            balance_before,
            balance_after,
            usage_record_id,
            queue_id,
        },
    )
    .await?;
    tx.commit().await.map_err(PgBillingRepository::map_error)?;
    Ok(DeductAndRecordResult {
        transaction_id,
        balance_before,
        balance_after,
    })
}

/// One planned charge line inside a tenant batch (pure, computed before any SQL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedCharge {
    pub queue_id: Uuid,
    pub usage_record_id: i64,
    pub amount: Decimal,
    pub balance_before: Decimal,
    pub balance_after: Decimal,
    /// `None` for zero/negative amounts: only queue/usage status is updated.
    pub transaction_id: Option<Uuid>,
}

/// Walks the balance cursor in input order, exactly like the former per-item loop.
pub(crate) fn plan_batch_charges(
    balance_before_batch: Decimal,
    items: &[(Uuid, i64, Decimal)],
    mut new_id: impl FnMut() -> Uuid,
) -> (Vec<PlannedCharge>, Decimal) {
    let mut cursor = balance_before_batch;
    let mut out = Vec::with_capacity(items.len());
    for &(queue_id, usage_record_id, amount) in items {
        if amount <= Decimal::ZERO {
            out.push(PlannedCharge {
                queue_id,
                usage_record_id,
                amount,
                balance_before: cursor,
                balance_after: cursor,
                transaction_id: None,
            });
            continue;
        }
        let before = cursor;
        cursor -= amount;
        out.push(PlannedCharge {
            queue_id,
            usage_record_id,
            amount,
            balance_before: before,
            balance_after: cursor,
            transaction_id: Some(new_id()),
        });
    }
    (out, cursor)
}

/// Set-based INSERT of `transactions` + `ledger_entries` rows for planned charges.
async fn insert_batch_charge_rows(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    account_id: Uuid,
    charged: &[&PlannedCharge],
    usage_ids: &[i64],
    event_map: &std::collections::HashMap<i64, Option<Uuid>>,
) -> Result<Vec<Uuid>, CoreError> {
    let tx_ids: Vec<Uuid> = charged.iter().filter_map(|p| p.transaction_id).collect();
    let amounts: Vec<Decimal> = charged.iter().map(|p| p.amount).collect();
    let befores: Vec<Decimal> = charged.iter().map(|p| p.balance_before).collect();
    let afters: Vec<Decimal> = charged.iter().map(|p| p.balance_after).collect();
    let ref_ids: Vec<String> = usage_ids.iter().map(ToString::to_string).collect();
    let tx_descs: Vec<String> = usage_ids
        .iter()
        .map(|id| format!("Usage charge for record {id}"))
        .collect();
    let ledger_descs: Vec<String> = usage_ids
        .iter()
        .map(|id| format!("Usage record {id}"))
        .collect();
    let event_ids: Vec<Option<Uuid>> = usage_ids
        .iter()
        .map(|id| event_map.get(id).copied().flatten())
        .collect();

    if !tx_ids.is_empty() {
        sqlx::query(
            r"INSERT INTO transactions
               (id, tenant_id, account_id, transaction_type, amount,
                balance_before, balance_after, reference_type, reference_id,
                description, status)
               SELECT u.id, $2, $3, 'charge', u.amount, u.bb, u.ba, 'usage', u.rid, u.descr, 'completed'
               FROM UNNEST($1::uuid[], $4::numeric[], $5::numeric[], $6::numeric[], $7::text[], $8::text[])
                    AS u(id, amount, bb, ba, rid, descr)",
        )
        .bind(&tx_ids)
        .bind(tenant_id)
        .bind(account_id)
        .bind(&amounts)
        .bind(&befores)
        .bind(&afters)
        .bind(&ref_ids)
        .bind(&tx_descs)
        .execute(&mut **tx)
        .await
        .map_err(PgBillingRepository::map_error)?;

        sqlx::query(
            r"INSERT INTO mazbri_billing.ledger_entries
               (tenant_id, interaction_event_id, entry_type, amount, direction, balance_before, balance_after, description)
               SELECT $1, u.ev, 'CHARGE', u.amount, 'DEBIT', u.bb, u.ba, u.descr
               FROM UNNEST($2::uuid[], $3::numeric[], $4::numeric[], $5::numeric[], $6::text[])
                    AS u(ev, amount, bb, ba, descr)",
        )
        .bind(tenant_id)
        .bind(&event_ids)
        .bind(&amounts)
        .bind(&befores)
        .bind(&afters)
        .bind(&ledger_descs)
        .execute(&mut **tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    }

    Ok(tx_ids)
}

/// One-tenant batch charge: one balance row UPDATE for the sum, then set-based bookkeeping
/// (constant number of statements per batch instead of ~5 per item). Single transaction.
pub(crate) async fn deduct_and_record_batch(
    repo: &PgBillingRepository,
    tenant_id: Uuid,
    account_id: Uuid,
    items: &[(Uuid, i64, Decimal)],
) -> Result<DeductAndRecordResult, CoreError> {
    if items.is_empty() {
        return Ok(DeductAndRecordResult {
            transaction_id: Uuid::nil(),
            balance_before: Decimal::ZERO,
            balance_after: Decimal::ZERO,
        });
    }
    let total: Decimal = items
        .iter()
        .map(|(_, _, amount)| *amount)
        .filter(|a| *a > Decimal::ZERO)
        .sum();
    if total <= Decimal::ZERO {
        return zero_charge_result(repo, tenant_id).await;
    }

    let mut tx = repo
        .pool
        .begin()
        .await
        .map_err(PgBillingRepository::map_error)?;
    let (balance_before_batch, balance_after_final) =
        debit_balance(&mut tx, tenant_id, total).await?;
    let (plan, cursor) = plan_batch_charges(balance_before_batch, items, Uuid::new_v4);
    if cursor != balance_after_final {
        return Err(CoreError::Validation(format!(
            "batch balance cursor mismatch: cursor {cursor} != db {balance_after_final}"
        )));
    }

    let charged: Vec<&PlannedCharge> = plan.iter().filter(|p| p.transaction_id.is_some()).collect();
    let usage_ids: Vec<i64> = charged.iter().map(|p| p.usage_record_id).collect();
    let rows = sqlx::query("SELECT id, event_id FROM usage_records WHERE id = ANY($1)")
        .bind(&usage_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    let event_map: std::collections::HashMap<i64, Option<Uuid>> = rows
        .iter()
        .map(|r| (r.get::<i64, _>("id"), r.get::<Option<Uuid>, _>("event_id")))
        .collect();

    let tx_ids = insert_batch_charge_rows(
        &mut tx, tenant_id, account_id, &charged, &usage_ids, &event_map,
    )
    .await?;

    let queue_ids: Vec<Uuid> = plan.iter().map(|p| p.queue_id).collect();
    let all_usage_ids: Vec<i64> = plan.iter().map(|p| p.usage_record_id).collect();
    let queue_tx_ids: Vec<Option<Uuid>> = plan.iter().map(|p| p.transaction_id).collect();
    sqlx::query(
        r"UPDATE usage_charge_queue q
           SET status = 'completed', transaction_id = COALESCE(u.tx, q.transaction_id), processed_at = NOW()
           FROM UNNEST($1::uuid[], $2::uuid[]) AS u(id, tx)
           WHERE q.id = u.id",
    )
    .bind(&queue_ids)
    .bind(&queue_tx_ids)
    .execute(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    sqlx::query(
        // Join on the PK (usage_records.charge_queue_id has no index); the
        // charge_queue_id predicate keeps the original matching semantics.
        r"UPDATE usage_records r SET charge_status = 'charged', charged_at = NOW()
           FROM UNNEST($1::uuid[], $2::bigint[]) AS u(qid, rid)
           WHERE r.id = u.rid AND r.charge_queue_id = u.qid",
    )
    .bind(&queue_ids)
    .bind(&all_usage_ids)
    .execute(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?;

    tx.commit().await.map_err(PgBillingRepository::map_error)?;
    Ok(DeductAndRecordResult {
        transaction_id: tx_ids.last().copied().unwrap_or_else(Uuid::nil),
        balance_before: balance_before_batch,
        balance_after: balance_after_final,
    })
}

pub(crate) async fn create_adjustment(
    repo: &PgBillingRepository,
    tenant_id: Uuid,
    amount: Decimal,
    direction: &str,
    description: &str,
) -> Result<Uuid, CoreError> {
    let mut tx = repo
        .pool
        .begin()
        .await
        .map_err(PgBillingRepository::map_error)?;
    let row =
        sqlx::query("SELECT id, balance FROM tenant_accounts WHERE tenant_id = $1 FOR UPDATE")
            .bind(tenant_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(PgBillingRepository::map_error)?
            .ok_or_else(|| CoreError::NotFound {
                resource: "tenant_account".to_string(),
                id: tenant_id.to_string(),
            })?;
    let account_id: Uuid = row.get("id");
    let balance_before: Decimal = row.get("balance");
    let balance_after = if direction == "CREDIT" {
        balance_before + amount
    } else {
        balance_before - amount
    };
    sqlx::query("UPDATE tenant_accounts SET balance = $2, updated_at = NOW() WHERE id = $1")
        .bind(account_id)
        .bind(balance_after)
        .execute(&mut *tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    let transaction_id = Uuid::new_v4();
    sqlx::query(
            "INSERT INTO transactions (id, tenant_id, account_id, transaction_type, amount, balance_before, balance_after, status, description)
         VALUES ($1, $2, $3, 'adjustment', $4, $5, $6, 'completed', $7)",
        )
        .bind(transaction_id)
        .bind(tenant_id)
        .bind(account_id)
        .bind(amount)
        .bind(balance_before)
        .bind(balance_after)
        .bind(description)
        .execute(&mut *tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    insert_ledger_entry(
        &mut tx,
        LedgerEntry {
            tenant_id,
            event_id: None,
            entry_type: "ADJUSTMENT",
            amount,
            direction,
            balance_before,
            balance_after,
            description: Some(description),
        },
    )
    .await?;
    tx.commit().await.map_err(PgBillingRepository::map_error)?;
    Ok(transaction_id)
}

#[cfg(test)]
mod batch_plan_tests {
    use super::*;
    use std::str::FromStr;
    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn cursor_walks_in_order_and_skips_zero() {
        let q = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let items = [
            (q[0], 1, d("0.001")),
            (q[1], 2, d("0")),
            (q[2], 3, d("0.002")),
        ];
        let mut n = 0u128;
        let (plan, cursor) = plan_batch_charges(d("10"), &items, || {
            n += 1;
            Uuid::from_u128(n)
        });
        assert_eq!(cursor, d("9.997"));
        assert_eq!(plan[0].balance_before, d("10"));
        assert_eq!(plan[0].balance_after, d("9.999"));
        assert_eq!(plan[1].transaction_id, None);
        assert_eq!(plan[1].balance_before, d("9.999"));
        assert_eq!(plan[2].balance_before, d("9.999"));
        assert_eq!(plan[2].balance_after, d("9.997"));
        assert_eq!(plan[2].transaction_id, Some(Uuid::from_u128(2)));
    }

    #[test]
    fn chain_is_contiguous() {
        let items: Vec<_> = (0..500).map(|i| (Uuid::new_v4(), i, d("0.001"))).collect();
        let (plan, cursor) = plan_batch_charges(d("100"), &items, Uuid::new_v4);
        for w in plan.windows(2) {
            assert_eq!(w[0].balance_after, w[1].balance_before);
        }
        assert_eq!(cursor, d("99.5"));
    }
}
