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

    pub async fn check(&self, tenant_id: Uuid, ip_addr: IpAddr) -> AuthResult<WhitelistResult> {
        let whitelist = self.load_whitelist(tenant_id).await?;
        if whitelist.is_empty() {
            return Ok(WhitelistResult::NotConfigured);
        }
        for allowed_ip in &whitelist {
            if self.ip_matches(ip_addr, allowed_ip)? {
                return Ok(WhitelistResult::Allowed);
            }
        }
        Ok(WhitelistResult::Denied(ip_addr.to_string()))
    }
    async fn load_whitelist(&self, tenant_id: Uuid) -> AuthResult<Vec<String>> {
        if let Some(cached) = self.cache.get(&tenant_id).await {
            return Ok(cached);
        }
        let rows = sqlx::query(
            r"
            SELECT ip_address, cidr
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
            VALUES ($1, $2, $3, $4)
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
    pub async fn remove_ip(&self, tenant_id: Uuid, ip_addr: IpAddr, cidr: u8) -> AuthResult<()> {
        let result = sqlx::query(
            r"
            UPDATE ip_whitelist
            SET is_active = false, updated_at = NOW()
            WHERE tenant_id = $1 AND ip_address = $2 AND cidr = $3
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
    fn ip_matches(&self, ip_addr: IpAddr, cidr: &str) -> AuthResult<bool> {
        let parts: Vec<&str> = cidr.split('/').collect();
        if parts.len() != 2 {
            return Err(AuthError::Core(mazbri_core::CoreError::Internal(format!(
                "Invalid CIDR format: {cidr}"
            ))));
        }
        let allowed_ip: IpAddr = parts[0].parse().map_err(|e| {
            AuthError::Core(mazbri_core::CoreError::Internal(format!(
                "Invalid IP address '{}': {}",
                parts[0], e
            )))
        })?;
        let prefix_len: u8 = parts[1].parse().map_err(|e| {
            AuthError::Core(mazbri_core::CoreError::Internal(format!(
                "Invalid prefix length '{}': {}",
                parts[1], e
            )))
        })?;
        match (ip_addr, allowed_ip) {
            (IpAddr::V4(addr), IpAddr::V4(allowed)) => {
                Ok(self.ipv4_matches(addr, allowed, prefix_len))
            }
            (IpAddr::V6(addr), IpAddr::V6(allowed)) => {
                Ok(self.ipv6_matches(addr, allowed, prefix_len))
            }
            _ => Ok(false),
        }
    }
    fn ipv4_matches(&self, addr: Ipv4Addr, allowed: Ipv4Addr, prefix_len: u8) -> bool {
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
    fn ipv6_matches(
        &self,
        addr: std::net::Ipv6Addr,
        allowed: std::net::Ipv6Addr,
        prefix_len: u8,
    ) -> bool {
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
