<template>
  <div class="ops-traffic art-full-height">
    <div class="ops-traffic__toolbar">
      <h2>{{ t("menus.mazbri.flowCache") }}</h2>
      <div class="ops-traffic__actions">
        <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
      </div>
    </div>

    <ElRow :gutter="12">
      <ElCol :xs="12" :lg="6">
        <StatCard
          :label="t('ops.flow.cacheHitRatio')"
          :value="cacheHitLatest"
          unit="%"
          :loading="loading"
          :digits="1"
        />
      </ElCol>
      <ElCol :xs="12" :lg="6">
        <StatCard
          :label="t('ops.flow.responseCacheHit')"
          :value="responseHit"
          unit="%"
          :loading="loading"
          :digits="1"
        />
      </ElCol>
      <ElCol :xs="12" :lg="6">
        <StatCard
          :label="t('ops.flow.cacheEntries')"
          :value="entriesLatest"
          unit=""
          :loading="loading"
          :digits="0"
        />
      </ElCol>
      <ElCol :xs="12" :lg="6">
        <StatCard
          :label="t('ops.flow.cacheMemory')"
          :value="memoryLatest"
          unit="MB"
          :loading="loading"
          :digits="1"
        />
      </ElCol>
    </ElRow>

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
import StatCard from "@/components/ops/StatCard.vue";
import { toOlapRange } from "@/composables/ops/useOlapRange";
import { useStep } from "@/composables/ops/useStep";
import { fetchMetricTimeseries } from "@/api/monitoring";
import { postOlapQuery } from "@/api/olap";
import type { MetricSeries } from "@/types/api/monitoring";
import type { ResponseCacheHitRow } from "@/types/api/olap";

defineOptions({ name: "OpsFlowCache" });

const CHART_METRICS = [
  "cache_hit_ratio",
  "cache_operations",
  "cache_entries",
  "cache_memory_mb",
  "cache_operation_p95_ms",
  "gateway_crypto_cache",
  "bypass_config_cache",
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
const responseHit = ref<number | null>(null);
const series = ref<Record<ChartMetric, MetricSeries | null>>(
  Object.fromEntries(CHART_METRICS.map((m) => [m, null])) as Record<
    ChartMetric,
    MetricSeries | null
  >,
);

function latest(metric: ChartMetric): number | null {
  const values = (series.value[metric]?.series ?? [])
    .map((s) => s.points[s.points.length - 1]?.value)
    .filter((v): v is number => typeof v === "number" && Number.isFinite(v));
  if (values.length === 0) return null;
  return values.reduce((a, b) => a + b, 0) / values.length;
}

const cacheHitLatest = computed(() => latest("cache_hit_ratio"));
const entriesLatest = computed(() => latest("cache_entries"));
const memoryLatest = computed(() => latest("cache_memory_mb"));

const trendCharts = computed(() => [
  {
    metric: "hit",
    title: "ops.flow.cacheHitRatio",
    data: series.value.cache_hit_ratio,
  },
  {
    metric: "ops",
    title: "ops.flow.cacheOperations",
    data: series.value.cache_operations,
  },
  {
    metric: "p95",
    title: "ops.flow.cacheP95",
    data: series.value.cache_operation_p95_ms,
  },
  {
    metric: "crypto",
    title: "ops.flow.cryptoCache",
    data: series.value.gateway_crypto_cache,
  },
  {
    metric: "bypass",
    title: "ops.flow.bypassCache",
    data: series.value.bypass_config_cache,
  },
  {
    metric: "mem",
    title: "ops.flow.cacheMemory",
    data: series.value.cache_memory_mb,
  },
]);

async function reload() {
  loading.value = true;
  const start = timeRange.value.start;
  const end = timeRange.value.end;
  const step = resolveStep(end - start);
  const range = toOlapRange(timeRange.value);
  try {
    const [resp, ...charts] = await Promise.all([
      postOlapQuery<ResponseCacheHitRow>({
        template: "response_cache_hit",
        params: { range },
      }),
      ...CHART_METRICS.map((metric) =>
        fetchMetricTimeseries({ metric, start, end, step }),
      ),
    ]);
    responseHit.value = resp.rows?.[0]?.hit_ratio ?? null;
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
