//! Trace 查询：基于 `GreptimeDB` `opentelemetry_traces` 表的检索、span 树与服务依赖聚合。
//!
//! - 检索：按 trace 聚合 span（起止、span 数、服务数、错误数），再取根 span 名；
//! - 详情：全部 span 按父子关系展开为深度优先列表，并计算关键路径；
//! - 依赖：`parent_span_id` 自关联得到跨服务调用边，另从 CLIENT span 推出 Redis / PG 依赖，
//!   从网关 `upstream.attempt` 与其下游 bypass span 推出 bypass → 上游供应商边。
use super::GreptimeDBClient;
use crate::error::Result;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

/// trace 表名。
const TRACE_TABLE: &str = "opentelemetry_traces";
/// 检索结果条数上限。
pub const MAX_SEARCH_LIMIT: u32 = 200;
/// 单条 trace 最多返回的 span 数。
const MAX_TRACE_SPANS: u32 = 5_000;
/// 计费 worker 回链查询窗口：原请求开始后的秒数。
const BILLING_LINK_WINDOW_SECS: i64 = 2 * 3_600;
/// 计费服务名。
const BILLING_SERVICE: &str = "mazbri-billing";
const STATUS_ERROR: &str = "STATUS_CODE_ERROR";
const ATTR_PREFIX: &str = "span_attributes.";
const RESOURCE_PREFIX: &str = "resource_attributes.";

/// 校验 W3C trace id（32 位十六进制）。
///
/// # Errors
///
/// 格式不符时返回错误。
pub fn validate_trace_id(raw: &str) -> std::result::Result<String, String> {
    let id = raw.trim().to_ascii_lowercase();
    if id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(id)
    } else {
        Err(format!("trace_id 格式无效: {raw}"))
    }
}

/// 校验服务名（字母、数字、`-`、`_`、`.`，最长 64）。
///
/// # Errors
///
/// 格式不符时返回错误。
pub fn validate_service_name(raw: &str) -> std::result::Result<String, String> {
    let name = raw.trim();
    if !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        Ok(name.to_string())
    } else {
        Err(format!("service 格式无效: {raw}"))
    }
}

/// 检索条件（已校验）。
#[derive(Debug, Clone, Default)]
pub struct TraceSearchFilter {
    /// Unix 秒，含。
    pub start: i64,
    /// Unix 秒，不含。
    pub end: i64,
    /// 已由 `trace_id` / `request_id` / 租户 / 产品解析出的候选 trace。
    pub trace_ids: Option<Vec<String>>,
    pub service: Option<String>,
    pub min_duration_ms: Option<u64>,
    pub max_duration_ms: Option<u64>,
    pub errors_only: bool,
    pub limit: u32,
}

/// 检索结果行。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TraceSummary {
    pub trace_id: String,
    /// 开始时间（Unix 毫秒）。
    pub start_ms: f64,
    pub duration_ms: f64,
    pub span_count: u64,
    pub service_count: u64,
    pub error_count: u64,
    pub root_service: String,
    pub root_span: String,
}

/// 单个 span（深度优先顺序中的一项）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SpanNode {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service_name: String,
    pub span_name: String,
    pub span_kind: String,
    pub status_code: String,
    pub status_message: String,
    /// 开始时间（Unix 纳秒）。
    pub start_ns: i64,
    pub end_ns: i64,
    pub duration_ns: i64,
    /// 树深度（根为 0）。
    pub depth: usize,
    pub child_count: usize,
    /// 是否在关键路径上。
    pub critical: bool,
    pub error: bool,
    pub attributes: BTreeMap<String, Value>,
    pub resource: BTreeMap<String, Value>,
    pub events: Value,
    pub links: Value,
}

/// 链接到本 trace 的其他 trace span（计费 worker 批次等）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LinkedSpan {
    pub trace_id: String,
    pub span_id: String,
    pub service_name: String,
    pub span_name: String,
    /// 被链接的本 trace span。
    pub linked_span_id: String,
}

/// trace 详情。
#[derive(Debug, Clone, Serialize)]
pub struct TraceDetail {
    pub trace_id: String,
    pub start_ns: i64,
    pub end_ns: i64,
    pub duration_ms: f64,
    pub services: Vec<String>,
    pub critical_path: Vec<String>,
    pub spans: Vec<SpanNode>,
    pub linked_from: Vec<LinkedSpan>,
}

/// 依赖边类别。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// 服务间 RPC / HTTP（父子 span 跨服务）。
    Rpc,
    /// 存储依赖（CLIENT span：Redis / PostgreSQL）。
    Storage,
    /// bypass → 上游供应商。
    Upstream,
}

/// 服务依赖边。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DependencyEdge {
    pub source: String,
    pub target: String,
    pub kind: EdgeKind,
    pub calls: u64,
    pub errors: u64,
    pub p99_ms: f64,
}

/// 依赖图。
#[derive(Debug, Clone, Serialize)]
pub struct DependencyGraph {
    pub start: i64,
    pub end: i64,
    pub nodes: Vec<String>,
    pub edges: Vec<DependencyEdge>,
}

