<template>
  <OpsPage :title="t('menus.mazbri.upstreamBreaker')">
    <template #actions>
      <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
    </template>

    <OpsCard :title="t('ops.upstream.breakerLanes')">
      <p class="ops-breaker__hint">{{ t("ops.upstream.breakerLegend") }}</p>
      <HeatmapChart
        :cells="laneCells"
        :x-labels="laneX"
        :y-labels="laneY"
        :loading="loading"
        :height="laneHeight"
        value-name="state"
      />
    </OpsCard>

    <ElRow :gutter="16">
      <ElCol :xs="24" :lg="12">
        <OpsCard :title="t('ops.upstream.cbproState')">
          <MultiSeriesChart :series="series.cbpro_state" :loading="loading" height="220px" />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="12">
        <OpsCard :title="t('ops.upstream.attempts')">
          <MultiSeriesChart
            :series="series.upstream_attempts"
            :loading="loading"
            height="220px"
          />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="12">
        <OpsCard :title="t('ops.upstream.bypassRequests')">
          <MultiSeriesChart
            :series="series.bypass_upstream_qps"
            :loading="loading"
            height="220px"
          />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="12">
        <OpsCard :title="t('ops.upstream.waitP95')">
          <MultiSeriesChart
            :series="series.upstream_wait_p95_ms"
            :loading="loading"
            height="220px"
          />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.upstream.h2Connects')">
          <MultiSeriesChart :series="series.h2_connects" :loading="loading" height="220px" />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.upstream.h2Streams')">
          <MultiSeriesChart
            :series="series.h2_streams_in_use"
            :loading="loading"
            height="220px"
          />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.upstream.h2Errors')">
          <MultiSeriesChart :series="series.h2_errors" :loading="loading" height="220px" />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24">
        <OpsCard :title="t('ops.upstream.h2ReadyWait')">
          <MultiSeriesChart
            :series="series.h2_ready_wait_p95_ms"
            :loading="loading"
            height="220px"
          />
        </OpsCard>
      </ElCol>
    </ElRow>
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
import HeatmapChart, { type HeatCell } from "@/components/ops/HeatmapChart.vue";
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import { useStep } from "@/composables/ops/useStep";
import { fetchMetricTimeseries } from "@/api/monitoring";
import type { MetricSeries } from "@/types/api/monitoring";

defineOptions({ name: "OpsUpstreamBreaker" });

const CHART_METRICS = [
  "cbpro_state",
  "upstream_attempts",
  "bypass_upstream_qps",
  "upstream_wait_p95_ms",
  "h2_connects",
  "h2_streams_in_use",
  "h2_errors",
  "h2_ready_wait_p95_ms",
] as const;
type ChartMetric = (typeof CHART_METRICS)[number];

const { t } = useI18n();
const { resolveStep } = useStep();

const timeRange = ref<TimeRangeValue>({
  preset: "1h",
  start: Math.floor(Date.now() / 1000) - 3600,
  end: Math.floor(Date.now() / 1000),
});
const refreshInterval = ref(30);
const loading = ref(false);
const lanes = ref<MetricSeries | null>(null);
const series = ref<Record<ChartMetric, MetricSeries | null>>(
  Object.fromEntries(CHART_METRICS.map((m) => [m, null])) as Record<
    ChartMetric,
    MetricSeries | null
  >,
);

function formatTs(ts: number) {
  const d = new Date(ts * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

function laneName(labels: Record<string, string>) {
  return labels.name || "—";
}

const laneX = computed(() => {
  const all = (lanes.value?.series ?? []).flatMap((s) => s.points.map((p) => p.timestamp));
  return [...new Set(all)].sort((a, b) => a - b).map(formatTs);
});
const laneY = computed(() =>
  [...new Set((lanes.value?.series ?? []).map((s) => laneName(s.labels)))].sort(),
);
const laneCells = computed<HeatCell[]>(() =>
  (lanes.value?.series ?? []).flatMap((s) =>
    s.points.map((p) => ({
      x: formatTs(p.timestamp),
      y: laneName(s.labels),
      value: p.value,
    })),
  ),
);
const laneHeight = computed(() => `${Math.max(160, laneY.value.length * 28 + 80)}px`);

async function reload() {
  loading.value = true;
  const start = timeRange.value.start;
  const end = timeRange.value.end;
  const step = resolveStep(end - start);
  try {
    const [laneRes, ...charts] = await Promise.all([
      fetchMetricTimeseries({ metric: "breaker_state", start, end, step }),
      ...CHART_METRICS.map((metric) => fetchMetricTimeseries({ metric, start, end, step })),
    ]);
    lanes.value = laneRes;
    const next = { ...series.value };
    CHART_METRICS.forEach((metric, i) => {
      next[metric] = charts[i] ?? null;
    });
    series.value = next;
  } catch {
    ElMessage.error(t("ops.upstream.loadFailed"));
  } finally {
    loading.value = false;
  }
}

onMounted(reload);
</script>

<style scoped>

.ops-breaker__hint {
  margin: 0 0 8px;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
</style>
