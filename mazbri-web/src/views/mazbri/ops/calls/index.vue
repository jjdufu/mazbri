<template>
  <OpsPage :title="t('menus.mazbri.opsCalls')">
    <template #actions>
      <TimeRangePicker
        :model-value="timeRange ?? undefined"
        @change="onRangeChange"
      />
      <AutoRefresh v-model="refreshInterval" @tick="onRefreshTick" />
    </template>

    <ElTabs v-model="activeTab" class="ops-calls__tabs" @tab-change="onTabChange">
      <ElTabPane :label="t('ops.calls.tabs.customer')" name="customer" />
      <ElTabPane :label="t('ops.calls.tabs.upstream')" name="upstream" />
    </ElTabs>

    <OpsCard
      :title="t('ops.calls.histogram.title')"
      :info="t('ops.calls.histogram.brushHint')"
    >
      <template #extra>
        <div class="ops-calls__histogram-extra">
          <ElButton
            v-if="previousRange"
            link
            type="primary"
            size="small"
            @click="restorePreviousRange"
          >
            {{ t("ops.calls.histogram.resetZoom") }}
          </ElButton>
          <span class="ops-calls__summary">
            {{
              t("ops.calls.histogram.summary", {
                total: histogram?.total ?? 0,
                errors: histogram?.errors ?? 0,
                bucket: histogram?.bucket_seconds ?? 0,
              })
            }}
          </span>
        </div>
      </template>
      <div v-loading="histogramLoading" class="ops-calls__chart-wrap">
        <EmptyState
          v-if="!histogramLoading && (!histogram || histogram.total === 0)"
          :description="t('operations.monitoring.empty.noData')"
        />
        <div
          v-show="histogram && histogram.total > 0"
          ref="chartRef"
          class="ops-calls__chart"
        ></div>
      </div>
    </OpsCard>
    <div class="ops-calls__table">
      <CallMonitorTable
        v-if="timeRange"
        ref="tableRef"
        :key="activeTab"
        :mode="activeTab"
        :title="
          activeTab === 'upstream'
            ? $t('operations.monitoring.upstreamTitle')
            : $t('operations.monitoring.customerTitle')
        "
        :time-range="tableRange"
        :seed-filters="seedFilters"
        @query-change="onQueryChange"
        @open-trace="openTrace"
      />
    </div>
  </OpsPage>
</template>

<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import { echarts, type EChartsOption } from "@/plugins/echarts";
import EmptyState from "@/components/common/empty-state/index.vue";
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import {
  OPS_CHART_GRID,
  bindOpsChartResize,
} from "@/composables/ops/useOpsChartHost";
import { fetchCallHistogram } from "@/api/call-monitor";
import type {
  CallHistogramQuery,
  CallHistogramResponse,
} from "@/types/api/call-monitor";
import type { ListFilter } from "@/types/list-query";
import { $t } from "@/locales";
import { CALLS_FILTER_KEYS, type CallsTab } from "@/utils/ops/calls-link";
import { buildTraceRoute } from "@/utils/ops/trace-link";
import { logger } from "@/utils/sys";
import CallMonitorTable, {
  type CallTimeRange,
} from "./modules/CallMonitorTable.vue";

defineOptions({ name: "OpsCalls" });

const { t } = useI18n();
const route = useRoute();
const router = useRouter();

const TABS: CallsTab[] = ["customer", "upstream"];
const NUMERIC_KEYS = new Set(["status_code", "duration_ms_gte"]);

function queryString(key: string): string {
  const raw = route.query[key];
  const value = Array.isArray(raw) ? raw[0] : raw;
  return typeof value === "string" ? value.trim() : "";
}

function initialTab(): CallsTab {
  const raw = queryString("tab");
  return (TABS as string[]).includes(raw) ? (raw as CallsTab) : "customer";
}

function filtersFromRoute(): ListFilter[] {
  const out: ListFilter[] = [];
  for (const key of CALLS_FILTER_KEYS) {
    const value = queryString(key);
    if (!value) continue;
    if (NUMERIC_KEYS.has(key)) {
      const n = Number(value);
      if (!Number.isFinite(n)) continue;
      if (key === "duration_ms_gte") {
        out.push({ field: "duration_ms", op: "gte", value: n });
      } else {
        out.push({ field: key, op: "eq", value: n });
      }
      continue;
    }
    out.push({ field: key, op: "eq", value });
  }
  return out;
}

const activeTab = ref<CallsTab>(initialTab());
const seedFilters = filtersFromRoute();
const timeRange = ref<TimeRangeValue | null>(null);
const previousRange = ref<TimeRangeValue | null>(null);
const refreshInterval = ref(0);
const tableRef = ref<InstanceType<typeof CallMonitorTable> | null>(null);

const tableRange = computed<CallTimeRange | null>(() =>
  timeRange.value
    ? { start: timeRange.value.start, end: timeRange.value.end }
    : null,
);

function onRangeChange(value: TimeRangeValue) {
  previousRange.value = null;
  timeRange.value = value;
}

function restorePreviousRange() {
  if (!previousRange.value) return;
  const prev = previousRange.value;
  previousRange.value = null;
  timeRange.value = prev;
  void router.replace({
    query: {
      ...route.query,
      tr_preset: prev.preset,
      tr_start: String(prev.start),
      tr_end: String(prev.end),
    },
  });
}

