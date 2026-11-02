<template>
  <OpsPage :title="t('menus.mazbri.flowRatelimit')">
    <template #actions>
      <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
    </template>

    <ElRow :gutter="16">
      <ElCol v-for="chart in trendCharts" :key="chart.metric" :xs="24" :lg="12">
        <OpsCard :title="t(chart.title)">
          <MultiSeriesChart
            :series="chart.data"
            :loading="loading"
            height="220px"
          />
        </OpsCard>
      </ElCol>
    </ElRow>

    <OpsCard :title="t('ops.flow.quotaUsageRatio')">
      <MultiSeriesChart
        :series="series.gateway_quota_usage_ratio"
        :loading="loading"
        height="220px"
      />
    </OpsCard>
  </OpsPage>
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
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
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
</style>
