<template>
  <div v-loading="loading" class="ops-multi-series-chart">
    <div
      v-if="!loading && isEmpty"
      class="ops-multi-series-chart__empty"
    >
      {{ emptyText }}
    </div>
    <div v-show="!isEmpty" ref="chartRef" class="ops-multi-series-chart__canvas"></div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { echarts, type EChartsOption } from "@/plugins/echarts";
import type { MetricSeries } from "@/types/api/monitoring";
import {
  OPS_CHART_GRID,
  bindOpsChartResize,
} from "@/composables/ops/useOpsChartHost";

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

const { t } = useI18n();
const chartRef = ref<HTMLElement | null>(null);
let chart: echarts.ECharts | null = null;
let unbindResize: (() => void) | null = null;

const pointCount = computed(() => {
  const seriesList = props.series?.series ?? [];
  return seriesList.reduce((sum, s) => sum + (s.points?.length ?? 0), 0);
});
const isEmpty = computed(() => pointCount.value === 0);
const fewPoints = computed(() => pointCount.value > 0 && pointCount.value <= 2);
const emptyText = computed(() => t("ops.charts.empty"));

function buildOption(data: MetricSeries | null): EChartsOption {
  const seriesList = data?.series ?? [];
  const legend = seriesList.map((s, i) => {
    const label = Object.values(s.labels || {})
      .filter(Boolean)
      .join(" / ");
    return label || `${data?.label || data?.metric || "series"}#${i + 1}`;
  });
  const sparse = fewPoints.value;
  return {
    tooltip: { trigger: "axis" },
    legend: { data: legend, type: "scroll", top: 0 },
    grid: { ...OPS_CHART_GRID },
    xAxis: {
      type: "time",
      axisLabel: {
        hideOverlap: true,
        formatter: (v: number) => {
          const d = new Date(v);
          return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
        },
      },
    },
    yAxis: {
      type: "value",
      name: data?.unit || "",
      // 单点/稀点禁止 scale 夸张放大；固定从 0 起。
      min: 0,
      scale: false,
      axisLabel: { hideOverlap: true },
    },
    series: seriesList.map((s, i) => {
      const base: Record<string, unknown> = {
        name: legend[i],
        type: "line",
        showSymbol: sparse,
        symbolSize: sparse ? 8 : 4,
        smooth: !sparse,
        data: (s.points || []).map((p) => [p.timestamp * 1000, p.value]),
      };
      if (i === 0 && (props.annotations?.length ?? 0) > 0) {
        base.markLine = {
          symbol: "none",
          label: {
            formatter: (p: { name?: string }) => p.name || "",
            position: "insideEndTop",
          },
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

function ensureChart() {
  if (!chartRef.value) return null;
  if (!chart) {
    chart = echarts.init(chartRef.value);
    chart.getZr().on("click", (event) => {
      if (props.onPointClick) handleZrClick(event.offsetX, event.offsetY);
    });
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
  instance.setOption(buildOption(props.series), true);
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
  () => [props.series, isEmpty.value] as const,
  () => render(),
  { deep: true },
);
</script>

<style scoped>
.ops-multi-series-chart {
  position: relative;
  width: 100%;
  height: v-bind(height);
  min-height: 200px;
  overflow: hidden;
}
.ops-multi-series-chart__canvas {
  width: 100%;
  height: 100%;
}
.ops-multi-series-chart__empty {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  color: var(--el-text-color-secondary);
  font-size: 13px;
}
</style>
