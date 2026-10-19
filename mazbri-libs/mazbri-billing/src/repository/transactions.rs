use super::{DeductAndRecordResult, PgBillingRepository};
use crate::service::AddFundsResult;
use chrono::{DateTime, Utc};
use mazbri_core::CoreError;
use rust_decimal::Decimal;
use sqlx::{Postgres, Transaction};
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
            r#"
        INSERT INTO mazbri_billing.ledger_entries 
        (tenant_id, interaction_event_id, entry_type, amount, direction, balance_before, balance_after, description)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
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
            "Insufficient balance: required {}, tenant {}",
            amount, tenant_id
        ))
    })?;
    let transaction_id = Uuid::new_v4();
    let balance_before = new_balance + amount;
    sqlx::query(
        r#"INSERT INTO transactions
           (id, tenant_id, account_id, transaction_type, amount,
            balance_before, balance_after, status)
           VALUES ($1, $2, $3, 'charge', $4, $5, $6, 'completed')"#,
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
    use sqlx::Row;
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
    .fetch_one(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    use sqlx::Row;
    let total_calls: i64 = usage_row.get("total_calls");
    let total_data_bytes: i64 = usage_row.get("total_data_bytes");
    let base_amount: Decimal = usage_row.get("base_amount");
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
pub(crate) async fn deduct_and_record(
    repo: &PgBillingRepository,
    tenant_id: Uuid,
    amount: Decimal,
    account_id: Uuid,
    usage_record_id: i64,
    queue_id: Uuid,
) -> Result<DeductAndRecordResult, CoreError> {
    if amount <= Decimal::ZERO {
        let row = sqlx::query("SELECT balance FROM tenant_accounts WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_optional(&*repo.pool)
            .await
            .map_err(PgBillingRepository::map_error)?;
        use sqlx::Row;
        let balance: Decimal = row.map(|r| r.get("balance")).unwrap_or(Decimal::ZERO);
        return Ok(DeductAndRecordResult {
            transaction_id: Uuid::nil(),
            balance_before: balance,
            balance_after: balance,
        });
    }
    let mut tx = repo
        .pool
        .begin()
        .await
        .map_err(PgBillingRepository::map_error)?;
    let row = sqlx::query(
        "UPDATE tenant_accounts
         SET balance = balance - $1
         WHERE tenant_id = $2 AND balance + credit_limit >= $1
         RETURNING balance + $1 AS balance_before, balance AS balance_after",
    )
    .bind(amount)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    use sqlx::Row;
    let (balance_before, balance_after) = match row {
        Some(r) => {
            let before: Decimal = r.get("balance_before");
            let after: Decimal = r.get("balance_after");
            (before, after)
        }
        None => {
            return Err(CoreError::Validation(format!(
                "Insufficient balance: required {}, tenant {}",
                amount, tenant_id
            )));
        }
    };
    let transaction_id = Uuid::new_v4();
    let event_id: Option<Uuid> =
        sqlx::query_scalar("SELECT event_id FROM usage_records WHERE id = $1")
            .bind(usage_record_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(PgBillingRepository::map_error)?;
    sqlx::query(
        r#"INSERT INTO transactions
           (id, tenant_id, account_id, transaction_type, amount,
            balance_before, balance_after, reference_type, reference_id,
            description, status)
           VALUES ($1, $2, $3, 'charge', $4, $5, $6, 'usage', $7, $8, 'completed')"#,
    )
    .bind(transaction_id)
    .bind(tenant_id)
    .bind(account_id)
    .bind(amount)
    .bind(balance_before)
    .bind(balance_after)
    .bind(usage_record_id.to_string())
    .bind(format!("Usage charge for record {}", usage_record_id))
    .execute(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    insert_ledger_entry(
        &mut tx,
        LedgerEntry {
            tenant_id,
            event_id,
            entry_type: "CHARGE",
            amount,
            direction: "DEBIT",
            balance_before,
            balance_after,
            description: Some(&format!("Usage record {}", usage_record_id)),
        },
    )
    .await?;
    sqlx::query(
        r#"UPDATE usage_charge_queue
           SET status = 'completed', transaction_id = $2, processed_at = NOW()
           WHERE id = $1"#,
    )
    .bind(queue_id)
    .bind(transaction_id)
    .execute(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    sqlx::query(
        "UPDATE usage_records SET charge_status = 'charged', charged_at = NOW() \
         WHERE charge_queue_id = $1",
    )
    .bind(queue_id)
    .execute(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?;
    tx.commit().await.map_err(PgBillingRepository::map_error)?;
    Ok(DeductAndRecordResult {
        transaction_id,
        balance_before,
        balance_after,
    })
}

/// One-tenant batch charge: one balance row UPDATE for the sum, then per-item bookkeeping.
/// Eliminates N serial row-lock round-trips under single-tenant high QPS.
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
    let total: Decimal = items.iter().map(|(_, _, amount)| *amount).sum();
    if total <= Decimal::ZERO {
        let row = sqlx::query("SELECT balance FROM tenant_accounts WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_optional(&*repo.pool)
            .await
            .map_err(PgBillingRepository::map_error)?;
        use sqlx::Row;
        let balance: Decimal = row.map(|r| r.get("balance")).unwrap_or(Decimal::ZERO);
        return Ok(DeductAndRecordResult {
            transaction_id: Uuid::nil(),
            balance_before: balance,
            balance_after: balance,
        });
    }

    let mut tx = repo
        .pool
        .begin()
        .await
        .map_err(PgBillingRepository::map_error)?;

    let row = sqlx::query(
        "UPDATE tenant_accounts
         SET balance = balance - $1
         WHERE tenant_id = $2 AND balance + credit_limit >= $1
         RETURNING balance + $1 AS balance_before, balance AS balance_after",
    )
    .bind(total)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(PgBillingRepository::map_error)?;

    use sqlx::Row;
    let (balance_before_batch, balance_after_final) = match row {
        Some(r) => {
            let before: Decimal = r.get("balance_before");
            let after: Decimal = r.get("balance_after");
            (before, after)
        }
        None => {
            return Err(CoreError::Validation(format!(
                "Insufficient balance: required {}, tenant {}",
                total, tenant_id
            )));
        }
    };

    let mut cursor = balance_before_batch;
    let mut last_tx_id = Uuid::nil();
    for (queue_id, usage_record_id, amount) in items {
        if *amount <= Decimal::ZERO {
            sqlx::query(
                r#"UPDATE usage_charge_queue
                   SET status = 'completed', processed_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(queue_id)
            .execute(&mut *tx)
            .await
            .map_err(PgBillingRepository::map_error)?;
            sqlx::query(
                "UPDATE usage_records SET charge_status = 'charged', charged_at = NOW() \
                 WHERE charge_queue_id = $1",
            )
            .bind(queue_id)
            .execute(&mut *tx)
            .await
            .map_err(PgBillingRepository::map_error)?;
            continue;
        }

        let item_before = cursor;
        let item_after = cursor - *amount;
        cursor = item_after;

        let transaction_id = Uuid::new_v4();
        last_tx_id = transaction_id;
        let event_id: Option<Uuid> =
            sqlx::query_scalar("SELECT event_id FROM usage_records WHERE id = $1")
                .bind(usage_record_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(PgBillingRepository::map_error)?;

        sqlx::query(
            r#"INSERT INTO transactions
               (id, tenant_id, account_id, transaction_type, amount,
                balance_before, balance_after, reference_type, reference_id,
                description, status)
               VALUES ($1, $2, $3, 'charge', $4, $5, $6, 'usage', $7, $8, 'completed')"#,
        )
        .bind(transaction_id)
        .bind(tenant_id)
        .bind(account_id)
        .bind(amount)
        .bind(item_before)
        .bind(item_after)
        .bind(usage_record_id.to_string())
        .bind(format!("Usage charge for record {usage_record_id}"))
        .execute(&mut *tx)
        .await
        .map_err(PgBillingRepository::map_error)?;

        insert_ledger_entry(
            &mut tx,
            LedgerEntry {
                tenant_id,
                event_id,
                entry_type: "CHARGE",
                amount: *amount,
                direction: "DEBIT",
                balance_before: item_before,
                balance_after: item_after,
                description: Some(&format!("Usage record {usage_record_id}")),
            },
        )
        .await?;

        sqlx::query(
            r#"UPDATE usage_charge_queue
               SET status = 'completed', transaction_id = $2, processed_at = NOW()
               WHERE id = $1"#,
        )
        .bind(queue_id)
        .bind(transaction_id)
        .execute(&mut *tx)
        .await
        .map_err(PgBillingRepository::map_error)?;

        sqlx::query(
            "UPDATE usage_records SET charge_status = 'charged', charged_at = NOW() \
             WHERE charge_queue_id = $1",
        )
        .bind(queue_id)
        .execute(&mut *tx)
        .await
        .map_err(PgBillingRepository::map_error)?;
    }

    debug_assert_eq!(cursor, balance_after_final);

    tx.commit().await.map_err(PgBillingRepository::map_error)?;
    Ok(DeductAndRecordResult {
        transaction_id: last_tx_id,
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
    use sqlx::Row;
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
