use uuid::Uuid;
#[derive(Debug, thiserror::Error)]
pub enum TenantValidationError {
    #[error("tenant_id is empty or blank")]
    Empty,
    #[error("tenant_id is not a valid UUID: {0}")]
    InvalidFormat(String),
}
pub fn validate_tenant_id(tenant_id: &str) -> Result<Uuid, TenantValidationError> {
    let trimmed = tenant_id.trim();
    if trimmed.is_empty() {
        return Err(TenantValidationError::Empty);
    }
    Uuid::parse_str(trimmed)
        .map_err(|e| TenantValidationError::InvalidFormat(format!("{tenant_id}: {e}")))
}
