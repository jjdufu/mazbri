<template>
  <div class="ops-traffic-panel">
    <ElRow :gutter="16">
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.traffic.qpsByMode')">
          <MultiSeriesChart :series="qpsSeries" :loading="loading" height="240px" />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.traffic.errorByMode')">
          <MultiSeriesChart :series="errorSeries" :loading="loading" height="240px" />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.traffic.p95ByMode')">
          <MultiSeriesChart :series="p95Series" :loading="loading" height="240px" />
        </OpsCard>
      </ElCol>
    </ElRow>

    <OpsCard>
      <div class="ops-traffic__row-head">
        <h3>{{ t("ops.traffic.topByDim") }}</h3>
        <ElRadioGroup v-model="dim" size="small" @change="loadTop">
          <ElRadioButton
            v-for="item in dimOptions"
            :key="item.value"
            :value="item.value"
          >
            {{ item.label }}
          </ElRadioButton>
        </ElRadioGroup>
        <ElSelect v-model="metric" style="width: 140px" @change="loadTop">
          <ElOption
            v-for="item in metricOptions"
            :key="item.value"
            :label="item.label"
            :value="item.value"
          />
        </ElSelect>
      </div>
      <CategoryBarChart
        horizontal
        :categories="topCategories"
        :series="topSeries"
        :loading="loadingTop"
        height="260px"
      />
      <OpsTableWrap>
        <ArtTable
          :data="topRows"
          :loading="loadingTop"
          :columns="topColumns"
          class="surface-panel-table"
          stripe
          @row-click="onTopRowClick"
        />
      </OpsTableWrap>
    </OpsCard>

    <ElRow :gutter="16">
      <ElCol :xs="24" :lg="14">
        <OpsCard :title="t('ops.traffic.statusHeatmap')">
          <HeatmapChart
            :cells="statusCells"
            :x-labels="statusX"
            :y-labels="statusY"
            :loading="loading"
            height="260px"
          />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="10">
        <OpsCard :title="t('ops.traffic.bytesTrend')">
          <MultiSeriesChart :series="bytesSeries" :loading="loading" height="260px" />
        </OpsCard>
      </ElCol>
    </ElRow>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import OpsTableWrap from "@/components/ops/OpsTableWrap.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import MultiSeriesChart from "@/components/ops/MultiSeriesChart.vue";
import CategoryBarChart from "@/components/ops/CategoryBarChart.vue";
import HeatmapChart, { type HeatCell } from "@/components/ops/HeatmapChart.vue";
import { toOlapRange } from "@/composables/ops/useOlapRange";
import { useOpsSharedRange } from "@/composables/ops/useOpsSharedRange";
import { useStep } from "@/composables/ops/useStep";
import { fetchMetricTimeseries } from "@/api/monitoring";
import { postOlapQuery } from "@/api/olap";
import { buildCallsRoute } from "@/utils/ops/calls-link";
import { formatClockHm } from "@/utils/ops/time-range";
import type { MetricSeries } from "@/types/api/monitoring";
import type {
  BytesRow,
  HeatmapRow,
  OlapDim,
  OlapMetric,
  TopRow,
} from "@/types/api/olap";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "OpsTrafficRedPanel" });

const { t } = useI18n();
const { resolveStep } = useStep();
const { timeRange, reloadTick } = useOpsSharedRange();

const loading = ref(false);
const loadingTop = ref(false);
const dim = ref<OlapDim>("product_id");
const metric = ref<OlapMetric>("count");
const qpsSeries = ref<MetricSeries | null>(null);
const errorSeries = ref<MetricSeries | null>(null);
const p95Series = ref<MetricSeries | null>(null);
const bytesSeries = ref<MetricSeries | null>(null);
const topRows = ref<TopRow[]>([]);
const statusRows = ref<HeatmapRow[]>([]);

