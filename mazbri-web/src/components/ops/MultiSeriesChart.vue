<template>
  <div ref="chartRef" v-loading="loading" class="ops-multi-series-chart"></div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { echarts, type EChartsOption } from "@/plugins/echarts";
import type { MetricSeries } from "@/types/api/monitoring";

const props = withDefaults(
  defineProps<{
    series?: MetricSeries | null;
    loading?: boolean;
    height?: string;
  }>(),
  {
    series: null,
    loading: false,
    height: "280px",
  },
);

const emit = defineEmits<{
  pointClick: [{ timestamp: number; value: number; seriesIndex: number }];
}>();

const chartRef = ref<HTMLElement | null>(null);
let chart: echarts.ECharts | null = null;

function buildOption(data: MetricSeries | null): EChartsOption {
  const seriesList = data?.series ?? [];
  const legend = seriesList.map((s, i) => {
    const label = Object.values(s.labels || {}).filter(Boolean).join(" / ");
    return label || `${data?.label || data?.metric || "series"}#${i + 1}`;
  });
  return {
    tooltip: { trigger: "axis" },
    legend: { data: legend, type: "scroll" },
    grid: { left: 48, right: 24, top: 40, bottom: 32 },
    xAxis: {
      type: "time",
      axisLabel: {
        formatter: (v: number) => {
          const d = new Date(v);
          return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
        },
      },
    },
    yAxis: {
      type: "value",
      name: data?.unit || "",
      scale: true,
    },
    series: seriesList.map((s, i) => ({
      name: legend[i],
      type: "line",
      showSymbol: false,
      smooth: true,
      data: (s.points || []).map((p) => [p.timestamp * 1000, p.value]),
    })),
  };
}

function render() {
  if (!chartRef.value) return;
  if (!chart) {
    chart = echarts.init(chartRef.value);
    chart.on("click", (params) => {
      const ts = Array.isArray(params.value) ? Number(params.value[0]) : NaN;
      const val = Array.isArray(params.value) ? Number(params.value[1]) : NaN;
      if (Number.isFinite(ts) && Number.isFinite(val)) {
        emit("pointClick", {
          timestamp: Math.floor(ts / 1000),
          value: val,
          seriesIndex: params.seriesIndex ?? 0,
        });
      }
    });
  }
  chart.setOption(buildOption(props.series), true);
}

function onResize() {
  chart?.resize();
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

watch(
  () => props.series,
  () => render(),
  { deep: true },
);
</script>

<style scoped>
.ops-multi-series-chart {
  width: 100%;
  height: v-bind(height);
  min-height: 200px;
}
</style>
