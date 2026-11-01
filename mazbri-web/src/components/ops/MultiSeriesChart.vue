<template>
  <div ref="chartRef" v-loading="loading" class="ops-multi-series-chart"></div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { echarts, type EChartsOption } from "@/plugins/echarts";
import type { MetricSeries } from "@/types/api/monitoring";

export interface PointClick {
  /** Unix seconds of the data point nearest to the click. */
  timestamp: number;
  value: number;
  seriesIndex: number;
}

export interface ChartAnnotation {
  /** Unix seconds */
  timestamp: number;
  label?: string;
  color?: string;
}

const props = withDefaults(
  defineProps<{
    series?: MetricSeries | null;
    loading?: boolean;
    height?: string;
    /** Vertical change markers (unix seconds). */
    annotations?: ChartAnnotation[];
    /** Bound via `@point-click`; when absent the chart is not clickable. */
    onPointClick?: (point: PointClick) => void;
  }>(),
  {
    series: null,
    loading: false,
    height: "280px",
    annotations: () => [],
    onPointClick: undefined,
  },
);

const chartRef = ref<HTMLElement | null>(null);
let chart: echarts.ECharts | null = null;

function buildOption(data: MetricSeries | null): EChartsOption {
  const seriesList = data?.series ?? [];
  const legend = seriesList.map((s, i) => {
    const label = Object.values(s.labels || {})
      .filter(Boolean)
      .join(" / ");
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
    series: seriesList.map((s, i) => {
      const base: Record<string, unknown> = {
        name: legend[i],
        type: "line",
        showSymbol: false,
        smooth: true,
        data: (s.points || []).map((p) => [p.timestamp * 1000, p.value]),
      };
      if (i === 0 && (props.annotations?.length ?? 0) > 0) {
        base.markLine = {
          symbol: "none",
          label: { formatter: (p: { name?: string }) => p.name || "", position: "insideEndTop" },
          data: (props.annotations || []).map((a) => ({
            xAxis: a.timestamp * 1000,
            name: a.label || "",
            lineStyle: { color: a.color || "#e6a23c", type: "dashed" },
          })),
        };
      }
      return base;
    }),
  };
}

/** Maps a click anywhere in the grid to the nearest data point in time (then value). */
function handleZrClick(x: number, y: number) {
  if (!chart || !props.onPointClick) return;
  if (!chart.containPixel({ gridIndex: 0 }, [x, y])) return;
  const [clickTs, clickVal] = chart.convertFromPixel({ gridIndex: 0 }, [
    x,
    y,
  ]) as number[];
  let best: PointClick | null = null;
  let bestDt = Infinity;
  let bestDv = Infinity;
  const seriesList = props.series?.series ?? [];
  for (const [seriesIndex, s] of seriesList.entries()) {
    for (const p of s.points || []) {
      const dt = Math.abs(p.timestamp * 1000 - clickTs);
      const dv = Math.abs(p.value - clickVal);
      if (dt < bestDt || (dt === bestDt && dv < bestDv)) {
        bestDt = dt;
        bestDv = dv;
        best = { timestamp: p.timestamp, value: p.value, seriesIndex };
      }
    }
  }
  if (best) props.onPointClick(best);
}

function render() {
  if (!chartRef.value) return;
  if (!chart) {
    chart = echarts.init(chartRef.value);
    chart.getZr().on("click", (event) => {
      if (props.onPointClick) handleZrClick(event.offsetX, event.offsetY);
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
