use mazbri_core::num::SaturatingFrom;
use super::BillingTarpcService;
use super::conversion::{billing_error_to_rpc, safe_to_amount, validation_error};
use mazbri_billing::UsageRecord;
use mazbri_rpc::RpcError;
use mazbri_rpc::billing::RecordUsageRequest;
use mazbri_rpc::billing::RecordUsageResponse;
use rust_decimal::Decimal;
use tracing::{debug, warn};
use uuid::Uuid;
pub(super) async fn handle_record_usage(
    service: &BillingTarpcService,
    req: RecordUsageRequest,
) -> RecordUsageResponse {
    let billing_id = Uuid::new_v4().to_string();
    let tenant_id = match Uuid::parse_str(&req.tenant_id) {
        Ok(id) => id,
        Err(e) => {
            return RecordUsageResponse {
                success: false,
                billing_id: String::new(),
                charge_id: String::new(),
                charged: None,
                new_balance: None,
                error: Some(validation_error(&format!("Invalid tenant ID: {e}"))),
                error_message: String::new(),
            };
        }
    };
    let Some(account_id) = service.billing_service.get_account_id(tenant_id).await else {
            return RecordUsageResponse {
                success: false,
                billing_id: String::new(),
                charge_id: String::new(),
                charged: None,
                new_balance: None,
                error: Some(RpcError::NotFound {
                    resource: "account".into(),
                    id: tenant_id.to_string(),
                }),
                error_message: String::new(),
            };
        };
    let calculator = match service
        .billing_service
        .get_calculator(tenant_id, None)
        .await
    {
        Ok(calc) => calc,
        Err(e) => {
            return RecordUsageResponse {
                success: false,
                billing_id: String::new(),
                charge_id: String::new(),
                charged: None,
                new_balance: None,
                error: Some(billing_error_to_rpc(&e)),
                error_message: String::new(),
            };
        }
    };
    let calc_result =
        match calculator.calculate_call(u64::saturating_from(req.request_size), u64::saturating_from(req.response_size)) {
            Ok(result) => result,
            Err(e) => {
                return RecordUsageResponse {
                    success: false,
                    billing_id: String::new(),
                    charge_id: String::new(),
                    charged: None,
                    new_balance: None,
                    error: Some(billing_error_to_rpc(&e)),
                    error_message: String::new(),
                };
            }
        };
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
    let usage_record_id = match service.billing_service.record_usage(usage_record).await {
        Ok(id) => id,
        Err(e) => {
            return RecordUsageResponse {
                success: false,
                billing_id: String::new(),
                charge_id: String::new(),
                charged: None,
                new_balance: None,
                error: Some(billing_error_to_rpc(&e)),
                error_message: String::new(),
            };
        }
    };
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
