use super::BillingTarpcService;
use super::conversion::{billing_error_to_rpc, to_amount, validation_error};
use mazbri_billing::{CalculationResult, RecordUsageOutcome, UsageRecord};
use mazbri_core::num::SaturatingFrom;
use mazbri_rpc::billing::{RecordUsageRequest, RecordUsageResponse};
use mazbri_rpc::{BillingCode, RpcError};
use rust_decimal::Decimal;
use tracing::debug;
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

pub(super) fn parse_mandatory_event_id(req: &RecordUsageRequest) -> Result<Uuid, RpcError> {
    let raw = if !req.event_id.trim().is_empty() {
        req.event_id.trim()
    } else if let Some(ev) = req
        .metadata
        .get("event_id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        ev
    } else if !req.request_id.trim().is_empty() {
        req.request_id.trim()
    } else {
        return Err(validation_error(
            "event_id is required and must be a valid UUID",
        ));
    };

    let id =
        Uuid::parse_str(raw).map_err(|e| validation_error(&format!("Invalid event_id UUID: {e}")))?;
    if id.is_nil() {
        return Err(validation_error("event_id must not be nil UUID"));
    }
    Ok(id)
}

fn resolve_billing_code_and_billable(req: &RecordUsageRequest) -> (i16, bool) {
    if let Some(raw_code) = req.metadata.get("billing_code")
        && let Ok(code_u16) = raw_code.trim().parse::<u16>()
    {
        let bc = BillingCode(code_u16);
        return (i16::saturating_from(bc.0), bc.default_billable());
    }
    let bc = match req.status_code {
        200..=299 => BillingCode::SUCCESS,
        404 => BillingCode::NOT_FOUND,
        408 => BillingCode::UPSTREAM_TIMEOUT,
        429 => BillingCode::RATE_LIMIT_EXCEEDED,
        500..=599 => BillingCode::UPSTREAM_ERROR,
        _ => BillingCode::BAD_REQUEST,
    };
    (i16::saturating_from(bc.0), bc.default_billable())
}

