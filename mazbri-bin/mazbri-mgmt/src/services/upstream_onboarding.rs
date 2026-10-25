//! 一站式上游接入：事务写入 Supplier + Channel + `RouteRule` + Binding。

use crate::audit::{AuditAction, AuditLogRequest, AuditResourceType, AuditService};
use crate::error::ApiError;
use crate::services::ssrf;
use mazbri_core::{SupplierConfig, mask_config_credentials};
use mazbri_upstream_runtime::SsrfChecker;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::str::FromStr;
use tracing::info;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamOnboardingRequest {
    pub supplier: OnboardingSupplier,
    pub channel: OnboardingChannel,
    pub route: OnboardingRoute,
    #[serde(default)]
    pub authorization: OnboardingAuthorization,
    #[serde(default)]
    pub options: OnboardingOptions,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingSupplier {
    pub supplier_code: String,
    pub name: String,
    /// 可与 `config.upstream_type` 二选一；两者都给时必须一致。
    #[serde(default)]
    pub upstream_type: Option<String>,
    pub upstream_url: String,
    #[serde(default = "default_active")]
    pub status: String,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

fn default_active() -> String {
    "active".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingChannel {
    pub channel_code: String,
    pub name: String,
    #[serde(default = "default_path_prefix")]
    pub path_prefix: String,
    #[serde(default)]
    pub cost_price: Option<String>,
    #[serde(default)]
    pub qps_limit: Option<i32>,
}

fn default_path_prefix() -> String {
    "/".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingRoute {
    pub product_ref: ProductRef,
    #[serde(default)]
    pub tenant_id: Option<uuid::Uuid>,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_version_tag")]
    pub version_tag: String,
    #[serde(default)]
    pub switch_condition: Option<serde_json::Value>,
}

fn default_priority() -> i32 {
    100
}
fn default_weight() -> i32 {
    100
}
fn default_version_tag() -> String {
    "v1.0".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductRef {
    /// 外部产品编码（如 8012345678），非内部 UUID。
    pub product_id: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingAuthorization {
    #[serde(default)]
    pub tenant_ids: Vec<uuid::Uuid>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingOptions {
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default = "default_true")]
    pub fail_if_route_exists: bool,
}

impl Default for OnboardingOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            fail_if_route_exists: true,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OnboardingResult {
    pub supplier_id: Option<uuid::Uuid>,
    pub channel_id: Option<uuid::Uuid>,
    pub route_rule_id: Option<uuid::Uuid>,
    pub binding_id: Option<uuid::Uuid>,
    pub route_version: Option<i64>,
    pub warnings: Vec<String>,
    pub dry_run: bool,
    pub plan: Option<OnboardingPlan>,
}

#[derive(Debug, Serialize)]
pub struct OnboardingPlan {
    pub supplier_code: String,
    pub channel_code: String,
    pub product_external_id: String,
    pub product_uuid: uuid::Uuid,
    pub upstream_type: String,
    pub upstream_url: String,
    pub actions: Vec<String>,
}

struct PreparedOnboarding {
    supplier_code: String,
    name: String,
    upstream_url: String,
    status: String,
    config_value: serde_json::Value,
    upstream_type: String,
    channel_code: String,
    channel_name: String,
    path_prefix: String,
    cost_price: Decimal,
    qps_limit: i32,
    product_uuid: uuid::Uuid,
    product_external_id: String,
    tenant_id: Option<uuid::Uuid>,
    priority: i32,
    weight: i32,
    version_tag: String,
    switch_condition: serde_json::Value,
    auth_tenant_ids: Vec<uuid::Uuid>,
    auth_enabled: bool,
    options: OnboardingOptions,
}

/// 校验供应商基础字段，返回 (`supplier_code`, `name`, `upstream_url`)。
fn validate_supplier_fields(
    supplier: &OnboardingSupplier,
) -> Result<(&str, &str, String), ApiError> {
    let supplier_code = supplier.supplier_code.trim();
    if supplier_code.is_empty() || supplier_code.len() > 100 {
        return Err(ApiError::validation(
            "supplier_code 长度必须在 1..=100".to_string(),
        ));
    }
    if !supplier_code
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ApiError::validation(
            "supplier_code 只能包含字母、数字、下划线和连字符".to_string(),
        ));
    }
    let name = supplier.name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err(ApiError::validation("name 长度必须在 1..=255".to_string()));
    }
    let upstream_url = supplier.upstream_url.trim().to_string();
    if upstream_url.is_empty() || upstream_url.len() > 500 {
        return Err(ApiError::validation(
            "upstream_url 长度必须在 1..=500".to_string(),
        ));
    }

    Ok((supplier_code, name, upstream_url))
}

/// 组装并校验供应商配置（合并 `upstream_type`、SSRF 检查、凭据加密）。
async fn build_supplier_config(
    checker: &SsrfChecker,
    supplier: &OnboardingSupplier,
    upstream_url: &str,
) -> Result<SupplierConfig, ApiError> {
    // 组装 config：确保 upstream_type 存在且一致
    let mut config_raw = supplier
        .config
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = config_raw.as_object_mut()
        && let Some(ref t) = supplier.upstream_type
    {
        match obj.get("upstream_type").and_then(|v| v.as_str()) {
            Some(existing) if existing != t => {
                return Err(ApiError::validation(format!(
                    "supplier.upstream_type('{t}') 与 config.upstream_type('{existing}') 不一致"
                )));
            }
            None => {
                obj.insert(
                    "upstream_type".to_string(),
                    serde_json::Value::String(t.clone()),
                );
            }
            _ => {}
        }
    }

    let mut cfg = SupplierConfig::parse_and_validate(&config_raw)
        .map_err(|e| ApiError::validation(format!("UPSTREAM_CONFIG_INVALID: {e}")))?;
    cfg = cfg.normalize_disabled_sections();
    cfg.validate_upstream_url_for_type(upstream_url)
        .map_err(|e| ApiError::validation(format!("UPSTREAM_CONFIG_INVALID: {e}")))?;

    if cfg.is_transparent() {
        ssrf::validate_public_http_url(checker, upstream_url)
            .await
            .map_err(|e| ApiError::validation(format!("UPSTREAM_SSRF_BLOCKED: {e}")))?;
        if let Some(ref auth) = cfg.auth_flow
            && auth.enabled
        {
            ssrf::validate_public_http_url(checker, &auth.token_url)
                .await
                .map_err(|e| {
                    ApiError::validation(format!(
                        "UPSTREAM_SSRF_BLOCKED (auth_flow.token_url): {e}"
                    ))
                })?;
        }
    }
    // G-05：落库前加密（SSRF 校验时 token_url 仍是明文，credentials 加密不影响）
    cfg = cfg
        .encrypt_credentials_for_storage()
        .map_err(|e| ApiError::internal(format!("加密供应商凭据失败: {e}")))?;

    Ok(cfg)
}

/// 校验渠道字段，返回 (`channel_code`, `name`, `path_prefix`, `cost_price`, `qps_limit`)。
fn validate_channel_fields(
    channel: &OnboardingChannel,
) -> Result<(String, &str, String, Decimal, i32), ApiError> {
    let channel_code = channel.channel_code.trim().to_string();
    if !channel_code.chars().enumerate().all(|(i, c)| {
        if i == 0 {
            c.is_ascii_uppercase()
        } else {
            c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-'
        }
    }) {
        return Err(ApiError::validation(
            "channel_code 必须匹配 ^[A-Z][A-Z0-9-]*$".to_string(),
        ));
    }
    let channel_name = channel.name.trim();
    if channel_name.is_empty() {
        return Err(ApiError::validation("channel.name 不能为空".to_string()));
    }
    let path_prefix = if channel.path_prefix.trim().is_empty() {
        "/".to_string()
    } else {
        channel.path_prefix.trim().to_string()
    };
    let cost_price = match &channel.cost_price {
        Some(s) => Decimal::from_str(s.trim())
            .map_err(|e| ApiError::validation(format!("cost_price 无效: {e}")))?,
        None => Decimal::ZERO,
    };
    let qps_limit = channel.qps_limit.unwrap_or(0);
    if qps_limit < 0 {
        return Err(ApiError::validation("qps_limit 不能为负".to_string()));
    }

    Ok((
        channel_code,
        channel_name,
        path_prefix,
        cost_price,
        qps_limit,
    ))
}

/// 查找目标产品并检查 supplier/channel/route 冲突，返回 (产品 UUID, 外部产品编码)。
async fn check_onboarding_targets(
    pool: &PgPool,
    supplier_code: &str,
    channel_code: &str,
    route: &OnboardingRoute,
    fail_if_route_exists: bool,
) -> Result<(uuid::Uuid, String), ApiError> {
    let product_external_id = route.product_ref.product_id.trim().to_string();
    let product_row =
        sqlx::query("SELECT id, product_id, enabled FROM products WHERE product_id = $1 LIMIT 1")
            .bind(&product_external_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| ApiError::internal(format!("查询产品失败: {e}")))?;
    let product_row = product_row
        .ok_or_else(|| ApiError::not_found("product".to_string(), product_external_id.clone()))?;
    let product_uuid: uuid::Uuid = product_row.get("id");
    let enabled: bool = product_row.get("enabled");
    if !enabled {
        return Err(ApiError::validation(format!(
            "产品 {product_external_id} 未启用"
        )));
    }

    // 冲突检查
    let code_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM suppliers WHERE supplier_code = $1)")
            .bind(supplier_code)
            .fetch_one(pool)
            .await
            .map_err(|e| ApiError::internal(format!("检查 supplier_code 失败: {e}")))?;
    if code_exists {
        return Err(ApiError::validation(format!(
            "SUPPLIER_CODE_EXISTS: supplier_code '{supplier_code}' 已存在"
        )));
    }
    let channel_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM channels WHERE channel_code = $1)")
            .bind(channel_code)
            .fetch_one(pool)
            .await
            .map_err(|e| ApiError::internal(format!("检查 channel_code 失败: {e}")))?;
    if channel_exists {
        return Err(ApiError::validation(format!(
            "CHANNEL_CODE_EXISTS: channel_code '{channel_code}' 已存在"
        )));
    }

    if fail_if_route_exists {
        let route_exists: bool = match route.tenant_id {
            Some(tid) => {
                sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM route_rules WHERE product_id = $1 AND tenant_id = $2 AND status = 'active')",
                )
                .bind(product_uuid)
                .bind(tid)
                .fetch_one(pool)
                .await
            }
            None => {
                sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM route_rules WHERE product_id = $1 AND tenant_id IS NULL AND status = 'active')",
                )
                .bind(product_uuid)
                .fetch_one(pool)
                .await
            }
        }
        .map_err(|e| ApiError::internal(format!("检查路由冲突失败: {e}")))?;
        if route_exists {
            return Err(ApiError::validation(
                "ROUTE_CONFLICT: 同 tenant/product 已存在 active 路由规则".to_string(),
            ));
        }
    }

    Ok((product_uuid, product_external_id))
}

