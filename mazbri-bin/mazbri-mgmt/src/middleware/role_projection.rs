//! 集中式角色视图字段投影与细粒度资源门禁（`RoleDataProjection`）。
//!
//! 负责在不放宽路由表的前提下，对跨角色共享的运维只读端点按调用者角色裁剪商业成本、
//! 内部查询表达式与调试凭据，并对 24 个 OLAP 模板与指标目录执行编译期穷举门禁。

use super::auth::AdminClaims;
use super::route_access::{ADMIN, BIZ};
use serde_json::Value;

/// 角色投影上下文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleDataProjection {
    is_admin: bool,
    is_biz: bool,
}

/// 全部 24 个 OLAP 查询模板（与 `mazbri-analytics::clickhouse::olap::OlapQuery` 一一对应，禁止通配符）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OlapTemplateKind {
    TopByDim,
    LatencyQuantiles,
    LatencyBreakdown,
    LatencyHeatmap,
    StageQuantiles,
    StatusHeatmap,
    BytesTrend,
    SlowRequests,
    ErrorMatrix,
    ErrorAttribution,
    ResponseFlags,
    NewErrorCodes,
    CallHistogram,
    CostTrend,
    SupplierSla,
    BillingFunnelCh,
    ExemplarTraces,
    QuotaTop,
    TenantOverview,
    TenantErrors,
    ResponseCacheHit,
    SecurityTop,
    SecurityEvents,
    ProductRank,
}

impl OlapTemplateKind {
    pub const ALL: &[Self] = &[
        Self::TopByDim,
        Self::LatencyQuantiles,
        Self::LatencyBreakdown,
        Self::LatencyHeatmap,
        Self::StageQuantiles,
        Self::StatusHeatmap,
        Self::BytesTrend,
        Self::SlowRequests,
        Self::ErrorMatrix,
        Self::ErrorAttribution,
        Self::ResponseFlags,
        Self::NewErrorCodes,
        Self::CallHistogram,
        Self::CostTrend,
        Self::SupplierSla,
        Self::BillingFunnelCh,
        Self::ExemplarTraces,
        Self::QuotaTop,
        Self::TenantOverview,
        Self::TenantErrors,
        Self::ResponseCacheHit,
        Self::SecurityTop,
        Self::SecurityEvents,
        Self::ProductRank,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TopByDim => "top_by_dim",
            Self::LatencyQuantiles => "latency_quantiles",
            Self::LatencyBreakdown => "latency_breakdown",
            Self::LatencyHeatmap => "latency_heatmap",
            Self::StageQuantiles => "stage_quantiles",
            Self::StatusHeatmap => "status_heatmap",
            Self::BytesTrend => "bytes_trend",
            Self::SlowRequests => "slow_requests",
            Self::ErrorMatrix => "error_matrix",
            Self::ErrorAttribution => "error_attribution",
            Self::ResponseFlags => "response_flags",
            Self::NewErrorCodes => "new_error_codes",
            Self::CallHistogram => "call_histogram",
            Self::CostTrend => "cost_trend",
            Self::SupplierSla => "supplier_sla",
            Self::BillingFunnelCh => "billing_funnel_ch",
            Self::ExemplarTraces => "exemplar_traces",
            Self::QuotaTop => "quota_top",
            Self::TenantOverview => "tenant_overview",
            Self::TenantErrors => "tenant_errors",
            Self::ResponseCacheHit => "response_cache_hit",
            Self::SecurityTop => "security_top",
            Self::SecurityEvents => "security_events",
            Self::ProductRank => "product_rank",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "top_by_dim" => Some(Self::TopByDim),
            "latency_quantiles" => Some(Self::LatencyQuantiles),
            "latency_breakdown" => Some(Self::LatencyBreakdown),
            "latency_heatmap" => Some(Self::LatencyHeatmap),
            "stage_quantiles" => Some(Self::StageQuantiles),
            "status_heatmap" => Some(Self::StatusHeatmap),
            "bytes_trend" => Some(Self::BytesTrend),
            "slow_requests" => Some(Self::SlowRequests),
            "error_matrix" => Some(Self::ErrorMatrix),
            "error_attribution" => Some(Self::ErrorAttribution),
            "response_flags" => Some(Self::ResponseFlags),
            "new_error_codes" => Some(Self::NewErrorCodes),
            "call_histogram" => Some(Self::CallHistogram),
            "cost_trend" => Some(Self::CostTrend),
            "supplier_sla" => Some(Self::SupplierSla),
            "billing_funnel_ch" => Some(Self::BillingFunnelCh),
            "exemplar_traces" => Some(Self::ExemplarTraces),
            "quota_top" => Some(Self::QuotaTop),
            "tenant_overview" => Some(Self::TenantOverview),
            "tenant_errors" => Some(Self::TenantErrors),
            "response_cache_hit" => Some(Self::ResponseCacheHit),
            "security_top" => Some(Self::SecurityTop),
            "security_events" => Some(Self::SecurityEvents),
            "product_rank" => Some(Self::ProductRank),
            _ => None,
        }
    }
}

