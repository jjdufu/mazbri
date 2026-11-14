use super::{
    AnalyticsHttpClient, BTreeMap, BillingError, DateTime, Decimal, HashMap, PgPool, Row,
    UpstreamBillItemDraft, UpstreamBillScope, UpstreamBillScopeKind, Utc, error, repo,
};
use chrono::Timelike;

#[derive(Debug, Clone)]
pub(super) struct ChannelPricingInfo {
    pub cost_price: Decimal,
    pub channel_code: String,
    pub supplier_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HourlySettlementBucket {
    pub window_start: DateTime<Utc>,
    pub supplier_code: String,
    pub channel_code: String,
    pub total_calls: i64,
    pub successful_calls: i64,
    pub failed_calls: i64,
    pub total_bytes: i64,
    pub unit_price: Decimal,
    pub settlement_amount: Decimal,
}

#[must_use]
pub fn floor_to_utc_hour(dt: DateTime<Utc>) -> DateTime<Utc> {
    let secs = dt.timestamp().div_euclid(3600) * 3600;
    DateTime::<Utc>::from_timestamp(secs, 0).unwrap_or(dt)
}

#[must_use]
pub fn ceil_to_utc_hour(dt: DateTime<Utc>) -> DateTime<Utc> {
    let floor = floor_to_utc_hour(dt);
    if dt > floor {
        floor + chrono::Duration::hours(1)
    } else {
        floor
    }
}

#[must_use]
fn is_utc_hour_aligned(dt: DateTime<Utc>) -> bool {
    dt.minute() == 0 && dt.second() == 0 && dt.nanosecond() == 0
}

#[must_use]
fn normalize_bill_exclusive_end(period_end: DateTime<Utc>) -> DateTime<Utc> {
    if period_end.minute() == 59 && period_end.second() == 59 {
        DateTime::<Utc>::from_timestamp(period_end.timestamp() + 1, 0).unwrap_or(period_end)
    } else {
        period_end
    }
}

fn collect_channel_keys_from_rows(rows: &[serde_json::Value]) -> (Vec<uuid::Uuid>, Vec<String>) {
    let channel_ids = rows
        .iter()
        .filter_map(|row| row.get("channel_id").and_then(serde_json::Value::as_str))
        .filter_map(|v| uuid::Uuid::parse_str(v).ok())
        .collect::<Vec<_>>();
    let channel_codes = rows
        .iter()
        .filter_map(|row| row.get("channel_code").and_then(serde_json::Value::as_str))
        .filter(|v| !v.trim().is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    (channel_ids, channel_codes)
}

/// 将分析库按整点小时分桶的上游调用行聚合并关联通道成本单价，生成幂等结算桶。
#[must_use]
pub(super) fn aggregate_hourly_settlement_rows(
    rows: &[serde_json::Value],
    pricing_map: &HashMap<String, ChannelPricingInfo>,
) -> Vec<HourlySettlementBucket> {
    let mut grouped: BTreeMap<(DateTime<Utc>, String, String), HourlySettlementBucket> =
        BTreeMap::new();

    for row in rows {
        let Some(window_start) = row
            .get("window_start")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| floor_to_utc_hour(dt.with_timezone(&Utc)))
        else {
            continue;
        };

        let raw_channel_id = string_field(row, "channel_id");
        let raw_channel_code = string_field(row, "channel_code");
        let lookup_key = first_non_empty(&[raw_channel_id, raw_channel_code.clone()]);
        if lookup_key.is_empty() {
            continue;
        }

        let pricing = pricing_map.get(&lookup_key);
        let channel_code = first_non_empty(&[
            raw_channel_code,
            pricing
                .map(|p| p.channel_code.clone())
                .unwrap_or_default(),
            lookup_key,
        ]);
        let supplier_code = first_non_empty(&[
            string_field(row, "supplier_code"),
            pricing
                .map(|p| p.supplier_code.clone())
                .unwrap_or_default(),
        ]);
        if supplier_code.is_empty() || channel_code.is_empty() {
            continue;
        }

        let unit_price = pricing.map_or(Decimal::ZERO, |p| p.cost_price);
        let total_calls = json_i64(row.get("total_calls"));
        let success_calls = json_i64(row.get("success_calls"));
        let failed_calls = json_i64(row.get("failed_calls"));
        let total_bytes = json_i64(row.get("total_payload_bytes"));

        let entry = grouped
            .entry((window_start, supplier_code.clone(), channel_code.clone()))
            .or_insert_with(|| HourlySettlementBucket {
                window_start,
                supplier_code,
                channel_code,
                total_calls: 0,
                successful_calls: 0,
                failed_calls: 0,
                total_bytes: 0,
                unit_price,
                settlement_amount: Decimal::ZERO,
            });
        entry.total_calls = entry.total_calls.saturating_add(total_calls);
        entry.successful_calls = entry.successful_calls.saturating_add(success_calls);
        entry.failed_calls = entry.failed_calls.saturating_add(failed_calls);
        entry.total_bytes = entry.total_bytes.saturating_add(total_bytes);
        if entry.unit_price.is_zero() && !unit_price.is_zero() {
            entry.unit_price = unit_price;
        }
        entry.settlement_amount = Decimal::from(entry.total_calls) * entry.unit_price;
    }

    grouped.into_values().collect()
}

async fn persist_hourly_settlement_buckets(
    billing_db: &PgPool,
    buckets: &[HourlySettlementBucket],
) -> Result<(), BillingError> {
    for bucket in buckets {
        repo::upsert_hourly_settlement(
            billing_db,
            repo::UpsertHourlySettlementParams {
                window_start: bucket.window_start,
                supplier_code: &bucket.supplier_code,
                channel_code: &bucket.channel_code,
                total_calls: bucket.total_calls,
                successful_calls: bucket.successful_calls,
                failed_calls: bucket.failed_calls,
                total_bytes: bucket.total_bytes,
                unit_price: bucket.unit_price,
                settlement_amount: bucket.settlement_amount,
            },
        )
        .await
        .map_err(|e| {
            error!("Failed upserting upstream hourly settlement: {}", e);
            BillingError::Database
        })?;
    }
    Ok(())
}

/// 同步左闭右开整点窗口 `[window_start, window_end)` 内的上游通道小时成本至 `upstream_hourly_settlements` 并刷新 `mv_daily_revenue_cost`。
///
/// # Errors
///
/// 分析库查询、平台通道单价查询或计费库写入失败时返回错误。
pub async fn sync_upstream_cost_settlements(
    billing_db: &PgPool,
    platform_db: &PgPool,
    analytics_client: &AnalyticsHttpClient,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    purge_non_hour_aligned: bool,
) -> Result<usize, BillingError> {
    let from = floor_to_utc_hour(window_start);
    let to = ceil_to_utc_hour(window_end);
    if from >= to {
        return Ok(0);
    }

    if purge_non_hour_aligned {
        sqlx::query(
            "DELETE FROM upstream_hourly_settlements \
             WHERE date_trunc('hour', window_start AT TIME ZONE 'UTC') <> (window_start AT TIME ZONE 'UTC')",
        )
        .execute(billing_db)
        .await
        .map_err(|e| {
            error!("Failed cleaning non-hour-aligned settlements: {}", e);
            BillingError::Database
        })?;
    }

    let val = analytics_client
        .fetch_upstream_hourly_usage(&from.to_rfc3339(), &to.to_rfc3339(), None, None)
        .await
        .map_err(BillingError::Analytics)?;
    let rows = val.as_array().cloned().unwrap_or_default();
    let buckets = if rows.is_empty() {
        Vec::new()
    } else {
        let (channel_ids, channel_codes) = collect_channel_keys_from_rows(&rows);
        let pricing_map =
            load_channel_pricing_map(platform_db, &channel_ids, &channel_codes).await?;
        let built = aggregate_hourly_settlement_rows(&rows, &pricing_map);
        persist_hourly_settlement_buckets(billing_db, &built).await?;
        built
    };

    crate::services::finance_ops::refresh_daily_revenue_cost(billing_db).await?;
    Ok(buckets.len())
}

/// 按上游用量行汇总通道账单明细（不再将跨日 `period_start` 直接写入小时结算表，避免跨日翻倍）；返回 `(drafts, total_data_bytes)`。
fn build_upstream_drafts_from_pricing(
    rows: &[serde_json::Value],
    pricing_map: &HashMap<String, ChannelPricingInfo>,
) -> (BTreeMap<String, UpstreamBillItemDraft>, i64) {
    let mut drafts: BTreeMap<String, UpstreamBillItemDraft> = BTreeMap::new();
    let mut total_data_bytes: i64 = 0;

    for row in rows {
        let channel_key = first_non_empty(&[
            string_field(row, "channel_id"),
            string_field(row, "channel_code"),
        ]);
        if channel_key.is_empty() {
            continue;
        }
        let pricing = pricing_map.get(&channel_key);
        let supplier_code = first_non_empty(&[
            string_field(row, "supplier_code"),
            pricing
                .map(|p| p.supplier_code.clone())
                .unwrap_or_default(),
        ]);
        let supplier_name =
            first_non_empty(&[string_field(row, "supplier_name"), supplier_code.clone()]);
        let channel_code = first_non_empty(&[
            string_field(row, "channel_code"),
            pricing
                .map(|p| p.channel_code.clone())
                .unwrap_or_default(),
            channel_key.clone(),
        ]);
        let channel_name =
            first_non_empty(&[string_field(row, "channel_name"), channel_code.clone()]);
        let unit_price = pricing.map_or(Decimal::ZERO, |p| p.cost_price);
        let total_calls = json_i64(row.get("total_calls"));
        let total_bytes = json_i64(row.get("total_payload_bytes"));

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

    (drafts, total_data_bytes)
}

fn upstream_bill_party(scope: Option<UpstreamBillScope>) -> (String, String) {
    match scope {
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
    }
}

/// 上游账单头字段。
struct UpstreamBillHeader<'a> {
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    party_id: Option<uuid::Uuid>,
    party_code: &'a str,
    party_name: &'a str,
    total_calls: i64,
    total_data_bytes: i64,
    total_amount: Decimal,
}

/// 复用同周期同主体的已有账单（清空明细并更新），否则新建；返回账单 ID。
async fn upsert_upstream_bill_header(
    conn: &mut sqlx::PgConnection,
    h: &UpstreamBillHeader<'_>,
) -> Result<uuid::Uuid, BillingError> {
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
    .bind(h.period_start)
    .bind(h.period_end)
    .bind(h.party_id)
    .bind(h.party_code)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| {
        error!("Database error querying existing upstream bill: {}", e);
        BillingError::Database
    })?;