/// 与 `dry_run` / 真实提交共用的校验管线。
async fn prepare(
    pool: &PgPool,
    checker: &SsrfChecker,
    req: &UpstreamOnboardingRequest,
) -> Result<PreparedOnboarding, ApiError> {
    let (supplier_code, name, upstream_url) = validate_supplier_fields(&req.supplier)?;
    let cfg = build_supplier_config(checker, &req.supplier, &upstream_url).await?;
    let (channel_code, channel_name, path_prefix, cost_price, qps_limit) =
        validate_channel_fields(&req.channel)?;
    let (product_uuid, product_external_id) = check_onboarding_targets(
        pool,
        supplier_code,
        &channel_code,
        &req.route,
        req.options.fail_if_route_exists,
    )
    .await?;

    if !(0..=100).contains(&req.route.weight) {
        return Err(ApiError::validation("weight 必须在 0..=100".to_string()));
    }

    let switch_condition = req.route.switch_condition.clone().unwrap_or_else(|| {
        serde_json::json!({
            "max_retries": 1,
            "timeout_ms": 3000,
            "switch_on_timeout": true,
            "switch_on_bad_gateway": true,
            "switch_on_internal_error": false,
            "switch_on_status_codes": []
        })
    });

    let status = req.supplier.status.trim();
    if !["active", "inactive", "suspended"].contains(&status) {
        return Err(ApiError::validation("supplier.status 无效".to_string()));
    }

    let config_value = cfg
        .to_json_value()
        .map_err(|e| ApiError::validation(e.to_string()))?;

    Ok(PreparedOnboarding {
        supplier_code: supplier_code.to_string(),
        name: name.to_string(),
        upstream_url,
        status: status.to_string(),
        config_value,
        upstream_type: cfg.upstream_type.clone(),
        channel_code,
        channel_name: channel_name.to_string(),
        path_prefix,
        cost_price,
        qps_limit,
        product_uuid,
        product_external_id,
        tenant_id: req.route.tenant_id,
        priority: req.route.priority,
        weight: req.route.weight,
        version_tag: req.route.version_tag.clone(),
        switch_condition,
        auth_tenant_ids: req.authorization.tenant_ids.clone(),
        auth_enabled: req.authorization.enabled,
        options: OnboardingOptions {
            dry_run: req.options.dry_run,
            fail_if_route_exists: req.options.fail_if_route_exists,
        },
    })
}

