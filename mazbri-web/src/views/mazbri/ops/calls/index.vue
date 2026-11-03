<template>
  <OpsPage :title="t('menus.mazbri.opsCalls')">
    <template #title>
      <div class="ops-calls__title">
        <h2>{{ t("menus.mazbri.opsCalls") }}</h2>
        <CallMonitorModeHint v-if="activeTab !== 'stages'" :hint="modeHint" />
      </div>
    </template>
    <template #actions>
      <TimeRangePicker
        v-if="activeTab !== 'stages'"
        :model-value="timeRange ?? undefined"
        @change="onRangeChange"
      />
    </template>

    <ElTabs v-model="activeTab" class="ops-calls__tabs" @tab-change="onTabChange">
      <ElTabPane :label="t('ops.calls.tabs.customer')" name="customer" />
      <ElTabPane :label="t('ops.calls.tabs.upstream')" name="upstream" />
      <ElTabPane :label="t('ops.calls.tabs.stages')" name="stages" />
    </ElTabs>

    <template v-if="activeTab !== 'stages'">
      <OpsCard>
        <div class="ops-calls__card-head">
          <h3>{{ t("ops.calls.histogram.title") }}</h3>
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
        <div ref="chartRef" v-loading="histogramLoading" class="ops-calls__chart"></div>
        <div class="ops-calls__hint">{{ t("ops.calls.histogram.brushHint") }}</div>
      </OpsCard>
      <div class="ops-calls__table">
        <CallMonitorTable
          v-if="timeRange"
          :key="activeTab"
          :mode="activeTab"
          :title="
            activeTab === 'upstream'
              ? $t('operations.monitoring.upstreamTitle')
              : $t('operations.monitoring.customerTitle')
          "
          :time-range="tableRange"
          :seed-filters="seedFilters"
          @mode-hint-change="modeHint = $event"
          @query-change="onQueryChange"
          @open-trace="openTrace"
        />
      </div>
    </template>

    <OpsCard v-else>
      <div class="ops-calls__stage-bar">
        <ElSelect v-model="stageKind" class="ops-calls__stage-kind">
          <ElOption label="request_id" value="request_id" />
          <ElOption label="trace_id" value="trace_id" />
        </ElSelect>
        <ElInput
          v-model="stageInput"
          clearable
          class="ops-calls__stage-input"
          :placeholder="t('ops.calls.stages.placeholder')"
          @keyup.enter="searchStages"
        />
        <ElButton type="primary" :loading="stageLoading" @click="searchStages">
          {{ t("common.search") }}
        </ElButton>
      </div>
      <ElTable
        v-loading="stageLoading"
        :data="stageCalls"
        stripe
        size="small"
        class="ops-calls__stage-table"
      >
        <ElTableColumn prop="timestamp" :label="t('ops.calls.stages.columns.time')" min-width="190">
          <template #default="{ row }">{{ formatTime(row.timestamp) }}</template>
        </ElTableColumn>
        <ElTableColumn prop="target" :label="t('ops.calls.stages.columns.source')" min-width="160" />
        <ElTableColumn prop="request_id" label="request_id" min-width="290" />
        <ElTableColumn prop="trace_id" label="trace_id" min-width="270">
          <template #default="{ row }">
            <ElLink v-if="row.trace_id" type="primary" @click="openTrace(row.trace_id, unixSecondsOf(row.timestamp))">
              {{ row.trace_id }}
            </ElLink>
          </template>
        </ElTableColumn>
        <ElTableColumn prop="supplier_code" :label="t('ops.calls.stages.columns.supplier')" min-width="120" />
        <ElTableColumn prop="channel_code" :label="t('ops.calls.stages.columns.channel')" min-width="140" />
        <ElTableColumn prop="status_code" :label="t('ops.calls.stages.columns.status')" width="80" />
        <ElTableColumn prop="duration_ms" :label="t('ops.calls.stages.columns.duration')" width="100" />
        <ElTableColumn prop="error_code" :label="t('ops.calls.stages.columns.errorCode')" min-width="140" />
      </ElTable>
      <h3 class="ops-calls__stage-title">{{ t("ops.calls.stages.breakdown") }}</h3>
      <StageBreakdown :stages="stageRows" />
    </OpsCard>
  </OpsPage>
</template>

