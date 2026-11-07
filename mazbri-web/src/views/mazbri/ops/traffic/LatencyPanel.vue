<template>
  <div class="ops-traffic-panel">
    <OpsCard :title="t('ops.traffic.latencyBreakdown')">
      <CategoryBarChart
        :categories="breakdownCategories"
        :series="breakdownSeries"
        :loading="loading"
        height="280px"
      />
    </OpsCard>

    <ElRow :gutter="16" class="ops-equal-row">
      <ElCol :xs="24" :lg="12">
        <OpsCard :title="t('ops.traffic.stageQuantiles')">
          <CategoryBarChart
            :categories="stageCategories"
            :series="stageSeries"
            :loading="loading"
            height="280px"
          />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="12">
        <OpsCard :title="t('ops.traffic.platformLocal')">
          <MultiSeriesChart
            :series="platformSeries"
            :loading="loading"
            height="280px"
          />
        </OpsCard>
      </ElCol>
    </ElRow>

    <OpsCard :title="t('ops.traffic.latencyHeatmap')">
      <HeatmapChart
        :cells="heatCells"
        :x-labels="heatX"
        :y-labels="heatY"
        :loading="loading"
        height="280px"
      />
    </OpsCard>

    <OpsCard :title="t('ops.traffic.slowRequests')">
      <OpsTableWrap>
        <ArtTable
          :data="slowRows"
          :loading="loading"
          :columns="slowColumns"
          class="surface-panel-table"
          stripe
          @row-click="onSlowClick"
        />
      </OpsTableWrap>
    </OpsCard>
  </div>
</template>

<script setup lang="ts">
import { formatDateTime } from "@/utils/format";
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import OpsTableWrap from "@/components/ops/OpsTableWrap.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import MultiSeriesChart from "@/components/ops/MultiSeriesChart.vue";
import CategoryBarChart from "@/components/ops/CategoryBarChart.vue";
import HeatmapChart, { type HeatCell } from "@/components/ops/HeatmapChart.vue";
import { toOlapWindow } from "@/composables/ops/useOlapRange";
import { useOpsSharedRange } from "@/composables/ops/useOpsSharedRange";
import { useStep } from "@/composables/ops/useStep";
import { fetchMetricTimeseries } from "@/api/monitoring";
import { postOlapQuery } from "@/api/olap";
import { buildCallsRoute } from "@/utils/ops/calls-link";
import { buildTraceRoute } from "@/utils/ops/trace-link";
import { formatClockHm } from "@/utils/ops/time-range";
import type { MetricSeries } from "@/types/api/monitoring";
import type {
  BreakdownRow,
  LatencyHeatRow,
  SlowRow,
  StageRow,
} from "@/types/api/olap";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "OpsTrafficLatencyPanel" });

const { t } = useI18n();
const { resolveStep } = useStep();
const { timeRange, reloadTick } = useOpsSharedRange();

const loading = ref(false);
const breakdown = ref<BreakdownRow[]>([]);
const stages = ref<StageRow[]>([]);
const heatRows = ref<LatencyHeatRow[]>([]);
const slowRows = ref<SlowRow[]>([]);
const platformSeries = ref<MetricSeries | null>(null);

const breakdownCategories = computed(() =>
  breakdown.value.map((r) => r.product_id || "—"),
);
const breakdownSeries = computed(() => [
  {
    name: "internal",
    stack: "lat",
    data: breakdown.value.map((r) => r.internal_p99),
  },
  {
    name: "dependency_wait",
    stack: "lat",
    data: breakdown.value.map((r) => r.dependency_wait_p99),
  },
  {
    name: "upstream",
    stack: "lat",
    data: breakdown.value.map((r) => r.upstream_p99),
  },
  {
    name: "post_response",
    stack: "lat",
    data: breakdown.value.map((r) => r.post_response_p99),
  },
]);

const stageCategories = computed(() =>
  stages.value.map((r) => r.stage_name || "—"),
);
const stageSeries = computed(() => [
  { name: "p50", data: stages.value.map((r) => r.p50) },
  { name: "p95", data: stages.value.map((r) => r.p95) },
  { name: "p99", data: stages.value.map((r) => r.p99) },
]);

const heatX = computed(() => [
  ...new Set(heatRows.value.map((r) => formatClockHm(r.ts))),
]);
const heatY = computed(() =>
  [...new Set(heatRows.value.map((r) => String(r.le_ms)))].sort(
    (a, b) => Number(a) - Number(b),
  ),
);
const heatCells = computed<HeatCell[]>(() =>
  heatRows.value.map((r) => ({
    x: formatClockHm(r.ts),
    y: String(r.le_ms),
    value: r.count,
  })),
);

const slowColumns = computed<ColumnOption<SlowRow>[]>(() => [
  {
    prop: "ts",
    label: t("ops.traffic.columns.time"),
    formatter: (row) => formatDateTime(row.ts),
  },
  { prop: "request_id", label: "request_id" },
  { prop: "trace_id", label: "trace_id" },
  { prop: "product_id", label: t("ops.traffic.dim.product") },
  { prop: "tenant_code", label: t("ops.traffic.dim.tenant") },
  { prop: "duration_ms", label: "duration_ms" },
  { prop: "upstream_service_time_ms", label: "upstream_ms" },
  { prop: "status_code", label: "status" },
  { prop: "error_code", label: "error_code" },
]);

const router = useRouter();

function onSlowClick(row: SlowRow) {
  if (row.trace_id) {
    const sec = row.ts > 1e12 ? Math.floor(row.ts / 1000) : row.ts;
    void router.push(
      buildTraceRoute(row.trace_id, { start: sec, requestId: row.request_id }),
    );
    return;
  }
  if (!row.request_id) return;
  void router.push(
    buildCallsRoute({ tab: "customer", filters: { request_id: row.request_id } }),
  );
}

async function reload() {
  loading.value = true;
  const window = toOlapWindow(timeRange.value);
  const start = timeRange.value.start;
  const end = timeRange.value.end;
  const step = resolveStep(end - start);
  try {
    const [bd, st, heat, slow, platform] = await Promise.all([
      postOlapQuery<BreakdownRow>({
        template: "latency_breakdown",
        params: { ...window, limit: 15 },
      }),
      postOlapQuery<StageRow>({
        template: "stage_quantiles",
        params: { ...window },
      }),
      postOlapQuery<LatencyHeatRow>({
        template: "latency_heatmap",
        params: { ...window },
      }),
      postOlapQuery<SlowRow>({
        template: "slow_requests",
        params: { ...window, limit: 50 },
      }),
      fetchMetricTimeseries({
        metric: "gateway_platform_local_ms",
        start,
        end,
        step,
      }),
    ]);
    breakdown.value = bd.rows ?? [];
    stages.value = st.rows ?? [];
    heatRows.value = heat.rows ?? [];
    slowRows.value = slow.rows ?? [];
    platformSeries.value = platform;
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
