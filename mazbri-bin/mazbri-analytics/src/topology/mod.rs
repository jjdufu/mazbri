//! 步骤 21：服务拓扑——静态骨架 + 指标边 / span 边 / CH 上游边合并。
use crate::clickhouse::ClickHouseClient;
use crate::greptimedb::GreptimeDBClient;
use crate::greptimedb::traces::{DependencyEdge, DependencyGraph, EdgeKind};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// 拓扑节点。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TopologyNode {
    pub id: String,
    /// `client` / `service` / `middleware` / `upstream`
    pub kind: String,
    /// `green` / `yellow` / `red` / `unknown`
    pub health: String,
}

/// 拓扑边（带 QPS / 错误率 / P99）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TopologyEdge {
    pub source: String,
    pub target: String,
    /// `rpc` / `storage` / `upstream`
    pub kind: String,
    /// `metric` / `span` / `request_log` / `static`
    pub origin: String,
    pub qps: f64,
    pub error_rate: f64,
    pub p99_ms: f64,
    pub calls: u64,
    pub errors: u64,
}

/// 合并后的拓扑图。
#[derive(Debug, Clone, Serialize)]
pub struct TopologyGraph {
    pub start: i64,
    pub end: i64,
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<TopologyEdge>,
}

const SERVICES: &[&str] = &[
    "mazbri-gateway",
    "mazbri-auth",
    "mazbri-ratelimit",
    "mazbri-cache",
    "mazbri-cbpro",
    "mazbri-cbpro-ext",
    "mazbri-billing",
    "mazbri-bypass",
    "mazbri-analytics",
    "mazbri-mgmt",
    "mazbri-cert",
];

const MIDDLEWARE: &[&str] = &["postgres", "redis", "redpanda", "clickhouse", "greptimedb"];

fn ratio_u64(numer: u64, denom: u64) -> f64 {
    if denom == 0 {
        return 0.0;
    }
    let n = u32::try_from(numer.min(u64::from(u32::MAX))).unwrap_or(u32::MAX);
    let d = u32::try_from(denom.min(u64::from(u32::MAX))).unwrap_or(u32::MAX).max(1);
    f64::from(n) / f64::from(d)
}

fn window_secs_f64(start: i64, end: i64) -> f64 {
    let secs = (end - start).max(1);
    let capped = i32::try_from(secs.min(i64::from(i32::MAX))).unwrap_or(i32::MAX);
    f64::from(capped.max(1))
}

/// 网关 → 内部服务的静态骨架边。
#[must_use]
pub fn static_skeleton_edges() -> Vec<TopologyEdge> {
    let mut edges = vec![TopologyEdge {
        source: "client".into(),
        target: "mazbri-gateway".into(),
        kind: "rpc".into(),
        origin: "static".into(),
        qps: 0.0,
        error_rate: 0.0,
        p99_ms: 0.0,
        calls: 0,
        errors: 0,
    }];
    for svc in [
        "mazbri-auth",
        "mazbri-ratelimit",
        "mazbri-cache",
        "mazbri-cbpro",
        "mazbri-billing",
        "mazbri-bypass",
    ] {
        edges.push(TopologyEdge {
            source: "mazbri-gateway".into(),
            target: svc.into(),
            kind: "rpc".into(),
            origin: "static".into(),
            qps: 0.0,
            error_rate: 0.0,
            p99_ms: 0.0,
            calls: 0,
            errors: 0,
        });
    }
    edges
}

fn node_kind(id: &str) -> &'static str {
    if id == "client" {
        "client"
    } else if id.starts_with("upstream:") || id == "upstream" {
        "upstream"
    } else if MIDDLEWARE.contains(&id) {
        "middleware"
    } else {
        "service"
    }
}

fn health_of(error_rate: f64, has_traffic: bool) -> &'static str {
    if !has_traffic {
        return "unknown";
    }
    if error_rate >= 0.05 {
        "red"
    } else if error_rate >= 0.01 {
        "yellow"
    } else {
        "green"
    }
}

fn edge_key(e: &TopologyEdge) -> (String, String, String) {
    (e.source.clone(), e.target.clone(), e.kind.clone())
}

