<template>
  <div class="traffic-traces-panel">
    <OpsCard :title="t('ops.traces.scatter.title')">
      <template #extra>
        <span class="ops-traces__muted">
          {{ t("ops.traces.scatter.summary", { count: pagination.total }) }} ·
          {{ t("ops.traces.scatter.currentPageSubtitle") }}
          <template v-if="resolvedFromLogs !== null">
            · {{ t("ops.traces.resolved", { count: resolvedFromLogs }) }}
          </template>
        </span>
      </template>
      <EmptyState
        v-if="searched && !loading && traces.length === 0"
        compact
        :description="emptyText"
      />
      <div
        v-show="!searched || loading || traces.length > 0"
        ref="scatterRef"
        v-loading="loading"
        class="ops-traces__scatter"
      ></div>
    </OpsCard>

    <ResourceTableShell
      v-model:columns="columns"
      :loading="loading"
      :data="traces"
      :pagination="artPagination"
      :show-total="false"
      :show-toolbar="false"
      :empty-text="emptyText"
      size="small"
      stripe
      table-class="ops-traces__table surface-panel-table"
      @pagination:size-change="handlePageSizeChange"
      @pagination:current-change="handleCurrentPageChange"
    >
      <template #headerLeft>
        <div class="ops-traces__quick-filters">
          <ElCheckTag
            :checked="form.errors_only"
            class="ops-traces__error-tag"
            @change="toggleErrorsOnly"
          >
            {{ t("ops.traces.form.errorsOnly") }}
          </ElCheckTag>
          <ElCheckbox
            v-model="form.include_internal"
            class="ops-traces__internal-check"
            @change="onIncludeInternalChange"
          >
            {{ t("ops.traces.form.includeInternal") }}
          </ElCheckbox>
        </div>
      </template>
      <template #actions>
        <ListQueryBar
          v-model:q="keyword"
          v-model:filters="listFilters"
          always-expanded
          :catalog="tracesFilterCatalog"
          @search="applyFiltersAndSearch"
        />
        <ElButton @click="resetFilters">{{ t("common.reset") }}</ElButton>
      </template>

      <template #start_ms="{ row }">{{ formatMs(row.start_ms) }}</template>
      <template #root_span="{ row }">
        <span class="ops-traces__service">{{ row.root_service }}</span>
        {{ row.root_span }}
      </template>
      <template #duration_ms="{ row }"
        >{{ row.duration_ms.toFixed(1) }} ms</template
      >
      <template #error_count="{ row }">
        <ElTag v-if="row.error_count > 0" type="danger" size="small">{{
          row.error_count
        }}</ElTag>
        <span v-else>0</span>
      </template>
      <template #trace_id="{ row }">
        <CopyableText
          :value="row.trace_id"
          :link="traceRouteOf(row)"
          :label="t('ops.traces.columns.traceId')"
        />
      </template>
    </ResourceTableShell>
  </div>
</template>

<script setup lang="ts">
import {
  computed,
  nextTick,
  onMounted,
  onUnmounted,
  reactive,
  ref,
  watch,
} from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import CopyableText from "@/components/common/copyable-text/index.vue";
import EmptyState from "@/components/common/empty-state/index.vue";
import ListQueryBar from "@/components/common/list-query-bar/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import { echarts, type EChartsOption } from "@/plugins/echarts";
import { fetchTraceServices, searchTraces } from "@/api/traces";
import type { TraceSearchQuery, TraceSummary } from "@/types/api/traces";
import type { ColumnOption } from "@/types/component";
import type { ListFilter, ResourceFilterCatalog } from "@/types/list-query";
import { formatDateTime } from "@/utils/format";
import { buildTraceRoute, TRACE_SEARCH_KEYS } from "@/utils/ops/trace-link";
import {
  OPS_CHART_GRID,
  bindOpsChartResize,
  getOpsChartThemeTokens,
  useOpsChartTheme,
} from "@/composables/ops/useOpsChartHost";
import { useOpsSharedRange } from "@/composables/ops/useOpsSharedRange";
import { logger } from "@/utils/sys";

defineOptions({ name: "TrafficTracesPanel" });

