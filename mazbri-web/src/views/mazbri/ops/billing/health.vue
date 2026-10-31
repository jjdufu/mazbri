<template>
  <div class="ops-traffic art-full-height">
    <div class="ops-traffic__toolbar">
      <h2>{{ t("menus.mazbri.billingHealth") }}</h2>
      <div class="ops-traffic__actions">
        <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
      </div>
    </div>

    <ElRow :gutter="12">
      <ElCol v-for="card in statCards" :key="card.label" :xs="12" :lg="6">
        <StatCard
          :label="card.label"
          :value="card.value"
          :unit="card.unit"
          :loading="loading"
          :digits="card.digits"
        />
      </ElCol>
    </ElRow>

    <ElRow :gutter="12">
      <ElCol :xs="24" :lg="12">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.billing.funnel") }}</h3>
          <CategoryBarChart
            :categories="funnelCategories"
            :series="funnelSeries"
            :loading="loading"
            height="240px"
          />
          <ArtTable
            :data="funnelRows"
            :loading="loading"
            :columns="funnelColumns"
            class="surface-panel-table"
            stripe
          />
        </div>
      </ElCol>
      <ElCol :xs="24" :lg="12">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.billing.queueTrend") }}</h3>
          <MultiSeriesChart
            :series="series.billing_queue_depth"
            :loading="loading"
            height="240px"
          />
        </div>
        <div class="ops-traffic__card">
          <h3>{{ t("ops.billing.batchThroughput") }}</h3>
          <MultiSeriesChart
            :series="series.billing_charge_items"
            :loading="loading"
            height="200px"
          />
        </div>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.billing.batches") }}</h3>
          <MultiSeriesChart
            :series="series.billing_charge_batches"
            :loading="loading"
            height="220px"
          />
        </div>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.billing.deductions") }}</h3>
          <MultiSeriesChart
            :series="series.billing_deductions"
            :loading="loading"
            height="220px"
          />
        </div>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.billing.precheck") }}</h3>
          <MultiSeriesChart
            :series="series.balance_precheck"
            :loading="loading"
            height="220px"
          />
        </div>
      </ElCol>
    </ElRow>

    <div class="ops-traffic__card">
      <h3>{{ t("ops.billing.failures") }}</h3>
      <ArtTable
        :data="failures"
        :loading="loading"
        :columns="failureColumns"
        class="surface-panel-table"
        stripe
      />
    </div>
    <div class="ops-traffic__card">
      <h3>{{ t("ops.billing.deadLetters") }}</h3>
      <ArtTable
        :data="deadLetters"
        :loading="loading"
        :columns="deadColumns"
        class="surface-panel-table"
        stripe
      />
    </div>
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
import CategoryBarChart from "@/components/ops/CategoryBarChart.vue";
import StatCard from "@/components/ops/StatCard.vue";
import { toOlapRange } from "@/composables/ops/useOlapRange";
import { useStep } from "@/composables/ops/useStep";
import { fetchMetricTimeseries } from "@/api/monitoring";
import { postOlapQuery } from "@/api/olap";
import { fetchBillingFailures, fetchBillingFunnel } from "@/api/billing-ops";
import type { MetricSeries } from "@/types/api/monitoring";
import type { FunnelChRow } from "@/types/api/olap";
import type {
  BillingFunnelPg,
  ChargeFailureRow,
  DeadLetterRow,
  FunnelStage,
} from "@/types/api/billing-ops";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "OpsBillingHealth" });