/// 同源优先：`metric` > `request_log` > `span` > `static`。
fn origin_rank(origin: &str) -> u8 {
    match origin {
        "metric" => 3,
        "request_log" => 2,
        "span" => 1,
        _ => 0,
    }
}

/// 合并边：同 (source,target,kind) 保留更高优先级来源；calls/errors 取较大者。
#[must_use]
pub fn merge_edges(parts: Vec<Vec<TopologyEdge>>) -> Vec<TopologyEdge> {
    let mut map: BTreeMap<(String, String, String), TopologyEdge> = BTreeMap::new();
    for batch in parts {
        for edge in batch {
            let key = edge_key(&edge);
            match map.get_mut(&key) {
                None => {
                    map.insert(key, edge);
                }
                Some(existing) => {
                    if origin_rank(&edge.origin) > origin_rank(&existing.origin) {
                        let prev_calls = existing.calls;
                        let prev_errors = existing.errors;
                        *existing = edge;
                        existing.calls = existing.calls.max(prev_calls);
                        existing.errors = existing.errors.max(prev_errors);
                    } else {
                        existing.calls = existing.calls.max(edge.calls);
                        existing.errors = existing.errors.max(edge.errors);
                        if existing.qps <= 0.0 && edge.qps > 0.0 {
                            existing.qps = edge.qps;
                        }
                        if existing.p99_ms <= 0.0 && edge.p99_ms > 0.0 {
                            existing.p99_ms = edge.p99_ms;
                        }
                        if existing.error_rate <= 0.0 && edge.error_rate > 0.0 {
                            existing.error_rate = edge.error_rate;
                        }
                    }
                }
            }
        }
    }
    let mut edges: Vec<_> = map.into_values().collect();
    edges.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then(b.qps.partial_cmp(&a.qps).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.source.cmp(&b.source))
    });
    edges
}

fn build_nodes(edges: &[TopologyEdge]) -> Vec<TopologyNode> {
    let mut ids: BTreeSet<String> = BTreeSet::new();
    ids.insert("client".into());
    for s in SERVICES {
        ids.insert((*s).into());
    }
    for m in MIDDLEWARE {
        ids.insert((*m).into());
    }
    let mut traffic: BTreeMap<String, (f64, bool)> = BTreeMap::new();
    for e in edges {
        ids.insert(e.source.clone());
        ids.insert(e.target.clone());
        let entry = traffic.entry(e.target.clone()).or_insert((0.0, false));
        if e.calls > 0 || e.qps > 0.0 {
            entry.1 = true;
            entry.0 = entry.0.max(e.error_rate);
        }
    }
    ids.into_iter()
        .map(|id| {
            let (err, has) = traffic.get(&id).copied().unwrap_or((0.0, false));
            TopologyNode {
                kind: node_kind(&id).into(),
                health: health_of(err, has).into(),
                id,
            }
        })
        .collect()
}

fn from_dependency_edge(e: &DependencyEdge, origin: &str, window_secs: f64) -> TopologyEdge {
    let calls_f = f64::from(u32::try_from(e.calls.min(u64::from(u32::MAX))).unwrap_or(u32::MAX));
    let qps = if window_secs > 0.0 { calls_f / window_secs } else { 0.0 };
    let error_rate = ratio_u64(e.errors, e.calls);
    TopologyEdge {
        source: e.source.clone(),
        target: e.target.clone(),
        kind: match e.kind {
            EdgeKind::Rpc => "rpc",
            EdgeKind::Storage => "storage",
            EdgeKind::Upstream => "upstream",
        }
        .into(),
        origin: origin.into(),
        qps,
        error_rate,
        p99_ms: e.p99_ms,
        calls: e.calls,
        errors: e.errors,
    }
}

fn label(map: &BTreeMap<String, String>, key: &str) -> String {
    map.get(key).cloned().unwrap_or_default()
}

