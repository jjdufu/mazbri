use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_core::{ErrorResponse, core_error_response_match};
use serde_json::json;
#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    #[error(transparent)]
    Core(#[from] mazbri_core::CoreError),
    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance {
        required: rust_decimal::Decimal,
        available: rust_decimal::Decimal,
    },
    #[error("Account not found: {0}")]
    AccountNotFound(uuid::Uuid),
    #[error("Bill not found: {0}")]
    BillNotFound(uuid::Uuid),
    #[error("Invalid billing configuration: {0}")]
    InvalidConfig(String),
    #[error("Invalid amount: {0}")]
    InvalidAmount(String),
    #[error("Billing calculation error: {0}")]
    CalculationError(String),
}
pub type BillingResult<T> = std::result::Result<T, BillingError>;
impl ErrorResponse for BillingError {
    fn status_code(&self) -> StatusCode {
        core_error_response_match!(self, status_code, {
            Self::InsufficientBalance { .. } => StatusCode::PAYMENT_REQUIRED,
            Self::AccountNotFound(_) | Self::BillNotFound(_) => StatusCode::NOT_FOUND,
            Self::InvalidConfig(_) | Self::InvalidAmount(_) => StatusCode::BAD_REQUEST,
            Self::CalculationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        })
    }
    fn error_code(&self) -> &'static str {
        core_error_response_match!(self, error_code, {
            Self::InsufficientBalance { .. } => "INSUFFICIENT_BALANCE",
            Self::AccountNotFound(_) => "ACCOUNT_NOT_FOUND",
            Self::BillNotFound(_) => "BILL_NOT_FOUND",
            Self::InvalidConfig(_) => "INVALID_CONFIG",
            Self::InvalidAmount(_) => "INVALID_AMOUNT",
            Self::CalculationError(_) => "CALCULATION_ERROR",
        })
    }
    fn safe_message(&self) -> String {
        core_error_response_match!(self, safe_message, {
            Self::InsufficientBalance { .. } => "余额不足".to_string(),
            Self::AccountNotFound(_) => "账户未找到".to_string(),
            Self::BillNotFound(_) => "账单未找到".to_string(),
            Self::InvalidConfig(_) => "无效的计费配置".to_string(),
            Self::InvalidAmount(_) => "无效的金额".to_string(),
            Self::CalculationError(_) => "计费计算错误".to_string(),
        })
    }
    fn details(&self) -> Option<serde_json::Value> {
        core_error_response_match!(self, details, {
            Self::InsufficientBalance {
                required,
                available,
            } => Some(json!(
                { "required" : required.to_string(), "available" : available
                .to_string() }
            )),
            Self::AccountNotFound(_) => None,
            Self::BillNotFound(_) => None,
            _ => None,
        })
    }
    fn detail(&self) -> Option<String> {
        core_error_response_match!(self, detail, {
            _ => Some(self.to_string()),
        })
    }
    fn title(&self) -> String {
        core_error_response_match!(self, title, {
            _ => self.to_string(),
        })
    }
}
impl IntoResponse for BillingError {
    fn into_response(self) -> Response {
        ErrorResponse::into_response(self)
    }
}
impl From<sqlx::Error> for BillingError {
    fn from(e: sqlx::Error) -> Self {
        Self::Core(mazbri_core::CoreError::from(e))
    }
}