/// OLAP 模板最低角色要求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OlapAccessRequirement {
    AnyRole,
    Biz,
    Admin,
}

impl RoleDataProjection {
    #[must_use]
    pub fn for_role(role: &str) -> Self {
        Self {
            is_admin: ADMIN.contains(&role),
            is_biz: BIZ.contains(&role),
        }
    }

    #[must_use]
    pub fn for_claims(claims: &AdminClaims) -> Self {
        Self::for_role(&claims.role)
    }

    #[must_use]
    pub const fn is_admin(self) -> bool {
        self.is_admin
    }

    #[must_use]
    pub const fn is_biz(self) -> bool {
        self.is_biz
    }

    #[must_use]
    pub const fn is_viewer(self) -> bool {
        !self.is_biz
    }

    /// 编译期穷举判定 24 个 OLAP 模板所需的最低角色（禁止 `_` 通配符）。
    #[must_use]
    pub fn olap_requirement(kind: OlapTemplateKind, metric: Option<&str>) -> OlapAccessRequirement {
        match kind {
            OlapTemplateKind::CostTrend | OlapTemplateKind::BillingFunnelCh => {
                OlapAccessRequirement::Biz
            }
            OlapTemplateKind::TopByDim | OlapTemplateKind::ProductRank => {
                if metric == Some("cost") {
                    OlapAccessRequirement::Biz
                } else {
                    OlapAccessRequirement::AnyRole
                }
            }
            OlapTemplateKind::SecurityTop | OlapTemplateKind::SecurityEvents => {
                OlapAccessRequirement::Admin
            }
            OlapTemplateKind::LatencyQuantiles
            | OlapTemplateKind::LatencyBreakdown
            | OlapTemplateKind::LatencyHeatmap
            | OlapTemplateKind::StageQuantiles
            | OlapTemplateKind::StatusHeatmap
            | OlapTemplateKind::BytesTrend
            | OlapTemplateKind::SlowRequests
            | OlapTemplateKind::ErrorMatrix
            | OlapTemplateKind::ErrorAttribution
            | OlapTemplateKind::ResponseFlags
            | OlapTemplateKind::NewErrorCodes
            | OlapTemplateKind::CallHistogram
            | OlapTemplateKind::SupplierSla
            | OlapTemplateKind::ExemplarTraces
            | OlapTemplateKind::QuotaTop
            | OlapTemplateKind::TenantOverview
            | OlapTemplateKind::TenantErrors
            | OlapTemplateKind::ResponseCacheHit => OlapAccessRequirement::AnyRole,
        }
    }

