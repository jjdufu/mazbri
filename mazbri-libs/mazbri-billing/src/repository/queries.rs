use super::mapping::{
    map_account_balance_row, map_bill_info, map_bill_line_item, map_usage_breakdown_item,
    map_usage_summary,
};
use super::transactions;
use super::{BillingRepository, DeductAndRecordResult, PgBillingRepository};
use crate::service::{AddFundsResult, UsageOutboxEvent, UsageRecord};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mazbri_core::CoreError;
use rust_decimal::Decimal;
use sqlx::Row;
use uuid::Uuid;
#[async_trait]
impl BillingRepository for PgBillingRepository {
    async fn get_account_id(&self, tenant_id: Uuid) -> Result<Option<Uuid>, CoreError> {
        let result =
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM tenant_accounts WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_optional(&*self.read_pool)
                .await
                .map_err(Self::map_error)?;
        Ok(result)
    }
    async fn record_usage(&self, record: &UsageRecord) -> Result<i64, CoreError> {
        let row = sqlx::query(
            "INSERT INTO usage_records (
                tenant_id, account_id, api_endpoint, http_method, response_status,
                request_size, response_size, billing_service_duration_ms,
                unit_price, billable_amount, billable_units,
                product_id, billing_code, event_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
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
        .bind(record.billing_result.amount)
        .bind(i64::try_from(record.billing_result.units).map_err(|e| {
            CoreError::Validation(format!(
                "Billing units {} exceeds maximum representable value: {}",
                record.billing_result.units, e
            ))
        })?)
        .bind(&record.product_id)
        .bind(record.billing_code)
        .bind(record.event_id)
        .fetch_one(&*self.pool)
        .await
        .map_err(Self::map_error)?;
        Ok(row.get("id"))
    }
    async fn insert_usage_outbox(&self, event: &UsageOutboxEvent) -> Result<(), CoreError> {
        outbox_insert_query(event)?
            .execute(&*self.pool)
            .await
            .map_err(Self::map_error)?;
        Ok(())
    }
    async fn check_balance(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<super::AccountBalanceRow>, CoreError> {
        self.get_balance(tenant_id).await
    }
    async fn get_balance(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<super::AccountBalanceRow>, CoreError> {
        let row = sqlx::query(
            "SELECT balance, credit_limit, available_balance, billing_cycle
             FROM tenant_accounts
             WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_optional(&*self.read_pool)
        .await
        .map_err(Self::map_error)?;
        Ok(row.map(|r| map_account_balance_row(&r)))
    }
    async fn get_bill(
        &self,
        tenant_id: Uuid,
        bill_id: Uuid,
    ) -> Result<Option<crate::service::BillInfo>, CoreError> {
        let row = sqlx::query(
            "SELECT id, tenant_id, status, total_amount, created_at, paid_at
             FROM bills WHERE id = $1 AND tenant_id = $2",
        )
        .bind(bill_id)
        .bind(tenant_id)
        .fetch_optional(&*self.read_pool)
        .await
        .map_err(Self::map_error)?;
        Ok(row.map(|r| map_bill_info(&r)))
    }
    async fn list_bills(
        &self,
        tenant_id: Uuid,
        page_size: i32,
        start_time: Option<i64>,
        end_time: Option<i64>,
        status: Option<&str>,
    ) -> Result<Vec<crate::service::BillInfo>, CoreError> {
        const MAX_PAGE_SIZE: i32 = 100;
        if page_size <= 0 {
            return Err(CoreError::Validation(
                "page_size must be a positive integer".to_string(),
            ));
        }
        if page_size > MAX_PAGE_SIZE {
            return Err(CoreError::Validation(format!(
                "page_size must not exceed {MAX_PAGE_SIZE}"
            )));
        }
        let mut conditions = vec!["tenant_id = $1".to_string()];
        let mut param_count = 1;
        let start_filter = if let Some(start) = start_time {
            param_count += 1;
            conditions.push(format!("created_at >= to_timestamp(${param_count})"));
            Some(start)
        } else {
            None
        };
        let end_filter = if let Some(end) = end_time {
            param_count += 1;
            conditions.push(format!("created_at <= to_timestamp(${param_count})"));
            Some(end)
        } else {
            None
        };
        let status_filter = if let Some(s) = status {
            if s.is_empty() {
                None
            } else {
                param_count += 1;
                conditions.push(format!("status = ${param_count}"));
                Some(s.to_string())
            }
        } else {
            None
        };
        param_count += 1;
        let limit_param = param_count;
        let query = format!(
            "SELECT id, tenant_id, status, total_amount, created_at, paid_at
             FROM bills
             WHERE {}
             ORDER BY created_at DESC
             LIMIT ${}",
            conditions.join(" AND "),
            limit_param
        );
        let mut q = sqlx::query(&query).bind(tenant_id);
        if let Some(start) = start_filter {
            q = q.bind(start);
        }
        if let Some(end) = end_filter {
            q = q.bind(end);
        }
        if let Some(ref s) = status_filter {
            q = q.bind(s);
        }
        q = q.bind(i64::from(page_size));
        let rows = q
            .fetch_all(&*self.read_pool)
            .await
            .map_err(Self::map_error)?;
        Ok(rows.into_iter().map(|r| map_bill_info(&r)).collect())
    }
    async fn get_usage_summary(
        &self,
        tenant_id: Uuid,
        start_time: i64,
        end_time: i64,
    ) -> Result<crate::service::UsageSummaryInfo, CoreError> {
        let row = sqlx::query(
            "SELECT
                COUNT(*) as total_requests,
                COALESCE(SUM(request_size), 0)::BIGINT as total_bytes_in,
                COALESCE(SUM(response_size), 0)::BIGINT as total_bytes_out,
                COALESCE(SUM(billing_service_duration_ms), 0)::BIGINT as total_billing_service_duration_ms,
                COALESCE(SUM(billable_amount), 0) as total_cost
             FROM usage_records
             WHERE tenant_id = $1
             AND recorded_at >= to_timestamp($2)
             AND recorded_at <= to_timestamp($3)",
        )
        .bind(tenant_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_one(&*self.read_pool)
        .await
        .map_err(Self::map_error)?;
        Ok(map_usage_summary(&row))
    }
    async fn get_bill_items(
        &self,
        tenant_id: Uuid,
        bill_id: Uuid,
    ) -> Result<Vec<crate::service::BillLineItem>, CoreError> {
        let bill_row = sqlx::query(
            "SELECT billing_period_start, billing_period_end
             FROM bills
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(bill_id)
        .bind(tenant_id)
        .fetch_optional(&*self.read_pool)
        .await
        .map_err(Self::map_error)?;
        let Some(bill_row) = bill_row else {
            return Err(CoreError::NotFound {
                resource: "bill".to_string(),
                id: bill_id.to_string(),
            });
        };
        let period_start: chrono::DateTime<chrono::Utc> = bill_row.get("billing_period_start");
        let period_end: chrono::DateTime<chrono::Utc> = bill_row.get("billing_period_end");
        let rows = sqlx::query(
            "SELECT
                api_endpoint,
                COUNT(*) as call_count,
                COALESCE(SUM(billable_units), 0) as total_units,
                COALESCE(SUM(billable_amount), 0) as total_amount,
                COALESCE(MIN(unit_price), 0) as unit_price
             FROM usage_records
             WHERE tenant_id = $1
             AND recorded_at >= $2
             AND recorded_at <= $3
             GROUP BY api_endpoint
             ORDER BY total_amount DESC",
        )
        .bind(tenant_id)
        .bind(period_start)
        .bind(period_end)
        .fetch_all(&*self.read_pool)
        .await
        .map_err(Self::map_error)?;
        Ok(rows.into_iter().map(|r| map_bill_line_item(&r)).collect())
    }
    async fn get_usage_breakdown(
        &self,
        tenant_id: Uuid,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<crate::service::UsageBreakdownItem>, CoreError> {
        let total_row = sqlx::query(
            "SELECT COALESCE(SUM(billable_amount), 0) as total_cost
             FROM usage_records
             WHERE tenant_id = $1
             AND recorded_at >= to_timestamp($2)
             AND recorded_at <= to_timestamp($3)",
        )
        .bind(tenant_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_one(&*self.read_pool)
        .await
        .map_err(Self::map_error)?;
        let total_cost: Decimal = total_row.get("total_cost");
        let rows = sqlx::query(
            "SELECT
                api_endpoint as category,
                COUNT(*) as request_count,
                COALESCE(SUM(billable_amount), 0) as cost
             FROM usage_records
             WHERE tenant_id = $1
             AND recorded_at >= to_timestamp($2)
             AND recorded_at <= to_timestamp($3)
             GROUP BY api_endpoint
             ORDER BY cost DESC",
        )
        .bind(tenant_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(&*self.read_pool)
        .await
        .map_err(Self::map_error)?;
        Ok(rows
            .into_iter()
            .map(|r| map_usage_breakdown_item(&r, total_cost))
            .collect())
    }
    async fn account_exists(&self, tenant_id: Uuid) -> Result<bool, CoreError> {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tenant_accounts WHERE tenant_id = $1)")
                .bind(tenant_id)
                .fetch_one(&*self.read_pool)
                .await
                .map_err(Self::map_error)?;
        Ok(exists)
    }
    async fn deduct_balance(&self, tenant_id: Uuid, amount: Decimal) -> Result<Decimal, CoreError> {
        transactions::deduct_balance(self, tenant_id, amount).await
    }
    async fn add_funds(
        &self,
        tenant_id: Uuid,
        amount: Decimal,
        payment_method: &str,
    ) -> Result<AddFundsResult, CoreError> {
        transactions::add_funds(self, tenant_id, amount, payment_method).await
    }
    async fn generate_bill(
        &self,
        tenant_id: Uuid,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Result<Uuid, CoreError> {
        transactions::generate_bill(self, tenant_id, period_start, period_end).await
    }
    async fn deduct_and_record(
        &self,
        tenant_id: Uuid,
        amount: Decimal,
        account_id: Uuid,
        usage_record_id: i64,
        queue_id: Uuid,
    ) -> Result<DeductAndRecordResult, CoreError> {
        transactions::deduct_and_record(
            self,
            tenant_id,
            amount,
            account_id,
            usage_record_id,
            queue_id,
        )
        .await
    }
    async fn deduct_and_record_batch(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        items: &[(Uuid, i64, Decimal)],
    ) -> Result<DeductAndRecordResult, CoreError> {
        transactions::deduct_and_record_batch(self, tenant_id, account_id, items).await
    }
    async fn create_adjustment(
        &self,
        tenant_id: Uuid,
        amount: Decimal,
        direction: &str,
        description: &str,
    ) -> Result<Uuid, CoreError> {
        transactions::create_adjustment(self, tenant_id, amount, direction, description).await
    }
}

/// Idempotent outbox insert (`ON CONFLICT (event_id) DO NOTHING`); usable with any executor
/// (pool or transaction). Fingerprint-based transport retries and Redpanda redeliveries share
/// `event_id`, so a duplicate can never create a second charge.
pub(crate) fn outbox_insert_query(
    event: &UsageOutboxEvent,
) -> Result<sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>, CoreError> {
    let units = i64::try_from(event.billing_result.units).map_err(|e| {
        CoreError::Validation(format!(
            "Billing units {} exceeds maximum representable value: {}",
            event.billing_result.units, e
        ))
    })?;
    Ok(sqlx::query(
        "INSERT INTO usage_outbox (
            event_id, request_id, tenant_id, account_id, product_id,
            api_endpoint, http_method, response_status, request_size,
            response_size, billing_service_duration_ms, unit_price,
            billable_units, billable_amount, billing_code
        ) VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8, $9,
            $10, $11, $12,
            $13, $14, $15
        )
        ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(event.event_id)
    .bind(event.request_id)
    .bind(event.tenant_id)
    .bind(event.account_id)
    .bind(&event.product_id)
    .bind(&event.api_endpoint)
    .bind(&event.http_method)
    .bind(i32::from(event.response_status))
    .bind(event.request_size)
    .bind(event.response_size)
    .bind(event.billing_service_duration_ms)
    .bind(event.billing_result.unit_price)
    .bind(units)
    .bind(event.billing_result.amount)
    .bind(event.billing_code))
}