const CHART_METRICS = [
  "billing_queue_depth",
  "billing_charge_items",
  "billing_charge_batches",
  "billing_deductions",
  "balance_precheck",
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
const chBillable = ref(0);
const funnel = ref<BillingFunnelPg | null>(null);
const failures = ref<ChargeFailureRow[]>([]);
const deadLetters = ref<DeadLetterRow[]>([]);
const series = ref<Record<ChartMetric, MetricSeries | null>>(
  Object.fromEntries(CHART_METRICS.map((m) => [m, null])) as Record<
    ChartMetric,
    MetricSeries | null
  >,
);

/** 四段漏斗：CH billable=1 → usage_outbox → usage_charge_queue → ledger_entries。 */
const funnelRows = computed<FunnelStage[]>(() => {
  const pg = funnel.value?.stages ?? [];
  const head: FunnelStage = { stage: "clickhouse_billable", count: chBillable.value, diff: 0 };
  return [head, ...pg].map((row, i, all) => ({
    ...row,
    diff: i === 0 ? 0 : all[i - 1].count - row.count,
  }));
});
const funnelCategories = computed(() => funnelRows.value.map((r) => r.stage));
const funnelSeries = computed(() => [
  { name: t("ops.billing.count"), data: funnelRows.value.map((r) => r.count) },
]);
const funnelColumns = computed<ColumnOption<FunnelStage>[]>(() => [
  { prop: "stage", label: t("ops.billing.stage") },
  { prop: "count", label: t("ops.billing.count") },
  { prop: "diff", label: t("ops.billing.diff") },
]);

const statCards = computed(() => {
  const lag = funnel.value?.charge_lag;
  return [
    { label: t("ops.billing.lagP50"), value: lag?.p50_seconds ?? null, unit: "s", digits: 3 },
    { label: t("ops.billing.lagP95"), value: lag?.p95_seconds ?? null, unit: "s", digits: 3 },
    { label: t("ops.billing.lagMax"), value: lag?.max_seconds ?? null, unit: "s", digits: 3 },
    {
      label: t("ops.billing.failureCount"),
      value: funnel.value?.charge_failures ?? null,
      unit: "",
      digits: 0,
    },
  ];
});

const failureColumns = computed<ColumnOption<ChargeFailureRow>[]>(() => [
  { prop: "created_at", label: t("ops.billing.columns.time") },
  { prop: "tenant_id", label: t("ops.billing.columns.tenant") },
  { prop: "queue_id", label: "queue_id" },
  { prop: "attempt_number", label: t("ops.billing.columns.attempt") },
  { prop: "queue_status", label: t("ops.billing.columns.status") },
  { prop: "error_message", label: t("ops.billing.columns.error") },
]);
const deadColumns = computed<ColumnOption<DeadLetterRow>[]>(() => [
  { prop: "created_at", label: t("ops.billing.columns.time") },
  { prop: "source", label: t("ops.billing.columns.source") },
  { prop: "id", label: "id" },
  { prop: "tenant_id", label: t("ops.billing.columns.tenant") },
  { prop: "retry_count", label: t("ops.billing.columns.attempt") },
  { prop: "error_message", label: t("ops.billing.columns.error") },
]);

async function reload() {
  loading.value = true;
  const start = timeRange.value.start;
  const end = timeRange.value.end;
  const step = resolveStep(end - start);
  const range = toOlapRange(timeRange.value);
  try {
    const [ch, pg, fail, ...charts] = await Promise.all([
      postOlapQuery<FunnelChRow>({ template: "billing_funnel_ch", params: { range } }),
      fetchBillingFunnel(range),
      fetchBillingFailures(range),
      ...CHART_METRICS.map((metric) => fetchMetricTimeseries({ metric, start, end, step })),
    ]);
    chBillable.value = ch.rows?.[0]?.billable ?? 0;
    funnel.value = pg;
    failures.value = fail.failures ?? [];
    deadLetters.value = fail.dead_letters ?? [];
    const next = { ...series.value };
    CHART_METRICS.forEach((metric, i) => {
      next[metric] = charts[i] ?? null;
    });
    series.value = next;
  } catch {
    ElMessage.error(t("ops.billing.loadFailed"));
  } finally {
    loading.value = false;
  }
}

onMounted(reload);
</script>

<style scoped>
.ops-traffic__toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}
.ops-traffic__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}
.ops-traffic__card {
  margin-bottom: 12px;
  padding: 12px 16px;
  border-radius: 8px;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
}
.ops-traffic__card h3 {
  margin: 0 0 8px;
  font-size: 14px;
  font-weight: 600;
}
</style>