fn onboarding_plan(prepared: &PreparedOnboarding) -> OnboardingPlan {
    OnboardingPlan {
        supplier_code: prepared.supplier_code.clone(),
        channel_code: prepared.channel_code.clone(),
        product_external_id: prepared.product_external_id.clone(),
        product_uuid: prepared.product_uuid,
        upstream_type: prepared.upstream_type.clone(),
        upstream_url: prepared.upstream_url.clone(),
        actions: vec![
            "insert suppliers".into(),
            "insert channels".into(),
            "insert route_rules".into(),
            "insert route_channel_bindings".into(),
            if prepared.auth_enabled && !prepared.auth_tenant_ids.is_empty() {
                "grant tenant_products".into()
            } else {
                "skip authorization".into()
            },
        ],
    }
}

/// 事务内写入 supplier 与 channel，返回 (`supplier_id`, `channel_id`)。
async fn insert_supplier_and_channel(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    p: &PreparedOnboarding,
) -> Result<(uuid::Uuid, uuid::Uuid), ApiError> {
    let supplier_id: uuid::Uuid = sqlx::query_scalar(
        r"
        INSERT INTO suppliers (supplier_code, name, upstream_url, status, config)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        ",
    )
    .bind(&p.supplier_code)
    .bind(&p.name)
    .bind(&p.upstream_url)
    .bind(&p.status)
    .bind(&p.config_value)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| map_db_conflict(&e, "创建 supplier 失败"))?;

    let channel_id: uuid::Uuid = sqlx::query_scalar(
        r"
        INSERT INTO channels (supplier_id, channel_code, name, path_prefix, cost_price, qps_limit, config, status)
        VALUES ($1, $2, $3, $4, $5, $6, '{}'::jsonb, 'active')
        RETURNING id
        ",
    )
    .bind(supplier_id)
    .bind(&p.channel_code)
    .bind(&p.channel_name)
    .bind(&p.path_prefix)
    .bind(p.cost_price)
    .bind(p.qps_limit)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| map_db_conflict(&e, "创建 channel 失败"))?;

    Ok((supplier_id, channel_id))
}

