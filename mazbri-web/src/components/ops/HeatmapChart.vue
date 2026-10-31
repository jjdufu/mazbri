<template>
  <div ref="chartRef" v-loading="loading" class="ops-heatmap-chart"></div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { echarts, type EChartsOption } from "@/plugins/echarts";

export interface HeatCell {
  x: string | number;
  y: string | number;
  value: number;
}

const props = withDefaults(
  defineProps<{
    cells?: HeatCell[];
    xLabels?: string[];
    yLabels?: string[];
    loading?: boolean;
    height?: string;
    valueName?: string;
  }>(),
  {
    cells: () => [],
    xLabels: () => [],
    yLabels: () => [],
    loading: false,
    height: "280px",
    valueName: "count",
  },
);

const chartRef = ref<HTMLElement | null>(null);
let chart: echarts.ECharts | null = null;

function buildOption(): EChartsOption {
  const xs = props.xLabels.length
    ? props.xLabels
    : [...new Set(props.cells.map((c) => String(c.x)))];
  const ys = props.yLabels.length
    ? props.yLabels
    : [...new Set(props.cells.map((c) => String(c.y)))];
  const xIndex = new Map(xs.map((v, i) => [v, i]));
  const yIndex = new Map(ys.map((v, i) => [v, i]));
  const data = props.cells
    .map((c) => {
      const xi = xIndex.get(String(c.x));
      const yi = yIndex.get(String(c.y));
      if (xi === undefined || yi === undefined) return null;
      return [xi, yi, c.value] as [number, number, number];
    })
    .filter((v): v is [number, number, number] => v !== null);
  const max = data.reduce((m, d) => Math.max(m, d[2]), 0);
  return {
    tooltip: {
      position: "top",
      formatter: (p: unknown) => {
        const item = p as { value?: number[] };
        const v = item.value;
        if (!v) return "";
        return `${xs[v[0]]} × ${ys[v[1]]}<br/>${props.valueName}: ${v[2]}`;
      },
    },
    grid: { left: 72, right: 24, top: 16, bottom: 48 },
    xAxis: {
      type: "category",
      data: xs,
      splitArea: { show: true },
      axisLabel: { hideOverlap: true },
    },
    yAxis: { type: "category", data: ys, splitArea: { show: true } },
    visualMap: {
      min: 0,
      max: Math.max(max, 1),
      calculable: true,
      orient: "horizontal",
      left: "center",
      bottom: 0,
      inRange: { color: ["#f0f5ff", "#409eff", "#1d39c4"] },
    },
    series: [
      {
        type: "heatmap",
        data,
        emphasis: { itemStyle: { shadowBlur: 6, shadowColor: "rgba(0,0,0,0.3)" } },
      },
    ],
  };
}

function render() {
  if (!chartRef.value) return;
  if (!chart) chart = echarts.init(chartRef.value);
  chart.setOption(buildOption(), true);
}

onMounted(() => {
  render();
  window.addEventListener("resize", onResize);
});
onUnmounted(() => {
  window.removeEventListener("resize", onResize);
  chart?.dispose();
  chart = null;
});
function onResize() {
  chart?.resize();
}
watch(
  () => [props.cells, props.xLabels, props.yLabels, props.loading],
  () => render(),
  { deep: true },
);
</script>

<style scoped>
.ops-heatmap-chart {
  width: 100%;
  height: v-bind(height);
}
</style>
