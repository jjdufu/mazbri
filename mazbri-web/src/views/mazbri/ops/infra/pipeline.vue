<template>
  <div class="ops-infra art-full-height">
    <div class="ops-infra__toolbar">
      <h2>{{ t("menus.mazbri.infraPipeline") }}</h2>
      <div class="ops-infra__actions">
        <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
      </div>
    </div>
    <p class="ops-infra__note">{{ t("ops.infra.pipelineNote") }}</p>
    <ElRow :gutter="12">
      <ElCol v-for="chart in chartDefs" :key="chart.metric" :xs="24" :lg="12">
        <div class="ops-infra__card">
          <h3>{{ t(chart.title) }}</h3>
          <MultiSeriesChart
            :series="series[chart.metric]"
            :loading="loading"
            height="220px"
          />
        </div>
      </ElCol>
    </ElRow>
  </div>
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
import { useStep } from "@/composables/ops/useStep";
import { fetchMetricTimeseries } from "@/api/monitoring";
import type { MetricSeries } from "@/types/api/monitoring";

defineOptions({ name: "OpsInfraPipeline" });

const CHART_METRICS = [
  "pipeline_freshness_seconds", "vector_events_received", "vector_events_sent", "vector_component_errors", "vector_buffer_usage_ratio", "redpanda_topic_bytes", "redpanda_consumer_lag", "clickhouse_inserted_rows", "clickhouse_kafka_errors", "clickhouse_parts_active", "clickhouse_merges", "clickhouse_query_time_ms", "greptime_write_rows", "greptime_promql_query_p99_ms",
] as const;
type ChartMetric = (typeof CHART_METRICS)[number];

const chartDefs = [
  { metric: "pipeline_freshness_seconds" as const, title: "ops.infra.charts.pipelineFreshness" },
  { metric: "vector_events_received" as const, title: "ops.infra.charts.vectorReceived" },
  { metric: "vector_events_sent" as const, title: "ops.infra.charts.vectorSent" },
  { metric: "vector_component_errors" as const, title: "ops.infra.charts.vectorErrors" },
  { metric: "vector_buffer_usage_ratio" as const, title: "ops.infra.charts.vectorBuffer" },
  { metric: "redpanda_topic_bytes" as const, title: "ops.infra.charts.redpandaBytes" },
  { metric: "redpanda_consumer_lag" as const, title: "ops.infra.charts.redpandaLag" },
  { metric: "clickhouse_inserted_rows" as const, title: "ops.infra.charts.chInsert" },
  { metric: "clickhouse_kafka_errors" as const, title: "ops.infra.charts.chKafkaErrors" },
  { metric: "clickhouse_parts_active" as const, title: "ops.infra.charts.chParts" },
  { metric: "clickhouse_merges" as const, title: "ops.infra.charts.chMerges" },
  { metric: "clickhouse_query_time_ms" as const, title: "ops.infra.charts.chQuery" },
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
.ops-infra__toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}
.ops-infra__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}
.ops-infra__card {
  margin-bottom: 12px;
  padding: 12px 16px;
  border-radius: 8px;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
}
.ops-infra__card h3 {
  margin: 0 0 8px;
  font-size: 14px;
  font-weight: 600;
}
.ops-infra__note {
  margin: 0 0 12px;
  color: var(--el-text-color-secondary);
  font-size: 13px;
}
</style>

