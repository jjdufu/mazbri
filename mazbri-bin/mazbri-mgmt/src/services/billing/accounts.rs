use super::{
    AccountListResponse, BillingError, PgPool, decode_cursor, encode_cursor, error,
    normalize_account_status, repo, validate_limit,
};
use tracing::{debug, warn};

/// Build the Redis key `{prefix}:billing:bal:{tenant_id}` holding a tenant's cached L2 balance.
#[must_use]
pub fn tenant_balance_cache_key(prefix: &str, tenant_id: uuid::Uuid) -> String {
    let trimmed = prefix.trim_end_matches(':');
    if trimmed.is_empty() {
        format!("billing:bal:{tenant_id}")
    } else {
        format!("{trimmed}:billing:bal:{tenant_id}")
    }
}

/// Build the Redis Pub/Sub channel `{prefix}:billing:bal_inv` for single-tenant L1 cache invalidation.
#[must_use]
pub fn tenant_balance_invalidation_channel(prefix: &str) -> String {
    let trimmed = prefix.trim_end_matches(':');
    if trimmed.is_empty() {
        "billing:bal_inv".to_string()
    } else {
        format!("{trimmed}:billing:bal_inv")
    }
}

/// Delete `{prefix}:billing:bal:{tenant_id}` in Redis and publish `tenant_id` on
/// `{prefix}:billing:bal_inv` so gateway replicas evict only this tenant's L1 balance entry.
///
/// Intended to be invoked by `mazbri-mgmt` right after committed recharge, refund,
/// account adjustment, or credit-limit updates.
pub async fn invalidate_tenant_balance_cache(
    redis: &mazbri_core::redis::RedisClient,
    tenant_id: uuid::Uuid,
) {
    let prefix = redis.key_prefix();
    let bal_key = tenant_balance_cache_key(prefix, tenant_id);
    let channel = tenant_balance_invalidation_channel(prefix);
    let payload = tenant_id.to_string();

    if let Err(err) = redis.del(&bal_key).await {
        warn!(
            %tenant_id,
            key = %bal_key,
            error = %err,
            "failed to delete tenant L2 balance cache key"
        );
    }
    match redis
        .eval_i64(
            "return redis.call('PUBLISH', ARGV[1], ARGV[2])",
            &[],
            &[channel.clone(), payload],
        )
        .await
    {
        Ok(subscribers) => {
            debug!(
                %tenant_id,
                %channel,
                subscribers,
                "published tenant balance cache invalidation"
            );
        }
        Err(err) => {
            warn!(
                %tenant_id,
                %channel,
                error = %err,
                "failed to publish tenant balance cache invalidation"
            );
        }
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_accounts(
    pool: &PgPool,
    tenant_id: Option<uuid::Uuid>,
    status: Option<String>,
    cursor: Option<String>,
    limit: u32,
) -> Result<AccountListResponse, BillingError> {
    validate_limit(limit)?;
    let status = normalize_account_status(status.as_ref())?;
    let cursor = cursor.as_deref().and_then(decode_cursor);
    let db_limit = i64::from(limit + 1);
    let accounts = repo::list_accounts(
        pool,
        tenant_id,
        status.as_deref(),
        cursor.map(|c| c.created_at),
        db_limit,
    )
    .await
    .map_err(|e| {
        error!("Database error listing accounts: {}", e);
        BillingError::Database
    })?;
    let has_more = accounts.len() > limit as usize;
    let accounts = if has_more {
        accounts.into_iter().take(limit as usize).collect()
    } else {
        accounts
    };
    let next_cursor = accounts
        .last()
        .map(|account| encode_cursor(account.created_at, account.id));
    Ok(AccountListResponse {
        accounts,
        next_cursor,
        has_more,
    })
}

#[cfg(test)]
mod tests {
    use super::{tenant_balance_cache_key, tenant_balance_invalidation_channel};
    use uuid::Uuid;

    #[test]
    fn balance_cache_keys_and_channels_respect_prefix() {
        let tid = Uuid::parse_str("fc391546-7925-4139-b06e-6cfe9f3c8a70").expect("uuid");
        assert_eq!(
            tenant_balance_cache_key("mazbri", tid),
            "mazbri:billing:bal:fc391546-7925-4139-b06e-6cfe9f3c8a70"
        );
        assert_eq!(
            tenant_balance_invalidation_channel("mazbri:"),
            "mazbri:billing:bal_inv"
        );
        assert_eq!(
            tenant_balance_cache_key("", tid),
            "billing:bal:fc391546-7925-4139-b06e-6cfe9f3c8a70"
        );
        assert_eq!(tenant_balance_invalidation_channel(""), "billing:bal_inv");
    }
}
