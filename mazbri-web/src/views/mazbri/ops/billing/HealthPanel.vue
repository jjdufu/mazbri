<template>
  <div class="ops-billing-panel">
    <OpsPanelToolbar>
      <TimeRangePicker v-model="timeRange" @change="reload" />
      <AutoRefresh v-model="refreshInterval" @tick="reload" />
    </OpsPanelToolbar>

    <ElRow :gutter="16">
      <ElCol
        v-for="card in statCards"
        :key="card.label"
        :xs="12"
        :sm="8"
        :lg="4"
      >
        <StatCard
          :label="card.label"
          :value="card.value"
          :unit="card.unit"
          :loading="loading"
          :digits="card.digits"
        />
      </ElCol>
    </ElRow>

    <ElRow :gutter="16">
      <ElCol :xs="24" :lg="12">
        <OpsCard :title="t('ops.billing.funnel')">
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
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="12">
        <OpsCard :title="t('ops.billing.queueTrend')">
          <MultiSeriesChart
            :series="series.billing_outbox_pending"
            :loading="loading"
            height="200px"
          />
        </OpsCard>
        <OpsCard :title="t('ops.billing.oldestAge')">
          <MultiSeriesChart
            :series="series.billing_oldest_pending_age"
            :loading="loading"
            height="200px"
          />
        </OpsCard>
      </ElCol>
      <ElCol v-for="chart in trendCharts" :key="chart.metric" :xs="24" :lg="8">
        <OpsCard :title="t(chart.title)">
          <MultiSeriesChart
            :series="chart.data"
            :loading="loading"
            height="220px"
          />
        </OpsCard>
      </ElCol>
    </ElRow>

    <OpsCard :title="t('ops.billing.failures')">
      <ArtTable
        :data="failures"
        :loading="loading"
        :columns="failureColumns"
        class="surface-panel-table"
        stripe
      />
      <div class="ops-traffic__pager">
        <ElPagination
          v-model:current-page="failurePage"
          :page-size="PAGE_SIZE"
          :total="failuresTotal"
          layout="total, prev, pager, next"
          @current-change="loadFailures"
        />
      </div>
    </OpsCard>
    <OpsCard :title="t('ops.billing.deadLetters')">
      <ArtTable
        :data="deadLetters"
        :loading="loading"
        :columns="deadColumns"
        class="surface-panel-table"
        stripe
      />
      <div class="ops-traffic__pager">
        <ElPagination
          v-model:current-page="deadPage"
          :page-size="PAGE_SIZE"
          :total="deadLettersTotal"
          layout="total, prev, pager, next"
          @current-change="loadDeadLetters"
        />
      </div>
    </OpsCard>
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
import OpsPanelToolbar from "@/components/ops/OpsPanelToolbar.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
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
import { aggregateLastMetricValue } from "@/utils/ops/metric-series";

defineOptions({ name: "OpsBillingHealthPanel" });

/** 趋势全部来自 analytics 指标目录（billing 自身暴露的 Prometheus 指标）。 */
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
const PAGE_SIZE = 20;

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
const failuresTotal = ref(0);
const deadLettersTotal = ref(0);
const failurePage = ref(1);
const deadPage = ref(1);
const series = ref<Record<ChartMetric, MetricSeries | null>>(
  Object.fromEntries(CHART_METRICS.map((m) => [m, null])) as Record<
    ChartMetric,
    MetricSeries | null
  >,
);

/** 四段漏斗：CH billable=1 → usage_outbox → usage_charge_queue → ledger_entries。 */
const funnelRows = computed<FunnelStage[]>(() => {
  const pg = funnel.value?.stages ?? [];
  const head: FunnelStage = {
    stage: "clickhouse_billable",
    count: chBillable.value,
    diff: 0,
  };
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

/** 每条序列取最后一个点后聚合；无数据为 null。 */
function latest(metric: ChartMetric, reduce: "sum" | "max"): number | null {
  return aggregateLastMetricValue(series.value[metric], reduce);
}

/** p50 / p99 两条序列合并到一张图，按分位数标注。 */
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
    metric: "items",
    title: "ops.billing.batchThroughput",
    data: series.value.billing_charge_items,
  },
  {
    metric: "batches",
    title: "ops.billing.batches",
    data: series.value.billing_charge_batches,
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
    label: t("ops.billing.backlog"),
    value: latest("billing_outbox_pending", "sum"),
    unit: "",
    digits: 0,
  },
  {
    label: t("ops.billing.oldestAgeNow"),
    value: latest("billing_oldest_pending_age", "max"),
    unit: "s",
    digits: 0,
  },
  {
    label: t("ops.billing.deadLettersNow"),
    value: latest("billing_dead_letters", "sum"),
    unit: "",
    digits: 0,
  },
  {
    label: t("ops.billing.lagP99"),
    value: latest("billing_charge_lag_p99_ms", "max"),
    unit: "ms",
    digits: 1,
  },
  {
    label: t("ops.billing.failureCount"),
    value: funnel.value?.charge_failures ?? null,
    unit: "",
    digits: 0,
  },
]);

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

function offsetOf(page: number) {
  return (page - 1) * PAGE_SIZE;
}

async function fetchPage(page: number) {
  return fetchBillingFailures(
    toOlapRange(timeRange.value),
    PAGE_SIZE,
    offsetOf(page),
  );
}

async function loadFailures() {
  try {
    const res = await fetchPage(failurePage.value);
    failures.value = res.failures ?? [];
    failuresTotal.value = res.failures_total ?? 0;
  } catch {
    ElMessage.error(t("ops.billing.loadFailed"));
  }
}

async function loadDeadLetters() {
  try {
    const res = await fetchPage(deadPage.value);
    deadLetters.value = res.dead_letters ?? [];
    deadLettersTotal.value = res.dead_letters_total ?? 0;
  } catch {
    ElMessage.error(t("ops.billing.loadFailed"));
  }
}

async function reload() {
  loading.value = true;
  const start = timeRange.value.start;
  const end = timeRange.value.end;
  const step = resolveStep(end - start);
  const range = toOlapRange(timeRange.value);
  failurePage.value = 1;
  deadPage.value = 1;
  try {
    const [ch, pg, fail, ...charts] = await Promise.all([
      postOlapQuery<FunnelChRow>({
        template: "billing_funnel_ch",
        params: { range },
      }),
      fetchBillingFunnel(range),
      fetchPage(1),
      ...CHART_METRICS.map((metric) =>
        fetchMetricTimeseries({ metric, start, end, step }),
      ),
    ]);
    chBillable.value = ch.rows?.[0]?.billable ?? 0;
    funnel.value = pg;
    failures.value = fail.failures ?? [];
    deadLetters.value = fail.dead_letters ?? [];
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

onMounted(reload);
</script>

<style scoped>
.ops-traffic__pager {
  display: flex;
  justify-content: flex-end;
  margin-top: 8px;
}
</style>
