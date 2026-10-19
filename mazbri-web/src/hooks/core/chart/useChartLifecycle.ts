import type { EChartsOption } from "@/plugins/echarts";
export function createChartLifecycleController(options: {
  threshold: number;
  chartRef: {
    value?: HTMLElement;
  };
  initChartInstance: (element: HTMLElement) => void;
  isDestroyed: () => boolean;
  getPendingOptions: () => EChartsOption | null;
  clearPendingOptions: () => void;
  onVisibleOptions: (options: EChartsOption) => void;
  onVisibilityInitError: (error: unknown) => void;
}) {
  const {
    threshold,
    chartRef,
    initChartInstance,
    isDestroyed,
    getPendingOptions,
    clearPendingOptions,
    onVisibleOptions,
    onVisibilityInitError,
  } = options;
  let intersectionObserver: IntersectionObserver | null = null;
  const cleanupIntersectionObserver = () => {
    if (intersectionObserver) {
      intersectionObserver.disconnect();
      intersectionObserver = null;
    }
  };
  const createIntersectionObserver = () => {
    if (intersectionObserver || !chartRef.value) return;
    intersectionObserver = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          const pendingOptions = getPendingOptions();
          if (entry.isIntersecting && pendingOptions && !isDestroyed()) {
            requestAnimationFrame(() => {
              const visibleOptions = getPendingOptions();
              if (!isDestroyed() && visibleOptions) {
                try {
                  initChartInstance(entry.target as HTMLElement);
                  const event = new CustomEvent("chartVisible", {
                    detail: { options: visibleOptions },
                  });
                  entry.target.dispatchEvent(event);
                  onVisibleOptions(visibleOptions);
                  clearPendingOptions();
                  cleanupIntersectionObserver();
                } catch (error) {
                  onVisibilityInitError(error);
                }
              }
            });
          }
        });
      },
      { threshold },
    );
    intersectionObserver.observe(chartRef.value);
  };
  const isContainerVisible = (element: HTMLElement): boolean => {
    const rect = element.getBoundingClientRect();
    return (
      rect.width > 0 &&
      rect.height > 0 &&
      rect.top < window.innerHeight &&
      rect.bottom > 0
    );
  };
  return {
    createIntersectionObserver,
    cleanupIntersectionObserver,
    isContainerVisible,
  };
}