/// 事务内写入路由规则、绑定与租户授权，返回 (`route_rule_id`, `binding_id`)。
async fn insert_route_and_grants(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    p: &PreparedOnboarding,
    channel_id: uuid::Uuid,
) -> Result<(uuid::Uuid, uuid::Uuid), ApiError> {
    let metadata = serde_json::json!({
        "source": "upstream_onboarding",
        "product_external_id": p.product_external_id,
    });
    let route_rule_id: uuid::Uuid = sqlx::query_scalar(
        r"
        INSERT INTO route_rules (tenant_id, product_id, priority, status, metadata)
        VALUES ($1, $2, $3, 'active', $4)
        RETURNING id
        ",
    )
    .bind(p.tenant_id)
    .bind(p.product_uuid)
    .bind(p.priority)
    .bind(&metadata)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| ApiError::internal(format!("创建 route_rule 失败: {e}")))?;

    let binding_id: uuid::Uuid = sqlx::query_scalar(
        r"
        INSERT INTO route_channel_bindings
            (route_rule_id, channel_id, channel_order, switch_condition, version_tag, weight)
        VALUES ($1, $2, 1, $3, $4, $5)
        RETURNING id
        ",
    )
    .bind(route_rule_id)
    .bind(channel_id)
    .bind(&p.switch_condition)
    .bind(&p.version_tag)
    .bind(p.weight)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| ApiError::internal(format!("创建 route binding 失败: {e}")))?;

    if p.auth_enabled {
        for tenant_uuid in &p.auth_tenant_ids {
            sqlx::query(
                r"
                INSERT INTO tenant_products (tenant_id, product_id, enabled, custom_config)
                VALUES ($1, $2, true, '{}'::jsonb)
                ON CONFLICT (tenant_id, product_id) DO UPDATE SET enabled = true, updated_at = NOW()
                ",
            )
            .bind(tenant_uuid)
            .bind(p.product_uuid)
            .execute(&mut **tx)
            .await
            .map_err(|e| ApiError::internal(format!("授权 tenant_products 失败: {e}")))?;
        }
    }

    Ok((route_rule_id, binding_id))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn onboard(
    pool: &PgPool,
    checker: &SsrfChecker,
    audit: &AuditService,
    req: &UpstreamOnboardingRequest,
    user_id: uuid::Uuid,
    username: &str,
    client_ip: Option<String>,
) -> Result<OnboardingResult, ApiError> {
    info!(by = %username, "Upstream onboarding requested");
    let prepared = prepare(pool, checker, req).await?;
    let warnings = Vec::new();

    let plan = onboarding_plan(&prepared);

    if prepared.options.dry_run {
        return Ok(OnboardingResult {
            supplier_id: None,
            channel_id: None,
            route_rule_id: None,
            binding_id: None,
            route_version: None,
            warnings: vec!["dry_run: 未写库".into()],
            dry_run: true,
            plan: Some(plan),
        });
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::internal(format!("开启事务失败: {e}")))?;

    let (supplier_id, channel_id) = insert_supplier_and_channel(&mut tx, &prepared).await?;
    let (route_rule_id, binding_id) =
        insert_route_and_grants(&mut tx, &prepared, channel_id).await?;

    tx.commit()
        .await
        .map_err(|e| ApiError::internal(format!("提交事务失败: {e}")))?;

    let route_version: i64 = sqlx::query_scalar(
        "SELECT version FROM route_sync_metadata WHERE key = 'global_route_version'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    audit.log(AuditLogRequest {
        user_id,
        action: AuditAction::UpstreamOnboarding,
        resource_type: AuditResourceType::Supplier,
        resource_id: Some(supplier_id.to_string()),
        details: Some(serde_json::json!({
            "supplier_id": supplier_id,
            "channel_id": channel_id,
            "route_rule_id": route_rule_id,
            "binding_id": binding_id,
            "route_version": route_version,
            "upstream_type": prepared.upstream_type,
            "product_id": prepared.product_external_id,
        })),
        ip_address: client_ip,
        old_value: None,
        new_value: Some(serde_json::json!({
            "supplier_code": prepared.supplier_code,
            "config": mask_config_credentials(prepared.config_value.clone()),
        })),
        ..Default::default()
    });

    info!(
        supplier_id = %supplier_id,
        channel_id = %channel_id,
        route_version,
        "Upstream onboarding completed"
    );

    Ok(OnboardingResult {
        supplier_id: Some(supplier_id),
        channel_id: Some(channel_id),
        route_rule_id: Some(route_rule_id),
        binding_id: Some(binding_id),
        route_version: Some(route_version),
        warnings,
        dry_run: false,
        plan: Some(plan),
    })
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn dry_run(
    pool: &PgPool,
    checker: &SsrfChecker,
    req: &UpstreamOnboardingRequest,
) -> Result<OnboardingResult, ApiError> {
    let mut forced = req.clone();
    forced.options.dry_run = true;
    let prepared = prepare(pool, checker, &forced).await?;
    Ok(OnboardingResult {
        supplier_id: None,
        channel_id: None,
        route_rule_id: None,
        binding_id: None,
        route_version: None,
        warnings: vec!["dry_run: 校验通过，未写库".into()],
        dry_run: true,
        plan: Some(OnboardingPlan {
            supplier_code: prepared.supplier_code,
            channel_code: prepared.channel_code,
            product_external_id: prepared.product_external_id,
            product_uuid: prepared.product_uuid,
            upstream_type: prepared.upstream_type,
            upstream_url: prepared.upstream_url,
            actions: vec![
                "insert suppliers".into(),
                "insert channels".into(),
                "insert route_rules".into(),
                "insert route_channel_bindings".into(),
            ],
        }),
    })
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn validate_config_payload(
    checker: &SsrfChecker,
    config: &serde_json::Value,
    upstream_url: &str,
) -> Result<serde_json::Value, ApiError> {
    let mut cfg = SupplierConfig::parse_and_validate(config)
        .map_err(|e| ApiError::validation(format!("UPSTREAM_CONFIG_INVALID: {e}")))?;
    cfg = cfg.normalize_disabled_sections();
    cfg.validate_upstream_url_for_type(upstream_url)
        .map_err(|e| ApiError::validation(e.to_string()))?;
    if cfg.is_transparent() {
        ssrf::validate_public_http_url(checker, upstream_url)
            .await
            .map_err(|e| ApiError::validation(format!("UPSTREAM_SSRF_BLOCKED: {e}")))?;
    }
    // dry-run/validate 不落库：返回脱敏预览用明文结构；不在此加密以免误导
    cfg.to_json_value()
        .map_err(|e| ApiError::validation(e.to_string()))
}

fn map_db_conflict(e: &sqlx::Error, ctx: &str) -> ApiError {
    if let sqlx::Error::Database(db) = &e
        && db.constraint().is_some()
    {
        return ApiError::validation(format!("{ctx}: 唯一约束冲突 ({e})"));
    }
    ApiError::internal(format!("{ctx}: {e}"))
}
