import { watch } from "vue";
import type { Ref } from "vue";
import type { EChartsOption } from "@/plugins/echarts";
export function createChartThemeController(options: {
  isDark: Ref<boolean>;
  autoTheme: boolean;
  emptyStateManager: {
    updateStyle: () => void;
  };
  isDestroyed: () => boolean;
  getCurrentOptions: () => EChartsOption | null;
  refreshChart: (options: EChartsOption) => void;
}) {
  const {
    isDark,
    autoTheme,
    emptyStateManager,
    isDestroyed,
    getCurrentOptions,
    refreshChart,
  } = options;
  let themeStopHandle: (() => void) | null = null;
  const setupThemeWatcher = () => {
    if (!autoTheme) {
      return;
    }
    themeStopHandle = watch(isDark, () => {
      emptyStateManager.updateStyle();
      if (isDestroyed()) {
        return;
      }
      requestAnimationFrame(() => {
        if (isDestroyed()) {
          return;
        }
        const currentOptions = getCurrentOptions();
        if (currentOptions) {
          refreshChart(currentOptions);
        }
      });
    });
  };
  const cleanupThemeWatcher = () => {
    themeStopHandle?.();
    themeStopHandle = null;
  };
  return {
    setupThemeWatcher,
    cleanupThemeWatcher,
  };
}
