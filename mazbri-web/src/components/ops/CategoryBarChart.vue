<template>
  <div v-loading="loading" class="ops-category-bar">
    <EmptyState
      v-if="!loading && isEmpty"
      compact
      :description="resolvedEmptyText"
    />
    <div
      v-show="!isEmpty"
      ref="chartRef"
      class="ops-category-bar__canvas"
    ></div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import EmptyState from "@/components/common/empty-state/index.vue";
import { echarts, type EChartsOption } from "@/plugins/echarts";
import {
  OPS_CHART_GRID,
  bindOpsChartResize,
  formatOpsMetricValue,
  getOpsChartThemeTokens,
  useOpsChartTheme,
  type OpsChartUnitType,
} from "@/composables/ops/useOpsChartHost";

export interface BarSeries {
  name: string;
  data: number[];
  stack?: string;
}

const props = withDefaults(
  defineProps<{
    categories?: string[];
    series?: BarSeries[];
    loading?: boolean;
    height?: string;
    horizontal?: boolean;
    unitType?: OpsChartUnitType;
    unit?: string;
    emptyText?: string;
  }>(),
  {
    categories: () => [],
    series: () => [],
    loading: false,
    height: "280px",
    horizontal: false,
    unitType: "raw",
    unit: "",
    emptyText: undefined,
  },
);

const { t, locale } = useI18n();
const chartRef = ref<HTMLElement | null>(null);
let chart: echarts.ECharts | null = null;
let unbindResize: (() => void) | null = null;

const { isDark } = useOpsChartTheme(() => render());

const isEmpty = computed(
  () =>
    props.categories.length === 0 ||
    props.series.length === 0 ||
    props.series.every((s) => (s.data?.length ?? 0) === 0),
);

const resolvedEmptyText = computed(
  () => props.emptyText ?? t("ops.charts.empty"),
);

function buildOption(): EChartsOption {
  const tokens = getOpsChartThemeTokens(isDark.value);
  const uType = props.unitType;
  const catAxis = {
    type: "category" as const,
    data: props.categories,
    axisLine: { lineStyle: { color: tokens.axisLineColor } },
    axisTick: { lineStyle: { color: tokens.axisLineColor } },
    axisLabel: {
      color: tokens.textColor,
      hideOverlap: true,
      overflow: "truncate" as const,
      width: props.horizontal ? 96 : undefined,
    },
  };
  const valueAxis = {
    type: "value" as const,
    ...(uType === "count" ? { minInterval: 1 } : {}),
    axisLine: { show: false },
    axisTick: { show: false },
    splitLine: { lineStyle: { color: tokens.splitLineColor } },
    axisLabel: {
      color: tokens.textColor,
      hideOverlap: true,
      formatter: (v: number) =>
        formatOpsMetricValue(v, uType, props.unit, locale.value, true),
    },
  };
  return {
    tooltip: {
      trigger: "axis",
      axisPointer: { type: "shadow" },
      backgroundColor: tokens.tooltipBg,
      borderColor: tokens.tooltipBorder,
      textStyle: { color: tokens.titleColor },
      valueFormatter: (v: unknown) =>
        typeof v === "number"
          ? formatOpsMetricValue(v, uType, props.unit, locale.value, false)
          : String(v ?? "—"),
    },
    legend: {
      show: props.series.length > 0,
      type: "scroll",
      top: 0,
      right: 0,
      left: 84,
      icon: "roundRect",
      itemWidth: 10,
      itemHeight: 8,
      padding: [2, 0, 2, 0],
      textStyle: { color: tokens.textColor, fontSize: 12 },
      pageTextStyle: { color: tokens.textColor },
      pageIconColor: tokens.titleColor,
      pageIconInactiveColor: tokens.axisLineColor,
    },
    grid: { ...OPS_CHART_GRID },
    xAxis: props.horizontal ? valueAxis : catAxis,
    yAxis: props.horizontal ? catAxis : valueAxis,
    series: props.series.map((s) => ({
      name: s.name,
      type: "bar",
      stack: s.stack,
      data: s.data,
      emphasis: { focus: "series" },
    })),
  };
}

function ensureChart() {
  if (!chartRef.value) return null;
  if (!chart) {
    chart = echarts.init(chartRef.value);
    unbindResize = bindOpsChartResize(chartRef.value, () => chart);
  }
  return chart;
}

function render() {
  if (isEmpty.value) {
    if (chart) {
      chart.clear();
    }
    return;
  }
  const instance = ensureChart();
  if (!instance) return;
  instance.setOption(buildOption(), { notMerge: true });
}

onMounted(() => {
  render();
});

onUnmounted(() => {
  unbindResize?.();
  unbindResize = null;
  chart?.dispose();
  chart = null;
});

watch(
  () =>
    [
      props.categories,
      props.series,
      props.loading,
      props.unitType,
      props.unit,
      isEmpty.value,
      locale.value,
    ] as const,
  () => render(),
  { deep: true },
);
</script>

<style scoped>
.ops-category-bar {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: v-bind(height);
  overflow: hidden;
}
.ops-category-bar__canvas {
  width: 100%;
  height: 100%;
}
</style>
