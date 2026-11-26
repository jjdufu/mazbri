use mazbri_core::num::SaturatingFrom;
use moka::future::Cache;
use sqlx::{PgPool, Row, postgres::PgRow};
use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

const TENANT_SECURITY_CACHE_TTL_SECS: u64 = 300;
const TENANT_SECURITY_CACHE_MAX_CAPACITY: u64 = 50_000;

const SELECT_SINGLE_TENANT_SECURITY_SQL: &str = r"
SELECT t.id, t.tenant_code, t.name, t.enabled, t.token_expiration_secs,
       COALESCE(
           ARRAY(
               SELECT host(w.ip_address) || '/' || w.cidr::text
               FROM ip_whitelist w
               WHERE w.tenant_id = t.id AND w.is_active = true
           ),
           ARRAY[]::text[]
       ) AS whitelist
FROM tenants t
WHERE t.id = $1
";

const SELECT_ALL_TENANT_SECURITY_SQL: &str = r"
SELECT t.id, t.tenant_code, t.name, t.enabled, t.token_expiration_secs,
       COALESCE(
           ARRAY(
               SELECT host(w.ip_address) || '/' || w.cidr::text
               FROM ip_whitelist w
               WHERE w.tenant_id = t.id AND w.is_active = true
           ),
           ARRAY[]::text[]
       ) AS whitelist
FROM tenants t
";

/// Exact inbound HTTP body byte count captured after `axum::body::to_bytes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActualRequestBytes(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompiledCidr {
    V4 { net: u32, mask: u32 },
    V6 { net: u128, mask: u128 },
    NeverMatch,
}

#[derive(Debug, Clone)]
enum CompiledWhitelist {
    NotConfigured,
    Rules(Vec<CompiledCidr>),
    Invalid(String),
}

fn normalize_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(v4) => IpAddr::V4(v4),
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
    }
}

impl CompiledWhitelist {
    fn compile(entries: &[String]) -> Self {
        if entries.is_empty() {
            return Self::NotConfigured;
        }
        let mut rules = Vec::with_capacity(entries.len());
        for cidr in entries {
            let Some((ip_part, prefix_part)) = cidr.split_once('/') else {
                return Self::Invalid(format!("Invalid CIDR format: {cidr}"));
            };
            if prefix_part.contains('/') {
                return Self::Invalid(format!("Invalid CIDR format: {cidr}"));
            }
            let Ok(parsed_ip) = ip_part.parse::<IpAddr>() else {
                return Self::Invalid(format!("Invalid IP address '{ip_part}'"));
            };
            let Ok(prefix_len) = prefix_part.parse::<u8>() else {
                return Self::Invalid(format!("Invalid prefix length '{prefix_part}'"));
            };
            match normalize_ip(parsed_ip) {
                IpAddr::V4(v4) => {
                    if prefix_len > 32 {
                        rules.push(CompiledCidr::NeverMatch);
                    } else {
                        let mask = if prefix_len == 0 {
                            0
                        } else {
                            u32::MAX << (32 - prefix_len)
                        };
                        rules.push(CompiledCidr::V4 {
                            net: u32::from(v4) & mask,
                            mask,
                        });
                    }
                }
                IpAddr::V6(v6) => {
                    if prefix_len > 128 {
                        rules.push(CompiledCidr::NeverMatch);
                    } else {
                        let mask = if prefix_len == 0 {
                            0
                        } else {
                            u128::MAX << (128 - prefix_len)
                        };
                        rules.push(CompiledCidr::V6 {
                            net: u128::from(v6) & mask,
                            mask,
                        });
                    }
                }
            }
        }
        Self::Rules(rules)
    }