const { t, locale } = useI18n();
const route = useRoute();
const router = useRouter();
const { timeRange, reloadTick } = useOpsSharedRange();

/** Search window cap enforced by analytics (7 days). */
const MAX_SEARCH_SECONDS = 7 * 86400;

interface SearchForm {
  trace_id: string;
  request_id: string;
  tenant_code: string;
  product_id: string;
  service: string;
  min_duration_ms: number | undefined;
  max_duration_ms: number | undefined;
  errors_only: boolean;
  include_internal: boolean;
}

function queryString(key: string): string {
  const raw = route.query[key];
  const v = Array.isArray(raw) ? raw[0] : raw;
  return typeof v === "string" ? v.trim() : "";
}

function queryNumber(key: string): number | undefined {
  const raw = queryString(key);
  if (!raw) return undefined;
  const n = Number(raw);
  return Number.isFinite(n) && n >= 0 ? n : undefined;
}

const form = reactive<SearchForm>({
  trace_id: queryString("trace_id"),
  request_id: queryString("request_id"),
  tenant_code: queryString("tenant_code"),
  product_id: queryString("product_id"),
  service: queryString("service"),
  min_duration_ms: queryNumber("min_duration_ms"),
  max_duration_ms: queryNumber("max_duration_ms"),
  errors_only: queryString("errors_only") === "true",
  include_internal: queryString("include_internal") === "true",
});

function buildInitialListFilters(): ListFilter[] {
  const seeded: ListFilter[] = [];
  if (form.trace_id && form.request_id) {
    seeded.push({ field: "request_id", op: "eq", value: form.request_id });
  }
  if (form.tenant_code) {
    seeded.push({ field: "tenant_code", op: "eq", value: form.tenant_code });
  }
  if (form.product_id) {
    seeded.push({ field: "product_id", op: "eq", value: form.product_id });
  }
  if (form.service) {
    seeded.push({ field: "service", op: "eq", value: form.service });
  }
  if (form.min_duration_ms !== undefined) {
    seeded.push({
      field: "min_duration_ms",
      op: "gte",
      value: form.min_duration_ms,
    });
  }
  if (form.max_duration_ms !== undefined) {
    seeded.push({
      field: "max_duration_ms",
      op: "lte",
      value: form.max_duration_ms,
    });
  }
  return seeded;
}

const keyword = ref(form.trace_id || form.request_id || "");
const listFilters = ref<ListFilter[]>(buildInitialListFilters());
const loading = ref(false);
const searched = ref(false);
const traces = ref<TraceSummary[]>([]);
const resolvedFromLogs = ref<number | null>(null);
const pagination = reactive({
  page: 1,
  pageSize: 20,
  total: 0,
});
const artPagination = computed(() => ({
  current: pagination.page,
  size: pagination.pageSize,
  total: pagination.total,
}));
const columns = ref<ColumnOption<TraceSummary>[]>([
  {
    prop: "start_ms",
    label: t("ops.traces.columns.start"),
    minWidth: 160,
    useSlot: true,
  },
  {
    prop: "root_span",
    label: t("ops.traces.columns.root"),
    minWidth: 220,
    useSlot: true,
  },
  {
    prop: "duration_ms",
    label: t("ops.traces.columns.duration"),
    width: 120,
    sortable: true,
    useSlot: true,
  },
  {
    prop: "span_count",
    label: t("ops.traces.columns.spans"),
    width: 90,
  },
  {
    prop: "service_count",
    label: t("ops.traces.columns.services"),
    width: 90,
  },
  {
    prop: "error_count",
    label: t("ops.traces.columns.errors"),
    width: 90,
    useSlot: true,
  },
  {
    prop: "trace_id",
    label: t("ops.traces.columns.traceId"),
    minWidth: 240,
    useSlot: true,
  },
]);

const scatterRef = ref<HTMLDivElement>();
let scatter: echarts.ECharts | null = null;

const catalogServices = ref<string[]>([]);
let lastCatalogRangeKey = "";

