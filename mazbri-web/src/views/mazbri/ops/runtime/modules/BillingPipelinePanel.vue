<template>
  <div class="ops-billing-pipeline">
    <div class="ops-billing-pipeline__kpi-grid">
      <StatCard
        v-for="card in statCards"
        :key="card.key"
        :label="card.label"
        :value="card.value"
        :unit="card.unit"
        :icon="card.icon"
        :icon-color="card.iconColor"
        :icon-background="card.iconBackground"
        :loading="loading"
        :digits="card.digits"
      />
    </div>

    <ElRow :gutter="16" class="ops-equal-row">
      <ElCol :xs="24" :lg="12">
        <OpsCard :title="funnelTitle" :info="funnelHint">
          <ArtTable
            :data="funnelRows"
            :loading="loading"
            :columns="funnelColumns"
            class="surface-panel-table"
            stripe
          />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="12" class="ops-billing-pipeline__stacked-col">
        <OpsCard :title="t('ops.billing.queueTrend')">
          <MultiSeriesChart
            :series="series.billing_outbox_pending"
            :loading="loading"
            :window="chartWindow"
            height="180px"
          />
        </OpsCard>
        <OpsCard :title="t('ops.billing.oldestAge')">
          <MultiSeriesChart
            :series="series.billing_oldest_pending_age"
            :loading="loading"
            :window="chartWindow"
            height="180px"
          />
        </OpsCard>
      </ElCol>
    </ElRow>

    <ElRow :gutter="16" class="ops-equal-row">
      <ElCol
        v-for="chart in trendCharts"
        :key="chart.metric"
        :xs="24"
        :md="12"
        :lg="8"
      >
        <OpsCard :title="t(chart.title)">
          <MultiSeriesChart
            :series="chart.data"
            :loading="loading"
            :window="chartWindow"
            height="220px"
          />
        </OpsCard>
      </ElCol>
    </ElRow>

    <ElRow :gutter="16" class="ops-equal-row">
      <ElCol :xs="24" :lg="12">
        <OpsCard :title="t('ops.billing.failures')">
          <ArtTable
            :data="failures"
            :loading="loading || failuresLoading"
            :columns="failureColumns"
            :pagination="failurePagination"
            class="surface-panel-table"
            stripe
            @pagination:current-change="handleFailurePageChange"
            @pagination:size-change="handleFailureSizeChange"
          />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="12">
        <OpsCard :title="t('ops.billing.deadLetters')">
          <ArtTable
            :data="deadLetters"
            :loading="loading || deadLoading"
            :columns="deadColumns"
            :pagination="deadPagination"
            :empty-text="t('common.noData')"
            class="surface-panel-table"
            stripe
            @pagination:current-change="handleDeadPageChange"
            @pagination:size-change="handleDeadSizeChange"
          />
        </OpsCard>
      </ElCol>
    </ElRow>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import MultiSeriesChart from "@/components/ops/MultiSeriesChart.vue";
import StatCard from "@/components/ops/StatCard.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import { toOpsRange } from "@/composables/ops/useOlapRange";
import { useOpsSharedRange } from "@/composables/ops/useOpsSharedRange";
import { useStep } from "@/composables/ops/useStep";
import { fetchMetricTimeseries } from "@/api/monitoring";
import { fetchBillingFailures, fetchBillingFunnel } from "@/api/billing-ops";
import type { MetricSeries } from "@/types/api/monitoring";
import type {
  BillingFunnelPg,
  ChargeFailureRow,
  DeadLetterRow,
  FunnelStage,
} from "@/types/api/billing-ops";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import { formatDateTime } from "@/utils/format";
import { getStatusLabel } from "@/utils/display-labels";
import { aggregateLastMetricValue } from "@/utils/ops/metric-series";
import {
  buildDeadLettersQueueQuery,
  buildFailuresQueueQuery,
  formatPipelineStageLabel,
} from "./billingPipelineHelpers";

defineOptions({ name: "BillingPipelinePanel" });

