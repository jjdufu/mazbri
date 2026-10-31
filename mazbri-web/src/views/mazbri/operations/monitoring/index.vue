<template>
  <ControlPanelPageShell
    :title="$t('menus.mazbri.serviceMonitoring')"
    icon="ri:pulse-line"
  >
    <div class="monitoring-page" style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
      <ResourceCardShell
      :title="$t('menus.mazbri.serviceMonitoring')"
      stack-header-on-mobile
    >
      <template #actions>
        <TimeRangePicker v-model="timeRange" @change="reloadData" />
        <AutoRefresh v-model="refreshInterval" @tick="reloadData" />
        <ElSelect v-model="service" class="service-select" @change="reloadData">
          <ElOption
            :label="$t('operations.monitoring.allServices')"
            value="all"
          />
          <ElOption
            v-for="item in catalog?.services || []"
            :key="item.key"
            :label="item.name"
            :value="item.key"
          />
        </ElSelect>
      </template>

      <ElRow :gutter="16" class="summary-grid">
        <ElCol
          v-for="item in summaryItems"
          :key="item.label"
          :xs="12"
          :sm="8"
          :lg="6"
        >
          <MetricSummaryCard
            :label="item.label"
            :icon="item.icon"
            :icon-color="item.color"
            :icon-background="item.background"
            :loading="loading"
          >
            <span class="metric-value">{{ item.value }}</span>
            <span class="metric-unit">{{ item.unit }}</span>
          </MetricSummaryCard>
        </ElCol>
      </ElRow>

      <ElRow :gutter="16" class="chart-grid">
        <ElCol :xs="24" :lg="8">
          <section class="monitor-section">
            <CommonSectionTitle
              :title="$t('operations.monitoring.sections.qps')"
            />
            <div ref="qpsChartRef" v-loading="loading" class="chart-box"></div>
          </section>
        </ElCol>
        <ElCol :xs="24" :lg="8">
          <section class="monitor-section">
            <CommonSectionTitle
              :title="$t('operations.monitoring.sections.p95Latency')"
            />
            <div
              ref="latencyChartRef"
              v-loading="loading"
              class="chart-box"
            ></div>
          </section>
        </ElCol>
        <ElCol :xs="24" :lg="8">
          <section class="monitor-section">
            <CommonSectionTitle
              :title="$t('operations.monitoring.sections.errorRate')"
            />
            <div
              ref="errorChartRef"
              v-loading="loading"
              class="chart-box"
            ></div>
          </section>
        </ElCol>
      </ElRow>

      <section class="monitor-section">
        <CommonSectionTitle
          :title="$t('operations.monitoring.sections.targets')"
        />
        <ArtTable
          :data="catalog?.services || []"
          :loading="loading"
          :columns="serviceColumns"
          class="surface-panel-table"
          stripe
        >
        </ArtTable>
      </section>
    </ResourceCardShell>
    </div>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import { echarts, graphic, type EChartsOption } from "@/plugins/echarts";
import MetricSummaryCard from "@/components/common/metric-summary-card/index.vue";
import ResourceCardShell from "@/components/common/resource-card-shell/index.vue";
import CommonSectionTitle from "@/components/common/section-title/index.vue";
import {
  fetchMetricTimeseries,
  fetchMetricsCatalog,
  fetchMetricsOverview,
} from "@/api/monitoring";
import type {
  BillingQueueStatus,
  MetricOverview,
  MetricSeries,
  MetricsCatalog,
} from "@/types/api/monitoring";
import { $t } from "@/locales";
import type { ColumnOption } from "@/types/component";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
defineOptions({ name: "ServiceMonitoring" });
const loading = ref(false);
const overview = ref<MetricOverview | null>(null);
const catalog = ref<MetricsCatalog | null>(null);
const service = ref("all");
const refreshInterval = ref(0);
const timeRange = ref<TimeRangeValue>({
  preset: "1h",
  start: Math.floor(Date.now() / 1000) - 3600,
  end: Math.floor(Date.now() / 1000),
});
const qpsChartRef = ref<HTMLElement>();
const latencyChartRef = ref<HTMLElement>();
const errorChartRef = ref<HTMLElement>();
let qpsChart: echarts.ECharts | null = null;
let latencyChart: echarts.ECharts | null = null;
let errorChart: echarts.ECharts | null = null;
const emptyCatalog: MetricsCatalog = {
  services: [],
  metrics: [],
};
const emptyOverview: MetricOverview = {
  qps: 0,
  error_rate: 0,
  error_rate_4xx: 0,
  error_rate_5xx: 0,
  p95_latency_ms: 0,
  p99_latency_ms: 0,
  active_connections: 0,
  proxy_active_connections: 0,
  rate_limit_rejections_per_sec: 0,
  billing_queue: { pending: 0, processing: 0, failed: 0, dead_letter: 0 },
  cert_min_expiry_days: 0,
};
const billingQueueStatuses: BillingQueueStatus[] = [
  "pending",
  "processing",
  "failed",
  "dead_letter",
];
const serviceColumns = computed<
  ColumnOption<MetricsCatalog["services"][number]>[]
