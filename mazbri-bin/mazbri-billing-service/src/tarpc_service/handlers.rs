use super::BillingTarpcService;
use super::charge::handle_record_usage;
use super::conversion::{
    MICRO_DECIMALS, billing_error_to_rpc, from_amount, map_bill_info_to_rpc, map_bill_line_item,
    map_usage_breakdown, to_amount, validation_error,
};
use mazbri_billing::{AccountBalance, UsageSummaryInfo};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::{
    AccessHttpFields, AccessLogEvent, AccessNetworkFields, AccessOutcomeFields, AccessRouteFields,
    AccessTimingFields, classify_outcome, emit_access_log,
};
use mazbri_rpc::RpcError;
use mazbri_rpc::billing::{
    Amount, BillingService as BillingServiceTrait, CheckBalanceRequest, CheckBalanceResponse,
    GetBalanceRequest, GetBalanceResponse, GetBillRequest, GetBillResponse, GetUsageSummaryRequest,
    GetUsageSummaryResponse, HealthResponse, ListBillsRequest, ListBillsResponse,
    RecordUsageRequest, RecordUsageResponse, UsageSummary,
};
use rust_decimal::Decimal;
use sqlx::types::chrono;
use tarpc::context::Context;
use uuid::Uuid;

fn log_access(
    service_name: &str,
    method_name: &str,
    tenant_id: Option<&str>,
    request_id: Option<&str>,
    status_code: Option<u16>,
    billing_service_duration_ms: Option<u64>,
) {
    let status_code = status_code.unwrap_or(0);
    let outcome = classify_outcome("", "", status_code);
    let mut event = AccessLogEvent::builder(service_name)
        .message("request_completed")
        .route(AccessRouteFields {
            tenant_id: tenant_id.map(std::string::ToString::to_string),
            tenant_code: None,
            product_id: None,
            supplier_id: None,
            supplier_code: None,
            channel_id: None,
            channel_code: None,
            attempt_order: Some(1),
            api_key_id: None,
        })
        .http(AccessHttpFields {
            method: method_name.to_string(),
            protocol: "tarpc".to_string(),
            host_header: None,
            path: format!("/tarpc/billing/{method_name}"),
            route_template: format!("/tarpc/billing/{method_name}"),
            status_code,
            request_bytes: 0,
            response_bytes: 0,
            user_agent: None,
        })
        .timing(AccessTimingFields {
            duration_ms: billing_service_duration_ms.unwrap_or(0),
            upstream_service_time_ms: None,
            dependency_wait_ms: 0,
            internal_ms: 0,
            post_response_ms: 0,
        })
        .network(AccessNetworkFields {
            upstream_host: None,
            upstream_status_code: Some(status_code),
            client_ip: None,
            xff: None,
            cache_hit: false,
            fallback_used: false,
            retry_reasons: Vec::new(),
            upstream_request_id: None,
        })
        .outcome(AccessOutcomeFields {
            error_code: String::new(),
            error_stage: String::new(),
            response_flags: outcome.response_flags.to_string(),
            error_source: outcome.error_source.as_str().to_string(),
            error_class: String::new(),
        })
        .build();
    if let Some(request_id) = request_id {
        event.request_id = request_id.to_string();
        event.trace_id = request_id.to_string();
    }
    emit_access_log("billing.access", &event);
}

pub(super) fn micros_to_estimated_decimal(estimated_cost_micros: i64) -> Result<Decimal, RpcError> {
    if estimated_cost_micros < 0 {
        return Err(validation_error(
            "estimated_cost must be non-negative micro-units",
        ));
    }
    let from_wire = from_amount(&Amount {
        value: estimated_cost_micros,
        currency: "CNY".to_string(),
        decimals: MICRO_DECIMALS,
    })?;
    let exact = Decimal::new(estimated_cost_micros, 6);
    debug_assert_eq!(from_wire, exact);
    Ok(exact)
}