    /// 校验当前角色是否可查询指定 OLAP 请求体；返回解析出的模板枚举或拒绝提示。
    ///
    /// # Errors
    ///
    /// 模板未知返回 `Err((400, msg))`，角色权限不足返回 `Err((403, msg))`。
    pub fn authorize_olap(
        self,
        body: &Value,
    ) -> Result<OlapTemplateKind, (axum::http::StatusCode, &'static str)> {
        let Some(raw_tpl) = body.get("template").and_then(Value::as_str) else {
            return Err((axum::http::StatusCode::BAD_REQUEST, "缺少 OLAP template 字段"));
        };
        let Some(kind) = OlapTemplateKind::parse(raw_tpl) else {
            return Err((axum::http::StatusCode::BAD_REQUEST, "未知 OLAP 模板"));
        };
        let metric = body
            .get("params")
            .and_then(|p| p.get("metric"))
            .and_then(Value::as_str);
        match Self::olap_requirement(kind, metric) {
            OlapAccessRequirement::AnyRole => Ok(kind),
            OlapAccessRequirement::Biz => {
                if self.is_biz {
                    Ok(kind)
                } else {
                    Err((
                        axum::http::StatusCode::FORBIDDEN,
                        "金额与计费类 OLAP 模板仅 R_ADMIN、R_FINANCE 可查询",
                    ))
                }
            }
            OlapAccessRequirement::Admin => {
                if self.is_admin {
                    Ok(kind)
                } else {
                    Err((
                        axum::http::StatusCode::FORBIDDEN,
                        "安全治理类 OLAP 模板仅 R_ADMIN 可查询",
                    ))
                }
            }
        }
    }

    /// 对 OLAP 查询结果执行编译期穷举字段裁剪（`R_VIEWER` 剥离 `product_rank` 与 `tenant_overview` 的 `cost` 字段）。
    pub fn project_olap_response(self, kind: OlapTemplateKind, data: &mut Value) {
        match kind {
            OlapTemplateKind::ProductRank | OlapTemplateKind::TenantOverview => {
                if self.is_viewer() {
                    strip_field_from_olap_rows(data, "cost");
                }
            }
            OlapTemplateKind::TopByDim
            | OlapTemplateKind::LatencyQuantiles
            | OlapTemplateKind::LatencyBreakdown
            | OlapTemplateKind::LatencyHeatmap
            | OlapTemplateKind::StageQuantiles
            | OlapTemplateKind::StatusHeatmap
            | OlapTemplateKind::BytesTrend
            | OlapTemplateKind::SlowRequests
            | OlapTemplateKind::ErrorMatrix
            | OlapTemplateKind::ErrorAttribution
            | OlapTemplateKind::ResponseFlags
            | OlapTemplateKind::NewErrorCodes
            | OlapTemplateKind::CallHistogram
            | OlapTemplateKind::CostTrend
            | OlapTemplateKind::SupplierSla
            | OlapTemplateKind::BillingFunnelCh
            | OlapTemplateKind::ExemplarTraces
            | OlapTemplateKind::QuotaTop
            | OlapTemplateKind::TenantErrors
            | OlapTemplateKind::ResponseCacheHit
            | OlapTemplateKind::SecurityTop
            | OlapTemplateKind::SecurityEvents => {}
        }
    }

    /// 指标时序与目录访问判定：
    /// - `cert_*`、`tls_*`、`license_*` 仅限 `R_ADMIN`；
    /// - `billing_*`、`balance_*` 仅限 `BIZ`（`R_ADMIN` / `R_FINANCE`）。
    #[must_use]
    pub fn can_read_metric(self, metric: &str) -> bool {
        let m = metric.trim();
        if m.starts_with("cert_") || m.starts_with("tls_") || m.starts_with("license_") {
            return self.is_admin;
        }
        if m.starts_with("billing_") || m.starts_with("balance_") {
            return self.is_biz;
        }
        true
    }

    /// 过滤 `GET /stats/metrics/catalog` 返回的指标列表。
    pub fn project_metrics_catalog(self, body: &mut Value) {
        if self.is_admin {
            return;
        }
        let Some(metrics) = body.get_mut("metrics") else {
            return;
        };
        filter_metric_items(metrics, self);
    }