>(() => [
  {
    prop: "name",
    label: $t("operations.monitoring.columns.service"),
    minWidth: 100,
  },
  { prop: "key", label: $t("operations.monitoring.columns.key"), width: 140 },
  { prop: "port", label: $t("operations.monitoring.columns.port"), width: 110 },
  {
    prop: "endpoint",
    label: $t("operations.monitoring.columns.endpoint"),
    minWidth: 150,
    showOverflowTooltip: true,
  },
]);
const summaryItems = computed(() => [
  {
    label: $t("operations.monitoring.metrics.currentQps"),
    value: formatNumber(overview.value?.qps),
    unit: $t("operations.monitoring.units.reqPerSec"),
    icon: "ri:pulse-line",
    color: "#409eff",
    background: "rgba(64, 158, 255, 0.1)",
  },
  {
    label: $t("operations.monitoring.metrics.errorRate"),
    value: formatNumber(overview.value?.error_rate),
    unit: $t("operations.monitoring.units.percent"),
    icon: "ri:error-warning-line",
    color: "#f56c6c",
    background: "rgba(245, 108, 108, 0.1)",
  },
  {
    label: $t("operations.monitoring.metrics.p95Latency"),
    value: formatNumber(overview.value?.p95_latency_ms),
    unit: $t("operations.monitoring.units.milliseconds"),
    icon: "ri:timer-flash-line",
    color: "#e6a23c",
    background: "rgba(230, 162, 60, 0.1)",
  },
  {
    label: $t("operations.monitoring.metrics.p99Latency"),
    value: formatNumber(overview.value?.p99_latency_ms),
    unit: $t("operations.monitoring.units.milliseconds"),
    icon: "ri:speed-up-line",
    color: "#626aef",
    background: "rgba(98, 106, 239, 0.1)",
  },
  {
    label: $t("operations.monitoring.metrics.activeConnections"),
    value: formatNumber(overview.value?.active_connections, 0),
    unit: $t("operations.monitoring.units.connections"),
    icon: "ri:links-line",
    color: "#67c23a",
    background: "rgba(103, 194, 58, 0.1)",
  },
  {
    label: $t("operations.monitoring.metrics.proxyConnections"),
    value: formatNumber(overview.value?.proxy_active_connections, 0),
    unit: $t("operations.monitoring.units.connections"),
    icon: "ri:route-line",
    color: "#14b8a6",
    background: "rgba(20, 184, 166, 0.1)",
  },
  {
    label: $t("operations.monitoring.metrics.rateLimitRejections"),
    value: formatNumber(overview.value?.rate_limit_rejections_per_sec),
    unit: $t("operations.monitoring.units.opsPerSec"),
    icon: "ri:stop-circle-line",
    color: "#fa8a6c",
    background: "rgba(250, 138, 108, 0.1)",
  },
  ...billingQueueStatuses.map((status) => ({
    label: $t("operations.monitoring.metrics.billingQueue", { status }),
    value: formatNumber(overview.value?.billing_queue?.[status], 0),
    unit: $t("operations.monitoring.units.jobs"),
    icon: status === "dead_letter" ? "ri:skull-line" : "ri:stack-line",
    color: status === "dead_letter" ? "#f56c6c" : "#8b5cf6",
    background:
      status === "dead_letter"
        ? "rgba(245, 108, 108, 0.1)"
        : "rgba(139, 92, 246, 0.1)",
  })),
  {
    label: $t("operations.monitoring.metrics.certMinExpiry"),
    value: formatNumber(overview.value?.cert_min_expiry_days, 0),
    unit: $t("operations.monitoring.units.days"),
    icon: "ri:shield-check-line",
    color: "#0ea5e9",
    background: "rgba(14, 165, 233, 0.1)",
  },
]);
const reloadData = async () => {
  loading.value = true;
  const start = timeRange.value.start;
  const end = timeRange.value.end;
  const serviceParam = service.value === "all" ? undefined : service.value;
  const [overviewRes, catalogRes, qps, latency, errors] =
    await Promise.allSettled([
      fetchMetricsOverview(),
      fetchMetricsCatalog(),
      fetchMetricTimeseries({
        metric: "qps",
        start,
        end,
        service: serviceParam,
      }),
      fetchMetricTimeseries({
        metric: "p95_latency_ms",
        start,
        end,
        service: serviceParam,
      }),
      fetchMetricTimeseries({
        metric: "error_rate",
        start,
        end,
        service: serviceParam,
      }),
    ]);
  overview.value = settledValue(overviewRes, emptyOverview);
  catalog.value = settledValue(catalogRes, emptyCatalog);
  await nextTick();
  renderChart(
    "qps",
    qpsChartRef.value,
    settledValue(
      qps,
      emptySeries(
        "qps",
        $t("operations.monitoring.metrics.currentQps"),
        $t("operations.monitoring.units.reqPerSec"),
      ),
    ),
    "#409eff",
  );
  renderChart(
    "latency",
    latencyChartRef.value,
    settledValue(
      latency,
      emptySeries(
        "p95_latency_ms",
        $t("operations.monitoring.metrics.p95Latency"),
        $t("operations.monitoring.units.milliseconds"),
      ),
    ),
    "#e6a23c",
  );
  renderChart(
    "error",
    errorChartRef.value,
    settledValue(
      errors,
      emptySeries(
        "error_rate",
        $t("operations.monitoring.metrics.errorRate"),
        $t("operations.monitoring.units.percent"),
      ),
    ),
    "#f56c6c",
  );
  loading.value = false;
};
const settledValue = <T,>(result: PromiseSettledResult<T>, fallback: T) =>
  result.status === "fulfilled" ? result.value : fallback;