function onRefreshTick() {
  tableRef.value?.refresh();
}

function onTabChange(name: string | number) {
  void router.replace({ query: { ...route.query, tab: String(name) } });
}

/* ---------- histogram ---------- */

const chartRef = ref<HTMLElement | null>(null);
const histogram = ref<CallHistogramResponse | null>(null);
const histogramLoading = ref(false);
let chart: echarts.ECharts | null = null;
let lastQuery: CallHistogramQuery | null = null;
let histogramSeq = 0;

function bucketLabel(ts: number, bucketSeconds: number): string {
  const d = new Date(ts * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  const hm = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  if (bucketSeconds >= 86400) return `${d.getMonth() + 1}-${pad(d.getDate())}`;
  if (bucketSeconds >= 3600) return `${d.getMonth() + 1}-${pad(d.getDate())} ${hm}`;
  return bucketSeconds < 60 ? `${hm}:${pad(d.getSeconds())}` : hm;
}

function buildOption(data: CallHistogramResponse): EChartsOption {
  const labels = data.buckets.map((b) => bucketLabel(b.ts, data.bucket_seconds));
  return {
    tooltip: { trigger: "axis", axisPointer: { type: "shadow" } },
    legend: { data: [t("ops.calls.histogram.calls"), t("ops.calls.histogram.errors")] },
    grid: { ...OPS_CHART_GRID },
    toolbox: { show: false },
    brush: {
      toolbox: ["lineX"],
      xAxisIndex: 0,
      brushMode: "single",
      throttleType: "debounce",
    },
    xAxis: { type: "category", data: labels },
    yAxis: { type: "value", minInterval: 1 },
    series: [
      {
        name: t("ops.calls.histogram.calls"),
        type: "bar",
        stack: "calls",
        data: data.buckets.map((b) => b.calls - b.errors),
        itemStyle: { color: "#409eff" },
      },
      {
        name: t("ops.calls.histogram.errors"),
        type: "bar",
        stack: "calls",
        data: data.buckets.map((b) => b.errors),
        itemStyle: { color: "#f56c6c" },
      },
    ],
  };
}

interface BrushArea {
  coordRange?: [number, number];
}

function onBrushEnd(params: unknown) {
  const data = histogram.value;
  if (!data || data.buckets.length === 0) return;
  const areas = (params as { areas?: BrushArea[] }).areas ?? [];
  const range = areas[0]?.coordRange;
  if (!range) return;
  const lo = Math.max(0, Math.min(range[0], range[1]));
  const hi = Math.min(data.buckets.length - 1, Math.max(range[0], range[1]));
  const first = data.buckets[lo];
  const last = data.buckets[hi];
  if (!first || !last) return;
  const start = first.ts;
  const end = Math.min(last.ts + data.bucket_seconds, data.end);
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) return;
  if (timeRange.value) {
    previousRange.value = { ...timeRange.value };
  }
  const value: TimeRangeValue = { preset: "custom", start, end };
  timeRange.value = value;
  void router.replace({
    query: {
      ...route.query,
      tr_preset: "custom",
      tr_start: String(start),
      tr_end: String(end),
    },
  });
}

function renderChart() {
  if (!chartRef.value || !histogram.value || histogram.value.total === 0) {
    unbindChartResize?.();
    unbindChartResize = null;
    chart?.dispose();
    chart = null;
    return;
  }
  if (!chart) {
    chart = echarts.init(chartRef.value);
    chart.on("brushEnd", onBrushEnd);
    unbindChartResize?.();
    unbindChartResize = bindOpsChartResize(chartRef.value, () => chart);
  }
  chart.setOption(buildOption(histogram.value), true);
  chart.resize();
  chart.dispatchAction({
    type: "takeGlobalCursor",
    key: "brush",
    brushOption: { brushType: "lineX", brushMode: "single" },
  });
}

async function loadHistogram() {
  if (!lastQuery || !timeRange.value) return;
  const seq = ++histogramSeq;
  histogramLoading.value = true;
  try {
    const res = await fetchCallHistogram(activeTab.value, {
      ...lastQuery,
      start_time: timeRange.value.start,
      end_time: timeRange.value.end,
    });
    if (seq !== histogramSeq) return;
    histogram.value = res;
    await nextTick();
    renderChart();
  } catch (error) {
    logger.error("Failed to load call histogram:", error);
    if (seq === histogramSeq) {
      ElMessage.error(t("ops.calls.histogram.loadFailed"));
    }
  } finally {
    if (seq === histogramSeq) histogramLoading.value = false;
  }
}

function onQueryChange(query: CallHistogramQuery) {
  lastQuery = query;
  void loadHistogram();
}

function openTrace(traceId: string, start?: number) {
  void router.push(buildTraceRoute(traceId, { start }));
}

let unbindChartResize: (() => void) | null = null;

onUnmounted(() => {
  unbindChartResize?.();
  unbindChartResize = null;
  chart?.dispose();
  chart = null;
});
</script>

<style scoped>
.ops-calls__histogram-extra {
  display: inline-flex;
  align-items: center;
  gap: 12px;
}
.ops-calls__summary {
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.ops-calls__chart-wrap {
  width: 100%;
  min-height: 180px;
}
.ops-calls__chart {
  width: 100%;
  height: 200px;
  overflow: hidden;
}
.ops-calls__table {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
}
</style>