    /// `GET /stats/ops/slos/status`：非 `R_ADMIN` 剥离 `definitions[].query` `PromQL` 表达式。
    pub fn project_slo_definitions(self, definitions: &mut Value) {
        if self.is_admin {
            return;
        }
        if let Some(items) = definitions.as_array_mut() {
            for item in items {
                if let Some(obj) = item.as_object_mut() {
                    obj.remove("query");
                }
            }
        }
    }

    /// `GET /channels` 与 `GET /channels/{id}`：`R_VIEWER` 剥离采购底价 `cost_price`。
    pub fn project_channel_value(self, value: &mut Value) {
        if !self.is_viewer() {
            return;
        }
        if let Some(obj) = value.as_object_mut() {
            obj.remove("cost_price");
            if let Some(arr) = obj.get_mut("channels").and_then(Value::as_array_mut) {
                for ch in arr {
                    if let Some(ch_obj) = ch.as_object_mut() {
                        ch_obj.remove("cost_price");
                    }
                }
            }
        }
    }

    /// `GET /tenants/{code}/analytics`：`R_VIEWER` 剥离 `total_amount` 与 `total_cost`。
    pub fn project_tenant_usage_analytics(self, analytics: &mut [Value], top_endpoints: &mut [Value]) {
        if !self.is_viewer() {
            return;
        }
        for item in analytics {
            if let Some(obj) = item.as_object_mut() {
                obj.remove("total_amount");
            }
        }
        for ep in top_endpoints {
            if let Some(obj) = ep.as_object_mut() {
                obj.remove("total_cost");
            }
        }
    }

    /// `GET /stats/calls/customer` 与 `/stats/calls/upstream`：`R_VIEWER` 脱敏 `detail.raw` 与 `api_key_id`。
    pub fn project_call_monitor_item(
        self,
        item: &mut crate::services::analytics::CallMonitorItem,
    ) {
        if !self.is_viewer() {
            return;
        }
        item.api_key_id.clear();
        redact_call_detail(&mut item.detail);
    }

    /// `GET /stats/calls/stages`：`R_VIEWER` 脱敏 `calls` 与 `stages` 中的 `raw` 和 `api_key_id`。
    pub fn project_call_stages(self, body: &mut Value) {
        if !self.is_viewer() {
            return;
        }
        let Some(obj) = body.as_object_mut() else {
            return;
        };
        if let Some(calls) = obj.get_mut("calls").and_then(Value::as_array_mut) {
            for call in calls {
                redact_call_detail(call);
            }
        }
        if let Some(stages) = obj.get_mut("stages").and_then(Value::as_array_mut) {
            for stage in stages {
                redact_call_detail(stage);
            }
        }
    }

    /// `GET /upstream-connectors/drafts*`：`R_VIEWER` 掩码 `workbench_env` 变量值与静态认证头。
    pub fn project_connector_draft(self, draft: &mut Value) {
        if !self.is_viewer() {
            return;
        }
        if let Some(obj) = draft.as_object_mut() {
            if let Some(items) = obj.get_mut("items").and_then(Value::as_array_mut) {
                for item in items {
                    mask_draft_object(item);
                }
                return;
            }
            if let Some(drafts) = obj.get_mut("drafts").and_then(Value::as_array_mut) {
                for item in drafts {
                    mask_draft_object(item);
                }
                return;
            }
        }
        mask_draft_object(draft);
    }

    /// `GET /upstream-connectors/.../runs*`：`R_VIEWER` 掩码试跑凭据回显。
    pub fn project_connector_runs(self, runs: &mut Value) {
        if !self.is_viewer() {
            return;
        }
        if let Some(arr) = runs.as_array_mut() {
            for run in arr {
                mask_run_object(run);
            }
            return;
        }
        if let Some(obj) = runs.as_object_mut()
            && let Some(items) = obj.get_mut("items").and_then(Value::as_array_mut)
        {
            for run in items {
                mask_run_object(run);
            }
            return;
        }
        mask_run_object(runs);
    }
}