const serviceOptions = computed(() => {
  const set = new Set<string>(catalogServices.value);
  for (const tr of traces.value) {
    if (tr.root_service) set.add(tr.root_service);
  }
  if (form.service) set.add(form.service);
  return [...set].filter(Boolean).sort();
});

const tracesFilterCatalog = computed<ResourceFilterCatalog>(() => ({
  resource: "traces",
  max_filters: 10,
  fields: [
    {
      field: "trace_id",
      value_type: "string",
      ops: ["eq"],
      in_q: true,
      label_key: "ops.traces.columns.traceId",
    },
    {
      field: "request_id",
      value_type: "string",
      ops: ["eq"],
      in_q: true,
      label_key: "listFilters.fields.requestId",
    },
    {
      field: "tenant_code",
      value_type: "string",
      ops: ["eq"],
      in_q: true,
      label_key: "ops.traces.form.tenant",
    },
    {
      field: "product_id",
      value_type: "string",
      ops: ["eq"],
      in_q: true,
      label_key: "ops.traces.form.product",
    },
    {
      field: "service",
      value_type: serviceOptions.value.length > 0 ? "enum" : "string",
      ops: ["eq"],
      in_q: true,
      label_key: "ops.traces.form.service",
      enum_values: serviceOptions.value,
    },
    {
      field: "min_duration_ms",
      value_type: "int",
      ops: ["gte", "eq"],
      label_key: "ops.traces.form.minDuration",
    },
    {
      field: "max_duration_ms",
      value_type: "int",
      ops: ["lte", "eq"],
      label_key: "ops.traces.form.maxDuration",
    },
  ],
}));

function syncFormFromFilters() {
  form.trace_id = "";
  form.request_id = "";
  form.tenant_code = "";
  form.product_id = "";
  form.service = "";
  form.min_duration_ms = undefined;
  form.max_duration_ms = undefined;

  const qTrim = keyword.value.trim();
  if (qTrim) {
    if (/^[0-9a-fA-F]{32}$/.test(qTrim)) {
      form.trace_id = qTrim.toLowerCase();
    } else {
      form.request_id = qTrim;
    }
  }

  for (const f of listFilters.value) {
    const valStr = String(f.value ?? "").trim();
    if (!valStr) continue;
    switch (f.field) {
      case "trace_id":
        form.trace_id = valStr.toLowerCase();
        break;
      case "request_id":
        form.request_id = valStr;
        break;
      case "tenant_code":
        form.tenant_code = valStr;
        break;
      case "product_id":
        form.product_id = valStr;
        break;
      case "service":
        form.service = valStr;
        break;
      case "min_duration_ms": {
        const n = Number(valStr);
        if (Number.isFinite(n) && n >= 0) form.min_duration_ms = n;
        break;
      }
      case "max_duration_ms": {
        const n = Number(valStr);
        if (Number.isFinite(n) && n >= 0) form.max_duration_ms = n;
        break;
      }
      default:
        break;
    }
  }
}

function applyFiltersAndSearch() {
  syncFormFromFilters();
  pagination.page = 1;
  void runSearch();
}

function resetFilters() {
  keyword.value = "";
  listFilters.value = [];
  form.errors_only = false;
  form.include_internal = false;
  syncFormFromFilters();
  pagination.page = 1;
  void runSearch();
}

function toggleErrorsOnly(checked: boolean) {
  form.errors_only = checked;
  pagination.page = 1;
  void runSearch();
}

function onIncludeInternalChange() {
  pagination.page = 1;
  void runSearch();
}

async function loadServiceCatalog() {
  const { start, end } = clampedRange(MAX_SEARCH_SECONDS);
  const rangeKey = `${start}:${end}`;
  if (rangeKey === lastCatalogRangeKey) return;
  lastCatalogRangeKey = rangeKey;
  try {
    const rows = await fetchTraceServices({ start, end });
    catalogServices.value = (rows ?? [])
      .map((r) => r.service_name)
      .filter((s) => s && !s.includes(":"));
  } catch (error) {
    lastCatalogRangeKey = "";
    logger.error("Failed to load trace services:", error);
  }
}

const emptyText = computed(() =>
  searched.value ? t("ops.traces.empty") : t("ops.traces.notSearched"),
);

