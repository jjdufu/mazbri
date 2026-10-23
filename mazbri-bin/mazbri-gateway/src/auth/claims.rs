use serde::{Deserialize, Serialize};
use std::fmt;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MazbriClaims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
    #[serde(default = "default_issuer")]
    pub iss: String,
    pub jti: String,
    pub tenant_id: String,
    pub tenant_code: String,
    pub tenant_name: String,
    pub account_type: AccountType,
    pub scopes: Vec<String>,
}
impl MazbriClaims {
    /// True if the token has any of the required scopes (or `api:admin`).
    #[must_use]
    pub fn has_any_scope(&self, required: &[&str]) -> bool {
        if self.scopes.iter().any(|s| s == "api:admin") {
            return true;
        }
        required
            .iter()
            .any(|need| self.scopes.iter().any(|have| have == need))
    }

    /// Proxy data-plane requires write (or admin).
    #[must_use]
    pub fn can_invoke(&self) -> bool {
        self.has_any_scope(&["api:write"])
    }
}
fn default_issuer() -> String {
    "mazbri.com".to_string()
}
/// Tenant plan tier claim. H-14: issuance always uses `Standard` until plan tiers are productized.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AccountType {
    Standard,
    Premium,
    Enterprise,
}
impl fmt::Display for AccountType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccountType::Standard => write!(f, "standard"),
            AccountType::Premium => write!(f, "premium"),
            AccountType::Enterprise => write!(f, "enterprise"),
        }
    }
}
impl std::str::FromStr for AccountType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "standard" => Ok(AccountType::Standard),
            "premium" => Ok(AccountType::Premium),
            "enterprise" => Ok(AccountType::Enterprise),
            _ => Err(format!("Invalid account type: {s}")),
        }
    }
}