impl BillingServiceTrait for BillingTarpcService {
    async fn record_usage(self, _context: Context, req: RecordUsageRequest) -> RecordUsageResponse {
        let trace_ref = if !req.event_id.is_empty() {
            Some(req.event_id.as_str())
        } else if !req.request_id.is_empty() {
            Some(req.request_id.as_str())
        } else {
            None
        };
        log_access(
            self.service_name.as_ref(),
            "record_usage",
            Some(&req.tenant_id),
            trace_ref,
            Some(u16::saturating_from(req.status_code)),
            Some(u64::saturating_from(req.billing_service_duration_ms)),
        );
        handle_record_usage(&self, req).await
    }

    async fn check_balance(
        self,
        _context: Context,
        req: CheckBalanceRequest,
    ) -> CheckBalanceResponse {
        log_access(
            self.service_name.as_ref(),
            "check_balance",
            Some(&req.tenant_id),
            None,
            None,
            None,
        );
        let tenant_id = match Uuid::parse_str(&req.tenant_id) {
            Ok(id) => id,
            Err(e) => {
                return CheckBalanceResponse {
                    sufficient: false,
                    current_balance: None,
                    credit_limit: None,
                    available: None,
                    error: Some(validation_error(&format!("Invalid tenant ID: {e}"))),
                    error_message: String::new(),
                };
            }
        };
        let estimated_cost = match micros_to_estimated_decimal(req.estimated_cost) {
            Ok(cost) => cost,
            Err(err) => {
                return CheckBalanceResponse {
                    sufficient: false,
                    current_balance: None,
                    credit_limit: None,
                    available: None,
                    error: Some(err),
                    error_message: String::new(),
                };
            }
        };
        match self
            .billing_service
            .check_balance(tenant_id, estimated_cost)
            .await
        {
            Ok(sufficient) => match self.billing_service.get_balance(tenant_id).await {
                Ok(balance) => {
                    let converted = (|| -> Result<(Amount, Amount, Amount), RpcError> {
                        Ok((
                            to_amount(balance.balance)?,
                            to_amount(balance.credit_limit)?,
                            to_amount(balance.available)?,
                        ))
                    })();
                    match converted {
                        Ok((cur, lim, avail)) => CheckBalanceResponse {
                            sufficient,
                            current_balance: Some(cur),
                            credit_limit: Some(lim),
                            available: Some(avail),
                            error: None,
                            error_message: String::new(),
                        },
                        Err(err) => CheckBalanceResponse {
                            sufficient: false,
                            current_balance: None,
                            credit_limit: None,
                            available: None,
                            error: Some(err),
                            error_message: String::new(),
                        },
                    }
                }
                Err(e) => CheckBalanceResponse {
                    sufficient,
                    current_balance: None,
                    credit_limit: None,
                    available: None,
                    error: Some(billing_error_to_rpc(&e)),
                    error_message: String::new(),
                },
            },
            Err(e) => CheckBalanceResponse {
                sufficient: false,
                current_balance: None,
                credit_limit: None,
                available: None,
                error: Some(billing_error_to_rpc(&e)),
                error_message: String::new(),
            },
        }
    }

    async fn get_balance(self, _context: Context, req: GetBalanceRequest) -> GetBalanceResponse {
        log_access(
            self.service_name.as_ref(),
            "get_balance",
            Some(&req.tenant_id),
            None,
            None,
            None,
        );
        let tenant_id = match Uuid::parse_str(&req.tenant_id) {
            Ok(id) => id,
            Err(e) => {
                return GetBalanceResponse {
                    balance: None,
                    credit_limit: None,
                    available: None,
                    cycle: None,
                    error: Some(validation_error(&format!("Invalid tenant ID: {e}"))),
                    error_message: String::new(),
                };
            }
        };
        match self.billing_service.get_balance(tenant_id).await {
            Ok(AccountBalance {
                balance,
                credit_limit,
                available,
                billing_cycle,
            }) => {
                let cycle = billing_cycle.map(|cycle_id| mazbri_rpc::billing::BillingCycle {
                    cycle_id,
                    start_at: 0,
                    end_at: 0,
                    period: mazbri_rpc::billing::BillingPeriod::Monthly,
                });
                let converted = (|| -> Result<(Amount, Amount, Amount), RpcError> {
                    Ok((
                        to_amount(balance)?,
                        to_amount(credit_limit)?,
                        to_amount(available)?,
                    ))
                })();
                match converted {
                    Ok((bal, lim, avail)) => GetBalanceResponse {
                        balance: Some(bal),
                        credit_limit: Some(lim),
                        available: Some(avail),
                        cycle,
                        error: None,
                        error_message: String::new(),
                    },
                    Err(err) => GetBalanceResponse {
                        balance: None,
                        credit_limit: None,
                        available: None,
                        cycle: None,
                        error: Some(err),
                        error_message: String::new(),
                    },
                }
            }
            Err(e) => GetBalanceResponse {
                balance: None,
                credit_limit: None,
                available: None,
                cycle: None,
                error: Some(billing_error_to_rpc(&e)),
                error_message: String::new(),
            },
        }
    }