/// 脱敏对账配置中的 `alert.webhook_url` 密钥段（保留协议、主机与首段路径前缀）。
#[must_use]
pub fn mask_webhook_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let Ok(mut parsed) = url::Url::parse(trimmed) else {
        return "••••••••".to_string();
    };
    let _ = parsed.set_password(None);
    if !parsed.username().is_empty() {
        let _ = parsed.set_username("••••••••");
    }
    parsed.set_query(None);
    parsed.set_fragment(None);
    let segments: Vec<String> = parsed
        .path_segments()
        .map_or_default(|s| s.filter(|seg| !seg.is_empty()).map(str::to_string).collect());
    if segments.len() <= 1 {
        parsed.set_path("/••••••••");
    } else {
        parsed.set_path(&format!("/{}/••••••••", segments[0]));
    }
    parsed.to_string()
}

/// 判断提交的 `webhook_url` 是否为未修改的脱敏占位值。
#[must_use]
pub fn is_masked_webhook_url(submitted: &str, existing: &str) -> bool {
    let s = submitted.trim();
    if s.is_empty() {
        return false;
    }
    s.contains("••••••••") || s.contains("********") || (!existing.is_empty() && s == mask_webhook_url(existing))
}

fn strip_field_from_olap_rows(data: &mut Value, field: &str) {
    if let Some(arr) = data.as_array_mut() {
        for row in arr {
            if let Some(obj) = row.as_object_mut() {
                obj.remove(field);
            }
        }
    } else if let Some(rows) = data
        .as_object_mut()
        .and_then(|o| o.get_mut("rows"))
        .and_then(Value::as_array_mut)
    {
        for row in rows {
            if let Some(obj) = row.as_object_mut() {
                obj.remove(field);
            }
        }
    }
}

fn filter_metric_items(node: &mut Value, proj: RoleDataProjection) {
    match node {
        Value::Array(items) => {
            items.retain(|item| {
                item.get("key")
                    .and_then(Value::as_str)
                    .is_none_or(|key| proj.can_read_metric(key))
            });
        }
        Value::Object(map) => {
            for val in map.values_mut() {
                filter_metric_items(val, proj);
            }
        }
        _ => {}
    }
}

fn redact_call_detail(val: &mut Value) {
    let Some(obj) = val.as_object_mut() else {
        return;
    };
    obj.remove("raw");
    if obj.contains_key("api_key_id") {
        obj.insert("api_key_id".to_string(), Value::String(String::new()));
    }
    for key in [
        "request_headers",
        "response_headers",
        "request_body",
        "response_body",
    ] {
        obj.remove(key);
    }
    if let Some(detail) = obj.get_mut("detail") {
        redact_call_detail(detail);
    }
}

fn is_sensitive_header_or_key(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "authorization" | "proxy-authorization" | "cookie" | "set-cookie" | "x-api-key"
    ) || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("passwd")
        || lower.contains("credential")
        || lower.contains("apikey")
        || lower.contains("api_key")
        || lower.contains("api-key")
        || lower.contains("signature")
        || lower.contains("sign")
        || lower.contains("auth")
}

fn mask_headers_map_or_list(headers: &mut Value) {
    match headers {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if is_sensitive_header_or_key(k)
                    && v.as_str().is_some_and(|s| !s.trim().is_empty())
                {
                    *v = Value::String("••••••••".to_string());
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                if let Some(obj) = item.as_object_mut() {
                    let key_name = obj
                        .get("key")
                        .or_else(|| obj.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if is_sensitive_header_or_key(&key_name)
                        && let Some(val) = obj.get_mut("value")
                        && val.as_str().is_some_and(|s| !s.trim().is_empty())
                    {
                        *val = Value::String("••••••••".to_string());
                    }
                }
            }
        }
        _ => {}
    }
}

