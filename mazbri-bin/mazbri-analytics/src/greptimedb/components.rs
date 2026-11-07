//! 组件健康（应用服务 / 中间件 / 观测组件）：Dashboard「实时状态」与大屏资源页的唯一数据源。
//!
//! - 应用服务：来自 `service_targets`（`metrics_targets` 配置或默认列表），状态由
//!   `mazbri_rpc_health_check` / `mazbri_rpc_ready` / `mazbri_config_center_connected` 判定；
//! - 中间件 / 观测组件：来自 [`MIDDLEWARE_CATALOG`]，状态由各自探针指标判定；
//! - 容器层：cAdvisor（compose service / k8s container 名）给出存活、近 1h 重启、CPU、内存；
//!   未登记在目录里的容器以 `other` 分组原样列出，不漏项、不重复。
//!
//! 显示名只在这里定义（中文），前端不再自行拼接服务名。
use super::client::GreptimeDBClient;
use super::metrics::{
    CONTAINER_BY, ServiceMetricTarget, container_changes_or, container_or, container_rate_or,
};
use crate::error::Result;
use serde::Serialize;
use std::collections::BTreeMap;

/// 组件分组（前端按此分组展示，顺序即展示顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentGroup {
    App,
    Middleware,
    Observability,
    Other,
}

/// 组件状态；排序时异常在前（red → yellow → unknown → green）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentStatus {
    Red,
    Yellow,
    Unknown,
    Green,
}

impl ComponentStatus {
    const fn rank(self) -> u8 {
        match self {
            Self::Red => 0,
            Self::Yellow => 1,
            Self::Unknown => 2,
            Self::Green => 3,
        }
    }

    const fn worse(self, other: Self) -> Self {
        if other.rank() < self.rank() { other } else { self }
    }
}

/// 状态原因码（前端 i18n：`ops.components.reason.<code>`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentReason {
    HealthCheckFailed,
    NotReady,
    ConfigCenterDisconnected,
    ProbeDown,
    ProbeAbsent,
    UnderReplicated,
    ContainerMissing,
    RecentRestart,
    ContainerMetricsAbsent,
}