const CHART_METRICS = [
  "billing_outbox_pending",
  "billing_oldest_pending_age",
  "billing_charge_failures",
  "billing_dead_letters",
  "billing_charge_lag_p50_ms",
  "billing_charge_lag_p99_ms",
  "billing_charge_items",
  "billing_charge_batches",
  "billing_deductions",
  "balance_precheck",
] as const;
type ChartMetric = (typeof CHART_METRICS)[number];

const { t } = useI18n();
const { resolveStep } = useStep();
const { timeRange, reloadTick } = useOpsSharedRange();

const chartWindow = computed(() => ({
  start: timeRange.value.start,
  end: timeRange.value.end,
}));

const loading = ref(false);
const failuresLoading = ref(false);
const deadLoading = ref(false);
const funnel = ref<BillingFunnelPg | null>(null);
const failures = ref<ChargeFailureRow[]>([]);
const deadLetters = ref<DeadLetterRow[]>([]);
const failuresTotal = ref(0);
const deadLettersTotal = ref(0);
const failurePage = ref(1);
const failurePageSize = ref(20);
const deadPage = ref(1);
const deadPageSize = ref(20);

const series = ref<Record<ChartMetric, MetricSeries | null>>(
  Object.fromEntries(CHART_METRICS.map((m) => [m, null])) as Record<
    ChartMetric,
    MetricSeries | null
  >,
);

const failurePagination = computed<PaginationConfig>(() => ({
  current: failurePage.value,
  size: failurePageSize.value,
  total: failuresTotal.value,
}));

const deadPagination = computed<PaginationConfig>(() => ({
  current: deadPage.value,
  size: deadPageSize.value,
  total: deadLettersTotal.value,
}));

const funnelRows = computed<FunnelStage[]>(() => funnel.value?.stages ?? []);
const funnelTitle = computed(() =>
  funnel.value?.mode === "sync_pass_through"
    ? t("ops.billing.pipelineHealth")
    : t("ops.billing.funnel"),
);
const funnelHint = computed(() => {
  if (funnel.value?.mode === "sync_pass_through") {
    return t("ops.billing.noBacklog");
  }
  if (funnel.value?.mode === "async_funnel") {
    return t("ops.billing.asyncFunnelHint");
  }
  return undefined;
});

const formatRatio = (ratio: number | null | undefined) => {
  if (ratio === null || ratio === undefined || Number.isNaN(ratio)) return "—";
  return `${(ratio * 100).toFixed(1)}%`;
};

const stageLabel = (stage: FunnelStage) =>
  formatPipelineStageLabel(stage.name || stage.key, t);

const funnelColumns = computed<ColumnOption<FunnelStage>[]>(() => [
  {
    prop: "name",
    label: t("ops.billing.stage"),
    minWidth: 180,
    formatter: (row: FunnelStage) => stageLabel(row),
  },
  {
    prop: "count",
    label: t("ops.billing.count"),
    minWidth: 100,
  },
  {
    prop: "ratio_from_prev",
    label: t("ops.billing.ratioFromPrev"),
    minWidth: 120,
    formatter: (row: FunnelStage) => formatRatio(row.ratio_from_prev),
  },
  {
    prop: "failed",
    label: t("ops.billing.stageFailed"),
    minWidth: 92,
  },
]);

function latest(metric: ChartMetric, reduce: "sum" | "max"): number | null {
  return aggregateLastMetricValue(series.value[metric], reduce);
}

const lagSeries = computed<MetricSeries | null>(() => {
  const p50 = series.value.billing_charge_lag_p50_ms;
  const p99 = series.value.billing_charge_lag_p99_ms;
  if (!p50 && !p99) return null;
  const tag = (data: MetricSeries | null, quantile: string) =>
    (data?.series ?? []).map((s) => ({
      ...s,
      labels: { ...s.labels, quantile },
    }));
  return {
    metric: "billing_charge_lag_ms",
    label: t("ops.billing.lagTrend"),
    unit: "ms",
    series: [...tag(p50, "p50"), ...tag(p99, "p99")],
  };
});

