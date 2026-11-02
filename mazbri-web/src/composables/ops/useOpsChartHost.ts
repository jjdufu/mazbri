import type { ECharts } from "echarts/core";

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

/** Shared grid defaults: small base margins + containLabel so axes stay inside. */
export const OPS_CHART_GRID = {
  left: 12,
  right: 16,
  top: 40,
  bottom: 24,
  containLabel: true,
} as const;