<script setup lang="ts">
import { formatDateTime } from "@/utils/format";
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import { echarts, type EChartsOption } from "@/plugins/echarts";
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import {
  OPS_CHART_GRID,
  bindOpsChartResize,
} from "@/composables/ops/useOpsChartHost";
import { fetchCallHistogram, fetchCallStages } from "@/api/call-monitor";
import type {
  CallHistogramQuery,
  CallHistogramResponse,
  CallStageAccessRow,
  CallStageEntry,
} from "@/types/api/call-monitor";
import type { ListFilter } from "@/types/list-query";
import { $t } from "@/locales";
import { CALLS_FILTER_KEYS, type CallsTab } from "@/utils/ops/calls-link";
import { buildTraceRoute, unixSecondsOf } from "@/utils/ops/trace-link";
import { logger } from "@/utils/sys";
import CallMonitorTable, {
  type CallTimeRange,
} from "./modules/CallMonitorTable.vue";
import CallMonitorModeHint, {
  type CallMonitorModeHintPayload,
} from "./modules/CallMonitorModeHint.vue";
import StageBreakdown from "./modules/StageBreakdown.vue";

defineOptions({ name: "OpsCalls" });

const { t } = useI18n();
const route = useRoute();
const router = useRouter();

const TABS: CallsTab[] = ["customer", "upstream", "stages"];
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
const modeHint = ref<CallMonitorModeHintPayload | null>(null);

const tableRange = computed<CallTimeRange | null>(() =>
  timeRange.value
    ? { start: timeRange.value.start, end: timeRange.value.end }
    : null,
);

function onRangeChange(value: TimeRangeValue) {
  timeRange.value = value;
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
  if (!chartRef.value || !histogram.value) return;
  if (!chart) {
    chart = echarts.init(chartRef.value);
    chart.on("brushEnd", onBrushEnd);
    unbindChartResize?.();
    unbindChartResize = bindOpsChartResize(chartRef.value, () => chart);
  }
  chart.setOption(buildOption(histogram.value), true);
  chart.dispatchAction({
    type: "takeGlobalCursor",
    key: "brush",
    brushOption: { brushType: "lineX", brushMode: "single" },
  });
}

async function loadHistogram() {
  if (!lastQuery || !timeRange.value || activeTab.value === "stages") return;
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

function resizeChart() {
  chart?.resize();
}

watch(activeTab, (tab) => {
  if (tab === "stages") {
    chart?.dispose();
    chart = null;
    histogram.value = null;
  }
});

/* ---------- stages ---------- */

const stageKind = ref<"request_id" | "trace_id">(
  queryString("trace_id") && !queryString("request_id") ? "trace_id" : "request_id",
);
const stageInput = ref(
  stageKind.value === "trace_id" ? queryString("trace_id") : queryString("request_id"),
);
const stageLoading = ref(false);
const stageCalls = ref<CallStageAccessRow[]>([]);
const stageRows = ref<CallStageEntry[]>([]);

function formatTime(value: string): string {
  const d = new Date(value);
  return Number.isNaN(d.getTime()) ? value : formatDateTime(d);
}

async function searchStages() {
  const value = stageInput.value.trim();
  if (!value) {
    ElMessage.warning(t("ops.calls.stages.inputRequired"));
    return;
  }
  stageLoading.value = true;
  try {
    const res = await fetchCallStages({ [stageKind.value]: value });
    stageCalls.value = res.calls;
    stageRows.value = res.stages;
    const query: Record<string, string | undefined> = {
      ...(route.query as Record<string, string>),
      tab: "stages",
      request_id: undefined,
      trace_id: undefined,
    };
    query[stageKind.value] = value;
    void router.replace({ query });
  } catch (error) {
    logger.error("Failed to load call stages:", error);
    ElMessage.error(t("ops.calls.stages.loadFailed"));
  } finally {
    stageLoading.value = false;
  }
}

function openTrace(traceId: string, start?: number) {
  void router.push(buildTraceRoute(traceId, { start }));
}

let unbindChartResize: (() => void) | null = null;

onMounted(() => {
  if (activeTab.value === "stages" && stageInput.value) {
    void searchStages();
  }
});

onUnmounted(() => {
  unbindChartResize?.();
  unbindChartResize = null;
  chart?.dispose();
  chart = null;
});
</script>

<style scoped>
.ops-calls__title {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.ops-calls__title h2 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
}
.ops-calls__card-head {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
}
.ops-calls__card h3 {
  margin: 0 0 8px;
  font-size: 14px;
  font-weight: 600;
}
.ops-calls__summary,
.ops-calls__hint {
  color: var(--el-text-color-secondary);
  font-size: 12px;
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
  min-height: 480px;
}
.ops-calls__stage-bar {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
}
.ops-calls__stage-kind {
  width: 140px;
}
.ops-calls__stage-input {
  max-width: 420px;
}
.ops-calls__stage-title {
  margin-top: 16px;
}
</style>