function formatMs(ms: number): string {
  return formatDateTime(ms);
}

function clampedRange(maxSeconds: number): { start: number; end: number } {
  const { start, end } = timeRange.value;
  return { start: Math.max(start, end - maxSeconds), end };
}

function buildQuery(): TraceSearchQuery {
  const q: TraceSearchQuery = {
    ...clampedRange(MAX_SEARCH_SECONDS),
    page: pagination.page,
    page_size: pagination.pageSize,
  };
  if (form.trace_id.trim()) q.trace_id = form.trace_id.trim().toLowerCase();
  if (form.request_id.trim()) q.request_id = form.request_id.trim();
  if (form.tenant_code.trim()) q.tenant_code = form.tenant_code.trim();
  if (form.product_id.trim()) q.product_id = form.product_id.trim();
  if (form.service) q.service = form.service;
  if (form.min_duration_ms !== undefined && form.min_duration_ms !== null) {
    q.min_duration_ms = form.min_duration_ms;
  }
  if (form.max_duration_ms !== undefined && form.max_duration_ms !== null) {
    q.max_duration_ms = form.max_duration_ms;
  }
  if (form.errors_only) q.errors_only = true;
  if (form.include_internal) q.include_internal = true;
  return q;
}

function syncRoute() {
  const current = route.query as Record<string, string | undefined>;
  const next: Record<string, string | undefined> = { ...current };
  const values: Record<string, string | number | boolean | undefined> = {
    ...form,
  };
  let changed = false;
  for (const key of TRACE_SEARCH_KEYS) {
    const v = values[key];
    const desired =
      v === undefined || v === null || v === "" || v === false
        ? undefined
        : String(v);
    if (current[key] !== desired) {
      changed = true;
    }
    if (desired === undefined) {
      delete next[key];
    } else {
      next[key] = desired;
    }
  }
  if (!changed) return;
  void router.replace({ query: next });
}

let searchSeq = 0;
async function runSearch() {
  const seq = ++searchSeq;
  loading.value = true;
  try {
    const res = await searchTraces(buildQuery());
    if (seq !== searchSeq) return;
    traces.value = res.traces;
    pagination.total = res.total;
    pagination.page = res.page;
    pagination.pageSize = res.page_size;
    resolvedFromLogs.value = res.resolved_from_logs;
    searched.value = true;
    syncRoute();
    await nextTick();
    renderScatter();
  } catch (error) {
    logger.error("Failed to search traces:", error);
    if (seq === searchSeq) ElMessage.error(t("ops.traces.loadFailed"));
  } finally {
    if (seq === searchSeq) loading.value = false;
  }
}

function handleCurrentPageChange(page: number) {
  pagination.page = page;
  void runSearch();
}

function handlePageSizeChange(pageSize: number) {
  pagination.pageSize = pageSize;
  pagination.page = 1;
  void runSearch();
}

function traceRouteOf(row: TraceSummary) {
  return buildTraceRoute(row.trace_id, {
    start: Math.floor(row.start_ms / 1000),
  });
}

const { isDark } = useOpsChartTheme(() => renderScatter());
watch(locale, () => renderScatter());

function renderScatter() {
  if (searched.value && !loading.value && traces.value.length === 0) {
    scatter?.clear();
    return;
  }
  if (!scatterRef.value) return;
  if (!scatter) {
    scatter = echarts.init(scatterRef.value);
    unbindScatterResize?.();
    unbindScatterResize = bindOpsChartResize(scatterRef.value, () => scatter);
    scatter.on("click", (params) => {
      const idx = typeof params.dataIndex === "number" ? params.dataIndex : -1;
      const series =
        params.seriesIndex === 1 ? errorTraces.value : okTraces.value;
      const row = series[idx];
      if (row) void router.push(traceRouteOf(row));
    });
  }
  scatter.setOption(buildScatterOption(), { notMerge: true });
}

const okTraces = computed(() =>
  traces.value.filter((r) => r.error_count === 0),
);
const errorTraces = computed(() =>
  traces.value.filter((r) => r.error_count > 0),
);

