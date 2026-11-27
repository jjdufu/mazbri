import React, { useEffect, useMemo, useRef, useState } from "react";
import * as echarts from "echarts";
import type { ECharts, EChartsCoreOption } from "echarts";
import { useThemeStore } from "@/stores/theme";
import {
  formatBytes,
  formatDateTime,
  formatIntervalHuman,
  formatMoney,
  formatWallClockHm,
  formatWallClockMdHm,
} from "@/utils/format";

export type OpsChartUnitType =
  | "count"
  | "percent"
  | "cores"
  | "bytes"
  | "bytesPerSec"
  | "durationMs"
  | "durationSec"
  | "amount"
  | "raw";

export interface ChartThemeTokens {
  readonly isDark: boolean;
  readonly textColor: string;
  readonly titleColor: string;
  readonly axisLineColor: string;
  readonly splitLineColor: string;
  readonly tooltipBg: string;
  readonly tooltipBorder: string;
  readonly tooltipText: string;
  readonly primaryColor: string;
  readonly successColor: string;
  readonly warningColor: string;
  readonly dangerColor: string;
  readonly palette: readonly string[];
}

export const OPS_CHART_GRID = {
  left: 12,
  right: 16,
  top: 44,
  bottom: 24,
  containLabel: true,
} as const;

const DISCRETE_COUNT_UNITS = new Set([
  "count",
  "conn",
  "fds",
  "tasks",
  "msgs",
  "parts",
  "merges",
  "keys",
  "partitions",
  "restarts",
  "events",
  "bool",
  "rows",
]);

/**
 * Forces Asia/Shanghai (UTC+8) wall-clock formatting for chart X-axis labels.
 * Uses `HH:mm` when span <= 24h and `MM-DD HH:mm` when span > 24h.
 */
export function formatChartAxisTime(ms: number, spanMs: number): string {
  if (!Number.isFinite(ms)) return "";
  return spanMs > 86_400_000
    ? formatWallClockMdHm(ms)
    : formatWallClockHm(ms);
}

/**
 * Forces Asia/Shanghai (UTC+8) wall-clock formatting (`YYYY-MM-DD HH:mm:ss`) for chart tooltips.
 */
export function formatChartTooltipTime(ms: number): string {
  if (!Number.isFinite(ms)) return "—";
  return formatDateTime(ms);
}

export function getChartThemeTokens(isDark: boolean): ChartThemeTokens {
  return {
    isDark,
    textColor: isDark ? "#94a3b8" : "#64748b",
    titleColor: isDark ? "#f1f5f9" : "#0f172a",
    axisLineColor: isDark ? "rgba(148, 163, 184, 0.22)" : "#cbd5e1",
    splitLineColor: isDark ? "rgba(148, 163, 184, 0.12)" : "#eef2f6",
    tooltipBg: isDark ? "#111520" : "#ffffff",
    tooltipBorder: isDark ? "rgba(148, 163, 184, 0.28)" : "#e2e8f0",
    tooltipText: isDark ? "#f1f5f9" : "#0f172a",
    primaryColor: "#5d87ff",
    successColor: isDark ? "#34d399" : "#047857",
    warningColor: isDark ? "#fbbf24" : "#d97706",
    dangerColor: isDark ? "#f87171" : "#dc2626",
    palette: isDark
      ? [
          "#5d87ff",
          "#34d399",
          "#fbbf24",
          "#f87171",
          "#a78bfa",
          "#38bdf8",
          "#fb923c",
          "#2dd4bf",
        ]
      : [
          "#5d87ff",
          "#059669",
          "#d97706",
          "#dc2626",
          "#7c3aed",
          "#0284c7",
          "#ea580c",
          "#0d9488",
        ],
  };
}

