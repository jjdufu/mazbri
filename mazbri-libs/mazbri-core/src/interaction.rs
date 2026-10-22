use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttemptStatus {
    Success,
    Failed(String),
    Timeout,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attempt {
    pub channel_id: String,
    pub supplier_id: Option<String>,
    pub supplier_code: Option<String>,
    pub supplier_name: Option<String>,
    pub channel_code: Option<String>,
    pub channel_name: Option<String>,
    pub endpoint_url: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: Option<AttemptStatus>,
}
#[derive(Debug, Clone, Copy)]
pub struct UpstreamAttempt<'a> {
    pub supplier_id: &'a str,
    pub supplier_code: &'a str,
    pub supplier_name: &'a str,
    pub channel_code: &'a str,
    pub channel_name: &'a str,
    pub endpoint_url: &'a str,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionEvent {
    pub id: Uuid,
    pub request_id: String,
    pub tenant_id: String,
    pub product_id: String,
    pub start_time: DateTime<Utc>,
    pub attempts: Vec<Attempt>,
    pub final_status: Option<String>,
}
impl InteractionEvent {
    #[must_use]
    pub fn new(request_id: &str, tenant_id: &str, product_id: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            request_id: request_id.to_string(),
            tenant_id: tenant_id.to_string(),
            product_id: product_id.to_string(),
            start_time: Utc::now(),
            attempts: Vec::new(),
            final_status: None,
        }
    }
    pub fn record_attempt(&mut self, channel_id: &str) {
        self.attempts.push(Attempt {
            channel_id: channel_id.to_string(),
            supplier_id: None,
            supplier_code: None,
            supplier_name: None,
            channel_code: None,
            channel_name: None,
            endpoint_url: None,
            start_time: Utc::now(),
            end_time: None,
            status: None,
        });
    }
    pub fn record_upstream_attempt(&mut self, channel_id: &str, attempt: UpstreamAttempt<'_>) {
        self.attempts.push(Attempt {
            channel_id: channel_id.to_string(),
            supplier_id: Some(attempt.supplier_id.to_string()),
            supplier_code: Some(attempt.supplier_code.to_string()),
            supplier_name: Some(attempt.supplier_name.to_string()),
            channel_code: Some(attempt.channel_code.to_string()),
            channel_name: Some(attempt.channel_name.to_string()),
            endpoint_url: Some(attempt.endpoint_url.to_string()),
            start_time: Utc::now(),
            end_time: None,
            status: None,
        });
    }
    pub fn mark_last_attempt(&mut self, status: AttemptStatus) {
        if let Some(attempt) = self.attempts.last_mut() {
            attempt.end_time = Some(Utc::now());
            attempt.status = Some(status);
        }
    }
    pub fn mark_final(&mut self, status: &str) {
        self.final_status = Some(status.to_string());
    }
}
