use super::{DeductAndRecordResult, PgBillingRepository};
use crate::queue::{ChargeEnqueue, ChargeQueue};
use crate::service::UsageRecord;
use crate::types::{AdjustmentDirection, FencedChargeItem, RecordUsageOutcome};
use chrono::{DateTime, Utc};
use mazbri_core::CoreError;
use rust_decimal::Decimal;
use sqlx::{Postgres, Row, Transaction};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub(crate) const FENCE_SINGLE_QUEUE_SQL: &str = "UPDATE usage_charge_queue \
     SET status = 'completed', processed_at = NOW() \
     WHERE id = $1 AND status = 'processing' AND lease_seq = $2";

pub(crate) const FENCE_BATCH_QUEUE_SQL: &str = r"UPDATE usage_charge_queue q
   SET status = 'completed', processed_at = NOW()
   FROM UNNEST($1::uuid[], $2::bigint[]) AS u(id, seq)
   WHERE q.id = u.id AND q.status = 'processing' AND q.lease_seq = u.seq
   RETURNING q.id";

pub(crate) const MARK_SINGLE_USAGE_CHARGED_SQL: &str = "UPDATE usage_records \
     SET charge_status = 'charged', charged_at = NOW() \
     WHERE id = $1 AND charge_queue_id = $2 \
       AND (charge_status IS NULL OR charge_status != 'charged') \
     RETURNING event_id";

pub(crate) const MARK_BATCH_USAGE_CHARGED_SQL: &str = r"UPDATE usage_records r
   SET charge_status = 'charged', charged_at = NOW()
   FROM UNNEST($1::uuid[], $2::bigint[]) AS u(qid, rid)
   WHERE r.id = u.rid AND r.charge_queue_id = u.qid
     AND (r.charge_status IS NULL OR r.charge_status != 'charged')
   RETURNING r.id, r.event_id";

pub(crate) const BILL_USAGE_TOTALS_SQL: &str = "SELECT
    COUNT(*) as total_calls,
    COALESCE(SUM(request_size + response_size), 0)::BIGINT as total_data_bytes,
    COALESCE(SUM(billable_amount), 0) as base_amount
 FROM usage_records
 WHERE tenant_id = $1
   AND recorded_at >= $2
   AND recorded_at < $3
   AND charge_status = 'charged'
   AND billable_amount > 0";

pub(crate) const BILL_SNAPSHOT_ITEMS_SQL: &str = "SELECT
    api_endpoint,
    COUNT(*) as quantity,
    COALESCE(SUM(billable_units), 0)::BIGINT as total_units,
    COALESCE(MIN(unit_price), 0) as unit_price,
    COALESCE(SUM(billable_amount), 0) as total
 FROM usage_records
 WHERE tenant_id = $1
   AND recorded_at >= $2
   AND recorded_at < $3
   AND charge_status = 'charged'
   AND billable_amount > 0
 GROUP BY api_endpoint
 ORDER BY total DESC, api_endpoint ASC";

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

fn validate_non_negative_debit_amount(amount: Decimal) -> Result<(), CoreError> {
    if amount < Decimal::ZERO {
        return Err(CoreError::Validation(format!(
            "Debit amount must be non-negative, got {amount}"
        )));
    }
    Ok(())
}

fn validate_adjustment_amount_and_overdraft(
    amount: Decimal,
    direction: AdjustmentDirection,
    balance_before: Decimal,
    credit_limit: Decimal,
    allow_overdraft: bool,
) -> Result<Decimal, CoreError> {
    if amount <= Decimal::ZERO {
        return Err(CoreError::Validation(format!(
            "Adjustment amount must be greater than zero, got {amount}"
        )));
    }
    let balance_after = match direction {
        AdjustmentDirection::Credit => balance_before + amount,
        AdjustmentDirection::Debit => {
            let after = balance_before - amount;
            if !allow_overdraft && after < -credit_limit {
                return Err(CoreError::Validation(format!(
                    "Insufficient balance for debit adjustment: balance_after {after} < -credit_limit (-{credit_limit})"
                )));
            }
            after
        }
    };
    Ok(balance_after)
}

