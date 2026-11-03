<template>
  <div class="ops-flow-cache-panel">
    <ElRow :gutter="16">
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
          :loading="olapLoading || loading"
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

    <OpsMetricChartGrid
      :charts="charts"
      :series="series"
      :loading="loading"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import OpsMetricChartGrid from "@/components/ops/OpsMetricChartGrid.vue";
import { aggregateLastMetricValue } from "@/utils/ops/metric-series";
import StatCard from "@/components/ops/StatCard.vue";
import { useMetricTimeseriesPanel } from "@/composables/ops/useMetricTimeseriesPanel";
import { toOlapWindow } from "@/composables/ops/useOlapRange";
import { useOpsSharedRange } from "@/composables/ops/useOpsSharedRange";
import { postOlapQuery } from "@/api/olap";
import {
  getFlowCacheLoadCharts,
  getFlowCharts,
} from "@/utils/ops/flow-charts";
import type { ResponseCacheHitRow } from "@/types/api/olap";

defineOptions({ name: "OpsFlowCachePanel" });

const { t } = useI18n();
const { timeRange, reloadTick } = useOpsSharedRange();
const charts = computed(() => getFlowCharts("cache"));
const loadCharts = computed(() => getFlowCacheLoadCharts());
const { loading, series } = useMetricTimeseriesPanel({
  charts: loadCharts,
  timeRange,
  reloadTick,
});

const olapLoading = ref(false);
const responseHit = ref<number | null>(null);

function latest(metric: string): number | null {
  return aggregateLastMetricValue(series.value[metric], "avg");
}

const cacheHitLatest = computed(() => latest("cache_hit_ratio"));
const entriesLatest = computed(() => latest("cache_entries"));
const memoryLatest = computed(() => latest("cache_memory_mb"));

async function reloadOlap() {
  olapLoading.value = true;
  try {
    const window = toOlapWindow(timeRange.value);
    const resp = await postOlapQuery<ResponseCacheHitRow>({
      template: "response_cache_hit",
      params: { ...window },
    });
    responseHit.value = resp.rows?.[0]?.hit_ratio ?? null;
  } catch {
    ElMessage.error(t("ops.flow.loadFailed"));
  } finally {
    olapLoading.value = false;
  }
}

onMounted(() => {
  void reloadOlap();
});

watch(
  () =>
    [
      timeRange.value.start,
      timeRange.value.end,
      timeRange.value.preset,
      reloadTick.value,
    ] as const,
  () => {
    void reloadOlap();
  },
);
</script>
