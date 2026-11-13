import { computed, watch, type ComputedRef } from "vue";
import type { ECharts } from "echarts/core";
import { useSettingStore } from "@/store/modules/setting";
import {
  formatBytes,
  formatIntervalHuman,
  formatMoney,
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

export interface OpsChartThemeTokens {
  textColor: string;
  titleColor: string;
  axisLineColor: string;
  splitLineColor: string;
  tooltipBg: string;
  tooltipBorder: string;
}

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

/* [不可更改] ECharts 全站主题令牌与防碰撞布局底座：切换亮暗主题时必须在 1 帧内重绘，禁止硬编码亮白网格线 */
export function getOpsChartThemeTokens(isDark: boolean): OpsChartThemeTokens {
  return {
    textColor: isDark ? "#A3A6AD" : "#606266",
    titleColor: isDark ? "#E5EAF3" : "#303133",
    axisLineColor: isDark ? "rgba(255, 255, 255, 0.12)" : "#DCDFE6",
    splitLineColor: isDark ? "rgba(255, 255, 255, 0.07)" : "#EBEEF5",
    tooltipBg: isDark ? "rgba(29, 30, 31, 0.96)" : "rgba(255, 255, 255, 0.96)",
    tooltipBorder: isDark ? "rgba(255, 255, 255, 0.12)" : "#E4E7ED",
  };
}

/**
 * 响应式监听全局亮暗主题变化，在不刷新浏览器的情况下触发当前挂载 ECharts 实例即时重绘。
 */
export function useOpsChartTheme(onThemeChange: () => void): {
  isDark: ComputedRef<boolean>;
} {
  const settingStore = useSettingStore();
  const isDark = computed(() => settingStore.isDark);
  watch(
    isDark,
    () => {
      onThemeChange();
    },
    { flush: "post" },
  );
  return { isDark };
}

/**
 * 根据后端 `MetricSeries.unit` 与指标名自动推断规范化单位类型。
 */
export function inferOpsChartUnitType(
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

/**
 * 格式化坐标轴刻度与 Tooltip 数值（复用 `utils/format.ts` 的字节/时长/金额换算）。
 */
export function formatOpsMetricValue(
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
      if (value === 0) {
        return forAxis ? "0" : `0 ${isEn ? "cores" : "核"}`;
      }
      if (Math.abs(value) < 0.1) {
        const mcpu = formatCompactDecimal(value * 1000, 2);
        return `${mcpu} mCPU`;
      }
      const cores = formatCompactDecimal(value, 2);
      return `${cores} ${isEn ? "cores" : "核"}`;
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
      return isEn ? `${sec}s` : `${sec} 秒`;
    }
    case "durationMs": {
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
 * Bind ResizeObserver + window.resize for an ops ECharts host.
 * Call after the chart instance is created; returns a disposer.
 */
export function bindOpsChartResize(
  el: HTMLElement,
  getChart: () => ECharts | null | undefined,
): () => void {
  const resize = () => {
    getChart()?.resize();
  };
  const observer = new ResizeObserver(() => resize());
  observer.observe(el);
  window.addEventListener("resize", resize);
  return () => {
    observer.disconnect();
    window.removeEventListener("resize", resize);
  };
}

/** Shared grid defaults: top 44 + containLabel reserves headroom for legend & yAxis.name without overlap. */
export const OPS_CHART_GRID = {
  left: 12,
  right: 16,
  top: 44,
  bottom: 24,
  containLabel: true,
} as const;