#[must_use]
fn ensure_charge_event_id(event_id: Option<Uuid>, usage_record_id: i64) -> Uuid {
    event_id.filter(|id| !id.is_nil()).unwrap_or_else(|| {
        let low = u64::from_be_bytes(usage_record_id.to_be_bytes());
        Uuid::from_u64_pair(0x4d41_5a42_5249_4348, low)
    })
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
    validate_non_negative_debit_amount(amount)?;
    if amount.is_zero() {
        let balance: Option<Decimal> =
            sqlx::query_scalar("SELECT balance FROM tenant_accounts WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_optional(&*repo.pool)
                .await
                .map_err(PgBillingRepository::map_error)?;
        return balance.ok_or_else(|| CoreError::NotFound {
            resource: "tenant_account".to_string(),
            id: tenant_id.to_string(),
        });
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
    let (balance_before, new_balance) = debit_balance(&mut tx, tenant_id, amount).await?;
    let transaction_id = Uuid::now_v7();
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

/// 统计账期内已入账且正金额的调用数、数据量与基础费用（左闭右开 `[period_start, period_end)`）。
async fn usage_totals(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<(i64, i64, Decimal), CoreError> {
    let usage_row = sqlx::query(BILL_USAGE_TOTALS_SQL)
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

async fn snapshot_bill_items_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    bill_id: Uuid,
    tenant_id: Uuid,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<(), CoreError> {
    let rows = sqlx::query(BILL_SNAPSHOT_ITEMS_SQL)
        .bind(tenant_id)
        .bind(period_start)
        .bind(period_end)
        .fetch_all(&mut **tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    sqlx::query("DELETE FROM bill_items WHERE bill_id = $1")
        .bind(bill_id)
        .execute(&mut **tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    for (index, row) in rows.into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO bill_items (
                bill_id, item_key, description,
                quantity, total_units, unit_price, total, metadata
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(bill_id)
        .bind(format!("item-{}", index + 1))
        .bind(row.get::<String, _>("api_endpoint"))
        .bind(row.get::<i64, _>("quantity"))
        .bind(row.get::<i64, _>("total_units"))
        .bind(row.get::<Decimal, _>("unit_price"))
        .bind(row.get::<Decimal, _>("total"))
        .bind(serde_json::json!({ "source": "customer_usage" }))
        .execute(&mut **tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    }
    Ok(())
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
           AND bill_kind = 'customer'
           AND billing_period_start = $2
           AND billing_period_end = $3
           AND status != 'cancelled'
         LIMIT 1
         FOR UPDATE",
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
    let bill_id = if let Some(bill_id) = existing_bill {
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
        bill_id
    } else {
        sqlx::query_scalar(
            "INSERT INTO bills (
                tenant_id, account_id, bill_kind,
                billing_period_start, billing_period_end,
                total_calls, total_data_bytes,
                base_amount, discount_amount, tax_amount, total_amount,
                status
            ) VALUES ($1, $2, 'customer', $3, $4, $5, $6, $7, $8, $9, $10, 'pending')
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
        .map_err(PgBillingRepository::map_error)?
    };
    snapshot_bill_items_in_tx(&mut tx, bill_id, tenant_id, period_start, period_end).await?;
    tx.commit().await.map_err(PgBillingRepository::map_error)?;
    Ok(bill_id)
}

async fn current_balance_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<Decimal, CoreError> {
    let row = sqlx::query("SELECT balance FROM tenant_accounts WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    Ok(row.map_or(Decimal::ZERO, |r| r.get("balance")))
}

/// 扣减余额（含非负硬校验与授信额度检查），返回 `(扣前, 扣后)`。
pub(crate) async fn debit_balance(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    amount: Decimal,
) -> Result<(Decimal, Decimal), CoreError> {
    validate_non_negative_debit_amount(amount)?;
    if amount.is_zero() {
        let balance: Option<Decimal> =
            sqlx::query_scalar("SELECT balance FROM tenant_accounts WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(PgBillingRepository::map_error)?;
        let balance = balance.ok_or_else(|| CoreError::NotFound {
            resource: "tenant_account".to_string(),
            id: tenant_id.to_string(),
        })?;
        return Ok((balance, balance));
    }
    let row = sqlx::query(
        "UPDATE tenant_accounts
         SET balance = balance - $1, updated_at = NOW()
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

async fn deduct_and_record_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    amount: Decimal,
    account_id: Uuid,
    usage_record_id: i64,
    queue_id: Uuid,
    lease_seq: i64,
) -> Result<DeductAndRecordResult, CoreError> {
    validate_non_negative_debit_amount(amount)?;
    // Step 1: Monotonic lease fence on usage_charge_queue inside the same PG transaction.
    let fenced = sqlx::query(FENCE_SINGLE_QUEUE_SQL)
        .bind(queue_id)
        .bind(lease_seq)
        .execute(&mut **tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    if fenced.rows_affected() == 0 {
        let bal = current_balance_in_tx(tx, tenant_id).await?;
        return Ok(DeductAndRecordResult {
            transaction_id: Uuid::nil(),
            balance_before: bal,
            balance_after: bal,
            charged_items: 0,
        });
    }

    if amount.is_zero() {
        let bal = current_balance_in_tx(tx, tenant_id).await?;
        return Ok(DeductAndRecordResult {
            transaction_id: Uuid::nil(),
            balance_before: bal,
            balance_after: bal,
            charged_items: 0,
        });
    }

    // Step 2: Transition usage_records with `charge_status != 'charged'` guard and read event_id.
    let usage_row = sqlx::query(MARK_SINGLE_USAGE_CHARGED_SQL)
        .bind(usage_record_id)
        .bind(queue_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    let Some(usage_row) = usage_row else {
        let bal = current_balance_in_tx(tx, tenant_id).await?;
        return Ok(DeductAndRecordResult {
            transaction_id: Uuid::nil(),
            balance_before: bal,
            balance_after: bal,
            charged_items: 0,
        });
    };
    let raw_event_id: Option<Uuid> = usage_row.get("event_id");
    let interaction_event_id = ensure_charge_event_id(raw_event_id, usage_record_id);

    // Step 3: Debit balance, write transaction & ledger entry with non-null interaction_event_id.
    let (balance_before, balance_after) = debit_balance(tx, tenant_id, amount).await?;
    let transaction_id = Uuid::now_v7();
    sqlx::query(
        r"INSERT INTO transactions
           (id, tenant_id, account_id, transaction_type, amount,
            balance_before, balance_after, reference_type, reference_id,
            description, status)
           VALUES ($1, $2, $3, 'charge', $4, $5, $6, 'usage', $7, $8, 'completed')",
    )
    .bind(transaction_id)
    .bind(tenant_id)
    .bind(account_id)
    .bind(amount)
    .bind(balance_before)
    .bind(balance_after)
    .bind(usage_record_id.to_string())
    .bind(format!("Usage charge for record {usage_record_id}"))
    .execute(&mut **tx)
    .await
    .map_err(PgBillingRepository::map_error)?;

    insert_ledger_entry(
        tx,
        LedgerEntry {
            tenant_id,
            event_id: Some(interaction_event_id),
            entry_type: "CHARGE",
            amount,
            direction: "DEBIT",
            balance_before,
            balance_after,
            description: Some(&format!("Usage record {usage_record_id}")),
        },
    )
    .await?;

    sqlx::query("UPDATE usage_charge_queue SET transaction_id = $2 WHERE id = $1")
        .bind(queue_id)
        .bind(transaction_id)
        .execute(&mut **tx)
        .await
        .map_err(PgBillingRepository::map_error)?;

    Ok(DeductAndRecordResult {
        transaction_id,
        balance_before,
        balance_after,
        charged_items: 1,
    })
}

pub(crate) async fn deduct_and_record(
    repo: &PgBillingRepository,
    tenant_id: Uuid,
    amount: Decimal,
    account_id: Uuid,
    usage_record_id: i64,
    queue_id: Uuid,
    lease_seq: i64,
) -> Result<DeductAndRecordResult, CoreError> {
    validate_non_negative_debit_amount(amount)?;
    let mut tx = repo
        .pool
        .begin()
        .await
        .map_err(PgBillingRepository::map_error)?;
    let result = deduct_and_record_in_tx(
        &mut tx,
        tenant_id,
        amount,
        account_id,
        usage_record_id,
        queue_id,
        lease_seq,
    )
    .await?;
    tx.commit().await.map_err(PgBillingRepository::map_error)?;
    Ok(result)
}

enum UpsertedUsageRow {
    Inserted(i64),
    AlreadyCharged {
        usage_record_id: i64,
        charge_id: Option<Uuid>,
        charged: Decimal,
    },
    AlreadyQueued {
        usage_record_id: i64,
        queue_id: Uuid,
    },
    Pending(i64),
}

async fn upsert_usage_record_for_charge_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    record: &UsageRecord,
    event_id: Uuid,
    units: i64,
    has_async_queue: bool,
) -> Result<UpsertedUsageRow, CoreError> {
    let amount = record.billing_result.amount;
    let inserted_id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO usage_records (
            tenant_id, account_id, api_endpoint, http_method, response_status,
            request_size, response_size, billing_service_duration_ms,
            unit_price, billable_amount, billable_units,
            product_id, billing_code, event_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT (event_id) WHERE event_id IS NOT NULL DO NOTHING
        RETURNING id",
    )
    .bind(record.tenant_id)
    .bind(record.account_id)
    .bind(&record.api_endpoint)
    .bind(&record.http_method)
    .bind(i32::from(record.response_status))
    .bind(record.request_size)
    .bind(record.response_size)
    .bind(record.billing_service_duration_ms)
    .bind(record.billing_result.unit_price)
    .bind(amount)
    .bind(units)
    .bind(&record.product_id)
    .bind(record.billing_code)
    .bind(event_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(PgBillingRepository::map_error)?;

    if let Some(id) = inserted_id {
        return Ok(UpsertedUsageRow::Inserted(id));
    }

    let existing = sqlx::query(
        "SELECT id, charge_status, charge_queue_id, billable_amount
         FROM usage_records
         WHERE event_id = $1
         FOR UPDATE",
    )
    .bind(event_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    let existing_id: i64 = existing.get("id");
    let existing_status: Option<String> = existing.get("charge_status");
    let existing_queue_id: Option<Uuid> = existing.get("charge_queue_id");
    let existing_amount: Decimal = existing.get("billable_amount");

    if existing_status.as_deref() == Some("charged") {
        return Ok(UpsertedUsageRow::AlreadyCharged {
            usage_record_id: existing_id,
            charge_id: existing_queue_id,
            charged: existing_amount,
        });
    }
    if has_async_queue
        && let Some(qid) = existing_queue_id
    {
        return Ok(UpsertedUsageRow::AlreadyQueued {
            usage_record_id: existing_id,
            queue_id: qid,
        });
    }
    Ok(UpsertedUsageRow::Pending(existing_id))
}

async fn sync_claim_and_deduct_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    account_id: Uuid,
    usage_record_id: i64,
    amount: Decimal,
) -> Result<(Uuid, Decimal), CoreError> {
    let (queue_id, lease_seq): (Uuid, i64) = sqlx::query_as(
        "INSERT INTO usage_charge_queue
            (tenant_id, account_id, usage_record_id, amount, status, lease_seq, locked_at, worker_node)
         VALUES ($1, $2, $3, $4, 'processing', 1, NOW(), 'sync-rpc')
         ON CONFLICT (usage_record_id) DO UPDATE
            SET status = 'processing',
                lease_seq = usage_charge_queue.lease_seq + 1,
                locked_at = NOW(),
                worker_node = 'sync-rpc'
          WHERE usage_charge_queue.status != 'completed'
         RETURNING id, lease_seq",
    )
    .bind(tenant_id)
    .bind(account_id)
    .bind(usage_record_id)
    .bind(amount)
    .fetch_one(&mut **tx)
    .await
    .map_err(PgBillingRepository::map_error)?;

    sqlx::query(
        "UPDATE usage_records
         SET charge_queue_id = $1
         WHERE id = $2 AND (charge_status IS NULL OR charge_status != 'charged')",
    )
    .bind(queue_id)
    .bind(usage_record_id)
    .execute(&mut **tx)
    .await
    .map_err(PgBillingRepository::map_error)?;

    let deduct_res = deduct_and_record_in_tx(
        tx,
        tenant_id,
        amount,
        account_id,
        usage_record_id,
        queue_id,
        lease_seq,
    )
    .await?;
    Ok((queue_id, deduct_res.balance_after))
}

pub(crate) async fn record_usage_and_charge(
    repo: &PgBillingRepository,
    record: &UsageRecord,
    async_queue: Option<&ChargeQueue>,
) -> Result<RecordUsageOutcome, CoreError> {
    let event_id = record
        .event_id
        .filter(|id| !id.is_nil())
        .ok_or_else(|| CoreError::Validation("event_id must be a valid non-nil UUID".into()))?;
    validate_non_negative_debit_amount(record.billing_result.amount)?;
    let units = i64::try_from(record.billing_result.units).map_err(|e| {
        CoreError::Validation(format!(
            "Billing units {} exceeds maximum representable value: {}",
            record.billing_result.units, e
        ))
    })?;
    let amount = record.billing_result.amount;

    let mut tx = repo
        .pool
        .begin()
        .await
        .map_err(PgBillingRepository::map_error)?;

    let usage_record_id =
        match upsert_usage_record_for_charge_in_tx(&mut tx, record, event_id, units, async_queue.is_some()).await? {
            UpsertedUsageRow::Inserted(id) | UpsertedUsageRow::Pending(id) => id,
            UpsertedUsageRow::AlreadyCharged {
                usage_record_id,
                charge_id,
                charged,
            } => {
                let bal = current_balance_in_tx(&mut tx, record.tenant_id).await?;
                tx.commit().await.map_err(PgBillingRepository::map_error)?;
                return Ok(RecordUsageOutcome::SyncCharged {
                    usage_record_id,
                    charge_id,
                    charged,
                    new_balance: bal,
                });
            }
            UpsertedUsageRow::AlreadyQueued {
                usage_record_id,
                queue_id,
            } => {
                tx.commit().await.map_err(PgBillingRepository::map_error)?;
                return Ok(RecordUsageOutcome::Queued {
                    usage_record_id,
                    queue_id,
                });
            }
        };

    if amount.is_zero() {
        tx.commit().await.map_err(PgBillingRepository::map_error)?;
        return Ok(RecordUsageOutcome::NoCharge { usage_record_id });
    }

    if let Some(queue) = async_queue {
        let trace_parent = mazbri_core::trace_propagation::current_span_context()
            .map(|span_context| mazbri_core::trace_spans::traceparent_of(&span_context));
        let queue_id = queue
            .enqueue_in_tx(
                &mut tx,
                ChargeEnqueue {
                    tenant_id: record.tenant_id,
                    account_id: record.account_id,
                    usage_record_id,
                    amount,
                    payload: None,
                    trace_parent: trace_parent.as_deref(),
                },
            )
            .await
            .map_err(CoreError::from)?;
        tx.commit().await.map_err(PgBillingRepository::map_error)?;
        return Ok(RecordUsageOutcome::Queued {
            usage_record_id,
            queue_id,
        });
    }

    let (queue_id, new_balance) = sync_claim_and_deduct_in_tx(
        &mut tx,
        record.tenant_id,
        record.account_id,
        usage_record_id,
        amount,
    )
    .await?;
    tx.commit().await.map_err(PgBillingRepository::map_error)?;
    Ok(RecordUsageOutcome::SyncCharged {
        usage_record_id,
        charge_id: Some(queue_id),
        charged: amount,
        new_balance,
    })
}

/// One planned charge line inside a tenant batch (pure, computed before balance cursor commit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedCharge {
    pub queue_id: Uuid,
    pub usage_record_id: i64,
    pub amount: Decimal,
    pub balance_before: Decimal,
    pub balance_after: Decimal,
    /// `None` for zero amounts: only queue/usage status is updated.
    pub transaction_id: Option<Uuid>,
}

/// Walks the balance cursor in input order over fenced, uncharged items.
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

/// Filter batch items down to those that won BOTH the `usage_charge_queue` `(status='processing', lease_seq)`
/// fence AND the `usage_records` `charge_status != 'charged'` fence.
pub(crate) fn filter_fenced_chargeable_items(
    items: &[FencedChargeItem],
    fenced_queue_ids: &HashSet<Uuid>,
    newly_charged_usage: &HashMap<i64, Option<Uuid>>,
) -> Vec<(Uuid, i64, Decimal, Uuid)> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if !fenced_queue_ids.contains(&item.queue_id) {
            continue;
        }
        if item.amount <= Decimal::ZERO {
            continue;
        }
        let Some(&raw_event_id) = newly_charged_usage.get(&item.usage_record_id) else {
            continue;
        };
        let event_id = ensure_charge_event_id(raw_event_id, item.usage_record_id);
        out.push((item.queue_id, item.usage_record_id, item.amount, event_id));
    }
    out
}

/// Set-based INSERT of `transactions` + `ledger_entries` rows for planned charges.
async fn insert_batch_charge_rows(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    account_id: Uuid,
    charged: &[PlannedCharge],
    event_ids: &[Uuid],
) -> Result<Vec<Uuid>, CoreError> {
    let tx_ids: Vec<Uuid> = charged.iter().filter_map(|p| p.transaction_id).collect();
    let amounts: Vec<Decimal> = charged.iter().map(|p| p.amount).collect();
    let befores: Vec<Decimal> = charged.iter().map(|p| p.balance_before).collect();
    let afters: Vec<Decimal> = charged.iter().map(|p| p.balance_after).collect();
    let ref_ids: Vec<String> = charged
        .iter()
        .map(|p| p.usage_record_id.to_string())
        .collect();
    let tx_descs: Vec<String> = charged
        .iter()
        .map(|p| format!("Usage charge for record {}", p.usage_record_id))
        .collect();
    let ledger_descs: Vec<String> = charged
        .iter()
        .map(|p| format!("Usage record {}", p.usage_record_id))
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
        .bind(event_ids)
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

async fn fence_and_mark_batch_usage_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    items: &[FencedChargeItem],
) -> Result<Vec<(Uuid, i64, Decimal, Uuid)>, CoreError> {
    let req_queue_ids: Vec<Uuid> = items.iter().map(|i| i.queue_id).collect();
    let req_lease_seqs: Vec<i64> = items.iter().map(|i| i.lease_seq).collect();
    let fenced_ids: Vec<Uuid> = sqlx::query_scalar(FENCE_BATCH_QUEUE_SQL)
        .bind(&req_queue_ids)
        .bind(&req_lease_seqs)
        .fetch_all(&mut **tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    let fenced_set: HashSet<Uuid> = fenced_ids.into_iter().collect();
    if fenced_set.is_empty() {
        return Ok(Vec::new());
    }

    let mut active_queue_ids = Vec::with_capacity(fenced_set.len());
    let mut active_usage_record_ids = Vec::with_capacity(fenced_set.len());
    for item in items {
        if fenced_set.contains(&item.queue_id) && item.amount > Decimal::ZERO {
            active_queue_ids.push(item.queue_id);
            active_usage_record_ids.push(item.usage_record_id);
        }
    }

    let mut newly_charged_usage: HashMap<i64, Option<Uuid>> = HashMap::new();
    if !active_queue_ids.is_empty() {
        let rows = sqlx::query(MARK_BATCH_USAGE_CHARGED_SQL)
            .bind(&active_queue_ids)
            .bind(&active_usage_record_ids)
            .fetch_all(&mut **tx)
            .await
            .map_err(PgBillingRepository::map_error)?;
        for r in rows {
            newly_charged_usage.insert(
                r.get::<i64, _>("id"),
                r.get::<Option<Uuid>, _>("event_id"),
            );
        }
    }

    Ok(filter_fenced_chargeable_items(
        items,
        &fenced_set,
        &newly_charged_usage,
    ))
}

/// One-tenant batch charge in a single PG transaction:
/// 1. Atomically fences `usage_charge_queue` with `WHERE id = $id AND status = 'processing' AND lease_seq = $seq`.
/// 2. Transitions `usage_records` (`AND charge_status != 'charged'`) for fenced items and reads `event_id`.
/// 3. Sums and debits `tenant_accounts` ONLY for rows that won both fences.
/// 4. Writes `transactions` + `ledger_entries` (non-null `interaction_event_id`) and sets `transaction_id`.
pub(crate) async fn deduct_and_record_batch(
    repo: &PgBillingRepository,
    tenant_id: Uuid,
    account_id: Uuid,
    items: &[FencedChargeItem],
) -> Result<DeductAndRecordResult, CoreError> {
    if items.is_empty() {
        return Ok(DeductAndRecordResult {
            transaction_id: Uuid::nil(),
            balance_before: Decimal::ZERO,
            balance_after: Decimal::ZERO,
            charged_items: 0,
        });
    }
    for item in items {
        validate_non_negative_debit_amount(item.amount)?;
    }

    let mut tx = repo
        .pool
        .begin()
        .await
        .map_err(PgBillingRepository::map_error)?;

    let chargeable = fence_and_mark_batch_usage_in_tx(&mut tx, items).await?;
    let total: Decimal = chargeable.iter().map(|(_, _, a, _)| *a).sum();
    if total <= Decimal::ZERO {
        let bal = current_balance_in_tx(&mut tx, tenant_id).await?;
        tx.commit().await.map_err(PgBillingRepository::map_error)?;
        return Ok(DeductAndRecordResult {
            transaction_id: Uuid::nil(),
            balance_before: bal,
            balance_after: bal,
            charged_items: 0,
        });
    }

    let (balance_before_batch, balance_after_final) =
        debit_balance(&mut tx, tenant_id, total).await?;
    let plan_inputs: Vec<(Uuid, i64, Decimal)> = chargeable
        .iter()
        .map(|&(qid, rid, amt, _)| (qid, rid, amt))
        .collect();
    let event_ids: Vec<Uuid> = chargeable.iter().map(|&(_, _, _, ev)| ev).collect();
    let (plan, cursor) = plan_batch_charges(balance_before_batch, &plan_inputs, Uuid::now_v7);
    if cursor != balance_after_final {
        return Err(CoreError::Validation(format!(
            "batch balance cursor mismatch: cursor {cursor} != db {balance_after_final}"
        )));
    }

    let tx_ids =
        insert_batch_charge_rows(&mut tx, tenant_id, account_id, &plan, &event_ids).await?;

    let charged_qids: Vec<Uuid> = plan.iter().map(|p| p.queue_id).collect();
    let charged_tx_ids: Vec<Option<Uuid>> = plan.iter().map(|p| p.transaction_id).collect();
    sqlx::query(
        r"UPDATE usage_charge_queue q
           SET transaction_id = COALESCE(u.tx, q.transaction_id)
           FROM UNNEST($1::uuid[], $2::uuid[]) AS u(id, tx)
           WHERE q.id = u.id",
    )
    .bind(&charged_qids)
    .bind(&charged_tx_ids)
    .execute(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?;

    tx.commit().await.map_err(PgBillingRepository::map_error)?;
    Ok(DeductAndRecordResult {
        transaction_id: tx_ids.last().copied().unwrap_or_else(Uuid::nil),
        balance_before: balance_before_batch,
        balance_after: balance_after_final,
        charged_items: plan.len(),
    })
}

pub(crate) async fn create_adjustment(
    repo: &PgBillingRepository,
    tenant_id: Uuid,
    amount: Decimal,
    direction: AdjustmentDirection,
    allow_overdraft: bool,
    description: &str,
) -> Result<Uuid, CoreError> {
    if amount <= Decimal::ZERO {
        return Err(CoreError::Validation(format!(
            "Adjustment amount must be greater than zero, got {amount}"
        )));
    }
    let mut tx = repo
        .pool
        .begin()
        .await
        .map_err(PgBillingRepository::map_error)?;
    let row = sqlx::query(
        "SELECT id, balance, credit_limit FROM tenant_accounts WHERE tenant_id = $1 FOR UPDATE",
    )
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
    let credit_limit: Decimal = row.get("credit_limit");
    let balance_after = validate_adjustment_amount_and_overdraft(
        amount,
        direction,
        balance_before,
        credit_limit,
        allow_overdraft,
    )?;
    sqlx::query("UPDATE tenant_accounts SET balance = $2, updated_at = NOW() WHERE id = $1")
        .bind(account_id)
        .bind(balance_after)
        .execute(&mut *tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    let transaction_id = Uuid::now_v7();
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
            direction: direction.as_str(),
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
        Decimal::from_str(s).expect("decimal")
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
    fn chain_is_contiguous_at_micro_precision() {
        let items: Vec<_> = (0..500)
            .map(|i| (Uuid::new_v4(), i, d("0.000040")))
            .collect();
        let (plan, cursor) = plan_batch_charges(d("100.000123"), &items, Uuid::new_v4);
        for w in plan.windows(2) {
            assert_eq!(w[0].balance_after, w[1].balance_before);
        }
        assert_eq!(cursor, d("99.980123"));
    }

    #[test]
    fn debit_and_adjustment_validation_enforces_sign_and_credit_limit() {
        assert!(validate_non_negative_debit_amount(d("-0.000001")).is_err());
        assert!(validate_non_negative_debit_amount(d("0")).is_ok());
        assert!(validate_non_negative_debit_amount(d("0.000001")).is_ok());

        // Adjustment requires strictly positive amount
        assert!(
            validate_adjustment_amount_and_overdraft(
                d("0"),
                AdjustmentDirection::Credit,
                d("10"),
                d("5"),
                false
            )
            .is_err()
        );
        assert!(
            validate_adjustment_amount_and_overdraft(
                d("-1"),
                AdjustmentDirection::Debit,
                d("10"),
                d("5"),
                false
            )
            .is_err()
        );

        // Normal debit adjustment enforces balance_after >= -credit_limit
        assert_eq!(
            validate_adjustment_amount_and_overdraft(
                d("15.000000"),
                AdjustmentDirection::Debit,
                d("10.000000"),
                d("5.000000"),
                false
            )
            .expect("exact credit limit boundary allowed"),
            d("-5.000000")
        );
        assert!(
            validate_adjustment_amount_and_overdraft(
                d("15.000001"),
                AdjustmentDirection::Debit,
                d("10.000000"),
                d("5.000000"),
                false
            )
            .is_err()
        );

        // Super-admin reversal override permits crossing -credit_limit
        assert_eq!(
            validate_adjustment_amount_and_overdraft(
                d("15.000001"),
                AdjustmentDirection::Debit,
                d("10.000000"),
                d("5.000000"),
                true
            )
            .expect("super-admin reversal allowed"),
            d("-5.000001")
        );
    }

    #[test]
    fn lease_fencing_filters_out_stale_queue_and_already_charged_usage() {
        let q1 = Uuid::new_v4();
        let q2_stale = Uuid::new_v4();
        let q3_already_charged = Uuid::new_v4();
        let ev1 = Uuid::new_v4();

        let items = [
            FencedChargeItem {
                queue_id: q1,
                lease_seq: 2,
                usage_record_id: 101,
                amount: d("0.001000"),
            },
            FencedChargeItem {
                queue_id: q2_stale,
                lease_seq: 1,
                usage_record_id: 102,
                amount: d("0.002000"),
            },
            FencedChargeItem {
                queue_id: q3_already_charged,
                lease_seq: 1,
                usage_record_id: 103,
                amount: d("0.003000"),
            },
        ];

        // q2_stale failed queue fence; q3 passed queue fence but usage_record 103 was already 'charged'
        let fenced_queues = HashSet::from([q1, q3_already_charged]);
        let newly_charged_usage = HashMap::from([(101_i64, Some(ev1))]);

        let chargeable =
            filter_fenced_chargeable_items(&items, &fenced_queues, &newly_charged_usage);
        assert_eq!(chargeable.len(), 1);
        assert_eq!(chargeable[0], (q1, 101, d("0.001000"), ev1));
    }

    #[test]
    fn fencing_and_bill_sql_invariants() {
        assert!(
            FENCE_SINGLE_QUEUE_SQL
                .contains("WHERE id = $1 AND status = 'processing' AND lease_seq = $2")
        );
        assert!(
            FENCE_BATCH_QUEUE_SQL
                .contains("WHERE q.id = u.id AND q.status = 'processing' AND q.lease_seq = u.seq")
        );
        assert!(MARK_SINGLE_USAGE_CHARGED_SQL.contains("charge_status != 'charged'"));
        assert!(MARK_BATCH_USAGE_CHARGED_SQL.contains("r.charge_status != 'charged'"));
        for sql in [BILL_USAGE_TOTALS_SQL, BILL_SNAPSHOT_ITEMS_SQL] {
            assert!(sql.contains("charge_status = 'charged'"));
            assert!(sql.contains("billable_amount > 0"));
            assert!(sql.contains("recorded_at >= $2"));
            assert!(sql.contains("recorded_at < $3"));
            assert!(!sql.contains("recorded_at <="));
            assert!(!sql.contains("response_status < 400"));
        }
    }
}
