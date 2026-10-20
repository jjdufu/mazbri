use super::{
    AnalyticsHttpClient, BTreeMap, BillingError, DateTime, Decimal, HashMap, PgPool, Row,
    UpstreamBillItemDraft, UpstreamBillScope, UpstreamBillScopeKind, Utc, error, repo,
};

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn generate_upstream_bill(
    billing_db: &PgPool,
    platform_db: &PgPool,
    analytics_client: &AnalyticsHttpClient,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    scope: Option<UpstreamBillScope>,
) -> Result<uuid::Uuid, BillingError> {
    if period_start >= period_end {
        return Err(BillingError::Validation(
            "Billing period start must be before end".to_string(),
        ));
    }

    let (supplier_id_filter, channel_id_filter) = match scope {
        Some(UpstreamBillScope {
            kind: UpstreamBillScopeKind::Channel,
            scope_id,
        }) => (None, Some(scope_id.to_string())),
        Some(UpstreamBillScope {
            kind: UpstreamBillScopeKind::Supplier,
            scope_id,
        }) => (Some(scope_id.to_string()), None),
        None => (None, None),
    };

    let summary_val = analytics_client
        .fetch_upstream_usage_summary(
            &period_start.to_rfc3339(),
            &period_end.to_rfc3339(),
            None,
            None,
            supplier_id_filter.as_deref(),
            channel_id_filter.as_deref(),
        )
        .await
        .map_err(|e| {
            error!("Failed to fetch upstream usage summary: {}", e);
            BillingError::Database
        })?;

    let rows = summary_val.as_array().cloned().unwrap_or_default();

    if rows.is_empty() {
        return Err(BillingError::Validation(
            "No upstream request logs found in this billing period".to_string(),
        ));
    }

    let channel_ids = rows
        .iter()
        .filter_map(|row| row.get("channel_id").and_then(|v| v.as_str()))
        .filter_map(|v| uuid::Uuid::parse_str(v).ok())
        .collect::<Vec<_>>();
    let channel_codes = rows
        .iter()
        .filter_map(|row| row.get("channel_code").and_then(|v| v.as_str()))
        .filter(|v| !v.trim().is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    let channel_map = load_channel_pricing_map(platform_db, &channel_ids, &channel_codes).await?;
    let mut drafts: BTreeMap<String, UpstreamBillItemDraft> = BTreeMap::new();
    let mut total_data_bytes: i64 = 0;

    for row in &rows {
        let supplier_code = string_field(row, "supplier_code");
        let supplier_name =
            first_non_empty(&[string_field(row, "supplier_name"), supplier_code.clone()]);
        let channel_key = first_non_empty(&[
            string_field(row, "channel_id"),
            string_field(row, "channel_code"),
        ]);
        if channel_key.is_empty() {
            continue;
        }
        let channel_code =
            first_non_empty(&[string_field(row, "channel_code"), channel_key.clone()]);
        let channel_name =
            first_non_empty(&[string_field(row, "channel_name"), channel_code.clone()]);
        let unit_price = channel_map.get(&channel_key).copied().unwrap_or_default();
        let total_calls = json_i64(row.get("total_calls"));
        let success_calls = json_i64(row.get("success_calls"));
        let failed_calls = json_i64(row.get("failed_calls"));
        let total_bytes = json_i64(row.get("total_payload_bytes"));

        let settlement_amount = Decimal::from(total_calls) * unit_price;
        let _ = repo::upsert_hourly_settlement(
            billing_db,
            repo::UpsertHourlySettlementParams {
                window_start: period_start,
                supplier_code: &supplier_code,
                channel_code: &channel_code,
                total_calls,
                successful_calls: success_calls,
                failed_calls,
                total_bytes,
                unit_price,
                settlement_amount,
            },
        )
        .await;

        let entry = drafts
            .entry(channel_key.clone())
            .or_insert_with(|| UpstreamBillItemDraft {
                supplier_code: supplier_code.clone(),
                supplier_name: supplier_name.clone(),
                channel_key: channel_key.clone(),
                channel_code: channel_code.clone(),
                channel_name: channel_name.clone(),
                unit_price,
                quantity: 0,
            });
        entry.quantity += total_calls;
        total_data_bytes = total_data_bytes.saturating_add(total_bytes);
        if entry.unit_price.is_zero() && !unit_price.is_zero() {
            entry.unit_price = unit_price;
        }
    }

    if drafts.is_empty() {
        return Err(BillingError::Validation(
            "No billable upstream request logs found in this billing period".to_string(),
        ));
    }

    let total_calls: i64 = drafts.values().map(|item| item.quantity).sum();
    let total_amount = drafts.values().fold(Decimal::ZERO, |acc, item| {
        acc + (Decimal::from(item.quantity) * item.unit_price)
    });

    let (bill_party_code, bill_party_name) = match scope {
        Some(UpstreamBillScope {
            kind: UpstreamBillScopeKind::Channel,
            ..
        }) => ("UPSTREAM_CHANNEL".to_string(), "上游通道账单".to_string()),
        Some(UpstreamBillScope {
            kind: UpstreamBillScopeKind::Supplier,
            ..
        }) => (
            "UPSTREAM_SUPPLIER".to_string(),
            "上游供应商账单".to_string(),
        ),
        None => ("UPSTREAM".to_string(), "全量上游账单".to_string()),
    };
    let bill_party_id = scope.map(|item| item.scope_id);

    let mut tx = billing_db.begin().await.map_err(|e| {
        error!("Database error starting upstream bill transaction: {}", e);
        BillingError::Database
    })?;

    let existing_bill: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM bills
         WHERE bill_kind = 'upstream'
           AND party_type = 'platform'
           AND billing_period_start = $1
           AND billing_period_end = $2
           AND (
                ($3::uuid IS NOT NULL AND (party_id = $3 OR (party_id IS NULL AND party_code = $4)))
                OR ($3::uuid IS NULL AND party_id IS NULL AND party_code = $4)
           )
         LIMIT 1",
    )
    .bind(period_start)
    .bind(period_end)
    .bind(bill_party_id)
    .bind(&bill_party_code)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        error!("Database error querying existing upstream bill: {}", e);
        BillingError::Database
    })?;

    let bill_id = if let Some(bill_id) = existing_bill {
        sqlx::query("DELETE FROM bill_items WHERE bill_id = $1")
            .bind(bill_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                error!("Database error clearing upstream bill items: {}", e);
                BillingError::Database
            })?;
        sqlx::query(
            "UPDATE bills
             SET party_id = $2,
                 party_code = $3,
                 party_name = $4,
                 total_calls = $5,
                 total_data_bytes = $6,
                 base_amount = $7,
                 discount_amount = 0,
                 tax_amount = 0,
                 total_amount = $8,
                 updated_at = NOW()
             WHERE id = $1",
        )
        .bind(bill_id)
        .bind(bill_party_id)
        .bind(&bill_party_code)
        .bind(&bill_party_name)
        .bind(total_calls)
        .bind(total_data_bytes)
        .bind(total_amount)
        .bind(total_amount)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            error!("Database error updating upstream bill: {}", e);
            BillingError::Database
        })?;
        bill_id
    } else {
        sqlx::query_scalar(
            "INSERT INTO bills (
                bill_kind, party_type, party_id, party_code, party_name,
                billing_period_start, billing_period_end,
                total_calls, total_data_bytes,
                base_amount, discount_amount, tax_amount, total_amount,
                status
            ) VALUES (
                'upstream', 'platform', $1, $2, $3,
                $4, $5,
                $6, $7,
                $8, 0, 0, $9,
                'pending'
            ) RETURNING id",
        )
        .bind(bill_party_id)
        .bind(&bill_party_code)
        .bind(&bill_party_name)
        .bind(period_start)
        .bind(period_end)
        .bind(total_calls)
        .bind(total_data_bytes)
        .bind(total_amount)
        .bind(total_amount)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            error!("Database error inserting upstream bill: {}", e);
            BillingError::Database
        })?
    };

    for (index, item) in drafts.values().enumerate() {
        let total = Decimal::from(item.quantity) * item.unit_price;
        sqlx::query(
            "INSERT INTO bill_items (
                bill_id, item_key, description, billing_type,
                quantity, total_units, unit_price, total, metadata
             ) VALUES ($1, $2, $3, 'call', $4, $5, $6, $7, $8)",
        )
        .bind(bill_id)
        .bind(format!("upstream-{}", index + 1))
        .bind(format!("{} / {}", item.supplier_name, item.channel_name))
        .bind(item.quantity)
        .bind(item.quantity)
        .bind(item.unit_price)
        .bind(total)
        .bind(serde_json::json!(
            { "supplier_code" : item.supplier_code, "channel_key" : item
            .channel_key, "channel_code" : item.channel_code, "channel_name" :
            item.channel_name, }
        ))
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            error!("Database error inserting upstream bill item: {}", e);
            BillingError::Database
        })?;
    }

    tx.commit().await.map_err(|e| {
        error!("Database error committing upstream bill: {}", e);
        BillingError::Database
    })?;

    Ok(bill_id)
}