const batchThroughputSeries = computed<MetricSeries | null>(() => {
  const items = series.value.billing_charge_items;
  const batches = series.value.billing_charge_batches;
  if (!items && !batches) return null;
  const tag = (data: MetricSeries | null, kind: string) =>
    (data?.series ?? []).map((s) => ({
      ...s,
      labels: { ...s.labels, kind },
    }));
  return {
    metric: "billing_charge_throughput",
    label: t("ops.billing.batchThroughput"),
    unit: items?.unit ?? batches?.unit ?? "",
    series: [...tag(items, "items"), ...tag(batches, "batches")],
  };
});

const trendCharts = computed(() => [
  {
    metric: "failures",
    title: "ops.billing.failureTrend",
    data: series.value.billing_charge_failures,
  },
  {
    metric: "dead",
    title: "ops.billing.deadLetterTrend",
    data: series.value.billing_dead_letters,
  },
  { metric: "lag", title: "ops.billing.lagTrend", data: lagSeries.value },
  {
    metric: "throughput",
    title: "ops.billing.batchThroughput",
    data: batchThroughputSeries.value,
  },
  {
    metric: "deductions",
    title: "ops.billing.deductions",
    data: series.value.billing_deductions,
  },
  {
    metric: "precheck",
    title: "ops.billing.precheck",
    data: series.value.balance_precheck,
  },
]);

const statCards = computed(() => [
  {
    key: "backlog",
    label: t("ops.billing.backlog"),
    value: latest("billing_outbox_pending", "sum"),
    unit: "",
    digits: 0,
    icon: "ri:inbox-archive-line",
    iconColor: "var(--el-color-primary)",
    iconBackground: "var(--el-color-primary-light-9)",
  },
  {
    key: "throughput",
    label: t("ops.billing.throughputNow"),
    value: latest("billing_charge_items", "sum"),
    unit: t("ops.units.perSec"),
    digits: 2,
    icon: "ri:speed-up-line",
    iconColor: "var(--el-color-success)",
    iconBackground: "var(--el-color-success-light-9)",
  },
  {
    key: "failures",
    label: t("ops.billing.failureCount"),
    value: funnel.value?.charge_failures ?? null,
    unit: "",
    digits: 0,
    icon: "ri:refresh-line",
    iconColor: "var(--el-color-warning)",
    iconBackground: "var(--el-color-warning-light-9)",
  },
  {
    key: "deadLetters",
    label: t("ops.billing.deadLettersNow"),
    value: latest("billing_dead_letters", "sum"),
    unit: "",
    digits: 0,
    icon: "ri:error-warning-line",
    iconColor: "var(--el-color-danger)",
    iconBackground: "var(--el-color-danger-light-9)",
  },
  {
    key: "lagP99",
    label: t("ops.billing.lagP99"),
    value: latest("billing_charge_lag_p99_ms", "max"),
    unit: "ms",
    digits: 1,
    icon: "ri:timer-flash-line",
    iconColor: "var(--el-color-info)",
    iconBackground: "var(--el-color-info-light-9)",
  },
]);

const failureColumns = computed<ColumnOption<ChargeFailureRow>[]>(() => [
  {
    prop: "created_at",
    label: t("ops.billing.columns.time"),
    minWidth: 164,
    formatter: (row: ChargeFailureRow) => formatDateTime(row.created_at),
  },
  {
    prop: "tenant_id",
    label: t("ops.billing.columns.tenant"),
    minWidth: 140,
    showOverflowTooltip: true,
  },
  {
    prop: "queue_id",
    label: t("ops.billing.columns.queueId"),
    minWidth: 140,
    showOverflowTooltip: true,
  },
  {
    prop: "attempt_number",
    label: t("ops.billing.columns.attempt"),
    minWidth: 92,
  },
  {
    prop: "queue_status",
    label: t("ops.billing.columns.status"),
    minWidth: 96,
    formatter: (row: ChargeFailureRow) =>
      row.queue_status ? getStatusLabel(row.queue_status) : "—",
  },
  {
    prop: "error_message",
    label: t("ops.billing.columns.error"),
    minWidth: 180,
    showOverflowTooltip: true,
  },
]);

