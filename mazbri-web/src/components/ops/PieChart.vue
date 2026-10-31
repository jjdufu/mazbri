<template>
  <div ref="chartRef" v-loading="loading" class="ops-pie-chart"></div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { echarts, type EChartsOption } from "@/plugins/echarts";

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

function buildOption(): EChartsOption {
  return {
    tooltip: { trigger: "item" },
    legend: { type: "scroll", bottom: 0 },
    series: [
      {
        type: "pie",
        radius: ["35%", "65%"],
        data: props.slices,
        emphasis: {
          itemStyle: { shadowBlur: 8, shadowOffsetX: 0, shadowColor: "rgba(0,0,0,0.2)" },
        },
      },
    ],
  };
}

function render() {
  if (!chartRef.value) return;
  if (!chart) chart = echarts.init(chartRef.value);
  chart.setOption(buildOption(), true);
}
onMounted(render);
onUnmounted(() => {
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
}
</style>