/// 从 `PromQL` 向量组装 RPC 边。
#[must_use]
pub fn rpc_edges_from_vectors(
    qps: &[(BTreeMap<String, String>, f64)],
    err_qps: &[(BTreeMap<String, String>, f64)],
    p99: &[(BTreeMap<String, String>, f64)],
) -> Vec<TopologyEdge> {
    let mut err_map: BTreeMap<(String, String), f64> = BTreeMap::new();
    for (labels, value) in err_qps {
        let from_svc = label(labels, "caller");
        let to_svc = label(labels, "callee");
        if !from_svc.is_empty() && !to_svc.is_empty() {
            err_map.insert((from_svc, to_svc), *value);
        }
    }
    let mut p99_map: BTreeMap<(String, String), f64> = BTreeMap::new();
    for (labels, value) in p99 {
        let from_svc = label(labels, "caller");
        let to_svc = label(labels, "callee");
        if !from_svc.is_empty() && !to_svc.is_empty() {
            p99_map.insert((from_svc, to_svc), *value * 1000.0);
        }
    }
    let mut out = Vec::new();
    for (labels, value) in qps {
        let from_svc = label(labels, "caller");
        let to_svc = label(labels, "callee");
        if from_svc.is_empty() || to_svc.is_empty() || !value.is_finite() || *value <= 0.0 {
            continue;
        }
        let e_qps = err_map
            .get(&(from_svc.clone(), to_svc.clone()))
            .copied()
            .unwrap_or(0.0);
        let error_rate = if *value > 0.0 {
            (e_qps / *value).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let p99_ms = p99_map
            .get(&(from_svc.clone(), to_svc.clone()))
            .copied()
            .unwrap_or(0.0);
        out.push(TopologyEdge {
            source: from_svc,
            target: to_svc,
            kind: "rpc".into(),
            origin: "metric".into(),
            qps: *value,
            error_rate,
            p99_ms,
            calls: 0,
            errors: 0,
        });
    }
    out
}

/// 从 `PromQL` 向量组装依赖（storage）边。
#[must_use]
pub fn dep_edges_from_vectors(
    count_qps: &[(BTreeMap<String, String>, f64)],
    err_qps: &[(BTreeMap<String, String>, f64)],
    p99: &[(BTreeMap<String, String>, f64)],
) -> Vec<TopologyEdge> {
    let mut err_map: BTreeMap<(String, String), f64> = BTreeMap::new();
    for (labels, value) in err_qps {
        let service = label(labels, "service");
        let dep = normalize_dep(&label(labels, "dep"));
        if !service.is_empty() && !dep.is_empty() {
            *err_map.entry((service, dep)).or_insert(0.0) += *value;
        }
    }
    let mut p99_map: BTreeMap<(String, String), f64> = BTreeMap::new();
    for (labels, value) in p99 {
        let service = label(labels, "service");
        let dep = normalize_dep(&label(labels, "dep"));
        if !service.is_empty() && !dep.is_empty() {
            p99_map.insert((service, dep), *value * 1000.0);
        }
    }
    let mut qps_map: BTreeMap<(String, String), f64> = BTreeMap::new();
    for (labels, value) in count_qps {
        let service = label(labels, "service");
        let dep = normalize_dep(&label(labels, "dep"));
        if service.is_empty() || dep.is_empty() || !value.is_finite() || *value <= 0.0 {
            continue;
        }
        *qps_map.entry((service, dep)).or_insert(0.0) += *value;
    }
    qps_map
        .into_iter()
        .map(|((service, dep), qps)| {
            let e_qps = err_map
                .get(&(service.clone(), dep.clone()))
                .copied()
                .unwrap_or(0.0);
            let error_rate = if qps > 0.0 {
                (e_qps / qps).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let p99_ms = p99_map
                .get(&(service.clone(), dep.clone()))
                .copied()
                .unwrap_or(0.0);
            TopologyEdge {
                source: service,
                target: dep,
                kind: "storage".into(),
                origin: "metric".into(),
                qps,
                error_rate,
                p99_ms,
                calls: 0,
                errors: 0,
            }
        })
        .collect()
}

fn normalize_dep(dep: &str) -> String {
    match dep {
        "pg" | "postgres" | "pgsql" => "postgres".into(),
        "kafka" | "redpanda" => "redpanda".into(),
        "ch" | "clickhouse" => "clickhouse".into(),
        "greptime" | "greptimedb" => "greptimedb".into(),
        other => other.to_string(),
    }
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct UpstreamAggRow {
    supplier_code: String,
    calls: u64,
    errors: u64,
    p99_ms: f64,
}

fn upstream_sql(start: i64, end: i64) -> String {
    let start_dt = crate::clickhouse::queries::ch_datetime_utc(start);
    let end_dt = crate::clickhouse::queries::ch_datetime_utc(end);
    format!(
        "SELECT supplier_code, toUInt64(count()) AS calls, \
         toUInt64(countIf(status_code >= 500 OR error_class IN ('timeout','upstream_error','connect_error'))) AS errors, \
         toFloat64(quantileTDigest(0.99)(duration_ms)) AS p99_ms \
         FROM request_logs \
         WHERE event_target = 'gateway' \
           AND timestamp >= {start_dt} AND timestamp < {end_dt} \
           AND supplier_code != '' \
         GROUP BY supplier_code \
         ORDER BY calls DESC \
         LIMIT 50"
    )
}

fn ch_upstream_edges(rows: &[UpstreamAggRow], window_secs: f64) -> Vec<TopologyEdge> {
    rows.iter()
        .filter(|r| !r.supplier_code.is_empty())
        .map(|r| {
            let calls_f = f64::from(u32::try_from(r.calls.min(u64::from(u32::MAX))).unwrap_or(u32::MAX));
            let qps = if window_secs > 0.0 { calls_f / window_secs } else { 0.0 };
            let error_rate = ratio_u64(r.errors, r.calls);
            TopologyEdge {
                source: "mazbri-gateway".into(),
                target: format!("upstream:{}", r.supplier_code),
                kind: "upstream".into(),
                origin: "request_log".into(),
                qps,
                error_rate,
                p99_ms: r.p99_ms,
                calls: r.calls,
                errors: r.errors,
            }
        })
        .collect()
}

/// 构建拓扑：静态 + 指标 + span + CH 上游。
///
/// # Errors
///
/// `GreptimeDB` / `ClickHouse` 查询失败时返回错误字符串（调用方记日志并降级）。
pub async fn build_topology(
    greptime: &GreptimeDBClient,
    clickhouse: &ClickHouseClient,
    start: i64,
    end: i64,
) -> Result<TopologyGraph, String> {
    let window_secs = window_secs_f64(start, end);
    let static_edges = static_skeleton_edges();

    let rpc_qps_q = r"sum by (caller, callee) (rate(mazbri_rpc_client_requests_total[5m]))";
    let rpc_err_q =
        r#"sum by (caller, callee) (rate(mazbri_rpc_client_requests_total{code!="ok"}[5m]))"#;
    let rpc_p99_q = r"histogram_quantile(0.99, sum by (caller, callee, le) (rate(mazbri_rpc_client_duration_seconds_bucket[5m])))";
    let dep_qps_q = r"sum by (service, dep) (rate(mazbri_dep_client_duration_seconds_count[5m]))";
    let dep_err_q = r"sum by (service, dep) (rate(mazbri_dep_client_errors_total[5m]))";
    let dep_p99_q = r"histogram_quantile(0.99, sum by (service, dep, le) (rate(mazbri_dep_client_duration_seconds_bucket[5m])))";

    let (rpc_qps, rpc_err, rpc_p99, dep_qps, dep_err, dep_p99) = tokio::join!(
        greptime.query_instant_vector(rpc_qps_q),
        greptime.query_instant_vector(rpc_err_q),
        greptime.query_instant_vector(rpc_p99_q),
        greptime.query_instant_vector(dep_qps_q),
        greptime.query_instant_vector(dep_err_q),
        greptime.query_instant_vector(dep_p99_q),
    );

    let metric_rpc = rpc_edges_from_vectors(
        &rpc_qps.unwrap_or_default(),
        &rpc_err.unwrap_or_default(),
        &rpc_p99.unwrap_or_default(),
    );
    let metric_dep = dep_edges_from_vectors(
        &dep_qps.unwrap_or_default(),
        &dep_err.unwrap_or_default(),
        &dep_p99.unwrap_or_default(),
    );

    let span_edges = match greptime.trace_dependencies(start, end).await {
        Ok(DependencyGraph { edges, .. }) => edges
            .iter()
            .map(|e| from_dependency_edge(e, "span", window_secs))
            .collect::<Vec<_>>(),
        Err(e) => {
            tracing::warn!(error = %e, "拓扑：span 依赖边不可用");
            Vec::new()
        }
    };

    let ch_edges = match clickhouse
        .execute_query::<UpstreamAggRow>(&upstream_sql(start, end))
        .await
    {
        Ok(rows) => ch_upstream_edges(&rows, window_secs),
        Err(e) => {
            tracing::warn!(error = %e, "拓扑：CH 上游边不可用");
            Vec::new()
        }
    };

    let edges = merge_edges(vec![
        static_edges,
        metric_rpc,
        metric_dep,
        span_edges,
        ch_edges,
    ]);
    let nodes = build_nodes(&edges);
    Ok(TopologyGraph {
        start,
        end,
        nodes,
        edges,
    })
}

/// 供 handler 使用的薄包装。
///
/// # Errors
///
/// `GreptimeDB` 未配置或构建失败时返回错误字符串。
pub async fn query_topology(
    greptime: Option<Arc<GreptimeDBClient>>,
    clickhouse: Arc<ClickHouseClient>,
    start: i64,
    end: i64,
) -> Result<TopologyGraph, String> {
    let Some(gt) = greptime.as_deref() else {
        return Err("greptimedb_unavailable".into());
    };
    build_topology(gt, clickhouse.as_ref(), start, end).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_prefers_metric_over_static() {
        let static_e = vec![TopologyEdge {
            source: "mazbri-gateway".into(),
            target: "mazbri-auth".into(),
            kind: "rpc".into(),
            origin: "static".into(),
            qps: 0.0,
            error_rate: 0.0,
            p99_ms: 0.0,
            calls: 0,
            errors: 0,
        }];
        let metric_e = vec![TopologyEdge {
            source: "mazbri-gateway".into(),
            target: "mazbri-auth".into(),
            kind: "rpc".into(),
            origin: "metric".into(),
            qps: 12.5,
            error_rate: 0.02,
            p99_ms: 40.0,
            calls: 13,
            errors: 1,
        }];
        let merged = merge_edges(vec![static_e, metric_e]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].origin, "metric");
        assert!((merged[0].qps - 12.5).abs() < f64::EPSILON);
        assert!((merged[0].error_rate - 0.02).abs() < f64::EPSILON);
    }

    #[test]
    fn health_thresholds() {
        assert_eq!(health_of(0.0, false), "unknown");
        assert_eq!(health_of(0.0, true), "green");
        assert_eq!(health_of(0.02, true), "yellow");
        assert_eq!(health_of(0.1, true), "red");
    }

    #[test]
    fn normalize_dep_aliases() {
        assert_eq!(normalize_dep("pg"), "postgres");
        assert_eq!(normalize_dep("kafka"), "redpanda");
        assert_eq!(normalize_dep("ch"), "clickhouse");
    }

    #[test]
    fn rpc_edges_compute_error_rate() {
        let mut ok = BTreeMap::new();
        ok.insert("caller".into(), "mazbri-gateway".into());
        ok.insert("callee".into(), "mazbri-auth".into());
        let err = ok.clone();
        let qps = vec![(ok, 10.0)];
        let err_qps = vec![(err, 1.0)];
        let edges = rpc_edges_from_vectors(&qps, &err_qps, &[]);
        assert_eq!(edges.len(), 1);
        assert!((edges[0].error_rate - 0.1).abs() < 1e-9);
        assert_eq!(edges[0].origin, "metric");
    }

    #[test]
    fn upstream_sql_uses_utc_datetime() {
        let sql = upstream_sql(1_700_000_000, 1_700_003_600);
        assert!(
            sql.contains("toDateTime(1700000000, 'UTC')"),
            "start bound must be UTC: {sql}"
        );
        assert!(
            sql.contains("toDateTime(1700003600, 'UTC')"),
            "end bound must be UTC: {sql}"
        );
        assert!(
            !sql.contains("toDateTime(1700000000) AND")
                && !sql.contains(">= toDateTime(1700000000) "),
            "bare toDateTime(unix) without UTC must not appear: {sql}"
        );
        assert!(sql.contains("event_target = 'gateway'"));
    }

    #[test]
    fn skeleton_includes_client_and_gateway_fanout() {
        let edges = static_skeleton_edges();
        assert!(
            edges
                .iter()
                .any(|e| e.source == "client" && e.target == "mazbri-gateway")
        );
        assert!(edges.iter().any(|e| e.target == "mazbri-bypass"));
        assert_eq!(edges.iter().filter(|e| e.origin == "static").count(), 7);
    }
}