export function inferChartUnitType(
  rawUnit?: string | null,
  metric?: string | null,
): OpsChartUnitType {
  const u = (rawUnit ?? "").trim();
  const m = (metric ?? "").trim();
  if (u === "cores" || m.endsWith("_cpu_cores")) return "cores";
  if (u === "B/s") return "bytesPerSec";
  if (u === "B" || (u === "" && m.endsWith("_bytes"))) return "bytes";
  if (u === "%") return "percent";
  if (u === "ms") return "durationMs";
  if (u === "s") return "durationSec";
  if (m === "pg_up" || m === "redis_up") return "count";
  if (u && DISCRETE_COUNT_UNITS.has(u)) return "count";
  return "raw";
}

function formatCompactDecimal(value: number, normalDecimals: number): number {
  if (value === 0) return 0;
  const abs = Math.abs(value);
  const threshold = Math.pow(10, -normalDecimals);
  if (abs < threshold) {
    return Number(value.toPrecision(2));
  }
  return parseFloat(value.toFixed(normalDecimals));
}

export function formatChartMetricValue(
  value: number,
  unitType: OpsChartUnitType,
  unitLabel: string,
  locale = "zh",
  forAxis = false,
): string {
  if (!Number.isFinite(value)) return "—";
  const isEn = locale.toLowerCase().startsWith("en");
  switch (unitType) {
    case "count": {
      const rounded = Math.round(value).toLocaleString();
      return forAxis || !unitLabel ? rounded : `${rounded} ${unitLabel}`;
    }
    case "cores": {
      const coreSuffix = unitLabel || "cores";
      if (value === 0) {
        return forAxis ? "0" : `0 ${coreSuffix}`;
      }
      if (Math.abs(value) < 0.1) {
        const mcpu = formatCompactDecimal(value * 1000, 2);
        return `${mcpu} mCPU`;
      }
      const cores = formatCompactDecimal(value, 2);
      return `${cores} ${coreSuffix}`;
    }
    case "bytesPerSec": {
      if (value === 0) return "0 B/s";
      return `${formatBytes(value)}/s`;
    }
    case "bytes":
      return formatBytes(value);
    case "durationSec": {
      if (Math.abs(value) >= 60) {
        return formatIntervalHuman(value, locale);
      }
      const sec = formatCompactDecimal(value, 2);
      return isEn ? `${sec}s` : formatIntervalHuman(sec, locale);
    }
    case "durationMs": {
      if (forAxis && unitLabel === "ms") {
        return new Intl.NumberFormat(locale, {
          maximumFractionDigits: 1,
        }).format(formatCompactDecimal(value, 1));
      }
      if (Math.abs(value) >= 1000) {
        return `${formatCompactDecimal(value / 1000, 2)} s`;
      }
      return `${formatCompactDecimal(value, 1)} ms`;
    }
    case "percent":
      return `${formatCompactDecimal(value, 1)}%`;
    case "amount":
      if (forAxis && Math.abs(value) >= 10000) {
        return `${parseFloat((value / 1000).toFixed(1))}k`;
      }
      return formatMoney(value);
    case "raw":
    default: {
      const formatted =
        value !== 0 && Math.abs(value) < 0.01
          ? String(Number(value.toPrecision(2)))
          : new Intl.NumberFormat(undefined, {
              maximumFractionDigits: 2,
            }).format(value);
      return forAxis || !unitLabel ? formatted : `${formatted} ${unitLabel}`;
    }
  }
}

/**
 * Subscribes to `useThemeStore.isDark` and observes `document.documentElement` class
 * so charts always stay synchronized across light/dark transitions.
 */
export function useChartDarkMode(): boolean {
  const storeIsDark = useThemeStore((s) => s.isDark);
  const [domDark, setDomDark] = useState<boolean>(() => {
    if (typeof document === "undefined") return storeIsDark;
    return document.documentElement.classList.contains("dark");
  });

  useEffect(() => {
    setDomDark(storeIsDark);
  }, [storeIsDark]);

  useEffect(() => {
    if (typeof document === "undefined" || typeof MutationObserver === "undefined") {
      return;
    }
    const root = document.documentElement;
    const sync = () => {
      setDomDark(root.classList.contains("dark"));
    };
    sync();
    const observer = new MutationObserver(sync);
    observer.observe(root, { attributes: true, attributeFilter: ["class"] });
    return () => observer.disconnect();
  }, []);

  return storeIsDark || domDark;
}

