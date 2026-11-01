<template>
  <div class="ops-traffic art-full-height">
    <div class="ops-traffic__toolbar">
      <h2>{{ t("menus.mazbri.flowRatelimit") }}</h2>
      <div class="ops-traffic__actions">
        <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
      </div>
    </div>

    <ElRow :gutter="12">
      <ElCol v-for="chart in trendCharts" :key="chart.metric" :xs="24" :lg="12">
        <div class="ops-traffic__card">
          <h3>{{ t(chart.title) }}</h3>
          <MultiSeriesChart
            :series="chart.data"
            :loading="loading"
            height="220px"
          />
        </div>
      </ElCol>
    </ElRow>

    <div class="ops-traffic__card">
      <h3>{{ t("ops.flow.quotaUsageRatio") }}</h3>
      <MultiSeriesChart
        :series="series.gateway_quota_usage_ratio"
        :loading="loading"
        height="220px"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
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

defineOptions({ name: "OpsFlowRatelimit" });

const CHART_METRICS = [
  "ratelimit_decisions",
  "ratelimit_channel_decisions",
  "ratelimit_snapshot_age_seconds",
  "ratelimit_snapshot_refresh",
  "ratelimit_usage_flush",
  "ratelimit_rejections",
  "gateway_quota_usage_ratio",
] as const;
type ChartMetric = (typeof CHART_METRICS)[number];

const { t } = useI18n();
const { resolveStep } = useStep();
const timeRange = ref<TimeRangeValue>({
  preset: "24h",
  start: Math.floor(Date.now() / 1000) - 86400,
  end: Math.floor(Date.now() / 1000),
});
const refreshInterval = ref(60);
const loading = ref(false);
const series = ref<Record<ChartMetric, MetricSeries | null>>(
  Object.fromEntries(CHART_METRICS.map((m) => [m, null])) as Record<
    ChartMetric,
    MetricSeries | null
  >,
);

const trendCharts = computed(() => [
  {
    metric: "decisions",
    title: "ops.flow.decisions",
    data: series.value.ratelimit_decisions,
  },
  {
    metric: "channel",
    title: "ops.flow.channelDecisions",
    data: series.value.ratelimit_channel_decisions,
  },
  {
    metric: "age",
    title: "ops.flow.snapshotAge",
    data: series.value.ratelimit_snapshot_age_seconds,
  },
  {
    metric: "refresh",
    title: "ops.flow.snapshotRefresh",
    data: series.value.ratelimit_snapshot_refresh,
  },
  {
    metric: "flush",
    title: "ops.flow.usageFlush",
    data: series.value.ratelimit_usage_flush,
  },
  {
    metric: "rejects",
    title: "ops.flow.rejections",
    data: series.value.ratelimit_rejections,
  },
]);

async function reload() {
  loading.value = true;
  const start = timeRange.value.start;
  const end = timeRange.value.end;
  const step = resolveStep(end - start);
  try {
    const charts = await Promise.all(
      CHART_METRICS.map((metric) =>
        fetchMetricTimeseries({ metric, start, end, step }),
      ),
    );
    const next = { ...series.value };
    CHART_METRICS.forEach((metric, i) => {
      next[metric] = charts[i] ?? null;
    });
    series.value = next;
  } catch {
    ElMessage.error(t("ops.flow.loadFailed"));
  } finally {
    loading.value = false;
  }
}

onMounted(reload);
</script>

<style scoped>
.ops-traffic__toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}
.ops-traffic__actions {
  display: flex;
  gap: 8px;
  align-items: center;
}
.ops-traffic__card {
  margin-top: 12px;
  padding: 12px;
  border-radius: 8px;
  background: var(--el-bg-color);
}
</style>