fn mask_workbench_env(env: &mut Value) {
    match env {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if k == "vars" || k == "variables" || k == "secrets" {
                    if let Some(vars_obj) = v.as_object_mut() {
                        for val in vars_obj.values_mut() {
                            if !val.is_null() {
                                *val = Value::String("••••••••".to_string());
                            }
                        }
                    } else if let Some(vars_arr) = v.as_array_mut() {
                        for entry in vars_arr {
                            if let Some(e_obj) = entry.as_object_mut()
                                && let Some(val) = e_obj.get_mut("value")
                            {
                                *val = Value::String("••••••••".to_string());
                            }
                        }
                    }
                } else if k == "environments" || k == "profiles" {
                    if let Some(envs) = v.as_array_mut() {
                        for item in envs {
                            mask_workbench_env(item);
                        }
                    } else if let Some(envs_map) = v.as_object_mut() {
                        for item in envs_map.values_mut() {
                            mask_workbench_env(item);
                        }
                    }
                } else if v.is_string() && k != "active_env_id" && k != "id" && k != "name" {
                    *v = Value::String("••••••••".to_string());
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                mask_workbench_env(item);
            }
        }
        _ => {}
    }
}

fn mask_draft_object(draft: &mut Value) {
    let Some(obj) = draft.as_object_mut() else {
        return;
    };
    if let Some(env) = obj.get_mut("workbench_env") {
        mask_workbench_env(env);
    }
    if let Some(auth_headers) = obj
        .get_mut("config")
        .and_then(Value::as_object_mut)
        .and_then(|c| c.get_mut("auth_flow"))
        .and_then(Value::as_object_mut)
        .and_then(|a| a.get_mut("headers"))
    {
        mask_headers_map_or_list(auth_headers);
    }
    if let Some(endpoints) = obj.get_mut("endpoints").and_then(Value::as_array_mut) {
        for ep in endpoints {
            let Some(ep_obj) = ep.as_object_mut() else {
                continue;
            };
            ep_obj.remove("cost_price");
            if let Some(h) = ep_obj.get_mut("headers") {
                mask_headers_map_or_list(h);
            }
            if let Some(egress) = ep_obj.get_mut("egress").and_then(Value::as_object_mut) {
                if let Some(req_h) = egress
                    .get_mut("request")
                    .and_then(Value::as_object_mut)
                    .and_then(|r| r.get_mut("headers"))
                {
                    mask_headers_map_or_list(req_h);
                }
                if let Some(auth_h) = egress
                    .get_mut("auth")
                    .and_then(Value::as_object_mut)
                    .and_then(|a| a.get_mut("headers"))
                {
                    mask_headers_map_or_list(auth_h);
                }
            }
        }
    }
}

fn mask_run_value_recursive(val: &mut Value) {
    match val {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if k == "captured" || k == "captures" || k == "credentials" {
                    if let Some(inner) = v.as_object_mut() {
                        for iv in inner.values_mut() {
                            *iv = Value::String("••••••••".to_string());
                        }
                    } else if !v.is_null() {
                        *v = Value::String("••••••••".to_string());
                    }
                } else if k == "headers" || k == "request_headers" || k == "response_headers" {
                    mask_headers_map_or_list(v);
                } else if is_sensitive_header_or_key(k) && v.is_string() {
                    *v = Value::String("••••••••".to_string());
                } else {
                    mask_run_value_recursive(v);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                mask_run_value_recursive(item);
            }
        }
        _ => {}
    }
}

