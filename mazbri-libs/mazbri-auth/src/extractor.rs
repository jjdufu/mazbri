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
    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<TenantContext>()
            .map(|context| TenantId(context.tenant_id))
            .ok_or_else(|| {
                AuthError::Core(mazbri_core::CoreError::Internal(
                    "Tenant context not found".to_string(),
                ))
            });
        std::future::ready(result)
    }
}
impl FromRef<TenantContext> for TenantId {
    fn from_ref(ctx: &TenantContext) -> Self {
        TenantId(ctx.tenant_id)
    }
}
impl TenantContext {
    #[must_use]
    pub fn new(tenant_id: Uuid, credential_type: CredentialType) -> Self {
        Self {
            tenant_id,
            credential_type,
            credential_id: None,
            product_id: None,
        }
    }
    #[must_use]
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
    #[must_use]
    pub fn id(&self) -> Uuid {
        self.tenant_id
    }
    #[must_use]
    pub fn credential_type(&self) -> CredentialType {
        self.credential_type
    }
    #[must_use]
    pub fn is_api_key(&self) -> bool {
        self.credential_type == CredentialType::ApiKey
    }
    #[must_use]
    pub fn is_jwt(&self) -> bool {
        self.credential_type == CredentialType::Jwt
    }
    #[must_use]
    pub fn with_product_id(mut self, product_id: String) -> Self {
        self.product_id = Some(product_id);
        self
    }
    #[must_use]
    pub fn product_id(&self) -> Option<&str> {
        self.product_id.as_deref()
    }
}
