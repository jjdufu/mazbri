use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessCheckResult {
    pub name: String,
    pub status: CheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessResult {
    pub status: CheckStatus,
    pub checks: Vec<ReadinessCheckResult>,
}
impl ReadinessResult {
    #[must_use]
    pub fn healthy(checks: Vec<ReadinessCheckResult>) -> Self {
        let all_ok = checks.iter().all(|c| c.status == CheckStatus::Ok);
        Self {
            status: if all_ok {
                CheckStatus::Ok
            } else {
                CheckStatus::Failed
            },
            checks,
        }
    }
}