const deadColumns = computed<ColumnOption<DeadLetterRow>[]>(() => [
  {
    prop: "created_at",
    label: t("ops.billing.columns.time"),
    minWidth: 164,
    formatter: (row: DeadLetterRow) => formatDateTime(row.created_at),
  },
  {
    prop: "source",
    label: t("ops.billing.columns.source"),
    minWidth: 156,
    formatter: (row: DeadLetterRow) =>
      formatPipelineStageLabel(row.source, t),
  },
  {
    prop: "id",
    label: t("ops.billing.columns.id"),
    minWidth: 140,
    showOverflowTooltip: true,
  },
  {
    prop: "tenant_id",
    label: t("ops.billing.columns.tenant"),
    minWidth: 140,
    showOverflowTooltip: true,
  },
  {
    prop: "retry_count",
    label: t("ops.billing.columns.attempt"),
    minWidth: 92,
  },
  {
    prop: "error_message",
    label: t("ops.billing.columns.error"),
    minWidth: 180,
    showOverflowTooltip: true,
  },
]);

async function loadFailures() {
  failuresLoading.value = true;
  try {
    const res = await fetchBillingFailures(
      buildFailuresQueueQuery(
        toOpsRange(timeRange.value),
        failurePage.value,
        failurePageSize.value,
      ),
    );
    failures.value = res.failures ?? [];
    failuresTotal.value = res.failures_total ?? 0;
  } catch {
    ElMessage.error(t("ops.billing.loadFailed"));
  } finally {
    failuresLoading.value = false;
  }
}

async function loadDeadLetters() {
  deadLoading.value = true;
  try {
    const res = await fetchBillingFailures(
      buildDeadLettersQueueQuery(
        toOpsRange(timeRange.value),
        deadPage.value,
        deadPageSize.value,
      ),
    );
    deadLetters.value = res.dead_letters ?? [];
    deadLettersTotal.value = res.dead_letters_total ?? 0;
  } catch {
    ElMessage.error(t("ops.billing.loadFailed"));
  } finally {
    deadLoading.value = false;
  }
}

function handleFailurePageChange(page: number) {
  failurePage.value = page;
  void loadFailures();
}

function handleFailureSizeChange(size: number) {
  failurePageSize.value = size;
  failurePage.value = 1;
  void loadFailures();
}

function handleDeadPageChange(page: number) {
  deadPage.value = page;
  void loadDeadLetters();
}

function handleDeadSizeChange(size: number) {
  deadPageSize.value = size;
  deadPage.value = 1;
  void loadDeadLetters();
}

async function reload() {
  loading.value = true;
  const start = timeRange.value.start;
  const end = timeRange.value.end;
  const step = resolveStep(end - start);
  const opsRange = toOpsRange(timeRange.value);
  failurePage.value = 1;
  deadPage.value = 1;
  try {
    const [pg, fail, ...charts] = await Promise.all([
      fetchBillingFunnel(opsRange),
      fetchBillingFailures({
        range: opsRange,
        limit: Math.max(failurePageSize.value, deadPageSize.value),
        failed_offset: 0,
        dlq_offset: 0,
      }),
      ...CHART_METRICS.map((metric) =>
        fetchMetricTimeseries({ metric, start, end, step }),
      ),
    ]);
    funnel.value = pg;
    failures.value = (fail.failures ?? []).slice(0, failurePageSize.value);
    deadLetters.value = (fail.dead_letters ?? []).slice(0, deadPageSize.value);
    failuresTotal.value = fail.failures_total ?? 0;
    deadLettersTotal.value = fail.dead_letters_total ?? 0;
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

watch(
  [
    () => timeRange.value.start,
    () => timeRange.value.end,
    () => timeRange.value.preset,
    reloadTick,
  ],
  () => {
    void reload();
  },
);

onMounted(() => {
  void reload();
});
</script>

<style scoped>
.ops-billing-pipeline {
  display: flex;
  flex-direction: column;
  gap: var(--ops-section-gap, 16px);
  min-width: 0;
}

.ops-billing-pipeline__kpi-grid {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 16px;
}

@media (max-width: 1200px) {
  .ops-billing-pipeline__kpi-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}

@media (max-width: 768px) {
  .ops-billing-pipeline__kpi-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

.ops-billing-pipeline__stacked-col {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.ops-billing-pipeline__stacked-col :deep(.ops-card) {
  flex: 1;
}
</style>