const emptySeries = (
  metric: string,
  label: string,
  unit: string,
): MetricSeries => ({
  metric,
  label,
  unit,
  series: [],
});
const renderChart = (
  chartName: "qps" | "latency" | "error",
  element: HTMLElement | undefined,
  series: MetricSeries,
  color: string,
) => {
  if (!element) return;
  const chart = getChart(chartName, element);
  chart.setOption(buildLineOption(series, color), true);
};
const getChart = (
  chartName: "qps" | "latency" | "error",
  element: HTMLElement,
) => {
  if (chartName === "qps") {
    qpsChart ||= echarts.init(element);
    return qpsChart;
  }
  if (chartName === "latency") {
    latencyChart ||= echarts.init(element);
    return latencyChart;
  }
  errorChart ||= echarts.init(element);
  return errorChart;
};
const buildLineOption = (
  series: MetricSeries,
  color: string,
): EChartsOption => {
  const points = series.series[0]?.points ?? [];
  const isEmpty = points.length === 0;
  const xAxisData = points.map((point) =>
    new Intl.DateTimeFormat(undefined, {
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(point.timestamp * 1000)),
  );
  return {
    title: isEmpty
      ? {
          text: $t("operations.monitoring.noData"),
          left: "center",
          top: "middle",
          textStyle: {
            color: "#909399",
            fontSize: 14,
            fontWeight: 400,
          },
        }
      : undefined,
    tooltip: {
      trigger: "axis",
      valueFormatter: (value) =>
        `${formatNumber(Number(value))} ${series.unit}`,
    },
    grid: {
      top: 24,
      right: 18,
      bottom: 24,
      left: 42,
      containLabel: true,
    },
    xAxis: {
      type: "category",
      boundaryGap: false,
      data: xAxisData,
      axisTick: { show: false },
    },
    yAxis: {
      type: "value",
      name: series.unit,
      min: 0,
      axisTick: { show: false },
      splitLine: { lineStyle: { type: "dashed", color: "#e5e7eb" } },
    },
    series: [
      {
        name: series.label,
        type: "line",
        smooth: true,
        symbol: "none",
        data: points.map((point) => Number(point.value.toFixed(3))),
        lineStyle: { width: 2, color },
        areaStyle: {
          color: new graphic.LinearGradient(0, 0, 0, 1, [
            { offset: 0, color: `${color}33` },
            { offset: 1, color: `${color}05` },
          ]),
        },
      },
    ],
  };
};
const formatNumber = (value?: number, digits = 2) => {
  if (value === undefined || value === null || Number.isNaN(value)) return "--";
  return value.toLocaleString(undefined, {
    maximumFractionDigits: digits,
    minimumFractionDigits: digits === 0 ? 0 : 0,
  });
};
const resizeCharts = () => {
  qpsChart?.resize();
  latencyChart?.resize();
  errorChart?.resize();
};
onMounted(() => {
  reloadData();
  window.addEventListener("resize", resizeCharts);
});
onBeforeUnmount(() => {
  window.removeEventListener("resize", resizeCharts);
  qpsChart?.dispose();
  latencyChart?.dispose();
  errorChart?.dispose();
});
</script>

<style scoped lang="scss">
.monitoring-page {
  // One outer card only: give its body a comfortable inset (shared default
  // for table cards is 0, which left tiles/charts flush with the border).
  > :deep(.surface-panel-card > .el-card__body) {
    padding: 16px;
  }

  // Metric tiles: borderless, subtle gray fill, no (hover) shadow.
  :deep(.metric-summary-card) {
    background: var(--art-gray-100) !important;
    border: none !important;
    box-shadow: none !important;

    &:hover {
      box-shadow: none !important;
    }

    .el-card__body {
      padding: 12px 16px;
    }
  }

  .service-select {
    width: 180px;
  }

  .summary-grid {
    row-gap: 16px;
  }

  .chart-grid {
    margin-top: 20px;
    row-gap: 16px;
  }

  .monitor-section {
    margin-top: 20px;
  }

  .chart-box {
    width: 100%;
    height: 280px;
    background: var(--art-gray-100);
    border-radius: 6px;
  }

  .metric-value {
    font-size: 24px;
    font-weight: 700;
    line-height: 1;
  }

  .metric-unit {
    margin-left: 6px;
    font-size: 12px;
    color: var(--el-text-color-secondary);
  }
}

@media (max-width: 768px) {
  .monitoring-page {
    .service-select {
      width: 100%;
    }
  }
}
</style>
