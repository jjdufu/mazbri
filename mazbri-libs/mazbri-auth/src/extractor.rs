use super::error::AuthError;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use uuid::Uuid;
#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub credential_type: CredentialType,
    pub credential_id: Option<String>,
    pub product_id: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialType {
    ApiKey,
    Jwt,
}
#[derive(Debug, Clone, Copy)]
pub struct TenantId(pub Uuid);
impl<S> FromRequestParts<S> for TenantId
where
    S: Send + Sync,
{
    type Rejection = AuthError;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let context = parts.extensions.get::<TenantContext>().ok_or_else(|| {
            AuthError::Core(mazbri_core::CoreError::Internal(
                "Tenant context not found".to_string(),
            ))
        })?;
        Ok(TenantId(context.tenant_id))
    }
}
impl FromRef<TenantContext> for TenantId {
    fn from_ref(ctx: &TenantContext) -> Self {
        TenantId(ctx.tenant_id)
    }
}
impl TenantContext {
    pub fn new(tenant_id: Uuid, credential_type: CredentialType) -> Self {
        Self {
            tenant_id,
            credential_type,
            credential_id: None,
            product_id: None,
        }
    }
    pub fn with_credential(
        tenant_id: Uuid,
        credential_type: CredentialType,
        credential_id: String,
    ) -> Self {
        Self {
            tenant_id,
            credential_type,
            credential_id: Some(credential_id),
            product_id: None,
        }
    }
    pub fn id(&self) -> Uuid {
        self.tenant_id
    }
    pub fn credential_type(&self) -> CredentialType {
        self.credential_type
    }
    pub fn is_api_key(&self) -> bool {
        self.credential_type == CredentialType::ApiKey
    }
    pub fn is_jwt(&self) -> bool {
        self.credential_type == CredentialType::Jwt
    }
    #[must_use]
    pub fn with_product_id(mut self, product_id: String) -> Self {
        self.product_id = Some(product_id);
        self
    }
    pub fn product_id(&self) -> Option<&str> {
        self.product_id.as_deref()
    }
}