function buildScatterOption(): EChartsOption {
  const tokens = getOpsChartThemeTokens(isDark.value);
  const { start, end } = clampedRange(MAX_SEARCH_SECONDS);
  const toPoint = (r: TraceSummary) => [
    r.start_ms,
    Number(r.duration_ms.toFixed(2)),
  ];
  return {
    grid: { ...OPS_CHART_GRID, top: 36 },
    tooltip: {
      trigger: "item",
      backgroundColor: tokens.tooltipBg,
      borderColor: tokens.tooltipBorder,
      textStyle: { color: tokens.titleColor },
      formatter: (p) => {
        const item = Array.isArray(p) ? p[0] : p;
        const series =
          item.seriesIndex === 1 ? errorTraces.value : okTraces.value;
        const row = series[item.dataIndex];
        if (!row) return "";
        return `${row.root_service} · ${row.root_span}<br/>${row.duration_ms.toFixed(1)} ms · ${row.span_count} ${t("ops.traces.columns.spans")}<br/>${row.trace_id}`;
      },
    },
    legend: {
      top: 0,
      right: 0,
      left: 84,
      icon: "circle",
      itemWidth: 8,
      itemHeight: 8,
      padding: [2, 0, 2, 0],
      textStyle: { color: tokens.textColor, fontSize: 12 },
    },
    xAxis: {
      type: "time",
      min: start * 1000,
      max: end * 1000,
      axisLine: { lineStyle: { color: tokens.axisLineColor } },
      axisTick: { lineStyle: { color: tokens.axisLineColor } },
      splitLine: { show: false },
      axisLabel: { color: tokens.textColor, hideOverlap: true },
    },
    yAxis: {
      type: "value",
      name: "ms",
      nameGap: 12,
      nameTextStyle: { color: tokens.textColor, align: "left" },
      scale: true,
      axisLine: { show: false },
      axisTick: { show: false },
      splitLine: { lineStyle: { color: tokens.splitLineColor } },
      axisLabel: { color: tokens.textColor, hideOverlap: true },
    },
    series: [
      {
        name: t("ops.traces.scatter.ok"),
        type: "scatter",
        symbol: "circle",
        symbolSize: 8,
        itemStyle: { color: "#409eff" },
        data: okTraces.value.map(toPoint),
      },
      {
        name: t("ops.traces.scatter.error"),
        type: "scatter",
        symbol: "circle",
        symbolSize: 10,
        itemStyle: { color: "#f56c6c" },
        data: errorTraces.value.map(toPoint),
      },
    ],
  };
}

watch(
  () => [timeRange.value.start, timeRange.value.end],
  ([nextStart, nextEnd], [prevStart, prevEnd]) => {
    if (nextStart === prevStart && nextEnd === prevEnd) return;
    pagination.page = 1;
    void loadServiceCatalog();
    void runSearch();
  },
);

watch(reloadTick, () => {
  void runSearch();
});

let unbindScatterResize: (() => void) | null = null;

onMounted(() => {
  void loadServiceCatalog();
  void runSearch();
});

onUnmounted(() => {
  unbindScatterResize?.();
  unbindScatterResize = null;
  scatter?.dispose();
  scatter = null;
});
</script>

<style scoped>
.traffic-traces-panel {
  display: flex;
  flex-direction: column;
  gap: var(--ops-section-gap, 16px);
  min-width: 0;
}
.ops-traces__quick-filters {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
}
.ops-traces__error-tag {
  display: inline-flex;
  align-items: center;
  height: var(--art-form-control-height, 32px);
  padding: 0 12px;
  border-radius: var(--art-form-control-radius, 4px);
  user-select: none;
}
.ops-traces__internal-check {
  height: var(--art-form-control-height, 32px);
  margin-right: 0;
}
.ops-traces__muted {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}
.ops-traces__scatter {
  height: 220px;
  overflow: hidden;
}
.ops-traces__service {
  display: inline-block;
  padding: 0 6px;
  margin-right: 6px;
  font-size: 12px;
  background: var(--el-fill-color);
  border-radius: var(--art-form-control-radius-sm);
}
</style>