pub(super) async fn load_channel_pricing_map(
    billing_db: &PgPool,
    channel_ids: &[uuid::Uuid],
    channel_codes: &[String],
) -> Result<HashMap<String, Decimal>, BillingError> {
    let mut map = HashMap::new();
    if !channel_ids.is_empty() {
        let rows = sqlx::query(
            "SELECT c.id, c.channel_code, c.cost_price
             FROM channels c
             WHERE c.id = ANY($1)",
        )
        .bind(channel_ids)
        .fetch_all(billing_db)
        .await
        .map_err(|e| {
            error!("Failed to load channel pricing map by id: {}", e);
            BillingError::Database
        })?;
        for row in rows {
            let id: uuid::Uuid = row.get("id");
            let channel_code: String = row.get("channel_code");
            let cost_price: Decimal = row.get("cost_price");
            map.insert(id.to_string(), cost_price);
            map.insert(channel_code, cost_price);
        }
    }
    if !channel_codes.is_empty() {
        let rows = sqlx::query(
            "SELECT c.id, c.channel_code, c.cost_price
             FROM channels c
             WHERE c.channel_code = ANY($1)",
        )
        .bind(channel_codes)
        .fetch_all(billing_db)
        .await
        .map_err(|e| {
            error!("Failed to load channel pricing map by code: {}", e);
            BillingError::Database
        })?;
        for row in rows {
            let id: uuid::Uuid = row.get("id");
            let channel_code: String = row.get("channel_code");
            let cost_price: Decimal = row.get("cost_price");
            map.insert(id.to_string(), cost_price);
            map.insert(channel_code, cost_price);
        }
    }
    Ok(map)
}

pub(super) fn first_non_empty(values: &[String]) -> String {
    values
        .iter()
        .find(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}

pub(super) fn json_i64(value: Option<&serde_json::Value>) -> i64 {
    value
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_u64().and_then(|n| i64::try_from(n).ok()))
                .or_else(|| v.as_f64().map(mazbri_core::num::f64_to_i64))
        })
        .unwrap_or(0)
}

pub(super) fn string_field(row: &serde_json::Value, key: &str) -> String {
    row.get(key)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}