fn mask_run_object(run: &mut Value) {
    let Some(obj) = run.as_object_mut() else {
        return;
    };
    if let Some(stages) = obj.get_mut("stages") {
        mask_run_value_recursive(stages);
    }
    if let Some(preview) = obj.get_mut("response_preview") {
        mask_run_value_recursive(preview);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

    #[test]
    fn olap_template_gate_exhaustively_matches_analytics_enum() {
        let olap_src = include_str!("../../../mazbri-analytics/src/clickhouse/olap.rs");
        let start = olap_src
            .find("pub fn template_name(&self) -> &'static str {")
            .expect("locate OlapQuery::template_name");
        let body = &olap_src[start..start + 1600];
        let mut analytics_templates = BTreeSet::new();
        for line in body.lines() {
            if let Some((_, rhs)) = line.split_once("=> \"")
                && let Some((name, _)) = rhs.split_once('"')
            {
                analytics_templates.insert(name.to_string());
            }
            if line.trim() == "}" && !analytics_templates.is_empty() {
                break;
            }
        }
        assert_eq!(analytics_templates.len(), 24);
        let mgmt_templates: BTreeSet<String> = OlapTemplateKind::ALL
            .iter()
            .map(|k| k.as_str().to_string())
            .collect();
        assert_eq!(mgmt_templates, analytics_templates);
    }

    #[test]
    fn olap_role_gates_and_cost_projection() {
        let viewer = RoleDataProjection::for_role("R_VIEWER");
        let finance = RoleDataProjection::for_role("R_FINANCE");
        let admin = RoleDataProjection::for_role("R_ADMIN");

        for tpl in ["cost_trend", "billing_funnel_ch"] {
            let req = json!({ "template": tpl, "params": {} });
            assert!(viewer.authorize_olap(&req).is_err(), "{tpl} must block viewer");
            assert!(finance.authorize_olap(&req).is_ok(), "{tpl} must allow finance");
            assert!(admin.authorize_olap(&req).is_ok());
        }
        let top_cost = json!({ "template": "top_by_dim", "params": { "metric": "cost" } });
        assert!(viewer.authorize_olap(&top_cost).is_err());
        assert!(finance.authorize_olap(&top_cost).is_ok());

        for sec in ["security_top", "security_events"] {
            let req = json!({ "template": sec, "params": {} });
            assert!(viewer.authorize_olap(&req).is_err());
            assert!(finance.authorize_olap(&req).is_err());
            assert!(admin.authorize_olap(&req).is_ok());
        }

        let rank_calls = json!({ "template": "product_rank", "params": { "metric": "calls" } });
        let kind = viewer.authorize_olap(&rank_calls).expect("viewer allowed");
        let mut resp = json!({
            "template": "product_rank",
            "rows": [{ "product_id": "P1", "req_count": 10, "cost": 1.25 }]
        });
        viewer.project_olap_response(kind, &mut resp);
        assert!(resp["rows"][0].get("cost").is_none());

        let mut ov = json!({
            "template": "tenant_overview",
            "rows": [{ "tenant_code": "T1", "calls": 5, "cost": "9.99" }]
        });
        viewer.project_olap_response(OlapTemplateKind::TenantOverview, &mut ov);
        assert!(ov["rows"][0].get("cost").is_none());
    }

    #[test]
    fn metric_and_webhook_masking_rules() {
        let viewer = RoleDataProjection::for_role("R_VIEWER");
        let finance = RoleDataProjection::for_role("R_FINANCE");
        let admin = RoleDataProjection::for_role("R_ADMIN");

        assert!(!viewer.can_read_metric("cert_active_count"));
        assert!(!finance.can_read_metric("tls_cert_expiry_days"));
        assert!(!finance.can_read_metric("license_expiry_days"));
        assert!(admin.can_read_metric("license_expiry_days"));

        assert!(!viewer.can_read_metric("billing_outbox_pending"));
        assert!(!viewer.can_read_metric("balance_precheck"));
        assert!(finance.can_read_metric("billing_outbox_pending"));
        assert!(viewer.can_read_metric("qps_by_mode"));

        let masked = mask_webhook_url("https://hooks.example.com/services/T000/B000/secret-token?sig=1");
        assert!(!masked.contains("secret-token"));
        assert!(!masked.contains("sig=1"));
        assert!(is_masked_webhook_url(&masked, "https://hooks.example.com/services/T000/B000/secret-token?sig=1"));
    }
}
