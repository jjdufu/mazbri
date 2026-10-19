import { ref, computed, watch, onMounted } from "vue";
import { useElementSize } from "@vueuse/core";
interface LayoutHeightOptions {
  extraSpacing?: number;
  updateCssVar?: boolean;
  cssVarName?: string;
}
export function useLayoutHeight(options: LayoutHeightOptions = {}) {
  const {
    extraSpacing = 15,
    updateCssVar = true,
    cssVarName = "--art-full-height",
  } = options;
  const headerRef = ref<HTMLElement>();
  const contentHeaderRef = ref<HTMLElement>();
  const { height: headerHeight } = useElementSize(headerRef);
  const { height: contentHeaderHeight } = useElementSize(contentHeaderRef);
  const containerMinHeight = computed(() => {
    const totalHeight =
      headerHeight.value + contentHeaderHeight.value + extraSpacing;
    return `calc(100vh - ${totalHeight}px)`;
  });
  if (updateCssVar) {
    watch(
      containerMinHeight,
      (newHeight) => {
        requestAnimationFrame(() => {
          document.documentElement.style.setProperty(cssVarName, newHeight);
        });
      },
      { immediate: true },
    );
  }
  return {
    containerMinHeight,
    headerRef,
    contentHeaderRef,
    headerHeight,
    contentHeaderHeight,
  };
}
export function useAutoLayoutHeight(
  headerIds: string[] = ["app-header", "app-content-header"],
  options: LayoutHeightOptions = {},
) {
  const {
    extraSpacing = 15,
    updateCssVar = true,
    cssVarName = "--art-full-height",
  } = options;
  const headerRef = ref<HTMLElement>();
  const contentHeaderRef = ref<HTMLElement>();
  const { height: headerHeight } = useElementSize(headerRef);
  const { height: contentHeaderHeight } = useElementSize(contentHeaderRef);
  const containerMinHeight = computed(() => {
    const totalHeight =
      headerHeight.value + contentHeaderHeight.value + extraSpacing;
    return `calc(100vh - ${totalHeight}px)`;
  });
  if (updateCssVar) {
    watch(
      containerMinHeight,
      (newHeight) => {
        requestAnimationFrame(() => {
          document.documentElement.style.setProperty(cssVarName, newHeight);
        });
      },
      { immediate: true },
    );
  }
  onMounted(() => {
    if (typeof document !== "undefined") {
      requestAnimationFrame(() => {
        const header = document.getElementById(headerIds[0]);
        const contentHeader = document.getElementById(headerIds[1]);
        if (header) {
          headerRef.value = header;
        }
        if (contentHeader) {
          contentHeaderRef.value = contentHeader;
        }
      });
    }
  });
  return {
    containerMinHeight,
    headerRef,
    contentHeaderRef,
    headerHeight,
    contentHeaderHeight,
  };
}