    async fn get_bill(self, _context: Context, req: GetBillRequest) -> GetBillResponse {
        log_access(
            self.service_name.as_ref(),
            "get_bill",
            Some(&req.tenant_id),
            Some(&req.bill_id),
            None,
            None,
        );
        let tenant_id = match Uuid::parse_str(&req.tenant_id) {
            Ok(id) => id,
            Err(e) => {
                return GetBillResponse {
                    bill: None,
                    error: Some(validation_error(&format!("Invalid tenant ID: {e}"))),
                    error_message: String::new(),
                };
            }
        };
        let bill_id = match Uuid::parse_str(&req.bill_id) {
            Ok(id) => id,
            Err(e) => {
                return GetBillResponse {
                    bill: None,
                    error: Some(validation_error(&format!("Invalid bill ID: {e}"))),
                    error_message: String::new(),
                };
            }
        };
        match self.billing_service.get_bill(tenant_id, bill_id).await {
            Ok(Some(bill_info)) => {
                let items_res: Result<Vec<mazbri_rpc::billing::BillLineItem>, RpcError> =
                    match self.billing_service.get_bill_items(tenant_id, bill_id).await {
                        Ok(line_items) => line_items
                            .iter()
                            .enumerate()
                            .map(|(i, item)| map_bill_line_item(i, item))
                            .collect(),
                        Err(e) => Err(billing_error_to_rpc(&e)),
                    };
                match items_res.and_then(|items| map_bill_info_to_rpc(&bill_info, items)) {
                    Ok(bill) => GetBillResponse {
                        bill: Some(bill),
                        error: None,
                        error_message: String::new(),
                    },
                    Err(err) => GetBillResponse {
                        bill: None,
                        error: Some(err),
                        error_message: String::new(),
                    },
                }
            }
            Ok(None) => GetBillResponse {
                bill: None,
                error: Some(RpcError::NotFound {
                    resource: "bill".into(),
                    id: req.bill_id.clone(),
                }),
                error_message: String::new(),
            },
            Err(e) => GetBillResponse {
                bill: None,
                error: Some(billing_error_to_rpc(&e)),
                error_message: String::new(),
            },
        }
    }

    async fn list_bills(self, _context: Context, req: ListBillsRequest) -> ListBillsResponse {
        log_access(
            self.service_name.as_ref(),
            "list_bills",
            Some(&req.tenant_id),
            None,
            None,
            None,
        );
        let tenant_id = match Uuid::parse_str(&req.tenant_id) {
            Ok(id) => id,
            Err(e) => {
                return ListBillsResponse {
                    bills: Vec::new(),
                    next_page_token: String::new(),
                    total_count: 0,
                    error: Some(validation_error(&format!("Invalid tenant ID: {e}"))),
                    error_message: String::new(),
                };
            }
        };
        let status_filter = match req.status {
            1 => Some("pending"),
            2 => Some("paid"),
            3 => Some("overdue"),
            4 => Some("cancelled"),
            _ => None,
        };
        let start_time = (req.start_time > 0).then_some(req.start_time);
        let end_time = (req.end_time > 0).then_some(req.end_time);
        match self
            .billing_service
            .list_bills(
                tenant_id,
                req.page_size,
                start_time,
                end_time,
                status_filter,
            )
            .await
        {
            Ok(bills) => {
                let converted: Result<Vec<mazbri_rpc::billing::Bill>, RpcError> = bills
                    .iter()
                    .map(|bill_info| map_bill_info_to_rpc(bill_info, Vec::new()))
                    .collect();
                match converted {
                    Ok(bills) => {
                        let total_count = i32::saturating_from(bills.len());
                        ListBillsResponse {
                            bills,
                            next_page_token: String::new(),
                            total_count,
                            error: None,
                            error_message: String::new(),
                        }
                    }
                    Err(err) => ListBillsResponse {
                        bills: Vec::new(),
                        next_page_token: String::new(),
                        total_count: 0,
                        error: Some(err),
                        error_message: String::new(),
                    },
                }
            }
            Err(e) => ListBillsResponse {
                bills: Vec::new(),
                next_page_token: String::new(),
                total_count: 0,
                error: Some(billing_error_to_rpc(&e)),
                error_message: String::new(),
            },
        }
    }