    fn evaluate(&self, client_ip: Option<IpAddr>) -> Result<(), TenantAccessError> {
        match self {
            Self::NotConfigured => Ok(()),
            Self::Invalid(msg) => Err(TenantAccessError::Internal(msg.clone())),
            Self::Rules(rules) => {
                let Some(raw_ip) = client_ip else {
                    return Err(TenantAccessError::IpForbidden(
                        "missing_client_ip".to_string(),
                    ));
                };
                let normalized = normalize_ip(raw_ip);
                let matched = match normalized {
                    IpAddr::V4(v4) => {
                        let bits = u32::from(v4);
                        rules.iter().any(|rule| {
                            matches!(rule, CompiledCidr::V4 { net, mask } if (bits & *mask) == *net)
                        })
                    }
                    IpAddr::V6(v6) => {
                        let bits = u128::from(v6);
                        rules.iter().any(|rule| {
                            matches!(rule, CompiledCidr::V6 { net, mask } if (bits & *mask) == *net)
                        })
                    }
                };
                if matched {
                    Ok(())
                } else {
                    Err(TenantAccessError::IpForbidden(normalized.to_string()))
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct TenantSecuritySnapshot {
    pub code: String,
    pub name: String,
    pub enabled: bool,
    pub token_expiration_secs: Option<usize>,
    compiled_whitelist: Arc<CompiledWhitelist>,
}

impl TenantSecuritySnapshot {
    #[must_use]
    pub fn new(
        code: String,
        name: String,
        enabled: bool,
        token_expiration_secs: Option<usize>,
        whitelist: &[String],
    ) -> Self {
        Self {
            code,
            name,
            enabled,
            token_expiration_secs,
            compiled_whitelist: Arc::new(CompiledWhitelist::compile(whitelist)),
        }
    }
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
    cache: Cache<Uuid, Option<Arc<TenantSecuritySnapshot>>>,
}

fn decode_tenant_security_row(
    row: &PgRow,
) -> Result<(Uuid, Arc<TenantSecuritySnapshot>), String> {
    let id: Uuid = row
        .try_get("id")
        .map_err(|err| format!("decode tenant id: {err}"))?;
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
    let whitelist: Vec<String> = row
        .try_get("whitelist")
        .map_err(|err| format!("decode whitelist: {err}"))?;
    let snapshot = TenantSecuritySnapshot::new(
        code,
        name,
        enabled,
        token_expiration_secs.map(usize::saturating_from),
        &whitelist,
    );
    Ok((id, Arc::new(snapshot)))
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

    /// Evaluates `enabled` and pre-compiled `whitelist` for an already-loaded snapshot (Fail-Closed on missing IP).
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
        snapshot.compiled_whitelist.evaluate(client_ip)
    }

    /// Preloads or refreshes all tenants' status and active IP whitelists in a single query,
    /// overwriting active entries in-place before evicting removed/negative entries (zero stampede).
    ///
    /// # Errors
    ///
    /// Returns error message when querying or decoding `tenants` / `ip_whitelist` fails.
    pub async fn warm_all_tenants(&self) -> Result<usize, String> {
        let rows = sqlx::query(SELECT_ALL_TENANT_SECURITY_SQL)
            .fetch_all(&*self.db)
            .await
            .map_err(|err| format!("warm_all_tenants query failed: {err}"))?;

        let mut loaded_ids = HashSet::with_capacity(rows.len());
        for row in &rows {
            let (id, snapshot) = decode_tenant_security_row(row)?;
            loaded_ids.insert(id);
            self.cache.insert(id, Some(snapshot)).await;
        }

        let stale_ids: Vec<Uuid> = self
            .cache
            .iter()
            .filter_map(|(cached_id, _)| (!loaded_ids.contains(&cached_id)).then_some(*cached_id))
            .collect();
        for stale_id in stale_ids {
            self.cache.invalidate(&stale_id).await;
        }
        Ok(loaded_ids.len())
    }

    /// Loads (with 300s bounded cache + event-driven refresh) and verifies `tenants.enabled == true` and `ip_whitelist`.
    ///
    /// # Errors
    ///
    /// Returns [`TenantAccessError`] when the tenant does not exist, is disabled, fails IP whitelist,
    /// or the database query fails.
    pub async fn verify_tenant_and_ip(
        &self,
        tenant_id: Uuid,
        client_ip: Option<IpAddr>,
    ) -> Result<Arc<TenantSecuritySnapshot>, TenantAccessError> {
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
    ) -> Result<Option<Arc<TenantSecuritySnapshot>>, String> {
        let maybe_row = sqlx::query(SELECT_SINGLE_TENANT_SECURITY_SQL)
            .bind(tenant_id)
            .fetch_optional(&*self.db)
            .await
            .map_err(|err| format!("query tenant security failed: {err}"))?;

        let Some(row) = maybe_row else {
            return Ok(None);
        };
        let (_, snapshot) = decode_tenant_security_row(&row)?;
        Ok(Some(snapshot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot(enabled: bool, whitelist: &[&str]) -> TenantSecuritySnapshot {
        let raw: Vec<String> = whitelist.iter().map(|s| (*s).to_string()).collect();
        TenantSecuritySnapshot::new(
            "TENANT01".into(),
            "Tenant One".into(),
            enabled,
            Some(1800),
            &raw,
        )
    }

    #[test]
    fn disabled_tenant_is_rejected() {
        let snap = sample_snapshot(false, &[]);
        assert_eq!(
            TenantStatusAndWhitelistCache::evaluate_snapshot(&snap, None),
            Err(TenantAccessError::Suspended)
        );
    }

    #[test]
    fn whitelist_enforces_cidr_and_fails_closed_when_ip_missing() {
        let snap = sample_snapshot(true, &["10.99.0.0/16", "2001:db8::/32"]);
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
        let mapped_v6: IpAddr = "::ffff:10.99.8.9".parse().unwrap();
        assert_eq!(
            TenantStatusAndWhitelistCache::evaluate_snapshot(&snap, Some(mapped_v6)),
            Ok(())
        );
        let allowed_v6: IpAddr = "2001:db8:abcd::1".parse().unwrap();
        assert_eq!(
            TenantStatusAndWhitelistCache::evaluate_snapshot(&snap, Some(allowed_v6)),
            Ok(())
        );
    }

    #[test]
    fn malformed_cidr_fails_closed_with_internal_error_and_single_sql_joins_whitelist() {
        let bad = sample_snapshot(true, &["not-a-cidr"]);
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        assert!(matches!(
            TenantStatusAndWhitelistCache::evaluate_snapshot(&bad, Some(ip)),
            Err(TenantAccessError::Internal(_))
        ));
        assert!(SELECT_SINGLE_TENANT_SECURITY_SQL.contains("FROM ip_whitelist w"));
        assert!(SELECT_SINGLE_TENANT_SECURITY_SQL.contains("WHERE t.id = $1"));
    }
}