/// 中间件探针。
#[derive(Debug, Clone, Copy)]
pub enum Probe {
    /// `max(<metric>) >= 1` 为可达。
    Up(&'static str),
    /// 指标存在即可达（组件自身暴露的指标被成功抓取）。
    Present(&'static str),
    /// Redpanda：分区指标存在；under-replicated > 0 为黄。
    Redpanda,
    /// 本查询即走 GreptimeDB：查询成功即可达。
    SelfQuery,
    /// 无组件级探针，只看容器。
    ContainerOnly,
}

/// 中间件 / 观测组件目录：`(容器 compose service 名, 中文显示名, 分组, 探针)`。
pub const MIDDLEWARE_CATALOG: &[(&str, &str, ComponentGroup, Probe)] = &[
    ("postgresql", "PostgreSQL 数据库", ComponentGroup::Middleware, Probe::Up(PG_UP)),
    ("redis", "Redis 缓存", ComponentGroup::Middleware, Probe::Up(REDIS_UP)),
    (
        "clickhouse",
        "ClickHouse 日志库",
        ComponentGroup::Middleware,
        Probe::Present(CLICKHOUSE_DISK),
    ),
    ("greptimedb", "GreptimeDB 指标库", ComponentGroup::Middleware, Probe::SelfQuery),
    ("redpanda", "Redpanda 消息队列", ComponentGroup::Middleware, Probe::Redpanda),
    ("vector", "Vector 采集管道", ComponentGroup::Observability, Probe::ContainerOnly),
    ("cadvisor", "cAdvisor 容器采集", ComponentGroup::Observability, Probe::ContainerOnly),
    (
        "postgres-exporter",
        "PostgreSQL 指标导出",
        ComponentGroup::Observability,
        Probe::Present(PG_UP),
    ),
    (
        "redis-exporter",
        "Redis 指标导出",
        ComponentGroup::Observability,
        Probe::Present(REDIS_UP),
    ),
];

/// 近期重启判定窗（秒）。
pub const RESTART_LOOKBACK_SECS: u64 = 3_600;
/// 容器 CPU rate 窗（秒，4 × 抓取间隔）。
const CONTAINER_RATE_SECS: u64 = 60;

#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
    /// 规范 key（应用 = `service` 标签；中间件 = compose service 名）。
    pub key: String,
    /// 中文显示名（唯一来源）。
    pub name: String,
    pub group: ComponentGroup,
    pub status: ComponentStatus,
    pub reasons: Vec<ComponentReason>,
    /// 容器是否在运行；cAdvisor 无数据时为 `null`。
    pub container_running: Option<bool>,
    /// 近 [`RESTART_LOOKBACK_SECS`] 秒容器重启次数。
    pub restarts_1h: Option<f64>,
    /// 容器 CPU（核）。
    pub cpu_cores: Option<f64>,
    /// 容器内存 working set（字节）。
    pub memory_bytes: Option<f64>,
    /// 应用服务版本（`mazbri_build_info`）。
    pub version: Option<String>,
}

/// 应用层探针读数（每个服务）。
#[derive(Debug, Clone, Copy, Default)]
pub struct AppProbe {
    pub health_ok: bool,
    pub ready: Option<bool>,
    pub config_center: Option<bool>,
}

/// 容器层读数（key → 值）。
#[derive(Debug, Default)]
pub struct ContainerReadings {
    /// cAdvisor 是否有任何容器序列。
    pub available: bool,
    pub running: BTreeMap<String, f64>,
    pub restarts: BTreeMap<String, f64>,
    pub cpu: BTreeMap<String, f64>,
    pub memory: BTreeMap<String, f64>,
}

/// 中间件探针读数（指标名 → 值）。
pub type ProbeReadings = BTreeMap<&'static str, f64>;

fn app_status(probe: AppProbe) -> (ComponentStatus, Vec<ComponentReason>) {
    let mut reasons = Vec::new();
    let mut status = ComponentStatus::Green;
    if !probe.health_ok {
        reasons.push(ComponentReason::HealthCheckFailed);
        status = ComponentStatus::Red;
    }
    if probe.ready == Some(false) {
        reasons.push(ComponentReason::NotReady);
        status = ComponentStatus::Red;
    }
    if probe.config_center == Some(false) {
        reasons.push(ComponentReason::ConfigCenterDisconnected);
        status = status.worse(ComponentStatus::Yellow);
    }
    (status, reasons)
}

fn middleware_status(
    probe: Probe,
    readings: &ProbeReadings,
) -> Option<(ComponentStatus, Vec<ComponentReason>)> {
    let absent = || (ComponentStatus::Unknown, vec![ComponentReason::ProbeAbsent]);
    Some(match probe {
        Probe::Up(metric) => match readings.get(metric) {
            None => absent(),
            Some(v) if *v >= 1.0 => (ComponentStatus::Green, Vec::new()),
            Some(_) => (ComponentStatus::Red, vec![ComponentReason::ProbeDown]),
        },
        Probe::Present(metric) => {
            if readings.contains_key(metric) {
                (ComponentStatus::Green, Vec::new())
            } else {
                absent()
            }
        }
        Probe::Redpanda => match (
            readings.get(REDPANDA_PARTITIONS),
            readings.get(REDPANDA_UNDER_REPLICATED),
        ) {
            (None, _) => absent(),
            (Some(_), Some(u)) if *u > 0.0 => {
                (ComponentStatus::Yellow, vec![ComponentReason::UnderReplicated])
            }
            _ => (ComponentStatus::Green, Vec::new()),
        },
        Probe::SelfQuery => (ComponentStatus::Green, Vec::new()),
        Probe::ContainerOnly => return None,
    })
}

const PG_UP: &str = "pg_up";
const REDIS_UP: &str = "redis_up";
const CLICKHOUSE_DISK: &str = "ClickHouseAsyncMetrics_FilesystemLogsPathTotalBytes";
const REDPANDA_PARTITIONS: &str = "redpanda_cluster_partitions";
const REDPANDA_UNDER_REPLICATED: &str = "redpanda_kafka_under_replicated_replicas";

/// 叠加容器层：
/// - 容器存在：正常叠加近期重启判定；
/// - 容器不存在但组件自身探针已就绪（Green/Yellow，如外部独立部署的中间件）：保留组件探针状态，`container_running = None`；
/// - 容器不存在且无探针（`ContainerOnly`）或探针异常/缺失（Red/Unknown）：判为红（`ContainerMissing`）。
fn apply_container(
    key: &str,
    base: Option<(ComponentStatus, Vec<ComponentReason>)>,
    containers: &ContainerReadings,
) -> (ComponentStatus, Vec<ComponentReason>, Option<bool>, Option<f64>) {
    if !containers.available {
        let (status, reasons) = base.unwrap_or_else(|| {
            (ComponentStatus::Unknown, vec![ComponentReason::ContainerMetricsAbsent])
        });
        return (status, reasons, None, None);
    }
    let running = containers.running.contains_key(key);
    if !running {
        if let Some((status @ (ComponentStatus::Green | ComponentStatus::Yellow), reasons)) = base {
            return (status, reasons, None, None);
        }
        let (_, mut reasons) = base.unwrap_or((ComponentStatus::Red, Vec::new()));
        reasons.push(ComponentReason::ContainerMissing);
        return (ComponentStatus::Red, reasons, Some(false), None);
    }
    let (mut status, mut reasons) = base.unwrap_or((ComponentStatus::Green, Vec::new()));
    let restarts = containers.restarts.get(key).copied();
    if restarts.is_some_and(|r| r > 0.0) {
        status = status.worse(ComponentStatus::Yellow);
        reasons.push(ComponentReason::RecentRestart);
    }
    (status, reasons, Some(true), restarts)
}

/// 汇总组件列表并排序：异常在前，其次按分组、目录顺序。
#[must_use]
pub fn assemble_components(
    targets: &[ServiceMetricTarget],
    apps: &BTreeMap<String, AppProbe>,
    versions: &BTreeMap<String, String>,
    probes: &ProbeReadings,
    containers: &ContainerReadings,
) -> Vec<ComponentHealth> {
    let mut out: Vec<(usize, ComponentHealth)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (order, target) in targets.iter().enumerate() {
        let probe = apps.get(&target.key).copied().unwrap_or_default();
        let (status, reasons, running, restarts) =
            apply_container(&target.key, Some(app_status(probe)), containers);
        seen.push(target.key.clone());
        out.push((
            order,
            ComponentHealth {
                key: target.key.clone(),
                name: target.name.clone(),
                group: ComponentGroup::App,
                status,
                reasons,
                container_running: running,
                restarts_1h: restarts,
                cpu_cores: containers.cpu.get(&target.key).copied(),
                memory_bytes: containers.memory.get(&target.key).copied(),
                version: versions.get(&target.key).cloned(),
            },
        ));
    }
    let base = targets.len();
    for (i, (key, name, group, probe)) in MIDDLEWARE_CATALOG.iter().enumerate() {
        let (status, reasons, running, restarts) =
            apply_container(key, middleware_status(*probe, probes), containers);
        seen.push((*key).to_owned());
        out.push((
            base + i,
            ComponentHealth {
                key: (*key).to_owned(),
                name: (*name).to_owned(),
                group: *group,
                status,
                reasons,
                container_running: running,
                restarts_1h: restarts,
                cpu_cores: containers.cpu.get(*key).copied(),
                memory_bytes: containers.memory.get(*key).copied(),
                version: None,
            },
        ));
    }
    let base = base + MIDDLEWARE_CATALOG.len();
    for (i, key) in containers
        .running
        .keys()
        .filter(|k| !seen.iter().any(|s| s == *k))
        .enumerate()
    {
        let (status, reasons, running, restarts) = apply_container(key, None, containers);
        out.push((
            base + i,
            ComponentHealth {
                key: key.clone(),
                name: key.clone(),
                group: ComponentGroup::Other,
                status,
                reasons,
                container_running: running,
                restarts_1h: restarts,
                cpu_cores: containers.cpu.get(key).copied(),
                memory_bytes: containers.memory.get(key).copied(),
                version: None,
            },
        ));
    }
    out.sort_by_key(|(order, c)| (c.status.rank(), c.group, *order));
    out.into_iter().map(|(_, c)| c).collect()
}

/// cAdvisor 序列 → 组件 key（compose service 优先，其次 k8s container 名）。
fn container_key(labels: &BTreeMap<String, String>) -> Option<String> {
    ["container_label_com_docker_compose_service", "container_label_io_kubernetes_container_name"]
        .iter()
        .find_map(|l| labels.get(*l).filter(|v| !v.is_empty()).cloned())
}

fn by_container(rows: Vec<(BTreeMap<String, String>, f64)>) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    for (labels, v) in rows {
        if let Some(key) = container_key(&labels) {
            *out.entry(key).or_insert(0.0) += v;
        }
    }
    out
}

