<template>
  <div ref="chartRef" v-loading="loading" class="ops-category-bar"></div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { echarts, type EChartsOption } from "@/plugins/echarts";
import {
  OPS_CHART_GRID,
  bindOpsChartResize,
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
  }>(),
  {
    categories: () => [],
    series: () => [],
    loading: false,
    height: "280px",
    horizontal: false,
  },
);

const chartRef = ref<HTMLElement | null>(null);
let chart: echarts.ECharts | null = null;
let unbindResize: (() => void) | null = null;

function buildOption(): EChartsOption {
  const catAxis = {
    type: "category" as const,
    data: props.categories,
    axisLabel: {
      hideOverlap: true,
      overflow: "truncate" as const,
      width: props.horizontal ? 96 : undefined,
    },
  };
  const valueAxis = {
    type: "value" as const,
    axisLabel: { hideOverlap: true },
  };
  return {
    tooltip: { trigger: "axis", axisPointer: { type: "shadow" } },
    legend: { type: "scroll", top: 0 },
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
  const instance = ensureChart();
  if (!instance) return;
  instance.setOption(buildOption(), true);
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
  () => [props.categories, props.series, props.loading],
  () => render(),
  { deep: true },
);
</script>

<style scoped>
.ops-category-bar {
  width: 100%;
  height: v-bind(height);
  overflow: hidden;
}
</style>
