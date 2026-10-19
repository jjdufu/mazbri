mod charge;
mod conversion;
mod handlers;
use mazbri_billing::{BillingService, ChargeQueue};
use std::sync::Arc;
#[derive(Clone)]
pub struct BillingTarpcService {
    pub(crate) service_name: Arc<str>,
    pub(crate) billing_service: Arc<BillingService>,
    pub(crate) charge_queue: Option<Arc<ChargeQueue>>,
}
impl BillingTarpcService {
    pub fn new(
        service_name: impl Into<Arc<str>>,
        billing_service: Arc<BillingService>,
        charge_queue: Option<Arc<ChargeQueue>>,
    ) -> Self {
        Self {
            service_name: service_name.into(),
            billing_service,
            charge_queue,
        }
    }
}
