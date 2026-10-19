use mazbri_core::{AttemptStatus, InteractionEvent};
use sqlx::PgPool;
pub async fn persist_interaction_event(
    pool: &PgPool,
    event: &InteractionEvent,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let final_status = event.final_status.as_deref().unwrap_or("unknown");
    sqlx::query(
        r#"
        INSERT INTO interaction_events (
            id, request_id, tenant_id, product_id, start_time, final_status
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(event.id)
    .bind(&event.request_id)
    .bind(&event.tenant_id)
    .bind(&event.product_id)
    .bind(event.start_time)
    .bind(final_status)
    .execute(&mut *tx)
    .await?;
    for (index, attempt) in event.attempts.iter().enumerate() {
        let (status, error_message) = attempt_status_parts(attempt.status.as_ref());
        sqlx::query(
            r#"
            INSERT INTO interaction_event_attempts (
                interaction_event_id,
                channel_id,
                supplier_id,
                supplier_code,
                supplier_name,
                channel_code,
                channel_name,
                endpoint_url,
                attempt_order,
                start_time,
                end_time,
                status,
                error_message
            )
            VALUES ($1, $2, $3::uuid, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(event.id)
        .bind(&attempt.channel_id)
        .bind(attempt.supplier_id.as_deref())
        .bind(attempt.supplier_code.as_deref())
        .bind(attempt.supplier_name.as_deref())
        .bind(attempt.channel_code.as_deref())
        .bind(attempt.channel_name.as_deref())
        .bind(attempt.endpoint_url.as_deref())
        .bind((index + 1) as i32)
        .bind(attempt.start_time)
        .bind(attempt.end_time)
        .bind(status)
        .bind(error_message)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}
fn attempt_status_parts(status: Option<&AttemptStatus>) -> (Option<&'static str>, Option<String>) {
    match status {
        Some(AttemptStatus::Success) => (Some("success"), None),
        Some(AttemptStatus::Timeout) => (Some("timeout"), None),
        Some(AttemptStatus::Failed(error)) => (Some("failed"), Some(error.clone())),
        None => (None, None),
    }
}
