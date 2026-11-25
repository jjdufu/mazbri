use super::{AuthResult, error::AuthError};
use moka::future::Cache;
use sqlx::{PgPool, Row};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;
const WHITELIST_CACHE_TTL_SECS: u64 = 300;
const WHITELIST_CACHE_MAX_ENTRIES: u64 = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhitelistResult {
    NotConfigured,

    Allowed,

    Denied(String),
}

#[derive(Clone)]
pub struct IpWhitelist {
    db: Arc<PgPool>,
    cache: Cache<Uuid, Vec<String>>,
}
impl IpWhitelist {
    #[must_use]
    pub fn new(db: Arc<PgPool>) -> Self {
        Self {
            db,
            cache: Cache::builder()
                .max_capacity(WHITELIST_CACHE_MAX_ENTRIES)
                .time_to_live(Duration::from_secs(WHITELIST_CACHE_TTL_SECS))
                .build(),
        }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn check(&self, tenant_id: Uuid, ip_addr: IpAddr) -> AuthResult<WhitelistResult> {
        self.check_optional(tenant_id, Some(ip_addr)).await
    }

    /// Checks optional client IP against tenant's active IP whitelist.
    /// If the tenant has no active whitelist entries, returns `NotConfigured`.
    /// If the tenant has active whitelist entries and `ip_addr` is `None`, fails closed (`Denied`).
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn check_optional(
        &self,
        tenant_id: Uuid,
        ip_addr: Option<IpAddr>,
    ) -> AuthResult<WhitelistResult> {
        let whitelist = self.load_whitelist(tenant_id).await?;
        if whitelist.is_empty() {
            return Ok(WhitelistResult::NotConfigured);
        }
        let Some(ip_addr) = ip_addr else {
            return Ok(WhitelistResult::Denied("missing_client_ip".to_string()));
        };
        Self::evaluate_entries(ip_addr, &whitelist)
    }

    /// Evaluates a pre-loaded list of `"ip/cidr"` rules against `ip_addr` (Fail-Closed when `ip_addr` is `None`).
    ///
    /// # Errors
    ///
    /// CIDR 格式不合法时返回错误。
    pub fn evaluate_optional(
        ip_addr: Option<IpAddr>,
        whitelist: &[String],
    ) -> AuthResult<WhitelistResult> {
        if whitelist.is_empty() {
            return Ok(WhitelistResult::NotConfigured);
        }
        let Some(ip_addr) = ip_addr else {
            return Ok(WhitelistResult::Denied("missing_client_ip".to_string()));
        };
        Self::evaluate_entries(ip_addr, whitelist)
    }

    fn evaluate_entries(ip_addr: IpAddr, whitelist: &[String]) -> AuthResult<WhitelistResult> {
        let normalized_ip = Self::normalize_ip(ip_addr);
        for allowed_ip in whitelist {
            if Self::ip_matches(normalized_ip, allowed_ip)? {
                return Ok(WhitelistResult::Allowed);
            }
        }
        Ok(WhitelistResult::Denied(normalized_ip.to_string()))
    }

    fn normalize_ip(ip: IpAddr) -> IpAddr {
        match ip {
            IpAddr::V4(v4) => IpAddr::V4(v4),
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        }
    }
    async fn load_whitelist(&self, tenant_id: Uuid) -> AuthResult<Vec<String>> {
        if let Some(cached) = self.cache.get(&tenant_id).await {
            return Ok(cached);
        }
        let rows = sqlx::query(
            r"
            SELECT host(ip_address) AS ip_address, cidr
            FROM ip_whitelist
            WHERE tenant_id = $1 AND is_active = true
            ",
        )
        .bind(tenant_id)
        .fetch_all(&*self.db)
        .await?;
        let mut whitelist = Vec::new();
        for row in rows {
            let ip_address: String = row.try_get("ip_address")?;
            let cidr: i32 = row.try_get("cidr")?;
            whitelist.push(format!("{ip_address}/{cidr}"));
        }
        self.cache.insert(tenant_id, whitelist.clone()).await;
        Ok(whitelist)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn add_ip(
        &self,
        tenant_id: Uuid,
        ip_addr: IpAddr,
        cidr: u8,
        description: Option<String>,
    ) -> AuthResult<()> {
        sqlx::query(
            r"
            INSERT INTO ip_whitelist (tenant_id, ip_address, cidr, description)
            VALUES ($1, $2::inet, $3, $4)
            ON CONFLICT (tenant_id, ip_address, cidr) DO UPDATE
            SET description = COALESCE($4, ip_whitelist.description),
                updated_at = NOW(),
                is_active = true
            ",
        )
        .bind(tenant_id)
        .bind(ip_addr.to_string())
        .bind(i32::from(cidr))
        .bind(description)
        .execute(&*self.db)
        .await?;
        self.invalidate_cache(tenant_id).await;
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn remove_ip(&self, tenant_id: Uuid, ip_addr: IpAddr, cidr: u8) -> AuthResult<()> {
        let result = sqlx::query(
            r"
            UPDATE ip_whitelist
            SET is_active = false, updated_at = NOW()
            WHERE tenant_id = $1 AND ip_address = $2::inet AND cidr = $3
            ",
        )
        .bind(tenant_id)
        .bind(ip_addr.to_string())
        .bind(i32::from(cidr))
        .execute(&*self.db)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AuthError::Core(mazbri_core::CoreError::Internal(
                "IP whitelist entry not found".to_string(),
            )));
        }
        self.invalidate_cache(tenant_id).await;
        Ok(())
    }
    async fn invalidate_cache(&self, tenant_id: Uuid) {
        self.cache.invalidate(&tenant_id).await;
    }
    /// 丢掉全部白名单缓存。下一笔检查会重新查库。
    pub fn invalidate_all(&self) {
        self.cache.invalidate_all();
    }
    fn ip_matches(ip_addr: IpAddr, cidr: &str) -> AuthResult<bool> {
        let parts: Vec<&str> = cidr.split('/').collect();
        if parts.len() != 2 {
            return Err(AuthError::Core(mazbri_core::CoreError::Internal(format!(
                "Invalid CIDR format: {cidr}"
            ))));
        }
        let allowed_ip = Self::normalize_ip(parts[0].parse().map_err(|e| {
            AuthError::Core(mazbri_core::CoreError::Internal(format!(
                "Invalid IP address '{}': {}",
                parts[0], e
            )))
        })?);
        let ip_addr = Self::normalize_ip(ip_addr);
        let prefix_len: u8 = parts[1].parse().map_err(|e| {
            AuthError::Core(mazbri_core::CoreError::Internal(format!(
                "Invalid prefix length '{}': {}",
                parts[1], e
            )))
        })?;
        match (ip_addr, allowed_ip) {
            (IpAddr::V4(addr), IpAddr::V4(allowed)) => {
                Ok(Self::ipv4_matches(addr, allowed, prefix_len))
            }
            (IpAddr::V6(addr), IpAddr::V6(allowed)) => {
                Ok(Self::ipv6_matches(addr, allowed, prefix_len))
            }
            _ => Ok(false),
        }
    }
    fn ipv4_matches(addr: Ipv4Addr, allowed: Ipv4Addr, prefix_len: u8) -> bool {
        if prefix_len > 32 {
            return false;
        }
        let addr_bytes = addr.octets();
        let allowed_bytes = allowed.octets();
        let full_bytes = (prefix_len / 8) as usize;
        let partial_bits = prefix_len % 8;
        for i in 0..full_bytes {
            if addr_bytes[i] != allowed_bytes[i] {
                return false;
            }
        }
        if partial_bits > 0 && full_bytes < 4 {
            let mask = 0xFF << (8 - partial_bits);
            if (addr_bytes[full_bytes] & mask) != (allowed_bytes[full_bytes] & mask) {
                return false;
            }
        }
        true
    }
    fn ipv6_matches(addr: std::net::Ipv6Addr, allowed: std::net::Ipv6Addr, prefix_len: u8) -> bool {
        if prefix_len > 128 {
            return false;
        }
        let addr_bytes = addr.octets();
        let allowed_bytes = allowed.octets();
        let full_bytes = (prefix_len / 8) as usize;
        let partial_bits = prefix_len % 8;
        for i in 0..full_bytes {
            if addr_bytes[i] != allowed_bytes[i] {
                return false;
            }
        }
        if partial_bits > 0 && full_bytes < 16 {
            let mask = 0xFF << (8 - partial_bits);
            if (addr_bytes[full_bytes] & mask) != (allowed_bytes[full_bytes] & mask) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_optional_fails_closed_when_whitelist_configured_and_ip_missing() {
        let empty: Vec<String> = Vec::new();
        assert_eq!(
            IpWhitelist::evaluate_optional(None, &empty).expect("empty"),
            WhitelistResult::NotConfigured
        );
        let rules = vec!["203.0.113.0/24".to_string()];
        assert_eq!(
            IpWhitelist::evaluate_optional(None, &rules).expect("missing ip"),
            WhitelistResult::Denied("missing_client_ip".to_string())
        );
        let mapped: IpAddr = "::ffff:203.0.113.42".parse().expect("mapped v6");
        assert_eq!(
            IpWhitelist::evaluate_optional(Some(mapped), &rules).expect("mapped match"),
            WhitelistResult::Allowed
        );
    }
}
