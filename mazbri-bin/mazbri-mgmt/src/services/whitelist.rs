use crate::repositories::whitelist as repo;
use sqlx::PgPool;
use std::net::IpAddr;
#[derive(Debug, serde::Serialize)]
pub struct AddResult {
    pub added: Vec<repo::InsertedEntry>,
    pub errors: Vec<serde_json::Value>,
    pub total_added: usize,
    pub total_errors: usize,
}
pub enum RemoveError {
    NotFound(String),
    InvalidFormat(String),
    Database(String),
}
impl std::fmt::Display for RemoveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) | Self::InvalidFormat(msg) | Self::Database(msg) => {
                write!(f, "{msg}")
            }
        }
    }
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn parse_ip_with_cidr(entry: &str) -> Result<(IpAddr, i32), serde_json::Value> {
    if entry.contains('/') {
        let parts: Vec<&str> = entry.split('/').collect();
        if parts.len() != 2 {
            return Err(serde_json::json!(
                { "ip_address" : entry, "error" : "invalid_cidr_format" }
            ));
        }
        let ip = parts[0];
        let cidr_str = parts[1];
        let ip_addr = ip.parse::<IpAddr>().map_err(|_| {
            serde_json::json!(
                { "ip_address" : entry, "error" : "invalid_ip_format" }
            )
        })?;
        let cidr: i32 = match cidr_str.parse() {
            Ok(c) => c,
            Err(_) => {
                return Err(serde_json::json!(
                    { "ip_address" : entry, "error" : "invalid_cidr_prefix" }
                ));
            }
        };
        if !(0..=32).contains(&cidr) {
            return Err(serde_json::json!(
                { "ip_address" : entry, "error" : "cidr_out_of_range" }
            ));
        }
        Ok((ip_addr, cidr))
    } else {
        let addr = entry.parse::<IpAddr>().map_err(|_| {
            serde_json::json!(
                { "ip_address" : entry, "error" : "invalid_ip_format" }
            )
        })?;
        Ok((addr, 32))
    }
}
pub async fn add_entries(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    ip_addresses: &[String],
    description: &Option<String>,
) -> AddResult {
    let mut added_entries = Vec::new();
    let mut errors = Vec::new();
    for entry in ip_addresses {
        let (ip_address, cidr) = match parse_ip_with_cidr(entry) {
            Ok(result) => result,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        match repo::insert_entry(pool, tenant_id, ip_address, cidr, description).await {
            Ok(inserted) => added_entries.push(inserted),
            Err(e) => {
                errors.push(serde_json::json!(
                    { "ip_address" : entry, "error" :
                    format!("database_error: {}", e) }
                ));
            }
        }
    }
    let total_added = added_entries.len();
    let total_errors = errors.len();
    AddResult {
        added: added_entries,
        errors,
        total_added,
        total_errors,
    }
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn remove_entry(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    ip: &str,
) -> Result<String, RemoveError> {
    let (ip_address, cidr): (IpAddr, i32) = if ip.contains('/') {
        let network: ipnetwork::IpNetwork = ip.parse().map_err(|_| {
            RemoveError::InvalidFormat(format!("Invalid IP network format: {ip}"))
        })?;
        (network.ip(), i32::from(network.prefix()))
    } else {
        let addr = ip.parse().map_err(|_| {
            RemoveError::InvalidFormat(format!("Invalid IP address format: {ip}"))
        })?;
        (addr, 32)
    };
    match repo::delete_entry(pool, tenant_id, ip_address, cidr).await {
        Ok(true) => Ok(format!("{ip_address}/{cidr}")),
        Ok(false) => Err(RemoveError::NotFound(format!(
            "Whitelist entry not found for IP: {ip_address}/{cidr}"
        ))),
        Err(e) => Err(RemoveError::Database(format!(
            "Failed to remove IP from whitelist: {e}"
        ))),
    }
}
