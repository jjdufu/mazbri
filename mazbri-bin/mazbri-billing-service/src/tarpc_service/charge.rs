use super::BillingTarpcService;
use super::conversion::{billing_error_to_rpc, safe_to_amount, validation_error};
use mazbri_billing::UsageRecord;
use mazbri_core::num::SaturatingFrom;
use mazbri_rpc::RpcError;
use mazbri_rpc::billing::RecordUsageRequest;
use mazbri_rpc::billing::RecordUsageResponse;
use rust_decimal::Decimal;
use tracing::{debug, warn};
use uuid::Uuid;
fn record_usage_failure(error: RpcError) -> RecordUsageResponse {
    RecordUsageResponse {
        success: false,
        billing_id: String::new(),
        charge_id: String::new(),
        charged: None,
        new_balance: None,
        error: Some(error),
        error_message: String::new(),
    }
}
pub(super) async fn handle_record_usage(
    service: &BillingTarpcService,
    req: RecordUsageRequest,
) -> RecordUsageResponse {
    let billing_id = Uuid::new_v4().to_string();
    match prepare_usage(service, req).await {
        Ok((tenant_id, account_id, usage_record_id, cost_value)) => {
            process_charge(
                service,
                billing_id,
                tenant_id,
                account_id,
                usage_record_id,
                cost_value,
            )
            .await
        }
        Err(error) => record_usage_failure(error),
    }
}
/// 校验请求、计费并落用量记录；返回 `(tenant_id, account_id, usage_record_id, cost)`。
async fn prepare_usage(
    service: &BillingTarpcService,
    req: RecordUsageRequest,
) -> Result<(Uuid, Uuid, i64, Decimal), RpcError> {
    let tenant_id = Uuid::parse_str(&req.tenant_id)
        .map_err(|e| validation_error(&format!("Invalid tenant ID: {e}")))?;
    let account_id = service
        .billing_service
        .get_account_id(tenant_id)
        .await
        .ok_or_else(|| RpcError::NotFound {
            resource: "account".into(),
            id: tenant_id.to_string(),
        })?;
    let calculator = service
        .billing_service
        .get_calculator(tenant_id, None)
        .await
        .map_err(|e| billing_error_to_rpc(&e))?;
    let calc_result = calculator
        .calculate_call(
            u64::saturating_from(req.request_size),
            u64::saturating_from(req.response_size),
        )
        .map_err(|e| billing_error_to_rpc(&e))?;
    let cost_value = calc_result.amount;
    let usage_record = UsageRecord {
        event_id: None,
        tenant_id,
        account_id,
        api_endpoint: req.api_endpoint,
        http_method: "GET".to_string(),
        response_status: u16::saturating_from(req.status_code),
        request_size: req.request_size,
        response_size: req.response_size,
        billing_service_duration_ms: req.billing_service_duration_ms,
        billing_result: calc_result,
        product_id: None,
        billing_code: None,
    };
    let usage_record_id = service
        .billing_service
        .record_usage(usage_record)
        .await
        .map_err(|e| billing_error_to_rpc(&e))?;
    Ok((tenant_id, account_id, usage_record_id, cost_value))
}
pub(super) async fn process_charge(
    service: &BillingTarpcService,
    billing_id: String,
    tenant_id: Uuid,
    account_id: Uuid,
    usage_record_id: i64,
    cost_value: Decimal,
) -> RecordUsageResponse {
    if let Some(ref queue) = service.charge_queue {
        if cost_value > Decimal::ZERO {
            match queue
                .enqueue(tenant_id, account_id, usage_record_id, cost_value, None)
                .await
            {
                Ok(queue_id) => {
                    debug!(
                        "Enqueued charge for async processing: queue_id={}, tenant_id={}, amount={}",
                        queue_id, tenant_id, cost_value
                    );
                    return RecordUsageResponse {
                        success: true,
                        billing_id,
                        charge_id: queue_id.to_string(),
                        charged: None,
                        new_balance: None,
                        error: None,
                        error_message: String::new(),
                    };
                }
                Err(e) => {
                    warn!("Failed to enqueue charge: {}", e);
                    return RecordUsageResponse {
                        success: false,
                        billing_id,
                        charge_id: String::new(),
                        charged: None,
                        new_balance: None,
                        error: Some(RpcError::Internal {
                            message: "入队计费失败".into(),
                        }),
                        error_message: String::new(),
                    };
                }
            }
        }
        return RecordUsageResponse {
            success: true,
            billing_id,
            charge_id: String::new(),
            charged: None,
            new_balance: None,
            error: None,
            error_message: String::new(),
        };
    }
    let new_balance = match service
        .billing_service
        .deduct_balance(tenant_id, cost_value)
        .await
    {
        Ok(balance) => balance,
        Err(e) => {
            return RecordUsageResponse {
                success: false,
                billing_id,
                charge_id: String::new(),
                charged: None,
                new_balance: None,
                error: Some(billing_error_to_rpc(&e)),
                error_message: String::new(),
            };
        }
    };
    RecordUsageResponse {
        success: true,
        billing_id,
        charge_id: String::new(),
        charged: Some(safe_to_amount(
            cost_value,
            &tenant_id,
            "record_usage: charged",
        )),
        new_balance: Some(safe_to_amount(
            new_balance,
            &tenant_id,
            "record_usage: new_balance",
        )),
        error: None,
        error_message: String::new(),
    }
}
