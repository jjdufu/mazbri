<template>
  <div class="ops-traffic-panel">
    <ElRow :gutter="16">
      <ElCol :xs="24" :lg="14">
        <OpsCard :title="t('ops.traffic.errorMatrix')">
          <HeatmapChart
            v-if="matrixRows.length > 0 || loading"
            :cells="matrixCells"
            :x-labels="matrixX"
            :y-labels="matrixY"
            :loading="loading"
            height="320px"
          />
          <ElEmpty
            v-else
            :description="t('ops.traffic.noErrors')"
            :image-size="72"
          />
          <OpsTableWrap v-if="matrixRows.length > 0 || loading">
            <ArtTable
              :data="matrixRows"
              :loading="loading"
              :columns="matrixColumns"
              class="surface-panel-table"
              stripe
              @row-click="onErrorClick"
            />
          </OpsTableWrap>
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="10">
        <OpsCard :title="t('ops.traffic.errorAttribution')">
          <PieChart :slices="attrSlices" :loading="loading" height="260px" />
        </OpsCard>
        <OpsCard :title="t('ops.traffic.responseFlags')">
          <CategoryBarChart
            horizontal
            :categories="flagCategories"
            :series="flagSeries"
            :loading="loading"
            height="220px"
          />
        </OpsCard>
      </ElCol>
    </ElRow>

    <OpsCard>
      <div class="ops-traffic__sample-head">
        <h3>
          {{ t("ops.traces.sampleTitle") }}
          <ElTag v-if="sampleCode" size="small" type="danger">{{
            sampleCode
          }}</ElTag>
        </h3>
        <ElButton
          v-if="sampleCode"
          size="small"
          link
          type="primary"
          @click="openCalls"
        >
          {{ t("ops.traces.viewCalls") }}
        </ElButton>
      </div>
      <OpsTableWrap>
        <ElTable
          v-loading="sampleLoading"
          :data="samples"
          size="small"
          stripe
          :empty-text="t('ops.traces.sampleHint')"
        >
          <ElTableColumn :label="t('ops.traces.columns.start')" min-width="170">
            <template #default="{ row }">{{
              formatSampleTime(row.timestamp)
            }}</template>
          </ElTableColumn>
          <ElTableColumn prop="serial_no" label="request_id" min-width="260" />
          <ElTableColumn
            prop="tenant_name"
            :label="t('ops.traces.form.tenant')"
            min-width="120"
          />
          <ElTableColumn prop="status_code" label="status" width="80" />
          <ElTableColumn prop="error_stage" label="error_stage" min-width="120" />
          <ElTableColumn prop="duration_ms" label="ms" width="90" />
          <ElTableColumn prop="trace_id" label="trace_id" min-width="280">
            <template #default="{ row }">
              <ElLink
                v-if="row.trace_id"
                type="primary"
                @click="openSampleTrace(row)"
              >
                {{ row.trace_id }}
              </ElLink>
              <span v-else>—</span>
            </template>
          </ElTableColumn>
        </ElTable>
      </OpsTableWrap>
    </OpsCard>

    <OpsCard :title="t('ops.traffic.newErrorCodes')">
      <OpsTableWrap>
        <ArtTable
          :data="newErrors"
          :loading="loading"
          :columns="newColumns"
          class="surface-panel-table"
          stripe
          @row-click="onErrorClick"
        />
      </OpsTableWrap>
    </OpsCard>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import OpsTableWrap from "@/components/ops/OpsTableWrap.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import CategoryBarChart from "@/components/ops/CategoryBarChart.vue";
import HeatmapChart, { type HeatCell } from "@/components/ops/HeatmapChart.vue";
import PieChart from "@/components/ops/PieChart.vue";
import { toOlapRange } from "@/composables/ops/useOlapRange";
import { useOpsSharedRange } from "@/composables/ops/useOpsSharedRange";
import { postOlapQuery } from "@/api/olap";
import { buildCallsRoute } from "@/utils/ops/calls-link";
import {
  timestampFiltersFromRange,
  timestampFiltersToUnix,
} from "@/utils/ops/call-filters";
import { buildTraceRoute } from "@/utils/ops/trace-link";
import { buildListQueryParams } from "@/utils/list-query/serialize";
import { fetchCustomerCallLogs } from "@/api/call-monitor";
import type {
  CallMonitorItem,
  CallMonitorQuery,
} from "@/types/api/call-monitor";
import type {
  AttributionRow,
  ErrorCellRow,
  FlagRow,
  NewErrorRow,
} from "@/types/api/olap";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "OpsTrafficErrorsPanel" });

const { t } = useI18n();
const { timeRange, reloadTick } = useOpsSharedRange();

