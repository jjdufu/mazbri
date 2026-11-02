<template>
  <OpsPage :title="t('menus.mazbri.infraMiddleware')">
    <template #actions>
      <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
    </template>
    <p class="ops-infra__note">{{ t("ops.infra.middlewareNote") }}</p>
    <ElRow :gutter="16">
      <ElCol v-for="chart in chartDefs" :key="chart.metric" :xs="24" :lg="12">
        <OpsCard :title="t(chart.title)">
          <MultiSeriesChart
            :series="series[chart.metric]"
            :loading="loading"
            height="220px"
          />
        </OpsCard>
      </ElCol>
    </ElRow>
  </OpsPage>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import MultiSeriesChart from "@/components/ops/MultiSeriesChart.vue";
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import { useStep } from "@/composables/ops/useStep";
import { fetchMetricTimeseries } from "@/api/monitoring";
import type { MetricSeries } from "@/types/api/monitoring";

defineOptions({ name: "OpsInfraMiddleware" });

const CHART_METRICS = [
  "pg_up", "pg_connections", "pg_max_connections", "pg_tps", "pg_cache_hit_ratio", "pg_deadlocks", "pg_slow_query_seconds", "pg_database_size_mb", "redis_up", "redis_memory_mb", "redis_hit_ratio", "redis_commands", "redis_connected_clients", "redis_evicted_keys", "redis_keyspace_keys", "clickhouse_disk_usage_ratio", "redpanda_partitions", "redpanda_under_replicated", "redpanda_disk_free_bytes", "greptime_memory_usage_mb", "greptime_write_rows", "greptime_promql_query_p99_ms",
] as const;
type ChartMetric = (typeof CHART_METRICS)[number];

const chartDefs = [
  { metric: "pg_up" as const, title: "ops.infra.charts.pgUp" },
  { metric: "pg_connections" as const, title: "ops.infra.charts.pgConn" },
  { metric: "pg_max_connections" as const, title: "ops.infra.charts.pgMaxConn" },
  { metric: "pg_tps" as const, title: "ops.infra.charts.pgTps" },
  { metric: "pg_cache_hit_ratio" as const, title: "ops.infra.charts.pgCache" },
  { metric: "pg_deadlocks" as const, title: "ops.infra.charts.pgDeadlocks" },
  { metric: "pg_slow_query_seconds" as const, title: "ops.infra.charts.pgSlow" },
  { metric: "pg_database_size_mb" as const, title: "ops.infra.charts.pgSize" },
  { metric: "redis_up" as const, title: "ops.infra.charts.redisUp" },
  { metric: "redis_memory_mb" as const, title: "ops.infra.charts.redisMem" },
  { metric: "redis_hit_ratio" as const, title: "ops.infra.charts.redisHit" },
  { metric: "redis_commands" as const, title: "ops.infra.charts.redisCmds" },
  { metric: "redis_connected_clients" as const, title: "ops.infra.charts.redisConn" },
  { metric: "redis_evicted_keys" as const, title: "ops.infra.charts.redisEvict" },
  { metric: "redis_keyspace_keys" as const, title: "ops.infra.charts.redisKeys" },
  { metric: "clickhouse_disk_usage_ratio" as const, title: "ops.infra.charts.chDisk" },
  { metric: "redpanda_partitions" as const, title: "ops.infra.charts.rpParts" },
  { metric: "redpanda_under_replicated" as const, title: "ops.infra.charts.rpUnder" },
  { metric: "redpanda_disk_free_bytes" as const, title: "ops.infra.charts.rpDisk" },
  { metric: "greptime_memory_usage_mb" as const, title: "ops.infra.charts.gtMem" },
  { metric: "greptime_write_rows" as const, title: "ops.infra.charts.gtWrite" },
  { metric: "greptime_promql_query_p99_ms" as const, title: "ops.infra.charts.gtQuery" },
];

const { t } = useI18n();
const { resolveStep } = useStep();

const timeRange = ref<TimeRangeValue>({
  preset: "1h",
  start: Math.floor(Date.now() / 1000) - 3600,
  end: Math.floor(Date.now() / 1000),
});
const refreshInterval = ref(60);
const loading = ref(false);
const series = ref<Partial<Record<ChartMetric, MetricSeries>>>({});

async function reload() {
  loading.value = true;
  try {
    const { start, end } = timeRange.value;
    const step = resolveStep(end - start);
    const next: Partial<Record<ChartMetric, MetricSeries>> = {};
    await Promise.all(
      CHART_METRICS.map(async (metric) => {
        const res = await fetchMetricTimeseries({ metric, start, end, step });
      next[metric] = res;
      }),
    );
    series.value = next;
  } catch (e) {
    ElMessage.error((e as Error).message || String(e));
  } finally {
    loading.value = false;
  }
}

onMounted(reload);
</script>

<style scoped>
.ops-infra__note {
  margin: 0 0 12px;
  color: var(--el-text-color-secondary);
  font-size: 13px;
}
</style>

