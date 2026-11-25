use mazbri_auth::{IpWhitelist, WhitelistResult};
use mazbri_core::num::SaturatingFrom;
use moka::future::Cache;
use sqlx::{PgPool, Row};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

const TENANT_SECURITY_CACHE_TTL_SECS: u64 = 5;
const TENANT_SECURITY_CACHE_MAX_CAPACITY: u64 = 50_000;

/// Exact inbound HTTP body byte count captured after `axum::body::to_bytes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActualRequestBytes(pub usize);

#[derive(Debug, Clone)]
pub struct TenantSecuritySnapshot {
    pub code: String,
    pub name: String,
    pub enabled: bool,
    pub token_expiration_secs: Option<usize>,
    pub whitelist: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TenantAccessError {
    NotFound,
    Suspended,
    IpForbidden(String),
    Internal(String),
}

#[derive(Clone)]
pub struct TenantStatusAndWhitelistCache {
    db: Arc<PgPool>,
    cache: Cache<Uuid, Option<TenantSecuritySnapshot>>,
}

impl TenantStatusAndWhitelistCache {
    #[must_use]
    pub fn new(db: Arc<PgPool>) -> Self {
        let cache = Cache::builder()
            .max_capacity(TENANT_SECURITY_CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(TENANT_SECURITY_CACHE_TTL_SECS))
            .build();
        Self { db, cache }
    }

    pub fn invalidate_all(&self) {
        self.cache.invalidate_all();
    }

    /// Evaluates `enabled` and `whitelist` for an already-loaded snapshot (Fail-Closed on missing IP).
    ///
    /// # Errors
    ///
    /// Returns [`TenantAccessError`] when the tenant is disabled, IP is not whitelisted, or CIDR is malformed.
    pub fn evaluate_snapshot(
        snapshot: &TenantSecuritySnapshot,
        client_ip: Option<IpAddr>,
    ) -> Result<(), TenantAccessError> {
        if !snapshot.enabled {
            return Err(TenantAccessError::Suspended);
        }
        match IpWhitelist::evaluate_optional(client_ip, &snapshot.whitelist) {
            Ok(WhitelistResult::NotConfigured | WhitelistResult::Allowed) => Ok(()),
            Ok(WhitelistResult::Denied(reason)) => Err(TenantAccessError::IpForbidden(reason)),
            Err(err) => Err(TenantAccessError::Internal(err.to_string())),
        }
    }

    /// Loads (with 5s bounded cache) and verifies both `tenants.enabled == true` and active `ip_whitelist`.
    ///
    /// # Errors
    ///
    /// Returns [`TenantAccessError`] when the tenant does not exist, is disabled, fails IP whitelist,
    /// or the database query fails.
    pub async fn verify_tenant_and_ip(
        &self,
        tenant_id: Uuid,
        client_ip: Option<IpAddr>,
    ) -> Result<TenantSecuritySnapshot, TenantAccessError> {
        let maybe_snapshot = self
            .cache
            .try_get_with(tenant_id, self.fetch_from_db(tenant_id))
            .await
            .map_err(|err| TenantAccessError::Internal(err.to_string()))?;
        let snapshot = maybe_snapshot.ok_or(TenantAccessError::NotFound)?;
        Self::evaluate_snapshot(&snapshot, client_ip)?;
        Ok(snapshot)
    }

    async fn fetch_from_db(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<TenantSecuritySnapshot>, String> {
        let tenant_row = sqlx::query(
            r"
            SELECT tenant_code, name, enabled, token_expiration_secs
            FROM tenants
            WHERE id = $1
            ",
        )
        .bind(tenant_id)
        .fetch_optional(&*self.db)
        .await
        .map_err(|err| format!("query tenants failed: {err}"))?;

        let Some(row) = tenant_row else {
            return Ok(None);
        };
        let code: String = row
            .try_get("tenant_code")
            .map_err(|err| format!("decode tenant_code: {err}"))?;
        let name: String = row
            .try_get("name")
            .map_err(|err| format!("decode name: {err}"))?;
        let enabled: bool = row
            .try_get("enabled")
            .map_err(|err| format!("decode enabled: {err}"))?;
        let token_expiration_secs: Option<i32> = row
            .try_get("token_expiration_secs")
            .map_err(|err| format!("decode token_expiration_secs: {err}"))?;

        let whitelist_rows = sqlx::query(
            r"
            SELECT host(ip_address) AS ip_address, cidr
            FROM ip_whitelist
            WHERE tenant_id = $1 AND is_active = true
            ",
        )
        .bind(tenant_id)
        .fetch_all(&*self.db)
        .await
        .map_err(|err| format!("query ip_whitelist failed: {err}"))?;

        let mut whitelist = Vec::with_capacity(whitelist_rows.len());
        for wl_row in whitelist_rows {
            let ip_address: String = wl_row
                .try_get("ip_address")
                .map_err(|err| format!("decode ip_address: {err}"))?;
            let cidr: i32 = wl_row
                .try_get("cidr")
                .map_err(|err| format!("decode cidr: {err}"))?;
            whitelist.push(format!("{ip_address}/{cidr}"));
        }

        Ok(Some(TenantSecuritySnapshot {
            code,
            name,
            enabled,
            token_expiration_secs: token_expiration_secs.map(usize::saturating_from),
            whitelist,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot(enabled: bool, whitelist: Vec<&str>) -> TenantSecuritySnapshot {
        TenantSecuritySnapshot {
            code: "TENANT01".into(),
            name: "Tenant One".into(),
            enabled,
            token_expiration_secs: Some(1800),
            whitelist: whitelist.into_iter().map(str::to_string).collect(),
        }
    }

    #[test]
    fn disabled_tenant_is_rejected() {
        let snap = sample_snapshot(false, vec![]);
        assert_eq!(
            TenantStatusAndWhitelistCache::evaluate_snapshot(&snap, None),
            Err(TenantAccessError::Suspended)
        );
    }

    #[test]
    fn whitelist_enforces_cidr_and_fails_closed_when_ip_missing() {
        let snap = sample_snapshot(true, vec!["10.99.0.0/16"]);
        assert_eq!(
            TenantStatusAndWhitelistCache::evaluate_snapshot(&snap, None),
            Err(TenantAccessError::IpForbidden("missing_client_ip".into()))
        );
        let denied_ip: IpAddr = "203.0.113.5".parse().unwrap();
        assert_eq!(
            TenantStatusAndWhitelistCache::evaluate_snapshot(&snap, Some(denied_ip)),
            Err(TenantAccessError::IpForbidden("203.0.113.5".into()))
        );
        let allowed_ip: IpAddr = "10.99.4.12".parse().unwrap();
        assert_eq!(
            TenantStatusAndWhitelistCache::evaluate_snapshot(&snap, Some(allowed_ip)),
            Ok(())
        );
    }
}
