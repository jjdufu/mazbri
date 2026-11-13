<template>
  <div v-loading="loading" class="ops-multi-series-chart">
    <EmptyState
      v-if="!loading && isEmpty"
      compact
      :description="emptyText"
    />
    <div
      v-show="!isEmpty"
      ref="chartRef"
      class="ops-multi-series-chart__canvas"
    ></div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import EmptyState from "@/components/common/empty-state/index.vue";
import { echarts, type EChartsOption } from "@/plugins/echarts";
import type { MetricSeries } from "@/types/api/monitoring";
import {
  OPS_CHART_GRID,
  bindOpsChartResize,
  formatOpsMetricValue,
  getOpsChartThemeTokens,
  inferOpsChartUnitType,
  useOpsChartTheme,
  type OpsChartUnitType,
} from "@/composables/ops/useOpsChartHost";
import { formatDateTime } from "@/utils/format";
import { formatAxisTime } from "@/utils/ops/time-range";
import { localizeUnit } from "@/utils/ops/units";
import { seriesLegendName } from "@/utils/ops/metric-series";

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
    /** 所选时间窗（Unix 秒）：x 轴固定为 [start, end]，不随数据点收缩。 */
    window?: { start: number; end: number } | null;
    /** y 轴单位覆盖（缺省用后端 unit）。 */
    unit?: string;
    /** 坐标轴与 Tooltip 数值格式化规范（缺省按后端 unit 与 metric 自动推断）。 */
    unitType?: OpsChartUnitType;
  }>(),
  {
    series: null,
    loading: false,
    height: "280px",
    annotations: () => [],
    onPointClick: undefined,
    window: null,
    unit: undefined,
    unitType: undefined,
  },
);

const { t, locale } = useI18n();
const chartRef = ref<HTMLElement | null>(null);
let chart: echarts.ECharts | null = null;
let unbindResize: (() => void) | null = null;

const { isDark } = useOpsChartTheme(() => render());

const pointCount = computed(() => {
  const seriesList = props.series?.series ?? [];
  return seriesList.reduce((sum, s) => sum + (s.points?.length ?? 0), 0);
});
const isEmpty = computed(() => pointCount.value === 0);
const fewPoints = computed(() => pointCount.value > 0 && pointCount.value <= 2);
const emptyText = computed(() => t("ops.charts.empty"));

const resolvedUnitType = computed<OpsChartUnitType>(
  () =>
    props.unitType ??
    inferOpsChartUnitType(
      props.unit ?? props.series?.unit,
      props.series?.metric,
    ),
);

function buildOption(data: MetricSeries | null): EChartsOption {
  const tokens = getOpsChartThemeTokens(isDark.value);
  const seriesList = data?.series ?? [];
  const legend = seriesList.map((s, i) =>
    seriesLegendName(s, i, seriesList.length, data?.label || data?.metric || ""),
  );
  const sparse = fewPoints.value;
  const win = props.window;
  const spanSecs = win
    ? win.end - win.start
    : spanOfPoints(seriesList.flatMap((s) => s.points || []));
  const rawUnit = props.unit ?? data?.unit ?? "";
  const unit = localizeUnit(rawUnit, t);
  const uType = resolvedUnitType.value;
  const allValues = seriesList.flatMap((s) =>
    (s.points || []).map((p) => Math.abs(p.value)),
  );
  const maxAbs = allValues.length > 0 ? Math.max(...allValues) : 0;
  const yAxisName =
    uType === "cores" && maxAbs > 0 && maxAbs < 0.1 ? "mCPU" : unit;

  return {
    tooltip: {
      trigger: "axis",
      backgroundColor: tokens.tooltipBg,
      borderColor: tokens.tooltipBorder,
      textStyle: { color: tokens.titleColor },
      axisPointer: {
        lineStyle: { color: tokens.axisLineColor },
        label: {
          formatter: (p: { value: unknown }) => formatDateTime(Number(p.value)),
        },
      },
      valueFormatter: (v: unknown) =>
        typeof v === "number"
          ? formatOpsMetricValue(v, uType, unit, locale.value, false)
          : String(v ?? "—"),
    },
    legend: {
      show: legend.length > 0,
      data: legend,
      type: "scroll",
      top: 0,
      right: 0,
      left: 84,
      icon: "circle",
      itemWidth: 8,
      itemHeight: 8,
      padding: [2, 0, 2, 0],
      textStyle: { color: tokens.textColor, fontSize: 12 },
      pageTextStyle: { color: tokens.textColor },
      pageIconColor: tokens.titleColor,
      pageIconInactiveColor: tokens.axisLineColor,
    },
    grid: { ...OPS_CHART_GRID },
    xAxis: {
      type: "time",
      ...(win ? { min: win.start * 1000, max: win.end * 1000 } : {}),
      axisLine: { lineStyle: { color: tokens.axisLineColor } },
      axisTick: { lineStyle: { color: tokens.axisLineColor } },
      splitLine: { show: false },
      axisLabel: {
        color: tokens.textColor,
        hideOverlap: true,
        formatter: (v: number) => formatAxisTime(v, spanSecs),
      },
    },
    yAxis: {
      type: "value",
      name: yAxisName,
      nameGap: 12,
      nameTextStyle: {
        color: tokens.textColor,
        align: "left",
      },
      // 单点/稀点禁止 scale 夸张放大；固定从 0 起；离散计数强制整数刻度。
      min: 0,
      scale: false,
      ...(uType === "count" ? { minInterval: 1 } : {}),
      axisLine: { show: false },
      axisTick: { show: false },
      splitLine: { lineStyle: { color: tokens.splitLineColor } },
      axisLabel: {
        color: tokens.textColor,
        hideOverlap: true,
        formatter: (v: number) =>
          formatOpsMetricValue(v, uType, unit, locale.value, true),
      },
    },
    series: seriesList.map((s, i) => {
      const base: Record<string, unknown> = {
        name: legend[i],
        type: "line",
        symbol: "circle",
        showSymbol: sparse,
        symbolSize: sparse ? 7 : 5,
        smooth: !sparse,
        data: (s.points || []).map((p) => [p.timestamp * 1000, p.value]),
      };
      if (i === 0 && (props.annotations?.length ?? 0) > 0) {
        base.markLine = {
          symbol: "none",
          label: {
            color: tokens.textColor,
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

function spanOfPoints(points: { timestamp: number }[]): number {
  if (points.length < 2) return 0;
  const ts = points.map((p) => p.timestamp);
  return Math.max(...ts) - Math.min(...ts);
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
  instance.setOption(buildOption(props.series), { notMerge: true });
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
      props.series,
      isEmpty.value,
      props.window,
      props.unit,
      props.unitType,
      locale.value,
    ] as const,
  () => render(),
  { deep: true },
);
</script>

<style scoped>
.ops-multi-series-chart {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: v-bind(height);
  min-height: 200px;
  overflow: hidden;
}
.ops-multi-series-chart__canvas {
  width: 100%;
  height: 100%;
}
</style>
