use super::BillingTarpcService;
use super::charge::handle_record_usage;
use super::conversion::{
    billing_error_to_rpc, from_amount, map_bill_info_to_rpc, map_bill_line_item,
    map_usage_breakdown, safe_to_amount, validation_error,
};
use mazbri_billing::{AccountBalance, AddFundsResult, UsageSummaryInfo};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::{
    AccessHttpFields, AccessLogEvent, AccessNetworkFields, AccessOutcomeFields, AccessRouteFields,
    AccessTimingFields, emit_access_log, response_flags_for_error,
};
use mazbri_rpc::RpcError;
use mazbri_rpc::billing::{
    AddFundsRequest, AddFundsResponse, BillingService as BillingServiceTrait, CheckBalanceRequest,
    CheckBalanceResponse, GetBalanceRequest, GetBalanceResponse, GetBillRequest, GetBillResponse,
    GetUsageSummaryRequest, GetUsageSummaryResponse, HealthResponse, ListBillsRequest,
    ListBillsResponse, RecordUsageRequest, RecordUsageResponse, UsageSummary,
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
    let response_flags = if status_code == 0 {
        "-".to_string()
    } else {
        response_flags_for_error("", "", status_code)
    };
    let event = AccessLogEvent::builder(service_name)
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
            retry_count: 0,
        })
        .outcome(AccessOutcomeFields {
            error_code: String::new(),
            error_stage: String::new(),
            response_flags,
            error_class: String::new(),
        })
        .build();
    let mut event = event;
    if let Some(request_id) = request_id {
        event.request_id = request_id.to_string();
        event.trace_id = request_id.to_string();
    }
    emit_access_log("billing.access", event);
}
impl BillingServiceTrait for BillingTarpcService {
    async fn record_usage(self, _context: Context, req: RecordUsageRequest) -> RecordUsageResponse {
        log_access(
            self.service_name.as_ref(),
            "record_usage",
            Some(&req.tenant_id),
            Some(&req.request_id),
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
        let estimated_cost = Decimal::from(req.estimated_cost);
        match self
            .billing_service
            .check_balance(tenant_id, estimated_cost)
            .await
        {
            Ok(sufficient) => match self.billing_service.get_balance(tenant_id).await {
                Ok(balance) => CheckBalanceResponse {
                    sufficient,
                    current_balance: Some(safe_to_amount(
                        balance.balance,
                        &tenant_id,
                        "check_balance: balance",
                    )),
                    credit_limit: Some(safe_to_amount(
                        balance.credit_limit,
                        &tenant_id,
                        "check_balance: credit_limit",
                    )),
                    available: Some(safe_to_amount(
                        balance.available,
                        &tenant_id,
                        "check_balance: available",
                    )),
                    error: None,
                    error_message: String::new(),
                },
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
                GetBalanceResponse {
                    balance: Some(safe_to_amount(balance, &tenant_id, "get_balance: balance")),
                    credit_limit: Some(safe_to_amount(
                        credit_limit,
                        &tenant_id,
                        "get_balance: credit_limit",
                    )),
                    available: Some(safe_to_amount(
                        available,
                        &tenant_id,
                        "get_balance: available",
                    )),
                    cycle,
                    error: None,
                    error_message: String::new(),
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
    async fn add_funds(self, _context: Context, req: AddFundsRequest) -> AddFundsResponse {
        log_access(
            self.service_name.as_ref(),
            "add_funds",
            Some(&req.tenant_id),
            None,
            None,
            None,
        );
        let tenant_id = match Uuid::parse_str(&req.tenant_id) {
            Ok(id) => id,
            Err(e) => {
                return AddFundsResponse {
                    success: false,
                    transaction_id: String::new(),
                    new_balance: None,
                    processed_at: 0,
                    error: Some(validation_error(&format!("Invalid tenant ID: {e}"))),
                    error_message: String::new(),
                };
            }
        };
        let amount = req.amount.as_ref().map_or(Decimal::ZERO, from_amount);
        match self
            .billing_service
            .add_funds(tenant_id, amount, &req.payment_method)
            .await
        {
            Ok(AddFundsResult {
                transaction_id,
                new_balance,
                processed_at,
            }) => AddFundsResponse {
                success: true,
                transaction_id: transaction_id.to_string(),
                new_balance: Some(safe_to_amount(
                    new_balance,
                    &tenant_id,
                    "add_funds: new_balance",
                )),
                processed_at,
                error: None,
                error_message: String::new(),
            },
            Err(e) => AddFundsResponse {
                success: false,
                transaction_id: String::new(),
                new_balance: None,
                processed_at: 0,
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
                let items = match self
                    .billing_service
                    .get_bill_items(tenant_id, bill_id)
                    .await
                {
                    Ok(line_items) => line_items
                        .into_iter()
                        .enumerate()
                        .map(|(i, item)| map_bill_line_item(i, &item, &tenant_id))
                        .collect(),
                    Err(_) => Vec::new(),
                };
                let bill = map_bill_info_to_rpc(&bill_info, items, &tenant_id);
                GetBillResponse {
                    bill: Some(bill),
                    error: None,
                    error_message: String::new(),
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
        let start_time = if req.start_time > 0 {
            Some(req.start_time)
        } else {
            None
        };
        let end_time = if req.end_time > 0 {
            Some(req.end_time)
        } else {
            None
        };
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
                let bills: Vec<mazbri_rpc::billing::Bill> = bills
                    .into_iter()
                    .map(|bill_info| map_bill_info_to_rpc(&bill_info, Vec::new(), &tenant_id))
                    .collect();
                let total_count = i32::saturating_from(bills.len());
                ListBillsResponse {
                    bills,
                    next_page_token: String::new(),
                    total_count,
                    error: None,
                    error_message: String::new(),
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
                let breakdown = match self
                    .billing_service
                    .get_usage_breakdown(tenant_id, req.start_time, req.end_time)
                    .await
                {
                    Ok(items) => items
                        .into_iter()
                        .map(|item| map_usage_breakdown(&item, &tenant_id))
                        .collect(),
                    Err(_) => Vec::new(),
                };
                let summary = UsageSummary {
                    total_requests,
                    total_bytes_in,
                    total_bytes_out,
                    total_billing_service_duration_ms,
                    average_billing_service_duration_ms,
                    total_cost: Some(safe_to_amount(
                        total_cost,
                        &tenant_id,
                        "get_usage_summary: total_cost",
                    )),
                    breakdown,
                };
                GetUsageSummaryResponse {
                    summary: Some(summary),
                    error: None,
                    error_message: String::new(),
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
