use super::types::{
    ActorType, AuditAction, AuditLogChanges, AuditLogDetailResponse, AuditLogListResponse,
    AuditLogQuery, AuditLogRequest, AuditLogResponse, AuditResourceType, AuditStatus,
};
use crate::sanitize::sanitize_json_value;
use axum::http::HeaderMap;
use sqlx::{Postgres, QueryBuilder, Row};
use std::fmt::Write as _;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, error};
#[must_use]
pub fn extract_client_ip(headers: &HeaderMap, socket_addr: Option<SocketAddr>) -> Option<String> {
    for header_name in [
        "x-real-ip",
        "x-forwarded-for",
        "x-client-ip",
        "cf-connecting-ip",
        "forwarded",
    ] {
        if let Some(header_value) = headers.get(header_name) {
            if let Ok(raw_value) = header_value.to_str() {
                if let Some(ip) = normalize_client_ip(raw_value) {
                    return Some(ip);
                }
            }
        }
    }
    socket_addr
        .map(|addr| addr.ip().to_string())
        .and_then(|ip| normalize_client_ip(&ip))
}
fn normalize_client_ip(raw: &str) -> Option<String> {
    let candidate = raw
        .split(',')
        .next()
        .map(str::trim)
        .unwrap_or_default()
        .trim_matches('"');
    if candidate.is_empty() || candidate.eq_ignore_ascii_case("unknown") {
        return None;
    }
    let candidate = if let Some(forwarded_for) = candidate.strip_prefix("for=") {
        forwarded_for
            .split(';')
            .next()
            .map(str::trim)
            .unwrap_or_default()
            .trim_matches('"')
    } else {
        candidate
    };
    let candidate = if candidate.starts_with('[') {
        candidate
            .trim_start_matches('[')
            .split(']')
            .next()
            .unwrap_or_default()
    } else {
        candidate
    };
    if let Ok(ip) = candidate.parse::<IpAddr>() {
        return Some(ip.to_string());
    }
    if let Ok(socket_addr) = candidate.parse::<SocketAddr>() {
        return Some(socket_addr.ip().to_string());
    }
    if let Some((host, port)) = candidate.rsplit_once(':') {
        if !host.contains(':')
            && port.chars().all(|ch| ch.is_ascii_digit())
            && let Ok(ip) = host.parse::<IpAddr>()
        {
            return Some(ip.to_string());
        }
    }
    Some(candidate.to_string())
}
fn fallback_ip_from_details(details: &serde_json::Value) -> Option<String> {
    ["ip_address", "client_ip", "actor_ip"]
        .iter()
        .find_map(|key| details.get(key).and_then(serde_json::Value::as_str))
        .and_then(normalize_client_ip)
}
fn resolve_ip_address(ip_address: Option<&str>, details: &serde_json::Value) -> Option<String> {
    ip_address
        .and_then(normalize_client_ip)
        .or_else(|| fallback_ip_from_details(details))
}
const ACTION_LABEL_SQL: &str = "CASE action \
    WHEN 'admin.login' THEN '管理员登录' \
    WHEN 'admin.logout' THEN '管理员登出' \
    WHEN 'admin.create' THEN '新增管理员' \
    WHEN 'admin.update' THEN '更新管理员' \
    WHEN 'admin.delete' THEN '删除管理员' \
    WHEN 'admin.password_reset' THEN '重置管理员密码' \
    WHEN 'admin.status_change' THEN '修改管理员状态' \
    WHEN 'tenant.create' THEN '创建租户' \
    WHEN 'tenant.update' THEN '更新租户' \
    WHEN 'tenant.delete' THEN '删除租户' \
    WHEN 'tenant.suspend' THEN '暂停租户' \
    WHEN 'tenant.activate' THEN '激活租户' \
    WHEN 'apikey.create' THEN '创建 API 密钥' \
    WHEN 'apikey.update' THEN '更新 API 密钥' \
    WHEN 'apikey.revoke' THEN '撤销 API 密钥' \
    WHEN 'user.login' THEN '登录' \
    WHEN 'user.logout' THEN '登出' \
    WHEN 'user.update' THEN '更新用户' \
    WHEN 'user.delete' THEN '删除用户' \
    WHEN 'quota.update' THEN '更新授权配置' \
    WHEN 'plan.change' THEN '变更套餐' \
    WHEN 'product.grant' THEN '授权产品' \
    WHEN 'product.revoke' THEN '撤销产品授权' \
    WHEN 'product.create' THEN '创建产品' \
    WHEN 'product.update' THEN '更新产品' \
    WHEN 'product.delete' THEN '删除产品' \
    WHEN 'product.reservation_create' THEN '预留产品编码' \
    WHEN 'product.reservation_release' THEN '释放产品编码' \
    WHEN 'account.recharge' THEN '账户充值' \
    WHEN 'account.refund' THEN '账户退款' \
    WHEN 'bill.generate' THEN '生成账单' \
    WHEN 'bill.delete' THEN '删除账单' \
    WHEN 'bill.status_update' THEN '更新账单状态' \
    WHEN 'account.adjustment' THEN '账户调整' \
    WHEN 'billing.reconciliation_audit' THEN '用量对账审计' \
    WHEN 'billing.reconciliation_discrepancy_update' THEN '对账差异处理' \
    WHEN 'billing.reconciliation_discrepancy_delete' THEN '对账差异删除' \
    WHEN 'billing.bill_adjustment_create' THEN '创建账单更正' \
    WHEN 'billing.bill_adjustment_void' THEN '作废账单更正' \
    WHEN 'billing.reconciliation_settings_update' THEN '更新用量对账设置' \
    WHEN 'certificate.issue' THEN '签发证书' \
    WHEN 'certificate.renew' THEN '续期证书' \
    WHEN 'certificate.revoke' THEN '吊销证书' \
    WHEN 'settings.update' THEN '更新系统设置' \
    WHEN 'password_policy.update' THEN '更新密码策略' \
    WHEN 'config_center.create' THEN '创建配置' \
    WHEN 'config_center.update' THEN '更新配置' \
    WHEN 'config_center.delete' THEN '删除配置' \
    WHEN 'config_center.rollback' THEN '回滚配置' \
    WHEN 'config_center.export' THEN '导出配置' \
    WHEN 'config_center.import' THEN '导入配置' \
    WHEN 'appearance.update' THEN '更新外观设置' \
    WHEN 'appearance.reset' THEN '重置外观设置' \
    WHEN 'mfa_settings.update' THEN '更新 MFA 设置' \
    WHEN 'tenant.whitelist_add' THEN '添加白名单' \
    WHEN 'tenant.whitelist_remove' THEN '删除白名单' \
    WHEN 'tenant.crypto_profile_update' THEN '更新租户加密配置' \
    WHEN 'tenant.crypto_key_create' THEN '创建租户加密密钥' \
    WHEN 'tenant.crypto_key_rotate' THEN '轮换租户加密密钥' \
    WHEN 'tenant.crypto_key_activate' THEN '激活租户加密密钥' \
    WHEN 'tenant.crypto_key_deprecate' THEN '废弃租户加密密钥' \
    WHEN 'tenant.crypto_key_disable' THEN '禁用租户加密密钥' \
    WHEN 'tenant.crypto_export' THEN '导出租户加密数据' \
    WHEN 'tenant.crypto_verify' THEN '校验租户加密密钥' \
    WHEN 'password.first_login_change' THEN '首次登录修改密码' \
    WHEN 'password.change' THEN '修改密码' \
    WHEN 'user.settings_update' THEN '更新用户设置' \
    WHEN 'tenant.ratelimit_update' THEN '更新限流配置' \
    WHEN 'tenant.ratelimit_delete' THEN '删除限流配置' \
    WHEN 'tenant.cache_update' THEN '更新缓存配置' \
    WHEN 'callback.delivery_retry' THEN '重试回调投递' \
    WHEN 'route.emergency_switch' THEN '紧急切流' \
    WHEN 'supplier.create' THEN '创建供应商' \
    WHEN 'supplier.update' THEN '更新供应商' \
    WHEN 'supplier.delete' THEN '删除供应商' \
    WHEN 'channel.create' THEN '创建通道' \
    WHEN 'channel.update' THEN '更新通道' \
    WHEN 'channel.delete' THEN '删除通道' \
    WHEN 'route_rule.create' THEN '创建路由规则' \
    WHEN 'route_rule.update' THEN '更新路由规则' \
    WHEN 'route_rule.delete' THEN '删除路由规则' \
    WHEN 'route_channel_binding.create' THEN '创建路由通道绑定' \
    WHEN 'route_channel_binding.update' THEN '更新路由通道绑定' \
    WHEN 'route_channel_binding.delete' THEN '删除路由通道绑定' \
    WHEN 'callback.subscription_create' THEN '创建回调订阅' \
    WHEN 'callback.subscription_update' THEN '更新回调订阅' \
    WHEN 'callback.subscription_delete' THEN '删除回调订阅' \
    WHEN 'authorization.import' THEN '导入授权包' \
    WHEN 'authorization.export' THEN '导出当前授权包' \
    WHEN 'authorization.renew' THEN '续期授权包' \
    WHEN 'authorization.activate' THEN '激活授权包' \
    WHEN 'authorization.revoke' THEN '撤销授权包' \
    WHEN 'authorization.rotate' THEN '轮换授权包' \
    WHEN 'authorization.disable' THEN '禁用授权包' \
    ELSE replace(replace(action, '.', ' / '), '_', ' ') \
END";
const RESOURCE_TYPE_LABEL_SQL: &str = "CASE resource_type \
    WHEN 'admin' THEN '管理员' \
    WHEN 'tenant' THEN '租户' \
    WHEN 'product' THEN '产品' \
    WHEN 'apikey' THEN 'API 密钥' \
    WHEN 'certificate' THEN '证书' \
    WHEN 'system' THEN '系统' \
    WHEN 'user' THEN '用户' \
    WHEN 'quota' THEN '授权配置' \
    WHEN 'plan' THEN '套餐' \
    WHEN 'account' THEN '账户' \
    WHEN 'config_center' THEN '配置中心' \
    WHEN 'route_rule' THEN '路由规则' \
    WHEN 'route_channel_binding' THEN '路由通道绑定' \
    WHEN 'supplier' THEN '供应商' \
    WHEN 'channel' THEN '通道' \
    WHEN 'callback' THEN '回调' \
    ELSE replace(resource_type, '_', ' ') \
END";
const CREATED_AT_SEARCH_SQL: &str =
    "to_char(created_at AT TIME ZONE 'Asia/Shanghai', 'YYYY/FMMM/FMDD HH24:MI:SS')";