fn time_window(column: &str, start: i64, end: i64) -> String {
    format!("{column} >= to_timestamp_seconds({start}) AND {column} < to_timestamp_seconds({end})")
}

fn quoted_list(ids: &[String]) -> String {
    ids.iter()
        .map(|id| format!("'{id}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// 检索聚合 SQL：每个 trace 一行，按开始时间倒序。
#[must_use]
pub fn build_search_sql(f: &TraceSearchFilter) -> String {
    let mut conds = vec![time_window("timestamp", f.start, f.end)];
    if let Some(ids) = &f.trace_ids {
        conds.push(format!("trace_id IN ({})", quoted_list(ids)));
    }
    if let Some(service) = &f.service {
        conds.push(format!(
            "trace_id IN (SELECT trace_id FROM {TRACE_TABLE} WHERE {} AND service_name = '{service}')",
            time_window("timestamp", f.start, f.end)
        ));
    }
    let duration = "(CAST(max(timestamp_end) AS BIGINT) - CAST(min(timestamp) AS BIGINT))";
    let mut having = Vec::new();
    if let Some(min) = f.min_duration_ms {
        having.push(format!("{duration} >= {}", min.saturating_mul(1_000_000)));
    }
    if let Some(max) = f.max_duration_ms {
        having.push(format!("{duration} <= {}", max.saturating_mul(1_000_000)));
    }
    if f.errors_only {
        having.push(format!(
            "sum(CASE WHEN span_status_code = '{STATUS_ERROR}' THEN 1 ELSE 0 END) > 0"
        ));
    }
    let having = if having.is_empty() {
        String::new()
    } else {
        format!(" HAVING {}", having.join(" AND "))
    };
    let order = if f.min_duration_ms.is_some() {
        "duration_ns DESC"
    } else {
        "start_ns DESC"
    };
    format!(
        "SELECT trace_id, CAST(min(timestamp) AS BIGINT) AS start_ns, \
         {duration} AS duration_ns, count(*) AS span_count, \
         count(DISTINCT service_name) AS service_count, \
         sum(CASE WHEN span_status_code = '{STATUS_ERROR}' THEN 1 ELSE 0 END) AS error_count \
         FROM {TRACE_TABLE} WHERE {conds} GROUP BY trace_id{having} ORDER BY {order} LIMIT {limit}",
        conds = conds.join(" AND "),
        limit = f.limit.clamp(1, MAX_SEARCH_LIMIT),
    )
}

/// 检索结果所需的 span 骨架（用于确定根 span）。
#[must_use]
pub fn build_skeleton_sql(trace_ids: &[String], start: i64, end: i64) -> String {
    format!(
        "SELECT trace_id, span_id, parent_span_id, service_name, span_name, \
         CAST(timestamp AS BIGINT) AS start_ns, duration_nano FROM {TRACE_TABLE} \
         WHERE {} AND trace_id IN ({})",
        time_window("timestamp", start, end),
        quoted_list(trace_ids)
    )
}

/// 单 trace 全部 span。
#[must_use]
pub fn build_detail_sql(trace_id: &str, start: i64, end: i64) -> String {
    format!(
        "SELECT * FROM {TRACE_TABLE} WHERE trace_id = '{trace_id}' AND {} \
         ORDER BY timestamp LIMIT {MAX_TRACE_SPANS}",
        time_window("timestamp", start, end)
    )
}

/// 链接到本 trace 的计费 worker span。
#[must_use]
pub fn build_linked_from_sql(trace_id: &str, start: i64) -> String {
    format!(
        "SELECT trace_id, span_id, service_name, span_name, span_links FROM {TRACE_TABLE} \
         WHERE service_name = '{BILLING_SERVICE}' AND {} \
         AND json_to_string(span_links) LIKE '%{trace_id}%' LIMIT 50",
        time_window("timestamp", start - 60, start + BILLING_LINK_WINDOW_SECS)
    )
}

/// 跨服务父子 span 边。
#[must_use]
pub fn build_rpc_edges_sql(start: i64, end: i64) -> String {
    format!(
        "SELECT p.service_name AS source, c.service_name AS target, count(*) AS calls, \
         sum(CASE WHEN c.span_status_code = '{STATUS_ERROR}' THEN 1 ELSE 0 END) AS errors, \
         approx_percentile_cont(c.duration_nano, 0.99) AS p99_ns \
         FROM {TRACE_TABLE} c JOIN {TRACE_TABLE} p \
           ON c.trace_id = p.trace_id AND c.parent_span_id = p.span_id \
         WHERE {} AND {} AND p.service_name <> c.service_name \
         GROUP BY p.service_name, c.service_name",
        time_window("c.timestamp", start, end),
        time_window("p.timestamp", start, end)
    )
}

/// CLIENT span 推出的存储依赖边。
#[must_use]
pub fn build_storage_edges_sql(start: i64, end: i64) -> String {
    format!(
        "SELECT service_name AS source, \
           CASE WHEN span_name LIKE 'redis.%' OR span_name LIKE 'cache.%' THEN 'redis' \
                WHEN span_name LIKE 'db.%' THEN 'postgres' END AS target, \
         count(*) AS calls, \
         sum(CASE WHEN span_status_code = '{STATUS_ERROR}' THEN 1 ELSE 0 END) AS errors, \
         approx_percentile_cont(duration_nano, 0.99) AS p99_ns \
         FROM {TRACE_TABLE} WHERE {} AND span_kind = 'SPAN_KIND_CLIENT' \
           AND (span_name LIKE 'redis.%' OR span_name LIKE 'cache.%' OR span_name LIKE 'db.%') \
         GROUP BY 1, 2",
        time_window("timestamp", start, end)
    )
}

/// Greptime 懒加载列：`upstream.attempt` 写入前可能尚不存在。
const SUPPLIER_CODE_ATTR: &str = "span_attributes.supplier_code";

/// 检查 trace 表是否已有 `span_attributes.supplier_code`（OTLP 懒加列）。
#[must_use]
pub fn build_supplier_code_column_check_sql() -> String {
    format!(
        "SELECT column_name FROM information_schema.columns          WHERE table_name = '{TRACE_TABLE}'            AND column_name = '{SUPPLIER_CODE_ATTR}' LIMIT 1"
    )
}

/// bypass → 上游供应商边：网关 `upstream.attempt` 记录供应商，其子 span 为 bypass 出站。
#[must_use]
pub fn build_upstream_edges_sql(start: i64, end: i64) -> String {
    format!(
        "SELECT c.service_name AS source, p.\"{SUPPLIER_CODE_ATTR}\" AS supplier, \
         count(*) AS calls, \
         sum(CASE WHEN p.span_status_code = '{STATUS_ERROR}' \
                    OR c.span_status_code = '{STATUS_ERROR}' THEN 1 ELSE 0 END) AS errors, \
         approx_percentile_cont(c.duration_nano, 0.99) AS p99_ns \
         FROM {TRACE_TABLE} c JOIN {TRACE_TABLE} p \
           ON c.trace_id = p.trace_id AND c.parent_span_id = p.span_id \
         WHERE {} AND {} AND p.span_name = 'upstream.attempt' \
           AND p.service_name <> c.service_name \
         GROUP BY 1, 2",
        time_window("c.timestamp", start, end),
        time_window("p.timestamp", start, end)
    )
}

fn str_field(row: &HashMap<String, Value>, key: &str) -> String {
    match row.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn int_field(row: &HashMap<String, Value>, key: &str) -> i64 {
    match row.get(key) {
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_f64().and_then(f64_to_i64))
            .unwrap_or(0),
        Some(Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

fn u64_field(row: &HashMap<String, Value>, key: &str) -> u64 {
    u64::try_from(int_field(row, key)).unwrap_or(0)
}

fn f64_field(row: &HashMap<String, Value>, key: &str) -> f64 {
    match row.get(key) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// 有限且落在 i64 范围内的浮点取整。
fn f64_to_i64(v: f64) -> Option<i64> {
    if !v.is_finite() {
        return None;
    }
    let rounded = v.round();
    format!("{rounded:.0}").parse().ok()
}

fn ns_to_ms(ns: i64) -> f64 {
    // 纳秒值远小于 2^53 ms 精度损失阈值的场景下按十进制换算。
    let whole = ns / 1_000_000;
    let frac = ns % 1_000_000;
    let whole = whole.to_string().parse::<f64>().unwrap_or(0.0);
    let frac = frac.to_string().parse::<f64>().unwrap_or(0.0);
    whole + frac / 1_000_000.0
}

fn parent_of(row: &HashMap<String, Value>) -> Option<String> {
    Some(str_field(row, "parent_span_id")).filter(|p| !p.is_empty())
}

/// 骨架行中每个 trace 的根 span（父 span 不在本 trace 内；多个时取最早开始、其次最长）。
fn pick_roots(rows: &[HashMap<String, Value>]) -> HashMap<String, (String, String)> {
    let mut by_trace: HashMap<String, Vec<&HashMap<String, Value>>> = HashMap::new();
    for row in rows {
        by_trace
            .entry(str_field(row, "trace_id"))
            .or_default()
            .push(row);
    }
    by_trace
        .into_iter()
        .filter_map(|(trace_id, spans)| {
            let ids: HashSet<String> = spans.iter().map(|r| str_field(r, "span_id")).collect();
            let root = spans
                .iter()
                .filter(|r| parent_of(r).is_none_or(|p| !ids.contains(&p)))
                .min_by_key(|r| {
                    (
                        int_field(r, "start_ns"),
                        std::cmp::Reverse(int_field(r, "duration_nano")),
                    )
                })?;
            Some((
                trace_id,
                (
                    str_field(root, "service_name"),
                    str_field(root, "span_name"),
                ),
            ))
        })
        .collect()
}

/// 聚合行 + 根 span → 检索结果。
fn summaries_from_rows(
    rows: &[HashMap<String, Value>],
    roots: &HashMap<String, (String, String)>,
) -> Vec<TraceSummary> {
    rows.iter()
        .map(|row| {
            let trace_id = str_field(row, "trace_id");
            let (root_service, root_span) = roots.get(&trace_id).cloned().unwrap_or_default();
            TraceSummary {
                start_ms: ns_to_ms(int_field(row, "start_ns")),
                duration_ms: ns_to_ms(int_field(row, "duration_ns")),
                span_count: u64_field(row, "span_count"),
                service_count: u64_field(row, "service_count"),
                error_count: u64_field(row, "error_count"),
                trace_id,
                root_service,
                root_span,
            }
        })
        .collect()
}

fn json_field(row: &HashMap<String, Value>, key: &str) -> Value {
    match row.get(key) {
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::String(s.clone())),
        Some(Value::Null) | None => Value::Array(Vec::new()),
        Some(v) => v.clone(),
    }
}

fn prefixed(row: &HashMap<String, Value>, prefix: &str) -> BTreeMap<String, Value> {
    row.iter()
        .filter(|(k, v)| k.starts_with(prefix) && !v.is_null())
        .map(|(k, v)| (k[prefix.len()..].to_string(), v.clone()))
        .collect()
}

/// 详情行 → 未排序的 span。
fn spans_from_rows(rows: &[HashMap<String, Value>]) -> Vec<SpanNode> {
    rows.iter()
        .map(|row| {
            let start_ns = int_field(row, "timestamp");
            let end_ns = int_field(row, "timestamp_end").max(start_ns);
            let status_code = str_field(row, "span_status_code");
            SpanNode {
                span_id: str_field(row, "span_id"),
                parent_span_id: parent_of(row),
                service_name: str_field(row, "service_name"),
                span_name: str_field(row, "span_name"),
                span_kind: str_field(row, "span_kind"),
                error: status_code == STATUS_ERROR,
                status_code,
                status_message: str_field(row, "span_status_message"),
                start_ns,
                end_ns,
                duration_ns: end_ns - start_ns,
                depth: 0,
                child_count: 0,
                critical: false,
                attributes: prefixed(row, ATTR_PREFIX),
                resource: prefixed(row, RESOURCE_PREFIX),
                events: json_field(row, "span_events"),
                links: json_field(row, "span_links"),
            }
        })
        .collect()
}

/// 子 span 覆盖父 span 时长的百分比达到该值且自身无子 span 时视为汇总 span
/// （如 gateway 的 `platform_summary`），不参与关键路径。
const SUMMARY_COVERAGE_PERCENT: i128 = 95;

/// 关键路径：自 `idx` 起，从结束时刻向前回溯，每次取在游标之前结束最晚的子 span
/// 并递归，游标移到其开始时刻；子 span 之间的串行等待因此都落在路径上。
/// 选中的子 span 若无子 span、但时间上包含另一个有子 span 或 CLIENT 类的兄弟
/// （阶段 span 与其内部真实调用平级上报），一并标记并递归进入被包含的兄弟。
fn critical_path(
    spans: &[SpanNode],
    children: &HashMap<String, Vec<usize>>,
    idx: usize,
    out: &mut Vec<String>,
) {
    out.push(spans[idx].span_id.clone());
    let Some(list) = children.get(&spans[idx].span_id) else {
        return;
    };
    let parent = &spans[idx];
    let child_count = |i: usize| children.get(&spans[i].span_id).map_or(0, Vec::len);
    let parent_ns = parent.duration_ns.max(1);
    let candidates: Vec<usize> = list
        .iter()
        .copied()
        .filter(|&i| {
            let covered = i128::from(spans[i].duration_ns) * 100;
            list.len() == 1
                || child_count(i) > 0
                || covered < i128::from(parent_ns) * SUMMARY_COVERAGE_PERCENT
        })
        .collect();
    let mut cursor = candidates
        .iter()
        .map(|&i| spans[i].end_ns)
        .max()
        .unwrap_or(parent.end_ns)
        .max(parent.end_ns);
    let mut used: HashSet<usize> = HashSet::new();
    loop {
        let pick = candidates
            .iter()
            .copied()
            .filter(|i| !used.contains(i) && spans[*i].end_ns <= cursor)
            .max_by(|a, b| {
                spans[*a]
                    .end_ns
                    .cmp(&spans[*b].end_ns)
                    .then(spans[*b].start_ns.cmp(&spans[*a].start_ns))
            });
        let Some(pick) = pick else {
            break;
        };
        used.insert(pick);
        critical_path(spans, children, pick, out);
        if child_count(pick) == 0 {
            let inner = candidates
                .iter()
                .copied()
                .filter(|&i| {
                    i != pick
                        && !used.contains(&i)
                        && spans[i].start_ns >= spans[pick].start_ns
                        && spans[i].end_ns <= spans[pick].end_ns
                        && (child_count(i) > 0 || spans[i].span_kind == "SPAN_KIND_CLIENT")
                })
                .max_by_key(|&i| spans[i].end_ns);
            if let Some(inner) = inner {
                used.insert(inner);
                critical_path(spans, children, inner, out);
            }
        }
        cursor = spans[pick].start_ns;
    }
}

/// 把 span 排成深度优先列表（兄弟按开始时间），填充深度、子数与关键路径标记；
/// 返回关键路径 span id（见 [`critical_path`]）。
#[must_use]
pub fn build_tree(spans: Vec<SpanNode>) -> (Vec<SpanNode>, Vec<String>) {
    let ids: HashSet<String> = spans.iter().map(|s| s.span_id.clone()).collect();
    let mut children: HashMap<String, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for (i, span) in spans.iter().enumerate() {
        match &span.parent_span_id {
            Some(p) if ids.contains(p) && p != &span.span_id => {
                children.entry(p.clone()).or_default().push(i);
            }
            _ => roots.push(i),
        }
    }
    let by_start = |a: &usize, b: &usize| {
        spans[*a]
            .start_ns
            .cmp(&spans[*b].start_ns)
            .then(spans[*b].duration_ns.cmp(&spans[*a].duration_ns))
    };
    roots.sort_by(by_start);
    for list in children.values_mut() {
        list.sort_by(by_start);
    }

    let mut critical = Vec::new();
    if let Some(&main) = roots.first() {
        critical_path(&spans, &children, main, &mut critical);
    }
    let critical_set: HashSet<&String> = critical.iter().collect();

    let mut order: Vec<(usize, usize)> = Vec::with_capacity(spans.len());
    let mut stack: Vec<(usize, usize)> = roots.iter().rev().map(|&r| (r, 0)).collect();
    let mut seen = HashSet::new();
    while let Some((idx, depth)) = stack.pop() {
        if !seen.insert(idx) {
            continue;
        }
        order.push((idx, depth));
        if let Some(list) = children.get(&spans[idx].span_id) {
            for &c in list.iter().rev() {
                stack.push((c, depth + 1));
            }
        }
    }
    let flags: Vec<(usize, usize, bool)> = order
        .iter()
        .map(|&(idx, depth)| {
            let count = children.get(&spans[idx].span_id).map_or(0, Vec::len);
            (depth, count, critical_set.contains(&spans[idx].span_id))
        })
        .collect();
    let mut slots: Vec<Option<SpanNode>> = spans.into_iter().map(Some).collect();
    let ordered = order
        .iter()
        .zip(flags)
        .filter_map(|(&(idx, _), (depth, count, is_critical))| {
            slots[idx].take().map(|mut s| {
                s.depth = depth;
                s.child_count = count;
                s.critical = is_critical;
                s
            })
        })
        .collect();
    (ordered, critical)
}

/// 计费 span 行 → 指向本 trace 的链接。
fn linked_from_rows(rows: &[HashMap<String, Value>], trace_id: &str) -> Vec<LinkedSpan> {
    let mut out = Vec::new();
    for row in rows {
        let links = json_field(row, "span_links");
        for link in links.as_array().into_iter().flatten() {
            if link.get("trace_id").and_then(Value::as_str) != Some(trace_id) {
                continue;
            }
            out.push(LinkedSpan {
                trace_id: str_field(row, "trace_id"),
                span_id: str_field(row, "span_id"),
                service_name: str_field(row, "service_name"),
                span_name: str_field(row, "span_name"),
                linked_span_id: link
                    .get("span_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            });
        }
    }
    out
}

fn edges_from_rows(
    rows: &[HashMap<String, Value>],
    kind: EdgeKind,
    target_of: impl Fn(&HashMap<String, Value>) -> Option<String>,
) -> Vec<DependencyEdge> {
    rows.iter()
        .filter_map(|row| {
            let target = target_of(row)?;
            Some(DependencyEdge {
                source: str_field(row, "source"),
                target,
                kind,
                calls: u64_field(row, "calls"),
                errors: u64_field(row, "errors"),
                p99_ms: f64_field(row, "p99_ns") / 1_000_000.0,
            })
        })
        .collect()
}

/// 合并三类边并生成节点表（按类别、调用量排序）。
fn assemble_graph(
    start: i64,
    end: i64,
    rpc: &[HashMap<String, Value>],
    storage: &[HashMap<String, Value>],
    upstream: &[HashMap<String, Value>],
) -> DependencyGraph {
    let non_empty = |s: String| Some(s).filter(|s| !s.is_empty());
    let mut edges = edges_from_rows(rpc, EdgeKind::Rpc, |r| non_empty(str_field(r, "target")));
    edges.extend(edges_from_rows(storage, EdgeKind::Storage, |r| {
        non_empty(str_field(r, "target"))
    }));
    edges.extend(edges_from_rows(upstream, EdgeKind::Upstream, |r| {
        let supplier = str_field(r, "supplier");
        Some(if supplier.is_empty() {
            "upstream".to_string()
        } else {
            format!("upstream:{supplier}")
        })
    }));
    edges.retain(|e| !e.source.is_empty());
    edges.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.calls.cmp(&a.calls)));
    let mut nodes: Vec<String> = edges
        .iter()
        .flat_map(|e| [e.source.clone(), e.target.clone()])
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    nodes.sort();
    DependencyGraph {
        start,
        end,
        nodes,
        edges,
    }
}

impl GreptimeDBClient {
    /// trace 检索。
    ///
    /// # Errors
    ///
    /// `GreptimeDB` 查询失败或超时时返回错误。
    pub async fn search_traces(&self, filter: &TraceSearchFilter) -> Result<Vec<TraceSummary>> {
        if filter.trace_ids.as_ref().is_some_and(Vec::is_empty) {
            return Ok(Vec::new());
        }
        let rows = self
            .with_query_timeout(
                "trace search",
                self.execute_trace_query_rows(&build_search_sql(filter), "trace search"),
            )
            .await?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = rows.iter().map(|r| str_field(r, "trace_id")).collect();
        let skeleton = self
            .with_query_timeout(
                "trace roots",
                self.execute_trace_query_rows(
                    &build_skeleton_sql(&ids, filter.start, filter.end),
                    "trace roots",
                ),
            )
            .await?;
        Ok(summaries_from_rows(&rows, &pick_roots(&skeleton)))
    }

    /// 单条 trace 详情；trace 不存在时返回 `None`。
    ///
    /// # Errors
    ///
    /// `GreptimeDB` 查询失败或超时时返回错误。
    pub async fn trace_detail(
        &self,
        trace_id: &str,
        start: i64,
        end: i64,
    ) -> Result<Option<TraceDetail>> {
        let rows = self
            .with_query_timeout(
                "trace detail",
                self.execute_trace_query_rows(
                    &build_detail_sql(trace_id, start, end),
                    "trace detail",
                ),
            )
            .await?;
        if rows.is_empty() {
            return Ok(None);
        }
        let (spans, critical_path) = build_tree(spans_from_rows(&rows));
        let start_ns = spans.iter().map(|s| s.start_ns).min().unwrap_or(0);
        let end_ns = spans.iter().map(|s| s.end_ns).max().unwrap_or(start_ns);
        let mut services: Vec<String> = spans
            .iter()
            .map(|s| s.service_name.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        services.sort();
        let linked = self
            .with_query_timeout(
                "trace linked spans",
                self.execute_trace_query_rows(
                    &build_linked_from_sql(trace_id, start_ns / 1_000_000_000),
                    "trace linked spans",
                ),
            )
            .await?;
        Ok(Some(TraceDetail {
            trace_id: trace_id.to_string(),
            start_ns,
            end_ns,
            duration_ms: ns_to_ms(end_ns - start_ns),
            services,
            critical_path,
            spans,
            linked_from: linked_from_rows(&linked, trace_id),
        }))
    }

    /// 服务依赖图。
    ///
    /// # Errors
    ///
    /// `GreptimeDB` 查询失败或超时时返回错误。
    pub async fn trace_dependencies(&self, start: i64, end: i64) -> Result<DependencyGraph> {
        let rpc_sql = build_rpc_edges_sql(start, end);
        let storage_sql = build_storage_edges_sql(start, end);
        let (rpc, storage, upstream) = tokio::try_join!(
            self.with_query_timeout(
                "trace rpc edges",
                self.execute_trace_query_rows(&rpc_sql, "trace rpc edges"),
            ),
            self.with_query_timeout(
                "trace storage edges",
                self.execute_trace_query_rows(&storage_sql, "trace storage edges"),
            ),
            self.upstream_edges_or_empty(start, end),
        )?;
        Ok(assemble_graph(start, end, &rpc, &storage, &upstream))
    }

    /// 上游边依赖懒加列 `span_attributes.supplier_code`；列不存在时退化为空边，避免 502。
    async fn upstream_edges_or_empty(
        &self,
        start: i64,
        end: i64,
    ) -> Result<Vec<HashMap<String, Value>>> {
        let cols = self
            .with_query_timeout(
                "trace supplier_code column",
                self.execute_trace_query_rows(
                    &build_supplier_code_column_check_sql(),
                    "trace supplier_code column",
                ),
            )
            .await?;
        if cols.is_empty() {
            tracing::debug!(
                "trace dependencies: skip upstream edges; column {SUPPLIER_CODE_ATTR} absent"
            );
            return Ok(Vec::new());
        }
        let upstream_sql = build_upstream_edges_sql(start, end);
        self.with_query_timeout(
            "trace upstream edges",
            self.execute_trace_query_rows(&upstream_sql, "trace upstream edges"),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn span(id: &str, parent: Option<&str>, start: i64, end: i64) -> SpanNode {
        SpanNode {
            span_id: id.to_string(),
            parent_span_id: parent.map(str::to_string),
            service_name: "svc".to_string(),
            span_name: id.to_string(),
            span_kind: "SPAN_KIND_INTERNAL".to_string(),
            status_code: String::new(),
            status_message: String::new(),
            start_ns: start,
            end_ns: end,
            duration_ns: end - start,
            depth: 0,
            child_count: 0,
            critical: false,
            error: false,
            attributes: BTreeMap::new(),
            resource: BTreeMap::new(),
            events: json!([]),
            links: json!([]),
        }
    }

    #[test]
    fn tree_is_depth_first_with_sorted_siblings() {
        // root 的父 span 来自客户端 traceparent，不在本 trace 内。
        let spans = vec![
            span("c2", Some("root"), 50, 90),
            span("g1", Some("c1"), 15, 20),
            span("root", Some("external"), 0, 100),
            span("c1", Some("root"), 10, 30),
        ];
        let (ordered, _) = build_tree(spans);
        let order: Vec<(&str, usize)> = ordered
            .iter()
            .map(|s| (s.span_id.as_str(), s.depth))
            .collect();
        assert_eq!(order, vec![("root", 0), ("c1", 1), ("g1", 2), ("c2", 1)]);
        assert_eq!(ordered[0].child_count, 2);
    }

    #[test]
    fn critical_path_follows_latest_ending_child() {
        let spans = vec![
            span("root", None, 0, 100),
            span("auth", Some("root"), 1, 5),
            span("upstream", Some("root"), 10, 95),
            span("attempt1", Some("upstream"), 10, 40),
            span("attempt2", Some("upstream"), 41, 94),
            span("billing", Some("root"), 20, 30),
        ];
        let (ordered, path) = build_tree(spans);
        // billing 与 upstream 并行、在 upstream 开始前未结束，不在关键路径上。
        assert_eq!(
            path,
            vec!["root", "upstream", "attempt2", "attempt1", "auth"]
        );
        let flagged: Vec<&str> = ordered
            .iter()
            .filter(|s| s.critical)
            .map(|s| s.span_id.as_str())
            .collect();
        assert_eq!(
            flagged,
            vec!["root", "auth", "upstream", "attempt1", "attempt2"]
        );
    }

    #[test]
    fn critical_path_skips_summary_and_enters_contained_call() {
        let mut attempt = span("upstream.attempt", Some("root"), 84, 199);
        attempt.span_kind = "SPAN_KIND_CLIENT".to_string();
        let spans = vec![
            span("root", None, 0, 222),
            span("platform_summary", Some("root"), 0, 222),
            span("auth", Some("root"), 0, 11),
            span("balance_check", Some("root"), 12, 23),
            span("upstream", Some("root"), 24, 200),
            attempt,
            span("bypass", Some("upstream.attempt"), 90, 199),
            span("billing.enqueue", Some("root"), 200, 222),
            span("post_response", Some("root"), 222, 225),
        ];
        let (_, path) = build_tree(spans);
        assert_eq!(
            path,
            vec![
                "root",
                "post_response",
                "billing.enqueue",
                "upstream",
                "upstream.attempt",
                "bypass",
                "balance_check",
                "auth",
            ]
        );
    }

    #[test]
    fn orphan_spans_become_extra_roots_after_main_root() {
        let spans = vec![
            span("late", Some("missing"), 500, 600),
            span("root", None, 0, 100),
            span("child", Some("root"), 5, 50),
        ];
        let (ordered, path) = build_tree(spans);
        assert_eq!(ordered.len(), 3);
        assert_eq!(ordered[0].span_id, "root");
        assert_eq!(ordered[2].span_id, "late");
        assert_eq!(ordered[2].depth, 0);
        assert_eq!(path, vec!["root", "child"]);
    }

    #[test]
    fn self_parent_does_not_loop() {
        let (ordered, path) = build_tree(vec![span("a", Some("a"), 0, 1)]);
        assert_eq!(ordered.len(), 1);
        assert_eq!(path, vec!["a"]);
    }

    fn row(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn roots_ignore_external_parent() {
        let rows = vec![
            row(&[
                ("trace_id", json!("t1")),
                ("span_id", json!("a")),
                ("parent_span_id", json!("client")),
                ("service_name", json!("mazbri-gateway")),
                ("span_name", json!("request")),
                ("start_ns", json!(10)),
                ("duration_nano", json!(100)),
            ]),
            row(&[
                ("trace_id", json!("t1")),
                ("span_id", json!("b")),
                ("parent_span_id", json!("a")),
                ("service_name", json!("mazbri-bypass")),
                ("span_name", json!("Execute")),
                ("start_ns", json!(5)),
                ("duration_nano", json!(10)),
            ]),
        ];
        let roots = pick_roots(&rows);
        assert_eq!(
            roots.get("t1"),
            Some(&("mazbri-gateway".to_string(), "request".to_string()))
        );
    }

    #[test]
    fn supplier_code_column_check_sql_targets_lazy_attr() {
        let sql = build_supplier_code_column_check_sql();
        assert!(sql.contains("information_schema.columns"));
        assert!(sql.contains(SUPPLIER_CODE_ATTR));
        assert!(sql.contains(TRACE_TABLE));
        assert!(!sql.contains("JOIN"));
    }

    #[test]
    fn upstream_edges_sql_quotes_supplier_attr() {
        let sql = build_upstream_edges_sql(1, 2);
        assert!(sql.contains(&format!("p.\"{SUPPLIER_CODE_ATTR}\"")));
        assert!(sql.contains("upstream.attempt"));
    }

    #[test]
    fn assemble_graph_allows_empty_upstream_when_column_missing() {
        let rpc = vec![row(&[
            ("source", json!("mazbri-gateway")),
            ("target", json!("mazbri-auth")),
            ("calls", json!(3)),
            ("errors", json!(0)),
            ("p99_ns", json!(1_000_000)),
        ])];
        let g = assemble_graph(0, 1, &rpc, &[], &[]);
        assert_eq!(g.edges.len(), 1);
        assert!(g.nodes.contains(&"mazbri-gateway".to_string()));
        assert!(g.nodes.contains(&"mazbri-auth".to_string()));
    }

    #[test]
    fn spans_split_attributes_and_parse_links() {
        let rows = vec![row(&[
            ("timestamp", json!(1_000)),
            ("timestamp_end", json!(3_000)),
            ("span_id", json!("s")),
            ("parent_span_id", Value::Null),
            ("span_status_code", json!(STATUS_ERROR)),
            ("span_attributes.supplier_code", json!("upupt-rest")),
            ("span_attributes.hit", Value::Null),
            ("span_links", json!([{"trace_id": "t", "span_id": "x"}])),
        ])];
        let spans = spans_from_rows(&rows);
        assert_eq!(spans[0].duration_ns, 2_000);
        assert!(spans[0].error);
        assert_eq!(spans[0].parent_span_id, None);
        assert_eq!(
            spans[0].attributes.keys().collect::<Vec<_>>(),
            vec!["supplier_code"]
        );
        let linked = linked_from_rows(
            &[row(&[
                ("trace_id", json!("billing")),
                ("span_id", json!("b1")),
                (
                    "span_links",
                    json!([{"trace_id": "t", "span_id": "x"}, {"trace_id": "o", "span_id": "y"}]),
                ),
            ])],
            "t",
        );
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].linked_span_id, "x");
    }

    #[test]
    fn graph_merges_edge_kinds() {
        let rpc = vec![row(&[
            ("source", json!("mazbri-gateway")),
            ("target", json!("mazbri-bypass")),
            ("calls", json!(10)),
            ("errors", json!(1)),
            ("p99_ns", json!(2_500_000.0)),
        ])];
        let storage = vec![row(&[
            ("source", json!("mazbri-gateway")),
            ("target", json!("redis")),
            ("calls", json!(30)),
            ("errors", json!(0)),
            ("p99_ns", json!(1_000_000)),
        ])];
        let upstream = vec![row(&[
            ("source", json!("mazbri-bypass")),
            ("supplier", json!("upupt-rest")),
            ("calls", json!(10)),
            ("errors", json!(0)),
            ("p99_ns", json!(100_000_000)),
        ])];
        let g = assemble_graph(0, 1, &rpc, &storage, &upstream);
        let pairs: Vec<(&str, &str, EdgeKind)> = g
            .edges
            .iter()
            .map(|e| (e.source.as_str(), e.target.as_str(), e.kind))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("mazbri-gateway", "mazbri-bypass", EdgeKind::Rpc),
                ("mazbri-gateway", "redis", EdgeKind::Storage),
                ("mazbri-bypass", "upstream:upupt-rest", EdgeKind::Upstream),
            ]
        );
        assert!((g.edges[0].p99_ms - 2.5).abs() < 1e-9);
        assert_eq!(g.nodes.len(), 4);
    }

    #[test]
    fn search_sql_applies_filters() {
        let sql = build_search_sql(&TraceSearchFilter {
            start: 100,
            end: 200,
            trace_ids: Some(vec!["ab".into()]),
            service: Some("mazbri-bypass".into()),
            min_duration_ms: Some(1_500),
            max_duration_ms: None,
            errors_only: true,
            limit: 9_999,
        });
        assert!(sql.contains("trace_id IN ('ab')"));
        assert!(sql.contains("service_name = 'mazbri-bypass'"));
        assert!(sql.contains(">= 1500000000"));
        assert!(sql.contains("> 0"));
        assert!(sql.contains("ORDER BY duration_ns DESC LIMIT 200"));
    }

    #[test]
    fn id_validation() {
        assert!(validate_trace_id("63F05844CA1775B24FF3B5D7BD4F9F8A").is_ok());
        assert!(validate_trace_id("x' OR 1=1").is_err());
        assert!(validate_service_name("mazbri-gateway").is_ok());
        assert!(validate_service_name("a'b").is_err());
    }

    #[test]
    fn ns_conversion() {
        assert!((ns_to_ms(1_500_000) - 1.5).abs() < 1e-12);
        assert!((ns_to_ms(1_793_495_098_958_500_000) - 1_793_495_098_958.5).abs() < 1e-3);
    }
}
