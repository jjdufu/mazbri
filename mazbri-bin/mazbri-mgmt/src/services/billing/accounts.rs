use super::{
    AccountListResponse, BillingError, PgPool, decode_cursor, encode_cursor, error,
    normalize_account_status, repo, validate_limit,
};

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