fn push_audit_keyword_filter(builder: &mut QueryBuilder<'_, Postgres>, keyword: &str) {
    let pattern = format!("%{keyword}%");
    builder.push(" AND (username ILIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" OR action ILIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" OR ");
    builder.push(ACTION_LABEL_SQL);
    builder.push(" ILIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" OR resource_type ILIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" OR ");
    builder.push(RESOURCE_TYPE_LABEL_SQL);
    builder.push(" ILIKE ");
    builder.push_bind(pattern.clone());
    builder
        .push(
            " OR COALESCE(ip_address, details->>'ip_address', details->>'client_ip', details->>'actor_ip') ILIKE ",
        );
    builder.push_bind(pattern.clone());
    builder.push(" OR ");
    builder.push(CREATED_AT_SEARCH_SQL);
    builder.push(" ILIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" OR replace(");
    builder.push(CREATED_AT_SEARCH_SQL);
    builder.push(", '/', '-') ILIKE ");
    builder.push_bind(pattern);
    builder.push(" OR status ILIKE ");
    builder.push_bind(format!("%{keyword}%"));
    builder.push(" OR CASE status WHEN 'success' THEN '成功' WHEN 'failure' THEN '失败' ELSE status END ILIKE ");
    builder.push_bind(format!("%{keyword}%"));
    builder.push(")");
}
#[derive(Clone)]
pub struct AuditService {
    db: Arc<sqlx::PgPool>,
}
impl AuditService {
    #[must_use]
    pub fn new(db: Arc<sqlx::PgPool>) -> Self {
        Self { db }
    }
    pub fn log(&self, request: AuditLogRequest) {
        let db = Arc::clone(&self.db);
        let action = request.action.as_str().to_string();
        let resource_type = request.resource_type.as_str().to_string();
        let actor_type = request.actor_type.as_str().to_string();
        let status = request.status.as_str().to_string();
        tokio::spawn(async move {
            let query = r"
                INSERT INTO audit_logs (
                    user_id, actor_type, action, resource_type, resource_id,
                    details, status, error_message, ip_address, user_agent,
                    old_value, new_value
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ";
            let result = sqlx::query(query)
                .bind(request.user_id)
                .bind(&actor_type)
                .bind(&action)
                .bind(&resource_type)
                .bind(&request.resource_id)
                .bind(&request.details)
                .bind(&status)
                .bind(&request.error_message)
                .bind(&request.ip_address)
                .bind(&request.user_agent)
                .bind(&request.old_value)
                .bind(&request.new_value)
                .execute(&*db)
                .await;
            match result {
                Ok(_) => {
                    debug!(
                        action = % action, resource_type = % resource_type, resource_id =
                        ? request.resource_id, status = % status, "审计日志已记录"
                    );
                }
                Err(e) => {
                    error!(
                        error = % e, action = % action, resource_type = % resource_type,
                        "记录审计日志失败"
                    );
                }
            }
        });
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn log_critical(&self, request: AuditLogRequest) -> Result<(), sqlx::Error> {
        self.log_async(request).await
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn log_async(&self, request: AuditLogRequest) -> Result<(), sqlx::Error> {
        let query = r"
            INSERT INTO audit_logs (
                user_id, actor_type, action, resource_type, resource_id,
                details, status, error_message, ip_address, user_agent,
                old_value, new_value
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ";
        sqlx::query(query)
            .bind(request.user_id)
            .bind(request.actor_type.as_str())
            .bind(request.action.as_str())
            .bind(request.resource_type.as_str())
            .bind(&request.resource_id)
            .bind(&request.details)
            .bind(request.status.as_str())
            .bind(&request.error_message)
            .bind(&request.ip_address)
            .bind(&request.user_agent)
            .bind(&request.old_value)
            .bind(&request.new_value)
            .execute(&*self.db)
            .await?;
        debug!(
            action = % request.action, resource_type = % request.resource_type,
            resource_id = ? request.resource_id, "审计日志已同步记录"
        );
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn list(&self, query: AuditLogQuery) -> Result<AuditLogListResponse, sqlx::Error> {
        let filters = crate::list_query::parse_and_validate_filters(
            query.filters.as_deref(),
            &crate::list_query::audit_logs_catalog(),
        )
        .map_err(|e| sqlx::Error::Protocol(e.message))?;
        let keyword = query
            .q
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                query
                    .actor_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            });
        let mut count_builder =
            sqlx::QueryBuilder::new("SELECT COUNT(*) FROM audit_logs_view WHERE 1=1");
        let mut select_builder = sqlx::QueryBuilder::new(
            "SELECT id, user_id, username, actor_type, action, \
             resource_type, resource_id, details, status, error_message, \
             ip_address, user_agent, old_value, new_value, created_at \
             FROM audit_logs_view WHERE 1=1",
        );
        push_audit_conditions(&mut count_builder, &query, keyword);
        push_audit_conditions(&mut select_builder, &query, keyword);
        crate::list_query::apply_pg_filters(&mut count_builder, &filters)
            .map_err(|e| sqlx::Error::Protocol(e.message))?;
        crate::list_query::apply_pg_filters(&mut select_builder, &filters)
            .map_err(|e| sqlx::Error::Protocol(e.message))?;
        let total: i64 = count_builder.build().fetch_one(&*self.db).await?.get(0);
        select_builder.push(" ORDER BY created_at DESC LIMIT ");
        select_builder.push_bind(query.effective_limit());
        select_builder.push(" OFFSET ");
        select_builder.push_bind(query.offset());
        let rows = select_builder.build().fetch_all(&*self.db).await?;
        let logs: Vec<AuditLogResponse> = rows.iter().map(audit_row_to_response).collect();
        Ok(AuditLogListResponse {
            logs,
            total,
            page: query.page,
            limit: query.limit.min(MAX_PAGE_SIZE),
        })
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_by_id(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<AuditLogDetailResponse>, sqlx::Error> {
        let query = r"
            SELECT id, user_id, username, actor_type, action,
                   resource_type, resource_id, details, status, error_message,
                   ip_address, user_agent, old_value, new_value, created_at
            FROM audit_logs_view
            WHERE id = $1
        ";
        let row = sqlx::query(query)
            .bind(id)
            .fetch_optional(&*self.db)
            .await?;
        Ok(row.map(|row| {
            let old_value: Option<serde_json::Value> = row.get("old_value");
            let new_value: Option<serde_json::Value> = row.get("new_value");
            let actor_type: Option<String> = row.get("actor_type");
            let status: Option<String> = row.get("status");
            let details: Option<serde_json::Value> = row.get("details");
            let details = details.unwrap_or(serde_json::json!({}));
            let changes = if old_value.is_some() || new_value.is_some() {
                Some(AuditLogChanges {
                    before: old_value,
                    after: new_value,
                })
            } else {
                None
            };
            AuditLogDetailResponse {
                id: row.get("id"),
                timestamp: row.get("created_at"),
                actor_id: row.get("user_id"),
                actor_type: actor_type.unwrap_or_else(|| "admin".to_string()),
                actor_name: row.get::<Option<String>, _>("username"),
                action: row.get("action"),
                resource_type: row.get("resource_type"),
                resource_id: row.get("resource_id"),
                details: details.clone(),
                ip_address: resolve_ip_address(
                    row.get::<Option<String>, _>("ip_address").as_deref(),
                    &details,
                ),
                user_agent: row.get("user_agent"),
                status: status.unwrap_or_else(|| "success".to_string()),
                error_message: row.get("error_message"),
                changes,
            }
        }))
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn export_csv(&self, query: AuditLogQuery) -> Result<String, sqlx::Error> {
        let tenant_id_str = query.tenant_id.map(|id| id.to_string());
        let mut select_builder = sqlx::QueryBuilder::new(
            "SELECT id, user_id, username, actor_type, action, \
             resource_type, resource_id, details, status, error_message, \
             ip_address, user_agent, created_at \
             FROM audit_logs_view WHERE 1=1",
        );
        if let Some(ref tid) = tenant_id_str {
            select_builder.push(" AND (details->>'tenant_id' = ");
            select_builder.push_bind(tid);
            select_builder.push(" OR resource_id = ");
            select_builder.push_bind(tid);
            select_builder.push(")");
        }
        if let Some(ref action) = query.action {
            select_builder.push(" AND action = ");
            select_builder.push_bind(action);
        }
        if let Some(ref resource_type) = query.resource_type {
            select_builder.push(" AND resource_type = ");
            select_builder.push_bind(resource_type);
        }
        if let Some(ref resource_id) = query.resource_id {
            select_builder.push(" AND resource_id = ");
            select_builder.push_bind(resource_id);
        }
        if let Some(actor_id) = query.actor_id {
            select_builder.push(" AND user_id = ");
            select_builder.push_bind(actor_id);
        }
        if let Some(ref actor_name) = query.actor_name {
            let actor_name = actor_name.trim();
            if !actor_name.is_empty() {
                push_audit_keyword_filter(&mut select_builder, actor_name);
            }
        }
        if let Some(ref status) = query.status {
            select_builder.push(" AND status = ");
            select_builder.push_bind(status);
        }
        if let Some(start_time) = query.start_time {
            if let Some(dt) = chrono::DateTime::from_timestamp(start_time, 0) {
                select_builder.push(" AND created_at >= ");
                select_builder.push_bind(dt);
            }
        }
        if let Some(end_time) = query.end_time {
            if let Some(dt) = chrono::DateTime::from_timestamp(end_time, 0) {
                select_builder.push(" AND created_at <= ");
                select_builder.push_bind(dt);
            }
        }
        select_builder.push(" ORDER BY created_at DESC LIMIT ");
        select_builder.push_bind(EXPORT_LIMIT);
        let rows = select_builder.build().fetch_all(&*self.db).await?;
        let mut csv = String::from(
            "\u{feff}id,timestamp,actor_id,actor_type,actor_name,action,resource_type,resource_id,details,status,error_message,ip_address,user_agent\n",
        );
        for row in rows {
            let id: uuid::Uuid = row.get("id");
            let timestamp: chrono::DateTime<chrono::Utc> = row.get("created_at");
            let actor_id: Option<uuid::Uuid> = row.get("user_id");
            let actor_type: Option<String> = row.get("actor_type");
            let actor_name: Option<String> = row.get("username");
            let action: String = row.get("action");
            let resource_type: Option<String> = row.get("resource_type");
            let resource_id: Option<String> = row.get("resource_id");
            let details: Option<serde_json::Value> = row.get("details");
            let status: Option<String> = row.get("status");
            let error_message: Option<String> = row.get("error_message");
            let ip_address: Option<String> = row.get("ip_address");
            let user_agent: Option<String> = row.get("user_agent");
            let details = details.unwrap_or_else(|| serde_json::json!({}));
            let resolved_ip = resolve_ip_address(ip_address.as_deref(), &details);
            let sanitized_details = sanitize_json_value(details);
            let _ = writeln!(
                csv,
                "{},{},{},{},{},{},{},{},{},{},{},{},{}",
                csv_escape(&id.to_string()),
                csv_escape(&timestamp.to_rfc3339()),
                csv_escape(&actor_id.map_or_else(|| "N/A".to_string(), |i| i.to_string())),
                csv_escape(&actor_type.unwrap_or_else(|| "N/A".to_string())),
                csv_escape(&actor_name.unwrap_or_else(|| "N/A".to_string())),
                csv_escape(&action),
                csv_escape(&resource_type.unwrap_or_else(|| "N/A".to_string())),
                csv_escape(&resource_id.unwrap_or_else(|| "N/A".to_string())),
                csv_escape(&sanitized_details.to_string()),
                csv_escape(&status.unwrap_or_else(|| "N/A".to_string())),
                csv_escape(&error_message.unwrap_or_else(|| "N/A".to_string())),
                csv_escape(&resolved_ip.unwrap_or_else(|| "N/A".to_string())),
                csv_escape(&user_agent.unwrap_or_else(|| "N/A".to_string()))
            );
        }
        Ok(csv)
    }
}
const MAX_PAGE_SIZE: u32 = 100;
const EXPORT_LIMIT: i64 = 10000;
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        let escaped = s.replace('"', "\"\"").replace('\r', "");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}
#[must_use]
pub fn create_audit_log(
    user_id: uuid::Uuid,
    action: AuditAction,
    resource_type: AuditResourceType,
    resource_id: impl Into<Option<String>>,
    details: impl Into<Option<serde_json::Value>>,
    ip_address: impl Into<Option<String>>,
) -> AuditLogRequest {
    AuditLogRequest {
        user_id,
        actor_type: ActorType::Admin,
        action,
        resource_type,
        resource_id: resource_id.into(),
        details: details.into(),
        ip_address: ip_address.into(),
        user_agent: None,
        status: AuditStatus::Success,
        error_message: None,
        old_value: None,
        new_value: None,
    }
}
#[must_use]
pub fn tenant_audit(
    user_id: uuid::Uuid,
    action: AuditAction,
    tenant_id: uuid::Uuid,
    details: impl Into<Option<serde_json::Value>>,
    ip_address: impl Into<Option<String>>,
) -> AuditLogRequest {
    create_audit_log(
        user_id,
        action,
        AuditResourceType::Tenant,
        tenant_id.to_string(),
        details.into(),
        ip_address.into(),
    )
}
#[must_use]
pub fn api_key_audit(
    user_id: uuid::Uuid,
    action: AuditAction,
    tenant_id: uuid::Uuid,
    key_prefix: &str,
    details: impl Into<Option<serde_json::Value>>,
    ip_address: impl Into<Option<String>>,
) -> AuditLogRequest {
    let mut audit_details = details.into().unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = audit_details.as_object_mut() {
        obj.insert(
            "tenant_id".to_string(),
            serde_json::json!(tenant_id.to_string()),
        );
        obj.insert("key_prefix".to_string(), serde_json::json!(key_prefix));
    }
    create_audit_log(
        user_id,
        action,
        AuditResourceType::ApiKey,
        key_prefix.to_string(),
        audit_details,
        ip_address.into(),
    )
}
#[must_use]
pub fn user_audit(
    user_id: uuid::Uuid,
    action: AuditAction,
    details: impl Into<Option<serde_json::Value>>,
    ip_address: impl Into<Option<String>>,
) -> AuditLogRequest {
    create_audit_log(
        user_id,
        action,
        AuditResourceType::User,
        user_id.to_string(),
        details.into(),
        ip_address.into(),
    )
}
#[must_use]
pub fn config_center_audit(
    user_id: uuid::Uuid,
    action: AuditAction,
    resource_id: impl Into<Option<String>>,
    details: impl Into<Option<serde_json::Value>>,
    ip_address: impl Into<Option<String>>,
) -> AuditLogRequest {
    create_audit_log(
        user_id,
        action,
        AuditResourceType::ConfigCenter,
        resource_id,
        details.into(),
        ip_address.into(),
    )
}

/// 对 count/select 两个查询追加相同的显式过滤条件。
fn push_audit_conditions(
    builder: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>,
    query: &AuditLogQuery,
    keyword: Option<&str>,
) {
    if let Some(tid) = query.tenant_id.map(|id| id.to_string()) {
        builder.push(" AND (details->>'tenant_id' = ");
        builder.push_bind(tid.clone());
        builder.push(" OR resource_id = ");
        builder.push_bind(tid);
        builder.push(")");
    }
    let eq_filters = [
        ("action", query.action.as_ref()),
        ("resource_type", query.resource_type.as_ref()),
        ("resource_id", query.resource_id.as_ref()),
    ];
    for (column, value) in eq_filters {
        if let Some(value) = value {
            builder.push(format!(" AND {column} = "));
            builder.push_bind(value.clone());
        }
    }
    if let Some(actor_id) = query.actor_id {
        builder.push(" AND user_id = ");
        builder.push_bind(actor_id);
    }
    if let Some(keyword) = keyword {
        push_audit_keyword_filter(builder, keyword);
    }
    if let Some(ref status) = query.status {
        builder.push(" AND status = ");
        builder.push_bind(status.clone());
    }
    if let Some(dt) = query
        .start_time
        .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
    {
        builder.push(" AND created_at >= ");
        builder.push_bind(dt);
    }
    if let Some(dt) = query
        .end_time
        .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
    {
        builder.push(" AND created_at <= ");
        builder.push_bind(dt);
    }
}

fn audit_row_to_response(row: &sqlx::postgres::PgRow) -> AuditLogResponse {
    let ip_address: Option<String> = row.get::<Option<String>, _>("ip_address");
    let user_agent: Option<String> = row.get::<Option<String>, _>("user_agent");
    let actor_type: Option<String> = row.get::<Option<String>, _>("actor_type");
    let status: Option<String> = row.get::<Option<String>, _>("status");
    let details: Option<serde_json::Value> = row.get("details");
    let details = details.unwrap_or(serde_json::json!({}));
    AuditLogResponse {
        id: row.get("id"),
        timestamp: row.get("created_at"),
        actor_id: row.get("user_id"),
        actor_type: actor_type.unwrap_or_else(|| "admin".to_string()),
        actor_name: row.get::<Option<String>, _>("username"),
        action: row.get("action"),
        resource_type: row.get("resource_type"),
        resource_id: row.get("resource_id"),
        details: details.clone(),
        ip_address: resolve_ip_address(ip_address.as_deref(), &details),
        user_agent,
        status: status.unwrap_or_else(|| "success".to_string()),
        error_message: row.get("error_message"),
    }
}
