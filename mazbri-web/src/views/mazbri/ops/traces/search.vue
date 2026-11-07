<template>
  <OpsPage :title="t('menus.mazbri.opsTraces')">
    <template #actions>
      <TimeRangePicker v-model="timeRange" @change="onRangeChange" />
    </template>

    <OpsCard>
      <ElForm
        :model="form"
        inline
        class="ops-traces__form"
        @submit.prevent="runSearch"
      >
        <ElFormItem label="trace_id">
          <ElInput v-model="form.trace_id" clearable class="ops-traces__wide" />
        </ElFormItem>
        <ElFormItem label="request_id">
          <ElInput
            v-model="form.request_id"
            clearable
            class="ops-traces__wide"
          />
        </ElFormItem>
        <ElFormItem :label="t('ops.traces.form.tenant')">
          <ElInput
            v-model="form.tenant_code"
            clearable
            class="ops-traces__mid"
          />
        </ElFormItem>
        <ElFormItem :label="t('ops.traces.form.product')">
          <ElInput
            v-model="form.product_id"
            clearable
            class="ops-traces__mid"
          />
        </ElFormItem>
        <ElFormItem :label="t('ops.traces.form.service')">
          <ElSelect
            v-model="form.service"
            clearable
            filterable
            allow-create
            default-first-option
            class="ops-traces__mid"
          >
            <ElOption
              v-for="s in serviceOptions"
              :key="s"
              :label="s"
              :value="s"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem :label="t('ops.traces.form.duration')">
          <ElInputNumber
            v-model="form.min_duration_ms"
            :min="0"
            controls-position="right"
            :placeholder="t('ops.traces.form.min')"
            class="ops-traces__num"
          />
          <span class="ops-traces__dash">–</span>
          <ElInputNumber
            v-model="form.max_duration_ms"
            :min="0"
            controls-position="right"
            :placeholder="t('ops.traces.form.max')"
            class="ops-traces__num"
          />
          <span class="ops-traces__unit">ms</span>
        </ElFormItem>
        <ElFormItem>
          <ElCheckbox v-model="form.errors_only">{{
            t("ops.traces.form.errorsOnly")
          }}</ElCheckbox>
        </ElFormItem>
        <ElFormItem>
          <ElButton type="primary" native-type="submit" :loading="loading">
            {{ t("common.search") }}
          </ElButton>
          <ElButton @click="resetForm">{{ t("common.reset") }}</ElButton>
        </ElFormItem>
      </ElForm>
    </OpsCard>

    <OpsCard :title="t('ops.traces.scatter.title')">
      <div class="ops-traces__card-head">
        <span class="ops-traces__muted">
          {{ t("ops.traces.scatter.summary", { count: traces.length }) }}
          <template v-if="resolvedFromLogs !== null">
            · {{ t("ops.traces.resolved", { count: resolvedFromLogs }) }}
          </template>
        </span>
      </div>
      <div
        ref="scatterRef"
        v-loading="loading"
        class="ops-traces__scatter"
      ></div>
    </OpsCard>
    <OpsCard>
      <ElTable
        v-loading="loading"
        :data="traces"
        stripe
        size="small"
        :empty-text="emptyText"
        class="ops-traces__table"
        @row-click="openTrace"
      >
        <ElTableColumn :label="t('ops.traces.columns.start')" min-width="160">
          <template #default="{ row }">{{ formatMs(row.start_ms) }}</template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.traces.columns.root')" min-width="220">
          <template #default="{ row }">
            <span class="ops-traces__service">{{ row.root_service }}</span>
            {{ row.root_span }}
          </template>
        </ElTableColumn>
        <ElTableColumn
          prop="duration_ms"
          :label="t('ops.traces.columns.duration')"
          width="120"
          sortable
        >
          <template #default="{ row }"
            >{{ row.duration_ms.toFixed(1) }} ms</template
          >
        </ElTableColumn>
        <ElTableColumn
          prop="span_count"
          :label="t('ops.traces.columns.spans')"
          width="90"
        />
        <ElTableColumn
          prop="service_count"
          :label="t('ops.traces.columns.services')"
          width="90"
        />
        <ElTableColumn
          prop="error_count"
          :label="t('ops.traces.columns.errors')"
          width="90"
        >
          <template #default="{ row }">
            <ElTag v-if="row.error_count > 0" type="danger" size="small">{{
              row.error_count
            }}</ElTag>
            <span v-else>0</span>
          </template>
        </ElTableColumn>
        <ElTableColumn prop="trace_id" label="trace_id" min-width="240">
          <template #default="{ row }">
            <ElLink type="primary" @click.stop="openTrace(row)">{{
              row.trace_id
            }}</ElLink>
          </template>
        </ElTableColumn>
      </ElTable>
    </OpsCard>
  </OpsPage>
</template>

<script setup lang="ts">
import { formatDateTime } from "@/utils/format";
import { computed, nextTick, onMounted, onUnmounted, reactive, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import { echarts, type EChartsOption } from "@/plugins/echarts";
import { fetchTraceServices, searchTraces } from "@/api/traces";
import type { TraceSearchQuery, TraceSummary } from "@/types/api/traces";
import { buildTraceRoute, TRACE_SEARCH_KEYS } from "@/utils/ops/trace-link";
import {
  OPS_CHART_GRID,
  bindOpsChartResize,
} from "@/composables/ops/useOpsChartHost";
import { logger } from "@/utils/sys";

defineOptions({ name: "OpsTraces" });

const { t } = useI18n();
const route = useRoute();
const router = useRouter();

/** Search window cap enforced by analytics. */
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
}

