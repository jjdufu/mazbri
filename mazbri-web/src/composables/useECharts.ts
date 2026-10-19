import {
  onMounted,
  onUnmounted,
  ref,
  shallowRef,
  type Ref,
  type ShallowRef,
} from "vue";
import * as echarts from "echarts";
import type { EChartsOption } from "echarts";
export interface UseEChartsReturn {
  chartRef: Ref<HTMLElement | null>;
  chartInstance: ShallowRef<echarts.ECharts | null>;
  initChart: (options: EChartsOption) => void;
  updateChart: (options: EChartsOption) => void;
  resizeChart: () => void;
  disposeChart: () => void;
}
export function useECharts(): UseEChartsReturn {
  const chartRef = ref<HTMLElement | null>(null);
  const chartInstance = shallowRef<echarts.ECharts | null>(null);
  const handleResize = () => {
    chartInstance.value?.resize();
  };
  const initChart = (options: EChartsOption) => {
    if (!chartRef.value) return;
    if (chartInstance.value) {
      chartInstance.value.dispose();
      chartInstance.value = null;
    }
    chartInstance.value = echarts.init(chartRef.value);
    chartInstance.value.setOption(options);
  };
  const updateChart = (options: EChartsOption) => {
    if (!chartInstance.value) {
      initChart(options);
      return;
    }
    chartInstance.value.setOption(options);
  };
  const resizeChart = () => {
    chartInstance.value?.resize();
  };
  const disposeChart = () => {
    if (chartInstance.value) {
      chartInstance.value.dispose();
      chartInstance.value = null;
    }
  };
  onMounted(() => {
    window.addEventListener("resize", handleResize);
  });
  onUnmounted(() => {
    window.removeEventListener("resize", handleResize);
    disposeChart();
  });
  return {
    chartRef,
    chartInstance,
    initChart,
    updateChart,
    resizeChart,
    disposeChart,
  };
}
