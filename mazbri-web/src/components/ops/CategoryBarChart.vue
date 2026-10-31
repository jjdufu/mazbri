<template>
  <div ref="chartRef" v-loading="loading" class="ops-category-bar"></div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { echarts, type EChartsOption } from "@/plugins/echarts";

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

function buildOption(): EChartsOption {
  const catAxis = { type: "category" as const, data: props.categories };
  const valueAxis = { type: "value" as const };
  return {
    tooltip: { trigger: "axis", axisPointer: { type: "shadow" } },
    legend: { type: "scroll" },
    grid: { left: 48, right: 24, top: 40, bottom: 32 },
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

function render() {
  if (!chartRef.value) return;
  if (!chart) chart = echarts.init(chartRef.value);
  chart.setOption(buildOption(), true);
}
onMounted(() => {
  render();
  window.addEventListener("resize", () => chart?.resize());
});
onUnmounted(() => {
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
}
</style>