function queryString(key: string): string {
  const v = route.query[key];
  return typeof v === "string" ? v : "";
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
});

const timeRange = ref<TimeRangeValue>({
  preset: "1h",
  start: Math.floor(Date.now() / 1000) - 3600,
  end: Math.floor(Date.now() / 1000),
});
const loading = ref(false);
const searched = ref(false);
const traces = ref<TraceSummary[]>([]);
const resolvedFromLogs = ref<number | null>(null);

const scatterRef = ref<HTMLDivElement>();
let scatter: echarts.ECharts | null = null;

const catalogServices = ref<string[]>([]);

const serviceOptions = computed(() => {
  const set = new Set<string>(catalogServices.value);
  for (const tr of traces.value) {
    if (tr.root_service) set.add(tr.root_service);
  }
  if (form.service) set.add(form.service);
  return [...set].filter(Boolean).sort();
});

async function loadServiceCatalog() {
  const { start, end } = clampedRange(MAX_SEARCH_SECONDS);
  try {
    const rows = await fetchTraceServices({ start, end });
    catalogServices.value = (rows ?? [])
      .map((r) => r.service_name)
      .filter((s) => s && !s.includes(":"));
  } catch (error) {
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
    limit: 100,
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
  return q;
}

function syncRoute() {
  const query: Record<string, string | undefined> = {
    ...(route.query as Record<string, string>),
  };
  for (const key of TRACE_SEARCH_KEYS) query[key] = undefined;
  const values: Record<string, string | number | boolean | undefined> = {
    ...form,
  };
  for (const key of TRACE_SEARCH_KEYS) {
    const v = values[key];
    if (v === undefined || v === null || v === "" || v === false) continue;
    query[key] = String(v);
  }
  query.tab = undefined;
  void router.replace({ query });
}

let searchSeq = 0;
async function runSearch() {
  const seq = ++searchSeq;
  loading.value = true;
  try {
    const res = await searchTraces(buildQuery());
    if (seq !== searchSeq) return;
    traces.value = res.traces;
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

function resetForm() {
  form.trace_id = "";
  form.request_id = "";
  form.tenant_code = "";
  form.product_id = "";
  form.service = "";
  form.min_duration_ms = undefined;
  form.max_duration_ms = undefined;
  form.errors_only = false;
  void runSearch();
}

function openTrace(row: TraceSummary) {
  void router.push(
    buildTraceRoute(row.trace_id, { start: Math.floor(row.start_ms / 1000) }),
  );
}

function renderScatter() {
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
      if (row) openTrace(row);
    });
  }
  scatter.setOption(buildScatterOption(), true);
}

const okTraces = computed(() =>
  traces.value.filter((r) => r.error_count === 0),
);
const errorTraces = computed(() =>
  traces.value.filter((r) => r.error_count > 0),
);

function buildScatterOption(): EChartsOption {
  const toPoint = (r: TraceSummary) => [
    r.start_ms,
    Number(r.duration_ms.toFixed(2)),
  ];
  return {
    grid: { ...OPS_CHART_GRID, top: 48 },
    tooltip: {
      trigger: "item",
      formatter: (p) => {
        const item = Array.isArray(p) ? p[0] : p;
        const series =
          item.seriesIndex === 1 ? errorTraces.value : okTraces.value;
        const row = series[item.dataIndex];
        if (!row) return "";
        return `${row.root_service} · ${row.root_span}<br/>${row.duration_ms.toFixed(1)} ms · ${row.span_count} spans<br/>${row.trace_id}`;
      },
    },
    legend: { top: 0, right: 0 },
    xAxis: { type: "time" },
    yAxis: { type: "value", name: "ms", scale: true },
    series: [
      {
        name: t("ops.traces.scatter.ok"),
        type: "scatter",
        symbolSize: 8,
        itemStyle: { color: "#409eff" },
        data: okTraces.value.map(toPoint),
      },
      {
        name: t("ops.traces.scatter.error"),
        type: "scatter",
        symbolSize: 10,
        itemStyle: { color: "#f56c6c" },
        data: errorTraces.value.map(toPoint),
      },
    ],
  };
}

function onRangeChange(value: TimeRangeValue) {
  timeRange.value = value;
  void loadServiceCatalog();
  void runSearch();
}

let unbindScatterResize: (() => void) | null = null;

onMounted(() => {
  void loadServiceCatalog();
});

onUnmounted(() => {
  unbindScatterResize?.();
  unbindScatterResize = null;
  scatter?.dispose();
  scatter = null;
});
</script>

<style scoped>
.ops-traces__card-head {
  display: flex;
  gap: 12px;
  align-items: baseline;
  justify-content: space-between;
}
.ops-traces__card-head h3 {
  margin: 0 0 8px;
  font-size: 15px;
}
.ops-traces__form :deep(.el-form-item) {
  margin-bottom: 8px;
}
.ops-traces__wide {
  width: 280px;
}
.ops-traces__mid {
  width: 160px;
}
.ops-traces__num {
  width: 110px;
}
.ops-traces__dash,
.ops-traces__unit {
  margin: 0 6px;
  color: var(--el-text-color-secondary);
}
.ops-traces__muted {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}
.ops-traces__scatter {
  height: 240px;
  overflow: hidden;
}
.ops-traces__table :deep(.el-table__row) {
  cursor: pointer;
}
.ops-traces__service {
  display: inline-block;
  padding: 0 6px;
  margin-right: 6px;
  font-size: 12px;
  background: var(--el-fill-color);
  border-radius: 4px;
}
</style>