function applyThemeDefaults(
  rawOption: EChartsCoreOption,
  tokens: ChartThemeTokens,
): EChartsCoreOption {
  const next: EChartsCoreOption = {
    backgroundColor: "transparent",
    color: [...tokens.palette],
    ...rawOption,
  };

  if (rawOption.tooltip && typeof rawOption.tooltip === "object" && !Array.isArray(rawOption.tooltip)) {
    const rawTooltip = rawOption.tooltip as Record<string, unknown>;
    const rawTextStyle =
      rawTooltip.textStyle && typeof rawTooltip.textStyle === "object"
        ? (rawTooltip.textStyle as Record<string, unknown>)
        : {};
    next.tooltip = {
      ...rawTooltip,
      backgroundColor: tokens.tooltipBg,
      borderColor: tokens.tooltipBorder,
      borderWidth: 1,
      textStyle: {
        ...rawTextStyle,
        color: tokens.tooltipText,
        fontSize: 12,
      },
    };
  }

  return next;
}

export interface EChartBoxProps {
  option:
    | EChartsCoreOption
    | ((tokens: ChartThemeTokens, isDark: boolean) => EChartsCoreOption);
  height?: number | string;
  className?: string;
  autoBrushLineX?: boolean;
  onReady?: (chart: ECharts) => void;
  onClick?: (params: unknown) => void;
  onBrushEnd?: (params: unknown) => void;
  onZrClick?: (offsetX: number, offsetY: number, chart: ECharts) => void;
  testId?: string;
}

export function EChartBox({
  option,
  height = 220,
  className,
  autoBrushLineX = false,
  onReady,
  onClick,
  onBrushEnd,
  onZrClick,
  testId,
}: EChartBoxProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const chartRef = useRef<ECharts | null>(null);
  const isDark = useChartDarkMode();
  const tokens = useMemo(() => getChartThemeTokens(isDark), [isDark]);

  const onClickRef = useRef(onClick);
  const onBrushEndRef = useRef(onBrushEnd);
  const onZrClickRef = useRef(onZrClick);
  const onReadyRef = useRef(onReady);
  onClickRef.current = onClick;
  onBrushEndRef.current = onBrushEnd;
  onZrClickRef.current = onZrClick;
  onReadyRef.current = onReady;

  const resolvedOption = useMemo(() => {
    const base = typeof option === "function" ? option(tokens, isDark) : option;
    return applyThemeDefaults(base, tokens);
  }, [option, tokens, isDark]);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const instance = echarts.init(el, undefined, { renderer: "canvas" });
    chartRef.current = instance;

    instance.on("click", (params) => {
      onClickRef.current?.(params);
    });
    instance.on("brushEnd", (params) => {
      onBrushEndRef.current?.(params);
    });
    instance.getZr().on("click", (event) => {
      onZrClickRef.current?.(event.offsetX, event.offsetY, instance);
    });

    onReadyRef.current?.(instance);

    const handleResize = () => {
      instance.resize();
    };
    const observer =
      typeof ResizeObserver !== "undefined"
        ? new ResizeObserver(handleResize)
        : null;
    observer?.observe(el);
    window.addEventListener("resize", handleResize);

    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", handleResize);
      instance.dispose();
      chartRef.current = null;
    };
  }, []);

  useEffect(() => {
    const instance = chartRef.current;
    if (!instance) return;
    instance.setOption(resolvedOption, { notMerge: true });
    if (autoBrushLineX) {
      instance.dispatchAction({
        type: "takeGlobalCursor",
        key: "brush",
        brushOption: { brushType: "lineX", brushMode: "single" },
      });
    }
  }, [resolvedOption, autoBrushLineX]);

  const styleHeight = typeof height === "number" ? `${height}px` : height;

  return (
    <div
      ref={containerRef}
      data-testid={testId}
      style={{ height: styleHeight }}
      className={className ? `w-full overflow-hidden ${className}` : "w-full overflow-hidden"}
    />
  );
}