    let bill_id = if let Some(bill_id) = existing_bill {
        sqlx::query("DELETE FROM bill_items WHERE bill_id = $1")
            .bind(bill_id)
            .execute(&mut *conn)
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
        .bind(h.party_id)
        .bind(h.party_code)
        .bind(h.party_name)
        .bind(h.total_calls)
        .bind(h.total_data_bytes)
        .bind(h.total_amount)
        .bind(h.total_amount)
        .execute(&mut *conn)
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
        .bind(h.party_id)
        .bind(h.party_code)
        .bind(h.party_name)
        .bind(h.period_start)
        .bind(h.period_end)
        .bind(h.total_calls)
        .bind(h.total_data_bytes)
        .bind(h.total_amount)
        .bind(h.total_amount)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| {
            error!("Database error inserting upstream bill: {}", e);
            BillingError::Database
        })?
    };
    Ok(bill_id)
}

async fn insert_upstream_bill_items(
    conn: &mut sqlx::PgConnection,
    bill_id: uuid::Uuid,
    drafts: &BTreeMap<String, UpstreamBillItemDraft>,
) -> Result<(), BillingError> {
    for (index, item) in drafts.values().enumerate() {
        let total = Decimal::from(item.quantity) * item.unit_price;
        sqlx::query(
            "INSERT INTO bill_items (
                bill_id, item_key, description,
                quantity, total_units, unit_price, total, metadata
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
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
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            error!("Database error inserting upstream bill item: {}", e);
            BillingError::Database
        })?;
    }
    Ok(())
}

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

    let effective_end = normalize_bill_exclusive_end(period_end);
    let hour_aligned =
        is_utc_hour_aligned(period_start) && is_utc_hour_aligned(effective_end);

    let rows = if hour_aligned {
        let hourly_val = analytics_client
            .fetch_upstream_hourly_usage(
                &period_start.to_rfc3339(),
                &effective_end.to_rfc3339(),
                supplier_id_filter.as_deref(),
                channel_id_filter.as_deref(),
            )
            .await
            .map_err(BillingError::Analytics)?;
        hourly_val.as_array().cloned().unwrap_or_default()
    } else {
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
            .map_err(BillingError::Analytics)?;
        summary_val.as_array().cloned().unwrap_or_default()
    };

    if rows.is_empty() {
        return Err(BillingError::Validation(
            "No upstream request logs found in this billing period".to_string(),
        ));
    }

    let (channel_ids, channel_codes) = collect_channel_keys_from_rows(&rows);
    let pricing_map = load_channel_pricing_map(platform_db, &channel_ids, &channel_codes).await?;

    if hour_aligned {
        let buckets = aggregate_hourly_settlement_rows(&rows, &pricing_map);
        persist_hourly_settlement_buckets(billing_db, &buckets).await?;
        let _ = crate::services::finance_ops::refresh_daily_revenue_cost(billing_db).await;
    } else {
        let _ = sync_upstream_cost_settlements(
            billing_db,
            platform_db,
            analytics_client,
            period_start,
            effective_end,
            false,
        )
        .await;
    }

    let (drafts, total_data_bytes) = build_upstream_drafts_from_pricing(&rows, &pricing_map);

    if drafts.is_empty() {
        return Err(BillingError::Validation(
            "No billable upstream request logs found in this billing period".to_string(),
        ));
    }

    let total_calls: i64 = drafts.values().map(|item| item.quantity).sum();
    let total_amount = drafts.values().fold(Decimal::ZERO, |acc, item| {
        acc + (Decimal::from(item.quantity) * item.unit_price)
    });

    let (bill_party_code, bill_party_name) = upstream_bill_party(scope);
    let bill_party_id = scope.map(|item| item.scope_id);

    persist_upstream_bill(
        billing_db,
        &UpstreamBillHeader {
            period_start,
            period_end,
            party_id: bill_party_id,
            party_code: &bill_party_code,
            party_name: &bill_party_name,
            total_calls,
            total_data_bytes,
            total_amount,
        },
        &drafts,
    )
    .await
}