async fn build_usage_record_for_rpc(
    service: &BillingTarpcService,
    req: RecordUsageRequest,
) -> Result<(Uuid, Uuid, UsageRecord), RpcError> {
    if req.request_size < 0 || req.response_size < 0 || req.billing_service_duration_ms < 0 {
        return Err(validation_error(
            "request_size, response_size, and billing_service_duration_ms must be non-negative",
        ));
    }
    let tenant_id = Uuid::parse_str(&req.tenant_id)
        .map_err(|e| validation_error(&format!("Invalid tenant ID: {e}")))?;
    let event_id = parse_mandatory_event_id(&req)?;

    let account_id = service
        .billing_service
        .try_get_account_id(tenant_id)
        .await
        .map_err(|e| billing_error_to_rpc(&e))?
        .ok_or_else(|| RpcError::NotFound {
            resource: "account".into(),
            id: tenant_id.to_string(),
        })?;

    let product_id = req
        .metadata
        .get("product_id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    let http_method = req
        .metadata
        .get("http_method")
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("GET")
        .to_string();

    let (billing_code, is_billable) = resolve_billing_code_and_billable(&req);
    let calc_result = if is_billable {
        let calculator = service
            .billing_service
            .get_calculator(tenant_id, product_id.as_deref())
            .await
            .map_err(|e| billing_error_to_rpc(&e))?;
        calculator.calculate_call()
    } else {
        CalculationResult {
            amount: Decimal::ZERO,
            units: 0,
            unit_price: Decimal::ZERO,
            description: format!("non-billable status {}", req.status_code),
        }
    };

    let usage_record = UsageRecord {
        event_id: Some(event_id),
        tenant_id,
        account_id,
        api_endpoint: req.api_endpoint,
        http_method,
        response_status: u16::saturating_from(req.status_code),
        request_size: req.request_size,
        response_size: req.response_size,
        billing_service_duration_ms: req.billing_service_duration_ms,
        billing_result: calc_result,
        product_id,
        billing_code: Some(billing_code),
    };
    Ok((tenant_id, event_id, usage_record))
}

pub(super) async fn handle_record_usage(
    service: &BillingTarpcService,
    req: RecordUsageRequest,
) -> RecordUsageResponse {
    let (tenant_id, event_id, usage_record) = match build_usage_record_for_rpc(service, req).await {
        Ok(built) => built,
        Err(err) => return record_usage_failure(err),
    };

    match service
        .billing_service
        .record_usage_and_charge(usage_record, service.charge_queue.as_deref())
        .await
    {
        Ok(RecordUsageOutcome::Queued {
            usage_record_id,
            queue_id,
        }) => {
            debug!(
                %event_id,
                %tenant_id,
                usage_record_id,
                %queue_id,
                "Recorded usage and enqueued charge in single transaction"
            );
            RecordUsageResponse {
                success: true,
                billing_id: usage_record_id.to_string(),
                charge_id: queue_id.to_string(),
                charged: None,
                new_balance: None,
                error: None,
                error_message: String::new(),
            }
        }
        Ok(RecordUsageOutcome::SyncCharged {
            usage_record_id,
            charge_id,
            charged,
            new_balance,
        }) => {
            let amounts = (|| -> Result<_, RpcError> {
                Ok((to_amount(charged)?, to_amount(new_balance)?))
            })();
            match amounts {
                Ok((charged_amt, bal_amt)) => RecordUsageResponse {
                    success: true,
                    billing_id: usage_record_id.to_string(),
                    charge_id: charge_id.map_or_else(String::new, |id| id.to_string()),
                    charged: Some(charged_amt),
                    new_balance: Some(bal_amt),
                    error: None,
                    error_message: String::new(),
                },
                Err(err) => record_usage_failure(err),
            }
        }
        Ok(RecordUsageOutcome::NoCharge { usage_record_id }) => RecordUsageResponse {
            success: true,
            billing_id: usage_record_id.to_string(),
            charge_id: String::new(),
            charged: None,
            new_balance: None,
            error: None,
            error_message: String::new(),
        },
        Err(e) => record_usage_failure(billing_error_to_rpc(&e)),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_mandatory_event_id, resolve_billing_code_and_billable};
    use mazbri_rpc::BillingCode;
    use mazbri_rpc::billing::RecordUsageRequest;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn base_req() -> RecordUsageRequest {
        RecordUsageRequest {
            tenant_id: Uuid::new_v4().to_string(),
            api_endpoint: "/v1/verify".to_string(),
            event_id: String::new(),
            request_id: String::new(),
            request_size: 10,
            response_size: 20,
            billing_service_duration_ms: 5,
            status_code: 200,
            timestamp: 0,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn record_usage_requires_non_nil_uuid_event_id() {
        let mut req = base_req();
        assert!(parse_mandatory_event_id(&req).is_err());

        req.event_id = "not-a-uuid".to_string();
        assert!(parse_mandatory_event_id(&req).is_err());

        req.event_id = Uuid::nil().to_string();
        assert!(parse_mandatory_event_id(&req).is_err());

        let ev = Uuid::now_v7();
        req.event_id = ev.to_string();
        assert_eq!(parse_mandatory_event_id(&req).expect("valid"), ev);

        // Fallback to metadata["event_id"] and request_id when event_id is omitted
        req.event_id.clear();
        let meta_ev = Uuid::new_v4();
        req.metadata
            .insert("event_id".to_string(), meta_ev.to_string());
        assert_eq!(parse_mandatory_event_id(&req).expect("meta ev"), meta_ev);
    }

    #[test]
    fn status_4xx_maps_to_bad_request_non_billable_in_rpc() {
        let mut req = base_req();
        req.status_code = 200;
        let (code, billable) = resolve_billing_code_and_billable(&req);
        assert_eq!(code, i16::try_from(BillingCode::SUCCESS.0).expect("i16"));
        assert!(billable);

        for s in [400, 401, 403, 422, 500] {
            req.status_code = s;
            let (_, billable) = resolve_billing_code_and_billable(&req);
            assert!(!billable, "status {s} must not be billable");
        }
    }
}