const loading = ref(false);
const matrixRows = ref<ErrorCellRow[]>([]);
const attribution = ref<AttributionRow[]>([]);
const flags = ref<FlagRow[]>([]);
const newErrors = ref<NewErrorRow[]>([]);

const matrixX = computed(() => [
  ...new Set(matrixRows.value.map((r) => r.error_code || "—")),
]);
const matrixY = computed(() => [
  ...new Set(matrixRows.value.map((r) => r.error_stage || "—")),
]);
const matrixCells = computed<HeatCell[]>(() =>
  matrixRows.value.map((r) => ({
    x: r.error_code || "—",
    y: r.error_stage || "—",
    value: r.count,
  })),
);
const matrixColumns = computed<ColumnOption<ErrorCellRow>[]>(() => [
  { prop: "error_code", label: "error_code" },
  { prop: "error_stage", label: "error_stage" },
  { prop: "count", label: t("ops.traffic.columns.count") },
]);

const attrSlices = computed(() =>
  attribution.value.map((r) => ({
    name: r.error_class || "unknown",
    value: r.count,
  })),
);
const flagCategories = computed(() =>
  flags.value.map((r) => r.response_flags || "—"),
);
const flagSeries = computed(() => [
  {
    name: t("ops.traffic.columns.count"),
    data: flags.value.map((r) => r.count),
  },
]);
const newColumns = computed<ColumnOption<NewErrorRow>[]>(() => [
  { prop: "error_code", label: "error_code" },
  { prop: "count", label: t("ops.traffic.columns.count") },
]);

const router = useRouter();

const sampleCode = ref("");
const samples = ref<CallMonitorItem[]>([]);
const sampleLoading = ref(false);
let sampleSeq = 0;

async function onErrorClick(row: ErrorCellRow | NewErrorRow) {
  if (!row.error_code) return;
  sampleCode.value = row.error_code;
  const seq = ++sampleSeq;
  sampleLoading.value = true;
  try {
    const params = buildListQueryParams({
      q: "",
      filters: timestampFiltersToUnix([
        { field: "error_code", op: "eq", value: row.error_code },
        ...timestampFiltersFromRange(timeRange.value),
      ]),
      encodeMode: "json",
      extra: { page: 1, page_size: 10 },
    }) as CallMonitorQuery;
    const res = await fetchCustomerCallLogs(params);
    if (seq === sampleSeq) samples.value = res.data;
  } catch {
    if (seq === sampleSeq) {
      samples.value = [];
      ElMessage.error(t("ops.traffic.loadFailed"));
    }
  } finally {
    if (seq === sampleSeq) sampleLoading.value = false;
  }
}

function openCalls() {
  if (!sampleCode.value) return;
  void router.push(
    buildCallsRoute({
      tab: "customer",
      start: timeRange.value.start,
      end: timeRange.value.end,
      filters: { error_code: sampleCode.value },
    }),
  );
}

function sampleSeconds(
  value: CallMonitorItem["timestamp"],
): number | undefined {
  if (value === null || value === "") return undefined;
  const ms = typeof value === "number" ? value : Date.parse(value);
  if (!Number.isFinite(ms)) return undefined;
  return Math.floor((ms > 1e12 ? ms : ms * 1000) / 1000);
}

function formatSampleTime(value: CallMonitorItem["timestamp"]): string {
  const sec = sampleSeconds(value);
  return sec === undefined
    ? String(value ?? "")
    : new Date(sec * 1000).toLocaleString();
}

function openSampleTrace(row: CallMonitorItem) {
  void router.push(
    buildTraceRoute(row.trace_id, {
      start: sampleSeconds(row.timestamp),
      requestId: row.serial_no,
    }),
  );
}

async function reload() {
  loading.value = true;
  const range = toOlapRange(timeRange.value, true);
  try {
    const [matrix, attr, flag, neu] = await Promise.all([
      postOlapQuery<ErrorCellRow>({
        template: "error_matrix",
        params: { range },
      }),
      postOlapQuery<AttributionRow>({
        template: "error_attribution",
        params: { range },
      }),
      postOlapQuery<FlagRow>({
        template: "response_flags",
        params: { range },
      }),
      postOlapQuery<NewErrorRow>({
        template: "new_error_codes",
        params: { range },
      }),
    ]);
    matrixRows.value = (matrix.rows ?? []).filter((r) => Boolean(r.error_code));
    attribution.value = attr.rows ?? [];
    flags.value = flag.rows ?? [];
    newErrors.value = (neu.rows ?? []).filter((r) => Boolean(r.error_code));
  } catch {
    ElMessage.error(t("ops.traffic.loadFailed"));
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void reload();
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
    void reload();
  },
);
</script>

<style scoped>
.ops-traffic__sample-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
}
</style>
