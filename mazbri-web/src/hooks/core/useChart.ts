import { echarts, type EChartsOption } from "@/plugins/echarts";
import { ErrorCodes } from "@/constants/error-codes";
import type {
  TooltipComponentOption,
  LegendComponentOption,
  GridComponentOption,
} from "echarts";
import { storeToRefs } from "pinia";
import { useSettingStore } from "@/store/modules/setting";
import { logger } from "@/utils/sys";
import { $t } from "@/locales";
import { getCssVar } from "@/utils/ui";
import type {
  BaseChartProps,
  ChartThemeConfig,
  UseChartOptions,
} from "@/types/component/chart";
import { createChartResizeController } from "./chart/useChartResize";
import { createChartThemeController } from "./chart/useChartTheme";
import { createChartLifecycleController } from "./chart/useChartLifecycle";
export const useChartOps = (): ChartThemeConfig => ({
  chartHeight: "16rem",
  fontSize: 13,
  fontColor: "#999",
  themeColor: getCssVar("--el-color-primary-light-1"),
  colors: [
    getCssVar("--el-color-primary-light-1"),
    "#4ABEFF",
    "#EDF2FF",
    "#14DEBA",
    "#FFAF20",
    "#FA8A6C",
    "#FFAF20",
  ],
});
export function useChart(options: UseChartOptions = {}) {
  const {
    initOptions,
    initDelay = 0,
    threshold = 0.1,
    autoTheme = true,
  } = options;
  const settingStore = useSettingStore();
  const { isDark, menuOpen, menuType } = storeToRefs(settingStore);
  const chartRef = ref<HTMLElement>();
  let chart: echarts.ECharts | null = null;
  let pendingOptions: EChartsOption | null = null;
  let isDestroyed = false;
  let emptyStateDiv: HTMLElement | null = null;
  const createLineStyle = (
    color: string,
    width = 1,
    type: "solid" | "dashed" = "solid",
  ) => ({
    color,
    width,
    type,
  });
  interface AxisLineStyle {
    show: boolean;
    lineStyle: {
      color: string;
      width: number;
    };
  }
  interface SplitLineStyle {
    show: boolean;
    lineStyle: {
      color: string;
      width: number;
      type: "solid" | "dashed";
    };
  }
  interface AxisLabelStyle {
    show: boolean;
    color: string;
    fontSize: number;
  }
  const styleCache = {
    axisLine: null as AxisLineStyle | null,
    splitLine: null as SplitLineStyle | null,
    axisLabel: null as AxisLabelStyle | null,
    lastDarkValue: isDark.value,
  };
  const clearStyleCache = () => {
    styleCache.axisLine = null;
    styleCache.splitLine = null;
    styleCache.axisLabel = null;
    styleCache.lastDarkValue = isDark.value;
  };
  const getAxisLineStyle = (show: boolean = true) => {
    if (styleCache.lastDarkValue !== isDark.value) {
      clearStyleCache();
    }
    if (!styleCache.axisLine) {
      styleCache.axisLine = {
        show,
        lineStyle: createLineStyle(isDark.value ? "#444" : "#EDEDED"),
      };
    }
    return styleCache.axisLine;
  };
  const getSplitLineStyle = (show: boolean = true) => {
    if (styleCache.lastDarkValue !== isDark.value) {
      clearStyleCache();
    }
    if (!styleCache.splitLine) {
      styleCache.splitLine = {
        show,
        lineStyle: createLineStyle(
          isDark.value ? "#444" : "#EDEDED",
          1,
          "dashed",
        ),
      };
    }
    return styleCache.splitLine;
  };
  const getAxisLabelStyle = (show: boolean = true) => {
    if (styleCache.lastDarkValue !== isDark.value) {
      clearStyleCache();
    }
    if (!styleCache.axisLabel) {
      const { fontColor, fontSize } = useChartOps();
      styleCache.axisLabel = {
        show,
        color: fontColor,
        fontSize,
      };
    }
    return styleCache.axisLabel;
  };
  const getAxisTickStyle = () => ({
    show: false,
  });
  const getAnimationConfig = (
    animationDelay: number = 50,
    animationDuration: number = 1500,
  ) => ({
    animationDelay: (idx: number) => idx * animationDelay + 200,
    animationDuration: (idx: number) => animationDuration - idx * 50,
    animationEasing: "quarticOut" as const,
  });
  const getTooltipStyle = (
    trigger: "item" | "axis" = "axis",
    customOptions: Partial<TooltipComponentOption> = {},
  ): TooltipComponentOption => ({
    trigger,
    backgroundColor: isDark.value
      ? "rgba(0, 0, 0, 0.8)"
      : "rgba(255, 255, 255, 0.9)",
    borderColor: isDark.value ? "#333" : "#ddd",
    borderWidth: 1,
    textStyle: {
      color: isDark.value ? "#fff" : "#333",
    },
    ...customOptions,
  });
  const getLegendStyle = (
    position: "bottom" | "top" | "left" | "right" = "bottom",
    customOptions: Partial<LegendComponentOption> = {},
  ): LegendComponentOption => {
    const baseConfig = {
      textStyle: {
        color: isDark.value ? "#fff" : "#333",
      },
      itemWidth: 12,
      itemHeight: 12,
      itemGap: 20,
      ...customOptions,
    };
    switch (position) {
      case "bottom":
        return {
          ...baseConfig,
          bottom: 0,
          left: "center",
          orient: "horizontal" as const,
          icon: "roundRect",
        };
      case "top":
        return {
          ...baseConfig,
          top: 0,
          left: "center",
          orient: "horizontal" as const,
          icon: "roundRect",
        };
      case "left":
        return {
          ...baseConfig,
          left: 0,
          top: "center",
          orient: "vertical" as const,
          icon: "roundRect",
        };
      case "right":
        return {
          ...baseConfig,
          right: 0,
          top: "center",
          orient: "vertical" as const,
          icon: "roundRect",
        };
      default:
        return baseConfig;
    }
  };
  const getGridWithLegend = (
    showLegend: boolean,
    legendPosition: "bottom" | "top" | "left" | "right" = "bottom",
    baseGrid: Partial<GridComponentOption> = {},
  ) => {
    const defaultGrid = {
      top: 15,
      right: 15,
      bottom: 8,
      left: 0,
      containLabel: true,
      ...baseGrid,
    };
    if (!showLegend) {
      return defaultGrid;
    }
    switch (legendPosition) {
      case "bottom":
        return {
          ...defaultGrid,
          bottom: 40,
        };
      case "top":
        return {
          ...defaultGrid,
          top: 40,
        };
      case "left":
        return {
          ...defaultGrid,
          left: 120,
        };
      case "right":
        return {
          ...defaultGrid,
          right: 120,
        };
      default:
        return defaultGrid;
    }
  };
  const initChartInstance = (element: HTMLElement) => {
    if (!chart && !isDestroyed) {
      chart = echarts.init(element);
    }
  };
  const performChartInit = (options: EChartsOption) => {
    if (!chart && chartRef.value && !isDestroyed) {
      initChartInstance(chartRef.value);
      resizeController.setupMenuWatchers();
      themeController.setupThemeWatcher();
    }
    if (chart && !isDestroyed) {
      chart.setOption(options);
      pendingOptions = null;
    }
  };
  const resizeController = createChartResizeController({
    menuOpen,
    menuType,
    handleResize: () => handleResize(),
  });
  const emptyStateManager = {
    create: () => {
      if (!chartRef.value || emptyStateDiv) return;
      emptyStateDiv = document.createElement("div");
      emptyStateDiv.style.cssText = `
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        bottom: 0;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        font-size: 12px;
        color: ${isDark.value ? "#555555" : "#B3B2B2"};
        background: transparent;
        z-index: 10;
      `;
      emptyStateDiv.textContent = $t("common.feedback.noData");
      if (
        chartRef.value.style.position !== "relative" &&
        chartRef.value.style.position !== "absolute"
      ) {
        chartRef.value.style.position = "relative";
      }
      chartRef.value.appendChild(emptyStateDiv);
    },
    remove: () => {
      if (emptyStateDiv && chartRef.value) {
        chartRef.value.removeChild(emptyStateDiv);
        emptyStateDiv = null;
      }
    },
    updateStyle: () => {
      if (emptyStateDiv) {
        emptyStateDiv.style.color = isDark.value ? "#666" : "#999";
      }
    },
  };
  const themeController = createChartThemeController({
    isDark,
    autoTheme,
    emptyStateManager,
    isDestroyed: () => isDestroyed,
    getCurrentOptions: () =>
      chart ? (chart.getOption() as EChartsOption) : null,
    refreshChart: (options) => updateChart(options),
  });
  const lifecycleController = createChartLifecycleController({
    threshold,
    chartRef,
    initChartInstance,
    isDestroyed: () => isDestroyed,
    getPendingOptions: () => pendingOptions,
    clearPendingOptions: () => {
      pendingOptions = null;
    },
    onVisibleOptions: () => {
      pendingOptions = null;
    },
    onVisibilityInitError: (error) => {
      logger.error("Chart initialization failed after visibility restore", {
        service: "mazbri-mgmt-web",
        event: "chart_init_after_visibility_failed",
        severity: "error",
        error_code: ErrorCodes.INTERNAL_ERROR,
        error,
      });
    },
  });
  const initChart = (options: EChartsOption = {}, isEmpty: boolean = false) => {
    if (!chartRef.value || isDestroyed) return;
    const mergedOptions = { ...initOptions, ...options };
    try {
      if (isEmpty) {
        if (chart) {
          chart.clear();
        }
        emptyStateManager.create();
        return;
      } else {
        emptyStateManager.remove();
      }
      if (lifecycleController.isContainerVisible(chartRef.value)) {
        if (initDelay > 0) {
          setTimeout(() => performChartInit(mergedOptions), initDelay);
        } else {
          performChartInit(mergedOptions);
        }
      } else {
        pendingOptions = mergedOptions;
        lifecycleController.createIntersectionObserver();
      }
    } catch (error) {
      logger.error("Chart initialization failed", {
        service: "mazbri-mgmt-web",
        event: "chart_init_failed",
        severity: "error",
        error_code: ErrorCodes.INTERNAL_ERROR,
        error,
      });
    }
  };
  const updateChart = (options: EChartsOption) => {
    if (isDestroyed) return;
    try {
      if (!chart) {
        initChart(options);
        return;
      }
      chart.setOption(options);
    } catch (error) {
      logger.error("Chart update failed", {
        service: "mazbri-mgmt-web",
        event: "chart_update_failed",
        severity: "error",
        error_code: ErrorCodes.INTERNAL_ERROR,
        error,
      });
    }
  };
  const handleResize = () => {
    if (chart && !isDestroyed) {
      try {
        chart.resize();
      } catch (error) {
        logger.error("Chart resize failed", {
          service: "mazbri-mgmt-web",
          event: "chart_resize_failed",
          severity: "error",
          error_code: ErrorCodes.INTERNAL_ERROR,
          error,
        });
      }
    }
  };
  const destroyChart = () => {
    isDestroyed = true;
    if (chart) {
      try {
        chart.dispose();
      } catch (error) {
        logger.error("Chart dispose failed", {
          service: "mazbri-mgmt-web",
          event: "chart_dispose_failed",
          severity: "error",
          error_code: ErrorCodes.INTERNAL_ERROR,
          error,
        });
      } finally {
        chart = null;
      }
    }
    resizeController.cleanupMenuWatchers();
    themeController.cleanupThemeWatcher();
    emptyStateManager.remove();
    lifecycleController.cleanupIntersectionObserver();
    resizeController.clearTimers();
    clearStyleCache();
    pendingOptions = null;
  };
  const getChartInstance = () => chart;
  const isChartInitialized = () => chart !== null;
  onMounted(() => {
    window.addEventListener("resize", resizeController.debouncedResize);
  });
  onBeforeUnmount(() => {
    window.removeEventListener("resize", resizeController.debouncedResize);
  });
  onUnmounted(() => {
    destroyChart();
  });
  return {
    isDark,
    chartRef,
    initChart,
    updateChart,
    handleResize,
    destroyChart,
    getChartInstance,
    isChartInitialized,
    emptyStateManager,
    getAxisLineStyle,
    getSplitLineStyle,
    getAxisLabelStyle,
    getAxisTickStyle,
    getAnimationConfig,
    getTooltipStyle,
    getLegendStyle,
    useChartOps,
    getGridWithLegend,
  };
}
interface UseChartComponentOptions<T extends BaseChartProps> {
  props: T;
  generateOptions: () => EChartsOption;
  checkEmpty?: () => boolean;
  watchSources?: (() => any)[];
  onVisible?: () => void;
  chartOptions?: UseChartOptions;
}
export function useChartComponent<T extends BaseChartProps>(
  options: UseChartComponentOptions<T>,
) {
  const {
    props,
    generateOptions,
    checkEmpty,
    watchSources = [],
    onVisible,
    chartOptions = {},
  } = options;
  const chart = useChart(chartOptions);
  const { chartRef, initChart, isDark, emptyStateManager } = chart;
  const isEmpty = computed(() => {
    if (props.isEmpty) return true;
    if (checkEmpty) return checkEmpty();
    return false;
  });
  const updateChart = () => {
    nextTick(() => {
      if (isEmpty.value) {
        if (chart.getChartInstance()) {
          chart.getChartInstance()?.clear();
        }
        emptyStateManager.create();
      } else {
        emptyStateManager.remove();
        initChart(generateOptions());
      }
    });
  };
  const handleChartVisible = () => {
    if (onVisible) {
      onVisible();
    } else {
      updateChart();
    }
  };
  const stopHandles: (() => void)[] = [];
  const setupWatchers = () => {
    if (watchSources.length > 0) {
      const stopHandle = watch(watchSources, updateChart, { deep: true });
      stopHandles.push(stopHandle);
    }
    const themeStopHandle = watch(isDark, () => {
      emptyStateManager.updateStyle();
      updateChart();
    });
    stopHandles.push(themeStopHandle);
  };
  const cleanupWatchers = () => {
    stopHandles.forEach((stop) => stop());
    stopHandles.length = 0;
  };
  const setupLifecycle = () => {
    onMounted(() => {
      updateChart();
      if (chartRef.value) {
        chartRef.value.addEventListener("chartVisible", handleChartVisible);
      }
    });
    onBeforeUnmount(() => {
      if (chartRef.value) {
        chartRef.value.removeEventListener("chartVisible", handleChartVisible);
      }
      cleanupWatchers();
      emptyStateManager.remove();
    });
  };
  setupWatchers();
  setupLifecycle();
  return {
    ...chart,
    isEmpty,
    updateChart,
    handleChartVisible,
  };
}
