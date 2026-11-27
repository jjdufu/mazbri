import { fourDotsSpinnerSvg } from "@/assets/svg/loading";
const getLoadingBackground = (): string => {
  const isDark = document.documentElement.classList.contains("dark");
  return isDark ? "rgba(7, 7, 7, 0.85)" : "#fff";
};
const DEFAULT_LOADING_CONFIG = {
  lock: true,
  get background() {
    return getLoadingBackground();
  },
  svg: fourDotsSpinnerSvg,
  svgViewBox: "0 0 40 40",
  customClass: "art-loading-fix",
} as const;
interface LoadingInstance {
  close: () => void;
}
let loadingInstance: LoadingInstance | null = null;
export const loadingService = {
  showLoading(): () => void {
    if (!loadingInstance) {
      const config = {
        ...DEFAULT_LOADING_CONFIG,
        background: getLoadingBackground(),
      };
      loadingInstance = ElLoading.service(config);
    }
    return () => this.hideLoading();
  },
  hideLoading(): void {
    if (loadingInstance) {
      loadingInstance.close();
      loadingInstance = null;
    }
  },
};
