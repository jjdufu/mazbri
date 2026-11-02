<template>
  <div ref="chartRef" v-loading="loading" class="ops-pie-chart"></div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { echarts, type EChartsOption } from "@/plugins/echarts";
import { bindOpsChartResize } from "@/composables/ops/useOpsChartHost";

export interface PieSlice {
  name: string;
  value: number;
}

const props = withDefaults(
  defineProps<{
    slices?: PieSlice[];
    loading?: boolean;
    height?: string;
  }>(),
  { slices: () => [], loading: false, height: "280px" },
);

const chartRef = ref<HTMLElement | null>(null);
let chart: echarts.ECharts | null = null;
let unbindResize: (() => void) | null = null;

function buildOption(): EChartsOption {
  return {
    tooltip: { trigger: "item" },
    legend: { type: "scroll", bottom: 0, left: "center" },
    series: [
      {
        type: "pie",
        // Lift center and shrink radius so the ring clears the bottom legend.
        center: ["50%", "44%"],
        radius: ["32%", "56%"],
        data: props.slices,
        emphasis: {
          itemStyle: {
            shadowBlur: 8,
            shadowOffsetX: 0,
            shadowColor: "rgba(0,0,0,0.2)",
          },
        },
      },
    ],
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

onMounted(render);

onUnmounted(() => {
  unbindResize?.();
  unbindResize = null;
  chart?.dispose();
  chart = null;
});

watch(
  () => [props.slices, props.loading],
  () => render(),
  { deep: true },
);
</script>

<style scoped>
.ops-pie-chart {
  width: 100%;
  height: v-bind(height);
  overflow: hidden;
}
</style>