async fn persist_upstream_bill(
    billing_db: &PgPool,
    header: &UpstreamBillHeader<'_>,
    drafts: &BTreeMap<String, UpstreamBillItemDraft>,
) -> Result<uuid::Uuid, BillingError> {
    let mut tx = billing_db.begin().await.map_err(|e| {
        error!("Database error starting upstream bill transaction: {}", e);
        BillingError::Database
    })?;

    let bill_id = upsert_upstream_bill_header(&mut tx, header).await?;
    insert_upstream_bill_items(&mut tx, bill_id, drafts).await?;

    tx.commit().await.map_err(|e| {
        error!("Database error committing upstream bill: {}", e);
        BillingError::Database
    })?;

    Ok(bill_id)
}

pub(super) async fn load_channel_pricing_map(
    platform_db: &PgPool,
    channel_ids: &[uuid::Uuid],
    channel_codes: &[String],
) -> Result<HashMap<String, ChannelPricingInfo>, BillingError> {
    let mut map = HashMap::new();
    if !channel_ids.is_empty() {
        let rows = sqlx::query(
            "SELECT c.id, c.channel_code, c.cost_price, COALESCE(s.supplier_code, '') AS supplier_code
             FROM channels c
             LEFT JOIN suppliers s ON s.id = c.supplier_id
             WHERE c.id = ANY($1)",
        )
        .bind(channel_ids)
        .fetch_all(platform_db)
        .await
        .map_err(|e| {
            error!("Failed to load channel pricing map by id: {}", e);
            BillingError::Database
        })?;
        for row in rows {
            let id: uuid::Uuid = row.get("id");
            let channel_code: String = row.get("channel_code");
            let cost_price: Decimal = row.get("cost_price");
            let supplier_code: String = row.get("supplier_code");
            let info = ChannelPricingInfo {
                cost_price,
                channel_code: channel_code.clone(),
                supplier_code,
            };
            map.insert(id.to_string(), info.clone());
            map.insert(channel_code, info);
        }
    }
    if !channel_codes.is_empty() {
        let rows = sqlx::query(
            "SELECT c.id, c.channel_code, c.cost_price, COALESCE(s.supplier_code, '') AS supplier_code
             FROM channels c
             LEFT JOIN suppliers s ON s.id = c.supplier_id
             WHERE c.channel_code = ANY($1)",
        )
        .bind(channel_codes)
        .fetch_all(platform_db)
        .await
        .map_err(|e| {
            error!("Failed to load channel pricing map by code: {}", e);
            BillingError::Database
        })?;
        for row in rows {
            let id: uuid::Uuid = row.get("id");
            let channel_code: String = row.get("channel_code");
            let cost_price: Decimal = row.get("cost_price");
            let supplier_code: String = row.get("supplier_code");
            let info = ChannelPricingInfo {
                cost_price,
                channel_code: channel_code.clone(),
                supplier_code,
            };
            map.insert(id.to_string(), info.clone());
            map.insert(channel_code, info);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn hourly_settlement_aggregation_buckets_by_hour_and_resolves_cost_price() {
        let mut pricing = HashMap::new();
        pricing.insert(
            "ch-1".to_string(),
            ChannelPricingInfo {
                cost_price: Decimal::from_str("0.700000").expect("dec"),
                channel_code: "CH_MAIN".to_string(),
                supplier_code: "SUP_A".to_string(),
            },
        );
        let rows = vec![
            serde_json::json!({
                "window_start": "2026-11-10T08:00:00Z",
                "supplier_id": "",
                "supplier_code": "",
                "channel_id": "ch-1",
                "channel_code": "CH_MAIN",
                "total_calls": 10,
                "success_calls": 9,
                "failed_calls": 1,
                "total_payload_bytes": 1024,
            }),
            serde_json::json!({
                "window_start": "2026-11-10T08:00:00Z",
                "supplier_id": "11111111-1111-1111-1111-111111111111",
                "supplier_code": "SUP_A",
                "channel_id": "ch-1",
                "channel_code": "CH_MAIN",
                "total_calls": 5,
                "success_calls": 5,
                "failed_calls": 0,
                "total_payload_bytes": 512,
            }),
            serde_json::json!({
                "window_start": "2026-11-11T09:00:00Z",
                "supplier_code": "SUP_A",
                "channel_id": "ch-1",
                "channel_code": "CH_MAIN",
                "total_calls": 4,
                "success_calls": 4,
                "failed_calls": 0,
                "total_payload_bytes": 256,
            }),
        ];

        let buckets = aggregate_hourly_settlement_rows(&rows, &pricing);
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].supplier_code, "SUP_A");
        assert_eq!(buckets[0].channel_code, "CH_MAIN");
        assert_eq!(buckets[0].total_calls, 15);
        assert_eq!(buckets[0].successful_calls, 14);
        assert_eq!(buckets[0].failed_calls, 1);
        assert_eq!(buckets[0].total_bytes, 1536);
        assert_eq!(
            buckets[0].settlement_amount,
            Decimal::from_str("10.500000").expect("dec")
        );
        assert_eq!(
            buckets[1].settlement_amount,
            Decimal::from_str("2.800000").expect("dec")
        );

        let (drafts, total_bytes) = build_upstream_drafts_from_pricing(&rows, &pricing);
        assert_eq!(total_bytes, 1792);
        assert_eq!(drafts["ch-1"].quantity, 19);
    }

    #[test]
    fn normalize_bill_end_advances_end_of_day_second_to_hour_boundary() {
        let end_2359 = DateTime::parse_from_rfc3339("2026-11-10T23:59:59Z")
            .expect("dt")
            .with_timezone(&Utc);
        let normalized = normalize_bill_exclusive_end(end_2359);
        assert!(is_utc_hour_aligned(normalized));
        assert_eq!(normalized.to_rfc3339(), "2026-11-11T00:00:00+00:00");
    }
}
