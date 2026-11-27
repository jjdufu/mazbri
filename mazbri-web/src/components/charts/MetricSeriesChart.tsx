import React, { useCallback, useMemo } from "react";
import type { ECharts, EChartsCoreOption } from "echarts";
import { useT } from "@/locales";
import type { MetricSeries } from "@/types/api/monitoring";
import { seriesLegendName } from "@/utils/ops/metric-series";
import { localizeUnit } from "@/utils/ops/units";
import {
  EChartBox,
  OPS_CHART_GRID,
  formatChartAxisTime,
  formatChartMetricValue,
  formatChartTooltipTime,
  inferChartUnitType,
  type ChartThemeTokens,
  type OpsChartUnitType,
} from "./EChartBox";

export interface MetricPointClick {
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

export function countMetricSeriesPoints(
  series: MetricSeries | null | undefined,
): number {
  const list = series?.series ?? [];
  return list.reduce((sum, s) => sum + (s.points?.length ?? 0), 0);
}

export function isMetricSeriesEmpty(
  series: MetricSeries | null | undefined,
): boolean {
  return countMetricSeriesPoints(series) === 0;
}

export function isMetricSeriesZeroOrEmpty(
  series: MetricSeries | null | undefined,
): boolean {
  const list = series?.series ?? [];
  let count = 0;
  for (const s of list) {
    for (const p of s.points ?? []) {
      if (typeof p.value === "number" && Number.isFinite(p.value)) {
        count += 1;
        if (Math.abs(p.value) > 0) return false;
      }
    }
  }
  return count === 0 || count > 0;
}

function spanOfPoints(points: readonly { timestamp: number }[]): number {
  if (points.length < 2) return 0;
  let min = Infinity;
  let max = -Infinity;
  for (const p of points) {
    if (p.timestamp < min) min = p.timestamp;
    if (p.timestamp > max) max = p.timestamp;
  }
  return Math.max(0, max - min);
}

export interface MetricSeriesChartProps {
  series?: MetricSeries | null;
  height?: number | string;
  annotations?: readonly ChartAnnotation[];
  onPointClick?: (point: MetricPointClick) => void;
  /** Time window in Unix seconds: locks X axis to [start, end]. */
  window?: { start: number; end: number } | null;
  unit?: string;
  unitType?: OpsChartUnitType;
  className?: string;
  testId?: string;
}

export function MetricSeriesChart({
  series,
  height = 220,
  annotations,
  onPointClick,
  window: win,
  unit: unitOverride,
  unitType: unitTypeOverride,
  className,
  testId,
}: MetricSeriesChartProps) {
  const { t, locale } = useT();

  const pointCount = useMemo(() => countMetricSeriesPoints(series), [series]);
  const fewPoints = pointCount > 0 && pointCount <= 2;

  const resolvedUnitType = useMemo<OpsChartUnitType>(
    () =>
      unitTypeOverride ??
      inferChartUnitType(unitOverride ?? series?.unit, series?.metric),
    [unitTypeOverride, unitOverride, series?.unit, series?.metric],
  );

  const buildOption = useCallback(
    (tokens: ChartThemeTokens): EChartsCoreOption => {
      const seriesList = series?.series ?? [];
      const legend = seriesList.map((s, i) =>
        seriesLegendName(
          s,
          i,
          seriesList.length,
          series?.label || series?.metric || "",
        ),
      );
      const spanSecs = win
        ? Math.max(0, win.end - win.start)
        : spanOfPoints(seriesList.flatMap((s) => s.points || []));
      const spanMs = spanSecs * 1000;
      const rawUnit = unitOverride ?? series?.unit ?? "";
      const isBinaryUpMetric =
        series?.metric === "pg_up" || series?.metric === "redis_up";
      const unitLabel = isBinaryUpMetric ? "" : localizeUnit(rawUnit, t);

      const allValues = seriesList.flatMap((s) =>
        (s.points || []).map((p) => Math.abs(p.value)),
      );
      const maxAbs = allValues.length > 0 ? Math.max(...allValues) : 0;
      const yAxisName =
        resolvedUnitType === "cores" && maxAbs > 0 && maxAbs < 0.1
          ? "mCPU"
          : unitLabel;

      const formatBinaryState = (v: number, forAxis: boolean) => {
        const rounded = Math.round(v);
        if (rounded === 1) {
          return forAxis
            ? `1 (${t("ops.infra.onlineLabel")})`
            : `1 · ${t("ops.infra.onlineLabel")}`;
        }
        if (rounded === 0) {
          return forAxis
            ? `0 (${t("ops.infra.offlineLabel")})`
            : `0 · ${t("ops.infra.offlineLabel")}`;
        }
        return String(rounded);
      };

      return {
        tooltip: {
          trigger: "axis",
          axisPointer: {
            lineStyle: { color: tokens.axisLineColor },
            label: {
              formatter: (p: { value: unknown }) =>
                formatChartTooltipTime(Number(p.value)),
            },
          },
          valueFormatter: (v: unknown) =>
            typeof v === "number"
              ? isBinaryUpMetric
                ? formatBinaryState(v, false)
                : formatChartMetricValue(
                    v,
                    resolvedUnitType,
                    unitLabel,
                    locale,
                    false,
                  )
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
            formatter: (v: number) => formatChartAxisTime(v, spanMs),
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
          min: 0,
          ...(isBinaryUpMetric ? { max: 1, minInterval: 1 } : {}),
          scale: false,
          ...(resolvedUnitType === "count" ? { minInterval: 1 } : {}),
          axisLine: { show: false },
          axisTick: { show: false },
          splitLine: { lineStyle: { color: tokens.splitLineColor } },
          axisLabel: {
            color: tokens.textColor,
            hideOverlap: true,
            formatter: (v: number) =>
              isBinaryUpMetric
                ? formatBinaryState(v, true)
                : formatChartMetricValue(
                    v,
                    resolvedUnitType,
                    unitLabel,
                    locale,
                    true,
                  ),
          },
        },
        series: seriesList.map((s, i) => {
          const base: Record<string, unknown> = {
            name: legend[i],
            type: "line",
            symbol: "circle",
            showSymbol: fewPoints,
            symbolSize: fewPoints ? 7 : 5,
            smooth: !fewPoints,
            data: (s.points || []).map((p) => [p.timestamp * 1000, p.value]),
          };
          if (i === 0 && annotations && annotations.length > 0) {
            base.markLine = {
              symbol: "none",
              label: {
                color: tokens.textColor,
                formatter: (p: { name?: string }) => p.name || "",
                position: "insideEndTop",
              },
              data: annotations.map((a) => ({
                xAxis: a.timestamp * 1000,
                name: a.label || "",
                lineStyle: {
                  color: a.color || tokens.warningColor,
                  type: "dashed",
                },
              })),
            };
          }
          return base;
        }),
      };
    },
    [series, win, unitOverride, resolvedUnitType, fewPoints, annotations, t, locale],
  );

  const handleZrClick = useCallback(
    (offsetX: number, offsetY: number, chart: ECharts) => {
      if (!onPointClick) return;
      if (!chart.containPixel({ gridIndex: 0 }, [offsetX, offsetY])) return;
      const converted = chart.convertFromPixel({ gridIndex: 0 }, [
        offsetX,
        offsetY,
      ]) as number[] | undefined;
      if (!converted || converted.length < 2) return;
      const [clickTs, clickVal] = converted;
      let best: MetricPointClick | null = null;
      let bestDt = Infinity;
      let bestDv = Infinity;
      const seriesList = series?.series ?? [];
      for (let seriesIndex = 0; seriesIndex < seriesList.length; seriesIndex++) {
        const s = seriesList[seriesIndex];
        for (const p of s?.points || []) {
          const dt = Math.abs(p.timestamp * 1000 - clickTs);
          const dv = Math.abs(p.value - clickVal);
          if (dt < bestDt || (dt === bestDt && dv < bestDv)) {
            bestDt = dt;
            bestDv = dv;
            best = { timestamp: p.timestamp, value: p.value, seriesIndex };
          }
        }
      }
      if (best) onPointClick(best);
    },
    [onPointClick, series],
  );

  return (
    <EChartBox
      option={buildOption}
      height={height}
      onZrClick={onPointClick ? handleZrClick : undefined}
      className={className}
      testId={testId}
    />
  );
}