const dimOptions = computed(() => [
  { value: "tenant_code" as const, label: t("ops.traffic.dim.tenant") },
  { value: "product_id" as const, label: t("ops.traffic.dim.product") },
  { value: "supplier_code" as const, label: t("ops.traffic.dim.supplier") },
  { value: "channel_code" as const, label: t("ops.traffic.dim.channel") },
  { value: "status_class" as const, label: t("ops.traffic.dim.status") },
  { value: "mode" as const, label: t("ops.traffic.dim.mode") },
]);
const metricOptions = computed(() => [
  { value: "count" as const, label: t("ops.traffic.metric.count") },
  { value: "p95" as const, label: "P95" },
  { value: "p99" as const, label: "P99" },
  { value: "request_bytes" as const, label: t("ops.traffic.metric.reqBytes") },
  { value: "response_bytes" as const, label: t("ops.traffic.metric.respBytes") },
]);

const topCategories = computed(() => topRows.value.map((r) => r.key || "—"));
const topSeries = computed(() => [
  { name: metric.value, data: topRows.value.map((r) => r.value) },
]);
const topColumns = computed<ColumnOption<TopRow>[]>(() => [
  { prop: "key", label: t("ops.traffic.columns.key") },
  { prop: "value", label: t("ops.traffic.columns.value") },
  { prop: "count", label: t("ops.traffic.columns.count") },
]);

const statusX = computed(() => {
  const set = new Set(statusRows.value.map((r) => formatClockHm(r.ts)));
  return [...set];
});
const statusY = computed(() => {
  const set = new Set(statusRows.value.map((r) => r.status_class || "—"));
  return [...set];
});
const statusCells = computed<HeatCell[]>(() =>
  statusRows.value.map((r) => ({
    x: formatClockHm(r.ts),
    y: r.status_class || "—",
    value: r.count,
  })),
);

const router = useRouter();

function onTopRowClick(row: TopRow) {
  if (dim.value === "product_id") {
    ElMessage.info(t("ops.traffic.drillDisabledProduct"));
    return;
  }
  if (dim.value === "tenant_code") {
    ElMessage.info(t("ops.traffic.drillDisabledTenant"));
    return;
  }
  if (!row.key) return;
  void router.push(
    buildCallsRoute({
      tab: "customer",
      start: timeRange.value.start,
      end: timeRange.value.end,
      filters: { [dim.value]: row.key },
    }),
  );
}

async function loadTop() {
  loadingTop.value = true;
  try {
    const res = await postOlapQuery<TopRow>({
      template: "top_by_dim",
      params: {
        dim: dim.value,
        metric: metric.value,
        range: toOlapRange(timeRange.value),
        limit: 15,
      },
    });
    topRows.value = res.rows ?? [];
  } catch {
    ElMessage.error(t("ops.traffic.loadFailed"));
  } finally {
    loadingTop.value = false;
  }
}

async function reload() {
  loading.value = true;
  const start = timeRange.value.start;
  const end = timeRange.value.end;
  const step = resolveStep(end - start);
  const range = toOlapRange(timeRange.value);
  try {
    const [qps, err, p95, heat, bytes] = await Promise.all([
      fetchMetricTimeseries({ metric: "qps_by_mode", start, end, step }),
      fetchMetricTimeseries({ metric: "error_rate_by_mode", start, end, step }),
      fetchMetricTimeseries({
        metric: "p95_latency_ms_by_mode",
        start,
        end,
        step,
      }),
      postOlapQuery<HeatmapRow>({
        template: "status_heatmap",
        params: { range },
      }),
      postOlapQuery<BytesRow>({ template: "bytes_trend", params: { range } }),
    ]);
    qpsSeries.value = qps;
    errorSeries.value = err;
    p95Series.value = p95;
    statusRows.value = heat.rows ?? [];
    const req: MetricSeries = {
      metric: "request_bytes",
      label: t("ops.traffic.metric.reqBytes"),
      unit: "B",
      series: [
        {
          labels: { kind: "request" },
          points: (bytes.rows ?? []).map((r) => ({
            timestamp: r.ts,
            value: r.request_bytes,
          })),
        },
        {
          labels: { kind: "response" },
          points: (bytes.rows ?? []).map((r) => ({
            timestamp: r.ts,
            value: r.response_bytes,
          })),
        },
      ],
    };
    bytesSeries.value = req;
    await loadTop();
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
.ops-traffic__row-head {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  margin-bottom: 8px;
}
.ops-traffic__row-head h3 {
  margin: 0;
  margin-right: auto;
}
</style>