    async fn get_usage_summary(
        self,
        _context: Context,
        req: GetUsageSummaryRequest,
    ) -> GetUsageSummaryResponse {
        log_access(
            self.service_name.as_ref(),
            "get_usage_summary",
            Some(&req.tenant_id),
            None,
            None,
            None,
        );
        let tenant_id = match Uuid::parse_str(&req.tenant_id) {
            Ok(id) => id,
            Err(e) => {
                return GetUsageSummaryResponse {
                    summary: None,
                    error: Some(validation_error(&format!("Invalid tenant ID: {e}"))),
                    error_message: String::new(),
                };
            }
        };
        match self
            .billing_service
            .get_usage_summary(tenant_id, req.start_time, req.end_time)
            .await
        {
            Ok(UsageSummaryInfo {
                total_requests,
                total_bytes_in,
                total_bytes_out,
                total_billing_service_duration_ms,
                total_cost,
            }) => {
                let average_billing_service_duration_ms = if total_requests > 0 {
                    total_billing_service_duration_ms / total_requests
                } else {
                    0
                };
                let breakdown_res: Result<Vec<mazbri_rpc::billing::UsageBreakdown>, RpcError> =
                    match self
                        .billing_service
                        .get_usage_breakdown(tenant_id, req.start_time, req.end_time)
                        .await
                    {
                        Ok(items) => items.iter().map(map_usage_breakdown).collect(),
                        Err(e) => Err(billing_error_to_rpc(&e)),
                    };
                match breakdown_res.and_then(|breakdown| {
                    Ok(UsageSummary {
                        total_requests,
                        total_bytes_in,
                        total_bytes_out,
                        total_billing_service_duration_ms,
                        average_billing_service_duration_ms,
                        total_cost: Some(to_amount(total_cost)?),
                        breakdown,
                    })
                }) {
                    Ok(summary) => GetUsageSummaryResponse {
                        summary: Some(summary),
                        error: None,
                        error_message: String::new(),
                    },
                    Err(err) => GetUsageSummaryResponse {
                        summary: None,
                        error: Some(err),
                        error_message: String::new(),
                    },
                }
            }
            Err(e) => GetUsageSummaryResponse {
                summary: None,
                error: Some(billing_error_to_rpc(&e)),
                error_message: String::new(),
            },
        }
    }

    fn health(self, _context: Context) -> impl std::future::Future<Output = HealthResponse> {
        std::future::ready(HealthResponse {
            status: "ok".to_string(),
            service: "billing-service".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::micros_to_estimated_decimal;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn check_balance_converts_micros_to_decimal_exact() {
        assert_eq!(
            micros_to_estimated_decimal(1000).expect("1000 micros = 0.001 CNY"),
            Decimal::from_str("0.001").expect("dec")
        );
        assert_eq!(
            micros_to_estimated_decimal(40).expect("40 micros = 0.000040 CNY"),
            Decimal::from_str("0.000040").expect("dec")
        );
        assert_eq!(
            micros_to_estimated_decimal(0).expect("zero"),
            Decimal::ZERO
        );
        assert!(micros_to_estimated_decimal(-1).is_err());
    }
}
