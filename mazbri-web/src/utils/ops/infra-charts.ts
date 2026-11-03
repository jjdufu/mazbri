/** 基础设施合并页图表配置：Greptime write/query 只在 pipeline 出现一次。 */

export type InfraChartTab = "pipeline" | "middleware" | "hosts";

export interface InfraChartDef {
  metric: string;
  /** i18n key under ops.infra.charts.* */
  title: string;
  /** hosts 容器类指标需要显示名重映射 */
  remapContainers?: boolean;
}

const PIPELINE_CHARTS: readonly InfraChartDef[] = [
  { metric: "pipeline_freshness_seconds", title: "ops.infra.charts.pipelineFreshness" },
  { metric: "vector_events_received", title: "ops.infra.charts.vectorReceived" },
  { metric: "vector_events_sent", title: "ops.infra.charts.vectorSent" },
  { metric: "vector_component_errors", title: "ops.infra.charts.vectorErrors" },
  { metric: "vector_buffer_usage_ratio", title: "ops.infra.charts.vectorBuffer" },
  { metric: "redpanda_topic_bytes", title: "ops.infra.charts.redpandaBytes" },
  { metric: "redpanda_consumer_lag", title: "ops.infra.charts.redpandaLag" },
  { metric: "clickhouse_inserted_rows", title: "ops.infra.charts.chInsert" },
  { metric: "clickhouse_kafka_errors", title: "ops.infra.charts.chKafkaErrors" },
  { metric: "clickhouse_parts_active", title: "ops.infra.charts.chParts" },
  { metric: "clickhouse_merges", title: "ops.infra.charts.chMerges" },
  { metric: "clickhouse_query_time_ms", title: "ops.infra.charts.chQuery" },
  // Greptime 写入/查询只保留在管道 Tab，中间件不再重复。
  { metric: "greptime_write_rows", title: "ops.infra.charts.gtWrite" },
  { metric: "greptime_promql_query_p99_ms", title: "ops.infra.charts.gtQuery" },
] as const;

const MIDDLEWARE_CHARTS: readonly InfraChartDef[] = [
  { metric: "pg_up", title: "ops.infra.charts.pgUp" },
  { metric: "pg_connections", title: "ops.infra.charts.pgConn" },
  { metric: "pg_max_connections", title: "ops.infra.charts.pgMaxConn" },
  { metric: "pg_tps", title: "ops.infra.charts.pgTps" },
  { metric: "pg_cache_hit_ratio", title: "ops.infra.charts.pgCache" },
  { metric: "pg_deadlocks", title: "ops.infra.charts.pgDeadlocks" },
  { metric: "pg_slow_query_seconds", title: "ops.infra.charts.pgSlow" },
  { metric: "pg_database_size_mb", title: "ops.infra.charts.pgSize" },
  { metric: "redis_up", title: "ops.infra.charts.redisUp" },
  { metric: "redis_memory_mb", title: "ops.infra.charts.redisMem" },
  { metric: "redis_hit_ratio", title: "ops.infra.charts.redisHit" },
  { metric: "redis_commands", title: "ops.infra.charts.redisCmds" },
  { metric: "redis_connected_clients", title: "ops.infra.charts.redisConn" },
  { metric: "redis_evicted_keys", title: "ops.infra.charts.redisEvict" },
  { metric: "redis_keyspace_keys", title: "ops.infra.charts.redisKeys" },
  { metric: "clickhouse_disk_usage_ratio", title: "ops.infra.charts.chDisk" },
  { metric: "redpanda_partitions", title: "ops.infra.charts.rpParts" },
  { metric: "redpanda_under_replicated", title: "ops.infra.charts.rpUnder" },
  { metric: "redpanda_disk_free_bytes", title: "ops.infra.charts.rpDisk" },
  { metric: "greptime_memory_usage_mb", title: "ops.infra.charts.gtMem" },
] as const;

const HOSTS_CHARTS: readonly InfraChartDef[] = [
  { metric: "host_cpu_usage_ratio", title: "ops.infra.charts.hostCpu" },
  { metric: "host_load1", title: "ops.infra.charts.hostLoad" },
  { metric: "host_memory_usage_ratio", title: "ops.infra.charts.hostMem" },
  { metric: "host_filesystem_usage_ratio", title: "ops.infra.charts.hostFs" },
  { metric: "host_disk_read_bytes", title: "ops.infra.charts.hostDiskRead" },
  { metric: "host_disk_write_bytes", title: "ops.infra.charts.hostDiskWrite" },
  { metric: "host_network_rx_bytes", title: "ops.infra.charts.hostNetRx" },
  { metric: "host_network_tx_bytes", title: "ops.infra.charts.hostNetTx" },
  { metric: "container_cpu_cores", title: "ops.infra.charts.ctrCpu", remapContainers: true },
  { metric: "container_memory_mb", title: "ops.infra.charts.ctrMem", remapContainers: true },
  { metric: "container_restarts", title: "ops.infra.charts.ctrRestarts", remapContainers: true },
  { metric: "container_oom_events", title: "ops.infra.charts.ctrOom", remapContainers: true },
  { metric: "container_network_rx_bytes", title: "ops.infra.charts.ctrNetRx", remapContainers: true },
  { metric: "container_network_tx_bytes", title: "ops.infra.charts.ctrNetTx", remapContainers: true },
] as const;

const BY_TAB: Record<InfraChartTab, readonly InfraChartDef[]> = {
  pipeline: PIPELINE_CHARTS,
  middleware: MIDDLEWARE_CHARTS,
  hosts: HOSTS_CHARTS,
};

/** Greptime 写入行数 / PromQL P99：整站 infra 配置只允许出现一次。 */
export const GREPTIME_DEDUPED_METRICS = [
  "greptime_write_rows",
  "greptime_promql_query_p99_ms",
] as const;

export function getInfraCharts(tab: InfraChartTab): readonly InfraChartDef[] {
  return BY_TAB[tab];
}

export function infraChartMetrics(tab: InfraChartTab): string[] {
  return getInfraCharts(tab).map((c) => c.metric);
}

/** 返回各 Tab 中出现 GREPTIME_DEDUPED_METRICS 的次数（验收用）。 */
export function countGreptimeDedupedOccurrences(): Record<
  (typeof GREPTIME_DEDUPED_METRICS)[number],
  number
> {
  const counts = {
    greptime_write_rows: 0,
    greptime_promql_query_p99_ms: 0,
  };
  for (const tab of Object.keys(BY_TAB) as InfraChartTab[]) {
    for (const chart of BY_TAB[tab]) {
      if (chart.metric === "greptime_write_rows") {
        counts.greptime_write_rows += 1;
      }
      if (chart.metric === "greptime_promql_query_p99_ms") {
        counts.greptime_promql_query_p99_ms += 1;
      }
    }
  }
  return counts;
}
