use super::config::AuditConfig;
use super::scope;
use super::settings::AuditSettingsCache;
use super::sink::{
    AuditSink, AuditStore, AuditWriterHandle, FallbackWriter, PgAuditStore, SinkOptions,
};
use super::types::{
    ActorType, AuditAction, AuditLogChanges, AuditLogDetailResponse, AuditLogListResponse,
    AuditLogQuery, AuditLogRequest, AuditLogResponse, AuditRecord, AuditResourceType, AuditStatus,
};
use sqlx::{Postgres, QueryBuilder, Row};
use std::fmt::Write as _;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

/// 读取历史记录时规整 IP（旧数据可能带端口或 `for=` 前缀）。
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
    let candidate = candidate.strip_prefix("for=").map_or(candidate, |rest| {
        rest.split(';')
            .next()
            .map(str::trim)
            .unwrap_or_default()
            .trim_matches('"')
    });
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
    if let Some((host, port)) = candidate.rsplit_once(':')
        && !host.contains(':')
        && port.chars().all(|ch| ch.is_ascii_digit())
        && let Ok(ip) = host.parse::<IpAddr>()
    {
        return Some(ip.to_string());
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

/// 动作 → 中文标签（SQL 搜索与导出共用；前端标签见 `audit-logs/labels.ts`，由测试保证同步）。
pub const ACTION_LABELS: &[(&str, &str)] = &[
    ("admin.login", "管理员登录"),
    ("admin.logout", "管理员登出"),
    ("admin.create", "新增管理员"),
    ("admin.update", "更新管理员"),
    ("admin.delete", "删除管理员"),
    ("admin.password_reset", "重置管理员密码"),
    ("admin.status_change", "修改管理员状态"),
    ("tenant.create", "创建租户"),
    ("tenant.update", "更新租户"),
    ("tenant.delete", "删除租户"),
    ("tenant.suspend", "暂停租户"),
    ("tenant.activate", "激活租户"),
    ("apikey.create", "创建 API 密钥"),
    ("apikey.update", "更新 API 密钥"),
    ("apikey.revoke", "撤销 API 密钥"),
    ("user.login", "登录"),
    ("user.logout", "登出"),
    ("user.update", "更新用户"),
    ("user.delete", "删除用户"),
    ("quota.update", "更新授权配置"),
    ("plan.change", "变更套餐"),
    ("product.grant", "授权产品"),
    ("product.revoke", "撤销产品授权"),
    ("product.create", "创建产品"),
    ("product.update", "更新产品"),
    ("product.delete", "删除产品"),
    ("product.reservation_create", "预留产品编码"),
    ("product.reservation_release", "释放产品编码"),
    ("account.recharge", "账户充值"),
    ("account.refund", "账户退款"),
    ("bill.generate", "生成账单"),
    ("bill.delete", "删除账单"),
    ("bill.status_update", "更新账单状态"),
    ("account.adjustment", "账户调整"),
    ("billing.reconciliation_audit", "用量对账审计"),
    ("billing.reconciliation_discrepancy_update", "对账差异处理"),
    ("billing.reconciliation_discrepancy_delete", "对账差异删除"),
    ("billing.bill_adjustment_create", "创建账单更正"),
    ("billing.bill_adjustment_void", "作废账单更正"),
    ("billing.reconciliation_settings_update", "更新用量对账设置"),
    ("certificate.issue", "签发证书"),
    ("certificate.renew", "续期证书"),
    ("certificate.revoke", "吊销证书"),
    ("settings.update", "更新系统设置"),
    ("password_policy.update", "更新密码策略"),
    ("config_center.create", "创建配置"),
    ("config_center.update", "更新配置"),
    ("config_center.delete", "删除配置"),
    ("config_center.rollback", "回滚配置"),
    ("config_center.export", "导出配置"),
    ("config_center.import", "导入配置"),
    ("appearance.update", "更新外观设置"),
    ("appearance.reset", "重置外观设置"),
    ("mfa_settings.update", "更新 MFA 设置"),
    ("tenant.whitelist_add", "添加白名单"),
    ("tenant.whitelist_remove", "删除白名单"),
    ("tenant.crypto_profile_update", "更新租户加密配置"),
    ("tenant.crypto_key_create", "创建租户加密密钥"),
    ("tenant.crypto_key_rotate", "轮换租户加密密钥"),
    ("tenant.crypto_key_activate", "激活租户加密密钥"),
    ("tenant.crypto_key_deprecate", "废弃租户加密密钥"),
    ("tenant.crypto_key_disable", "禁用租户加密密钥"),
    ("tenant.crypto_export", "导出租户加密数据"),
    ("tenant.crypto_verify", "校验租户加密密钥"),
    ("password.first_login_change", "首次登录修改密码"),
    ("password.change", "修改密码"),
    ("user.settings_update", "更新用户设置"),
    ("tenant.ratelimit_update", "更新限流配置"),
    ("tenant.ratelimit_delete", "删除限流配置"),
    ("tenant.cache_update", "更新缓存配置"),
    ("callback.delivery_retry", "重试回调投递"),
    ("route.emergency_switch", "紧急切流"),
    ("supplier.create", "创建供应商"),
    ("supplier.update", "更新供应商"),
    ("supplier.delete", "删除供应商"),
    ("channel.create", "创建通道"),
    ("channel.update", "更新通道"),
    ("channel.delete", "删除通道"),
    ("route_rule.create", "创建路由规则"),
    ("route_rule.update", "更新路由规则"),
    ("route_rule.delete", "删除路由规则"),
    ("route_channel_binding.create", "创建路由通道绑定"),
    ("route_channel_binding.update", "更新路由通道绑定"),
    ("route_channel_binding.delete", "删除路由通道绑定"),
    ("callback.subscription_create", "创建回调订阅"),
    ("callback.subscription_update", "更新回调订阅"),
    ("callback.subscription_delete", "删除回调订阅"),
    ("authorization.import", "导入授权包"),
    ("authorization.export", "导出当前授权包"),
    ("authorization.renew", "续期授权包"),
    ("authorization.activate", "激活授权包"),
    ("authorization.revoke", "撤销授权包"),
    ("authorization.rotate", "轮换授权包"),
    ("authorization.disable", "禁用授权包"),
    ("tenant.ratelimit_command_create", "创建限流指令"),
    ("tenant.ratelimit_command_cancel", "取消限流指令"),
    ("tenant.crypto_key_delete", "删除租户加密密钥"),
    ("upstream.onboarding", "上游接入"),
    ("supplier.validate_config", "校验供应商配置"),
    ("connector.run", "运行连接器"),
    ("connector.publish", "发布连接器"),
    ("connector.force_publish", "强制发布连接器"),
    ("connector.rollback", "回滚连接器"),
    ("connector.import", "导入连接器"),
    ("admin.force_password_reset", "强制管理员改密"),
    ("admin.mfa_reset", "重置管理员 MFA"),
    ("user.mfa_setup", "初始化 MFA"),
    ("user.mfa_enable", "启用 MFA"),
    ("user.mfa_disable", "停用 MFA"),
    ("user.mfa_regenerate_codes", "重新生成 MFA 备用码"),
    ("user.mfa_required_setup", "强制 MFA 初始化"),
    ("user.mfa_required_enable", "强制 MFA 启用"),
    ("user.token_refresh", "刷新令牌"),
    ("tenant.request_key_create", "创建请求签名密钥"),
    ("tenant.request_key_retire", "退役请求签名密钥"),
    ("tenant.request_key_list", "查看请求签名密钥"),
    ("tenant.crypto_key_list", "查看租户加密密钥"),
    ("tenant.enabled_change", "启停租户"),
    ("apikey.list", "查看 API 密钥"),
    ("certificate.view_full", "查看完整证书"),
    ("certificate.auto_renewal_update", "更新证书自动续期"),
    ("authorization.license_request", "导出授权申请"),
    ("audit.list", "查看审计日志"),
    ("audit.view", "查看审计详情"),
    ("audit.export", "导出审计日志"),
    ("connector.draft_create", "创建连接器草稿"),
    ("connector.draft_update", "更新连接器草稿"),
    ("connector.draft_delete", "删除连接器草稿"),
    ("connector.draft_validate", "校验连接器草稿"),
    ("connector.transform_preview", "预览连接器转换"),
    ("connector.draft_from_supplier", "从供应商生成草稿"),
    ("connector.artifact_create", "创建连接器制品"),
    ("connector.export", "导出连接器"),
    ("config_center.validate", "校验配置"),
    ("upstream.onboarding_dry_run", "上游接入预演"),
    ("product.tenant_config_update", "更新租户产品配置"),
    ("bill.generate_async", "异步生成账单"),
    ("billing.reconciliation_run", "定时用量对账"),
    // ---- 其他服务写入的动作（cert / ratelimit，见 EXTERNAL_SERVICE_ACTIONS） ----
    ("certificate.verify", "证书校验失败"),
    ("authorization.root_material_read", "读取根授权素材"),
    ("config_center.tag_create", "新增配置标签"),
    ("config_center.tag_update", "更新配置标签"),
    ("config_center.tag_delete", "删除配置标签"),
    ("config_center.category_create", "新增配置分类"),
    ("config_center.category_update", "更新配置分类"),
    ("config_center.category_delete", "删除配置分类"),
    ("ratelimit.command_ack", "确认限流控制命令"),
    ("api.read", "API 读取"),
    ("api.get", "API 读取"),
    ("api.post", "API 提交"),
    ("api.put", "API 更新"),
    ("api.patch", "API 修改"),
    ("api.delete", "API 删除"),
];
/// 资源类型 → 中文标签。
pub const RESOURCE_TYPE_LABELS: &[(&str, &str)] = &[
    ("admin", "管理员"),
    ("tenant", "租户"),
    ("product", "产品"),
    ("apikey", "API 密钥"),
    ("certificate", "证书"),
    ("system", "系统"),
    ("user", "用户"),
    ("quota", "授权配置"),
    ("plan", "套餐"),
    ("account", "账户"),
    ("config_center", "配置中心"),
    ("route_rule", "路由规则"),
    ("route_channel_binding", "路由通道绑定"),
    ("supplier", "供应商"),
    ("channel", "通道"),
    ("callback", "回调"),
    ("audit_log", "审计日志"),
    ("connector", "连接器"),
    ("authorization", "授权"),
    ("billing", "账单"),
    ("ratelimit", "限流"),
];

/// 由 cert-service / ratelimit-service 写入、mgmt 枚举中没有的动作（标签同步测试覆盖；
/// cert 侧另有测试保证其动作全部出现在前端 labels.ts）。
pub const EXTERNAL_SERVICE_ACTIONS: &[&str] = &[
    "certificate.issue",
    "certificate.renew",
    "certificate.revoke",
    "certificate.auto_renewal_update",
    "certificate.view_full",
    "certificate.verify",
    "authorization.license_request",
    "authorization.import",
    "authorization.root_material_read",
    "config_center.update",
    "config_center.delete",
    "config_center.rollback",
    "config_center.tag_create",
    "config_center.tag_update",
    "config_center.tag_delete",
    "config_center.category_create",
    "config_center.category_update",
    "config_center.category_delete",
    "ratelimit.command_ack",
];

fn build_case_sql(column: &str, labels: &[(&str, &str)], otherwise: &str) -> String {
    let mut sql = format!("CASE {column} ");
    for (key, label) in labels {
        let _ = write!(sql, "WHEN '{key}' THEN '{label}' ");
    }
    let _ = write!(sql, "ELSE {otherwise} END");
    sql
}
static ACTION_LABEL_SQL: LazyLock<String> = LazyLock::new(|| {
    build_case_sql(
        "action",
        ACTION_LABELS,
        "replace(replace(action, '.', ' / '), '_', ' ')",
    )
});
static RESOURCE_TYPE_LABEL_SQL: LazyLock<String> = LazyLock::new(|| {
    build_case_sql(
        "resource_type",
        RESOURCE_TYPE_LABELS,
        "replace(resource_type, '_', ' ')",
    )
});
const STATUS_LABEL_SQL: &str = "CASE status WHEN 'success' THEN '成功' WHEN 'failure' THEN '失败' WHEN 'denied' THEN '拒绝' ELSE status END";
const CREATED_AT_SEARCH_SQL: &str =
    "to_char(created_at AT TIME ZONE 'Asia/Shanghai', 'YYYY/FMMM/FMDD HH24:MI:SS')";
fn push_audit_keyword_filter(builder: &mut QueryBuilder<'_, Postgres>, keyword: &str) {
    let pattern = format!("%{keyword}%");
    builder.push(" AND (username ILIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" OR action ILIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" OR ");
    builder.push(ACTION_LABEL_SQL.as_str());
    builder.push(" ILIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" OR resource_type ILIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" OR ");
    builder.push(RESOURCE_TYPE_LABEL_SQL.as_str());
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
    builder.push_bind(pattern.clone());
    builder.push(" OR failure_reason ILIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" OR request_id ILIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" OR path ILIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" OR status ILIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" OR ");
    builder.push(STATUS_LABEL_SQL);
    builder.push(" ILIKE ");
    builder.push_bind(pattern);
    builder.push(")");
}

const SELECT_COLUMNS: &str = "id, user_id, username, actor_type, actor_role, action, \
     resource_type, resource_id, details, status, error_message, failure_reason, \
     ip_address, user_agent, old_value, new_value, request_id, method, path, \
     status_code, duration_ms, request_summary, created_at, service";

/// 审计服务：业务事件入口 + 查询。可廉价克隆。
#[derive(Clone)]
pub struct AuditService {
    db: Arc<sqlx::PgPool>,
    sink: AuditSink,
    settings: Arc<AuditSettingsCache>,
}

impl AuditService {
    /// 创建服务并启动后台写线程（需在 tokio 运行时内调用）。
    #[must_use]
    pub fn new(
        db: Arc<sqlx::PgPool>,
        config: &AuditConfig,
        log_dir: &Path,
    ) -> (Self, AuditWriterHandle) {
        let fallback_path = config.fallback_file.as_ref().map_or_else(
            || log_dir.join("mgmt-audit-fallback.jsonl"),
            std::path::PathBuf::from,
        );
        let options = SinkOptions {
            queue_capacity: config.queue_capacity,
            batch_size: config.batch_size,
            flush_interval: Duration::from_millis(config.flush_interval_ms),
            ..SinkOptions::default()
        };
        let store: Arc<dyn AuditStore> = Arc::new(PgAuditStore::new(Arc::clone(&db)));
        let (sink, handle) = AuditSink::start(
            store,
            Arc::new(FallbackWriter::new(Some(fallback_path))),
            options,
        );
        let settings = Arc::new(AuditSettingsCache::from_db(
            Arc::clone(&db),
            Duration::from_secs(config.settings_cache_ttl_secs),
        ));
        (Self { db, sink, settings }, handle)
    }

    /// 由现成组件组装（测试注入内存存储与固定开关）。
    #[must_use]
    pub fn from_parts(
        db: Arc<sqlx::PgPool>,
        sink: AuditSink,
        settings: Arc<AuditSettingsCache>,
    ) -> Self {
        Self { db, sink, settings }
    }

    pub(crate) fn sink(&self) -> &AuditSink {
        &self.sink
    }

    /// 普通审计记录是否落库（`enable_audit_log`）。
    pub async fn audit_enabled(&self) -> bool {
        self.settings.enabled().await
    }

    /// 系统设置保存后调用，使开关立即生效。
    pub async fn invalidate_settings(&self) {
        self.settings.invalidate().await;
    }

    /// 提交业务事件：请求内并入该请求的唯一审计记录；请求外（后台任务）异步入队。
    pub fn log(&self, request: AuditLogRequest) {
        if let Some(request) = scope::push_event(request, false) {
            self.sink.enqueue(AuditRecord::from(request));
        }
    }

    /// 提交关键事件：请求内标记为同步落库；请求外立即同步写入。
    ///
    /// # Errors
    ///
    /// 请求外同步落库失败（已写入兜底文件）时返回错误。
    pub async fn log_critical(&self, request: AuditLogRequest) -> Result<(), sqlx::Error> {
        match scope::push_event(request, true) {
            None => Ok(()),
            Some(request) => {
                if self.sink.write_now(AuditRecord::from(request)).await {
                    Ok(())
                } else {
                    Err(sqlx::Error::Protocol(
                        "audit record written to fallback file".to_string(),
                    ))
                }
            }
        }
    }

    /// 同 [`Self::log_critical`]。
    ///
    /// # Errors
    ///
    /// 请求外同步落库失败（已写入兜底文件）时返回错误。
    pub async fn log_async(&self, request: AuditLogRequest) -> Result<(), sqlx::Error> {
        self.log_critical(request).await
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn list(&self, query: AuditLogQuery) -> Result<AuditLogListResponse, sqlx::Error> {
        let mut count_builder =
            sqlx::QueryBuilder::new("SELECT COUNT(*) FROM audit_logs_view WHERE 1=1");
        let mut select_builder = sqlx::QueryBuilder::new(format!(
            "SELECT {SELECT_COLUMNS} FROM audit_logs_view WHERE 1=1"
        ));
        push_audit_where(&mut count_builder, &query)?;
        push_audit_where(&mut select_builder, &query)?;
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
        let row = sqlx::query(&format!(
            "SELECT {SELECT_COLUMNS} FROM audit_logs_view WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&*self.db)
        .await?;
        Ok(row.map(|row| {
            let old_value: Option<serde_json::Value> = row.get("old_value");
            let new_value: Option<serde_json::Value> = row.get("new_value");
            let changes = if old_value.is_some() || new_value.is_some() {
                Some(AuditLogChanges {
                    before: old_value,
                    after: new_value,
                })
            } else {
                None
            };
            AuditLogDetailResponse {
                log: audit_row_to_response(&row),
                changes,
            }
        }))
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn export_csv(&self, query: AuditLogQuery) -> Result<String, sqlx::Error> {
        let mut select_builder = sqlx::QueryBuilder::new(format!(
            "SELECT {SELECT_COLUMNS} FROM audit_logs_view WHERE 1=1"
        ));
        push_audit_where(&mut select_builder, &query)?;
        select_builder.push(" ORDER BY created_at DESC LIMIT ");
        select_builder.push_bind(EXPORT_LIMIT);
        let rows = select_builder.build().fetch_all(&*self.db).await?;
        let mut csv = format!("\u{feff}{}\n", CSV_HEADER.join(","));
        for row in &rows {
            let log = audit_row_to_response(row);
            push_csv_row(&mut csv, &log);
        }
        Ok(csv)
    }
}

const CSV_HEADER: &[&str] = &[
    "id",
    "timestamp",
    "actor_id",
    "actor_type",
    "actor_name",
    "actor_role",
    "action",
    "resource_type",
    "resource_id",
    "details",
    "status",
    "failure_reason",
    "error_message",
    "ip_address",
    "user_agent",
    "request_id",
    "method",
    "path",
    "status_code",
    "duration_ms",
    "service",
];

fn push_csv_row(csv: &mut String, log: &AuditLogResponse) {
    fn or_na(value: Option<&str>) -> &str {
        value.unwrap_or("N/A")
    }
    let fields = [
        log.id.to_string(),
        log.timestamp.to_rfc3339(),
        log.actor_id
            .map_or_else(|| "N/A".to_string(), |id| id.to_string()),
        log.actor_type.clone(),
        or_na(log.actor_name.as_deref()).to_string(),
        or_na(log.actor_role.as_deref()).to_string(),
        log.action.clone(),
        or_na(log.resource_type.as_deref()).to_string(),
        or_na(log.resource_id.as_deref()).to_string(),
        log.details.to_string(),
        log.status.clone(),
        or_na(log.failure_reason.as_deref()).to_string(),
        or_na(log.error_message.as_deref()).to_string(),
        or_na(log.ip_address.as_deref()).to_string(),
        or_na(log.user_agent.as_deref()).to_string(),
        or_na(log.request_id.as_deref()).to_string(),
        or_na(log.method.as_deref()).to_string(),
        or_na(log.path.as_deref()).to_string(),
        log.status_code
            .map_or_else(|| "N/A".to_string(), |code| code.to_string()),
        log.duration_ms
            .map_or_else(|| "N/A".to_string(), |ms| ms.to_string()),
        log.service.clone(),
    ];
    let line: Vec<String> = fields.iter().map(|field| csv_escape(field)).collect();
    csv.push_str(&line.join(","));
    csv.push('\n');
}

const MAX_PAGE_SIZE: u32 = 100;
const EXPORT_LIMIT: i64 = 10000;

/// CSV 转义 + 公式注入防护（以 `= + - @ \t \r` 开头的单元格前置单引号）。
fn csv_escape(raw: &str) -> String {
    let guarded = if raw.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{raw}")
    } else {
        raw.to_string()
    };
    if guarded.contains([',', '"', '\n', '\r']) {
        let escaped = guarded.replace('"', "\"\"").replace('\r', "");
        format!("\"{escaped}\"")
    } else {
        guarded
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
        user_id: Some(user_id).filter(|id| !id.is_nil()),
        actor_type: ActorType::Admin,
        action,
        resource_type,
        resource_id: resource_id.into(),
        details: details.into(),
        ip_address: ip_address.into(),
        user_agent: None,
        status: AuditStatus::Success,
        error_message: None,
        failure_reason: None,
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

/// list / count / export 共用的完整 WHERE 条件：显式字段 + `q` + `filters`。
fn push_audit_where(
    builder: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>,
    query: &AuditLogQuery,
) -> Result<(), sqlx::Error> {
    let filters = crate::list_query::parse_and_validate_filters(
        query.filters.as_deref(),
        &crate::list_query::audit_logs_catalog(),
    )
    .map_err(|e| sqlx::Error::Protocol(e.message))?;
    let keyword = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    push_audit_conditions(builder, query, keyword);
    crate::list_query::apply_pg_filters(builder, &filters)
        .map_err(|e| sqlx::Error::Protocol(e.message))
}

/// 追加显式字段过滤条件。
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
        ("service", query.service.as_ref()),
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
    let ip_address: Option<String> = row.get("ip_address");
    let actor_type: Option<String> = row.get("actor_type");
    let status: Option<String> = row.get("status");
    let details: Option<serde_json::Value> = row.get("details");
    let details = details.unwrap_or_else(|| serde_json::json!({}));
    let service: Option<String> = row.get("service");
    AuditLogResponse {
        id: row.get("id"),
        timestamp: row.get("created_at"),
        actor_id: row.get("user_id"),
        actor_type: actor_type.unwrap_or_else(|| "admin".to_string()),
        actor_name: row.get("username"),
        actor_role: row.get("actor_role"),
        action: row.get("action"),
        resource_type: row.get("resource_type"),
        resource_id: row.get("resource_id"),
        ip_address: resolve_ip_address(ip_address.as_deref(), &details),
        details,
        user_agent: row.get("user_agent"),
        status: status.unwrap_or_else(|| "success".to_string()),
        error_message: row.get("error_message"),
        failure_reason: row.get("failure_reason"),
        request_id: row.get("request_id"),
        method: row.get("method"),
        path: row.get("path"),
        status_code: row.get("status_code"),
        duration_ms: row.get("duration_ms"),
        request_summary: row.get("request_summary"),
        service: service.unwrap_or_else(|| mazbri_audit::record::SERVICE_MGMT.to_string()),
    }
}

#[cfg(test)]
mod where_tests {
    use super::push_audit_where;
    use crate::audit::types::AuditLogQuery;

    fn where_sql(query: &AuditLogQuery, head: &str) -> String {
        let mut b = sqlx::QueryBuilder::<sqlx::Postgres>::new(head);
        push_audit_where(&mut b, query).unwrap();
        b.sql().trim_start_matches(head).to_string()
    }

    #[test]
    fn list_and_export_share_q_and_filters() {
        let query = AuditLogQuery {
            q: Some(" alice ".into()),
            action: Some("create".into()),
            filters: Some(r#"[{"field":"action","op":"eq","value":"create"}]"#.into()),
            ..AuditLogQuery::default()
        };
        let count = where_sql(&query, "SELECT COUNT(*) FROM audit_logs_view WHERE 1=1");
        let export = where_sql(&query, "SELECT id FROM audit_logs_view WHERE 1=1");
        assert_eq!(count, export);
        assert!(count.contains("ILIKE"));
        assert!(count.contains("action"));
    }

    #[test]
    fn unknown_query_field_rejected() {
        let r: Result<AuditLogQuery, _> =
            serde_json::from_value(serde_json::json!({"actor_name": "x"}));
        assert!(r.is_err());
    }
}

#[cfg(test)]
mod label_tests {
    use super::{ACTION_LABELS, RESOURCE_TYPE_LABELS, csv_escape};
    use crate::audit::route_map;
    use crate::audit::types::AuditAction;

    const FRONTEND_LABELS: &str =
        include_str!("../../../../mazbri-web/src/views/mazbri/control-panel/audit-logs/labels.ts");

    fn all_backend_actions() -> Vec<String> {
        let mut actions: Vec<String> = AuditAction::ALL
            .iter()
            .map(|action| action.as_str().to_string())
            .collect();
        actions.extend(
            route_map::ROUTES
                .iter()
                .filter_map(|route| route.action.map(|action| action.as_str().to_string())),
        );
        actions.push("api.read".to_string());
        actions.extend(
            super::EXTERNAL_SERVICE_ACTIONS
                .iter()
                .map(ToString::to_string),
        );
        for method in ["get", "post", "put", "patch", "delete"] {
            actions.push(route_map::fallback_action(method));
        }
        actions.sort();
        actions.dedup();
        actions
    }

    #[test]
    fn every_action_has_sql_label() {
        for action in all_backend_actions() {
            assert!(
                ACTION_LABELS.iter().any(|(key, _)| *key == action),
                "ACTION_LABELS 缺少 {action}"
            );
        }
    }

    #[test]
    fn every_action_has_frontend_label() {
        for action in all_backend_actions() {
            assert!(
                FRONTEND_LABELS.contains(&format!("\"{action}\":")),
                "labels.ts 缺少 {action}"
            );
        }
        for resource in [
            "audit_log",
            "connector",
            "authorization",
            "billing",
            "ratelimit",
        ] {
            assert!(
                FRONTEND_LABELS.contains(&format!("{resource}:")),
                "labels.ts 缺少资源类型 {resource}"
            );
        }
    }

    /// 收集生产代码中 `set_failure_reason("…")` 的字面量（跳过测试文件与 `#[cfg(test)]` 之后的内容）。
    fn literal_failure_reasons() -> Vec<String> {
        fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    walk(&path, out);
                    continue;
                }
                let is_rust = path.extension().is_some_and(|ext| ext == "rs");
                let stem = path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                if !is_rust || stem.ends_with("tests") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).expect("read source");
                let production = source.split("#[cfg(test)]").next().unwrap_or_default();
                let needle = "set_failure_reason(\"";
                let mut rest = production;
                while let Some(pos) = rest.find(needle) {
                    rest = &rest[pos + needle.len()..];
                    if let Some(end) = rest.find('"') {
                        out.push(rest[..end].to_string());
                    }
                }
            }
        }
        let mut reasons = Vec::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut reasons,
        );
        reasons.sort();
        reasons.dedup();
        reasons
    }

    #[test]
    fn every_literal_failure_reason_has_frontend_label() {
        let reasons = literal_failure_reasons();
        assert!(reasons.len() > 5, "扫描结果异常: {reasons:?}");
        for reason in reasons {
            assert!(
                FRONTEND_LABELS.contains(&format!("  {reason}: \"auditLogs.failureReasons.")),
                "labels.ts FAILURE_REASON_LABELS 缺少 {reason}"
            );
        }
    }

    #[test]
    fn labels_are_sql_safe() {
        for (key, label) in ACTION_LABELS.iter().chain(RESOURCE_TYPE_LABELS) {
            assert!(!key.contains('\'') && !label.contains('\''), "{key}");
        }
    }

    #[test]
    fn csv_escape_blocks_formula_injection() {
        assert_eq!(csv_escape("=cmd()"), "'=cmd()");
        assert_eq!(csv_escape("+1"), "'+1");
        assert_eq!(csv_escape("-1"), "'-1");
        assert_eq!(csv_escape("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(csv_escape("\tx"), "'\tx");
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("=a,\"b\""), "\"'=a,\"\"b\"\"\"");
        assert_eq!(csv_escape("plain"), "plain");
    }
}

/// 需要临时 PostgreSQL（已执行 common/functions.sql + admin/init.sql）。
/// 运行：`MAZBRI_AUDIT_TEST_DATABASE_URL=postgres://... cargo test -- --ignored audit_pg`
#[cfg(test)]
mod pg_tests {
    use super::AuditService;
    use crate::audit::settings::AuditSettingsCache;
    use crate::audit::sink::{AuditSink, FallbackWriter, PgAuditStore, SinkOptions};
    use crate::audit::types::{AuditAction, AuditLogQuery, AuditLogRequest, AuditStatus};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    async fn service() -> (AuditService, crate::audit::AuditWriterHandle) {
        let url = std::env::var("MAZBRI_AUDIT_TEST_DATABASE_URL")
            .expect("MAZBRI_AUDIT_TEST_DATABASE_URL must point to a temporary database");
        let db = Arc::new(sqlx::PgPool::connect(&url).await.unwrap());
        let (sink, handle) = AuditSink::start(
            Arc::new(PgAuditStore::new(db.clone())),
            Arc::new(FallbackWriter::new(None)),
            SinkOptions {
                flush_interval: Duration::from_millis(10),
                ..SinkOptions::default()
            },
        );
        let settings = Arc::new(AuditSettingsCache::fixed(Arc::new(AtomicBool::new(true))));
        (AuditService::from_parts(db, sink, settings), handle)
    }

    fn query(value: serde_json::Value) -> AuditLogQuery {
        serde_json::from_value(value).unwrap()
    }

    #[tokio::test]
    #[ignore = "requires a temporary PostgreSQL via MAZBRI_AUDIT_TEST_DATABASE_URL"]
    async fn audit_pg_roundtrip_filters_and_export() {
        let (service, handle) = service().await;
        let marker = uuid::Uuid::new_v4().simple().to_string();
        // 不存在的 user_id 触发外键冲突，应被清空后重试成功。
        service
            .log_critical(AuditLogRequest {
                user_id: Some(uuid::Uuid::new_v4()),
                action: AuditAction::UserLogin,
                status: AuditStatus::Denied,
                failure_reason: Some(format!("invalid_password_{}", &marker[..8])),
                resource_id: Some(marker.clone()),
                ..AuditLogRequest::default()
            })
            .await
            .unwrap();
        service.log(AuditLogRequest {
            action: AuditAction::TenantCreate,
            resource_id: Some(marker.clone()),
            details: Some(serde_json::json!({ "password": "pw-secret-x", "name": "=cmd" })),
            ..AuditLogRequest::default()
        });
        handle.shutdown(Duration::from_secs(5)).await;

        let all = service
            .list(query(
                serde_json::json!({ "resource_id": marker, "page": 1, "limit": 20 }),
            ))
            .await
            .unwrap();
        assert_eq!(all.total, 2);
        let denied = service
            .list(query(serde_json::json!({
                "resource_id": marker, "status": "denied", "page": 1, "limit": 20
            })))
            .await
            .unwrap();
        assert_eq!(denied.total, 1);
        let row = &denied.logs[0];
        assert_eq!(row.actor_id, None);
        assert_eq!(row.status, "denied");
        assert!(
            row.failure_reason
                .as_deref()
                .unwrap()
                .starts_with("invalid_password_")
        );
        let detail = service.get_by_id(row.id).await.unwrap().unwrap();
        assert_eq!(detail.log.id, row.id);

        let csv = service
            .export_csv(query(
                serde_json::json!({ "resource_id": marker, "page": 1, "limit": 20 }),
            ))
            .await
            .unwrap();
        assert!(csv.contains("invalid_password_"));
        assert!(!csv.contains("pw-secret-x"));
    }
}

#[cfg(test)]
mod service_filter_tests {
    use super::{CSV_HEADER, SELECT_COLUMNS, push_audit_where};
    use crate::audit::types::AuditLogQuery;

    #[test]
    fn service_query_param_is_bound_equality() {
        let query: AuditLogQuery =
            serde_json::from_value(serde_json::json!({"service": "cert"})).expect("query");
        assert_eq!(query.service.as_deref(), Some("cert"));
        let head = "SELECT id FROM audit_logs_view WHERE 1=1";
        let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(head);
        push_audit_where(&mut builder, &query).expect("where");
        let sql = builder.sql().trim_start_matches(head).to_string();
        assert!(sql.contains("AND service = $1"), "{sql}");
        assert!(
            !sql.contains("cert"),
            "value must be bound, not inlined: {sql}"
        );
    }

    #[test]
    fn service_filter_from_catalog() {
        let query = AuditLogQuery {
            filters: Some(r#"[{"field":"service","op":"eq","value":"ratelimit"}]"#.into()),
            ..AuditLogQuery::default()
        };
        let head = "SELECT id FROM audit_logs_view WHERE 1=1";
        let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(head);
        push_audit_where(&mut builder, &query).expect("where");
        assert!(builder.sql().contains("service"));
    }

    #[test]
    fn service_is_selected_and_exported() {
        assert!(SELECT_COLUMNS.contains("service"));
        assert_eq!(CSV_HEADER.last(), Some(&"service"));
    }
}