impl GreptimeDBClient {
    /// 拉取中间件探针读数（[`MIDDLEWARE_CATALOG`] 引用的全部指标）。
    pub(super) async fn load_probe_readings(&self) -> Result<ProbeReadings> {
        let one = |m: &'static str| async move {
            let rows = self.query_instant_vector(&format!("max({m})")).await?;
            Ok::<_, crate::error::AnalyticsError>(rows.into_iter().next().map(|(_, v)| (m, v)))
        };
        let (pg, redis, ch, parts, under) = tokio::try_join!(
            one(PG_UP),
            one(REDIS_UP),
            one(CLICKHOUSE_DISK),
            one(REDPANDA_PARTITIONS),
            one(REDPANDA_UNDER_REPLICATED),
        )?;
        Ok([pg, redis, ch, parts, under].into_iter().flatten().collect())
    }

    /// 拉取 cAdvisor 容器读数。
    pub(super) async fn load_container_readings(&self) -> Result<ContainerReadings> {
        let sum = |inner: String| format!("sum by ({CONTAINER_BY}) ({inner})");
        let running_q = sum(container_or("container_start_time_seconds", ""));
        let restarts_q = sum(container_changes_or(
            "container_start_time_seconds",
            RESTART_LOOKBACK_SECS,
            "",
        ));
        let cpu_q = sum(container_rate_or(
            "container_cpu_usage_seconds_total",
            CONTAINER_RATE_SECS,
            "",
        ));
        let mem_q = sum(container_or("container_memory_working_set_bytes", ""));
        let (running, restarts, cpu, memory) = tokio::try_join!(
            self.query_instant_vector(&running_q),
            self.query_instant_vector(&restarts_q),
            self.query_instant_vector(&cpu_q),
            self.query_instant_vector(&mem_q),
        )?;
        let running = by_container(running);
        Ok(ContainerReadings {
            available: !running.is_empty(),
            running,
            restarts: by_container(restarts),
            cpu: by_container(cpu),
            memory: by_container(memory),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(key: &str, name: &str) -> ServiceMetricTarget {
        ServiceMetricTarget {
            key: key.into(),
            name: name.into(),
            port: 9000,
            endpoint: format!("http://{key}:9000/metrics"),
        }
    }

    fn containers(keys: &[&str]) -> ContainerReadings {
        ContainerReadings {
            available: true,
            running: keys.iter().map(|k| ((*k).to_owned(), 1.0)).collect(),
            ..ContainerReadings::default()
        }
    }

    #[test]
    fn app_status_reasons() {
        let (s, r) = app_status(AppProbe { health_ok: false, ready: Some(true), config_center: None });
        assert_eq!(s, ComponentStatus::Red);
        assert_eq!(r, vec![ComponentReason::HealthCheckFailed]);
        let (s, r) = app_status(AppProbe { health_ok: true, ready: None, config_center: Some(false) });
        assert_eq!(s, ComponentStatus::Yellow);
        assert_eq!(r, vec![ComponentReason::ConfigCenterDisconnected]);
        let (s, _) = app_status(AppProbe { health_ok: true, ready: Some(true), config_center: Some(true) });
        assert_eq!(s, ComponentStatus::Green);
    }

    #[test]
    fn assemble_groups_sorts_abnormal_first_without_duplicates() {
        let targets = vec![target("mazbri-auth", "认证服务"), target("mazbri-gateway", "网关")];
        let apps = BTreeMap::from([
            ("mazbri-auth".to_owned(), AppProbe { health_ok: true, ready: Some(true), config_center: Some(true) }),
            ("mazbri-gateway".to_owned(), AppProbe { health_ok: true, ready: Some(true), config_center: None }),
        ]);
        let probes = ProbeReadings::from([("pg_up", 1.0), ("redis_up", 0.0)]);
        let mut c = containers(&["mazbri-auth", "mazbri-gateway", "postgresql", "redis", "vector", "mystery"]);
        c.restarts.insert("mazbri-gateway".into(), 1.0);
        let list = assemble_components(&targets, &apps, &BTreeMap::new(), &probes, &c);
        // 唯一 key
        let mut keys: Vec<_> = list.iter().map(|x| x.key.clone()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), list.len());
        // 第一个是红（redis 探针 down 或缺容器），绿在最后
        assert_eq!(list[0].status, ComponentStatus::Red);
        assert_eq!(list.last().map(|x| x.status), Some(ComponentStatus::Green));
        let redis = list.iter().find(|x| x.key == "redis").expect("redis");
        assert_eq!(redis.reasons, vec![ComponentReason::ProbeDown]);
        let gw = list.iter().find(|x| x.key == "mazbri-gateway").expect("gw");
        assert_eq!(gw.status, ComponentStatus::Yellow);
        assert_eq!(gw.reasons, vec![ComponentReason::RecentRestart]);
        let ch = list.iter().find(|x| x.key == "clickhouse").expect("ch");
        assert_eq!(ch.status, ComponentStatus::Red, "探针缺失 + 容器缺失");
        let mystery = list.iter().find(|x| x.key == "mystery").expect("other");
        assert_eq!(mystery.group, ComponentGroup::Other);
        assert_eq!(mystery.status, ComponentStatus::Green);
        // 同状态内应用服务排在中间件之前
        let greens: Vec<_> = list.iter().filter(|x| x.status == ComponentStatus::Green).collect();
        assert_eq!(greens[0].group, ComponentGroup::App);
    }

    #[test]
    fn no_cadvisor_marks_container_only_unknown() {
        let list = assemble_components(&[], &BTreeMap::new(), &BTreeMap::new(), &ProbeReadings::new(), &ContainerReadings::default());
        let vector = list.iter().find(|x| x.key == "vector").expect("vector");
        assert_eq!(vector.status, ComponentStatus::Unknown);
        assert_eq!(vector.container_running, None);
        let gt = list.iter().find(|x| x.key == "greptimedb").expect("gt");
        assert_eq!(gt.status, ComponentStatus::Green);
    }

    #[test]
    fn container_key_prefers_compose_service() {
        let labels = BTreeMap::from([
            ("container_label_com_docker_compose_service".to_owned(), "redis".to_owned()),
            ("container_label_io_kubernetes_container_name".to_owned(), "x".to_owned()),
        ]);
        assert_eq!(container_key(&labels).as_deref(), Some("redis"));
    }
}
