import { nextTick, watch } from "vue";
import type { Ref } from "vue";
const RESIZE_DELAYS = [50, 100, 200, 350] as const;
const MENU_RESIZE_DELAYS = [50, 100, 200] as const;
const RESIZE_DEBOUNCE_DELAY = 100;
export function createChartResizeController(options: {
  menuOpen: Ref<boolean>;
  menuType: Ref<string>;
  handleResize: () => void;
}) {
  const { menuOpen, menuType, handleResize } = options;
  let resizeTimeoutId: number | null = null;
  let resizeFrameId: number | null = null;
  let menuOpenStopHandle: (() => void) | null = null;
  let menuTypeStopHandle: (() => void) | null = null;
  const clearTimers = () => {
    if (resizeTimeoutId) {
      clearTimeout(resizeTimeoutId);
      resizeTimeoutId = null;
    }
    if (resizeFrameId) {
      cancelAnimationFrame(resizeFrameId);
      resizeFrameId = null;
    }
  };
  const requestAnimationResize = () => {
    if (resizeFrameId) {
      cancelAnimationFrame(resizeFrameId);
    }
    resizeFrameId = requestAnimationFrame(() => {
      handleResize();
      resizeFrameId = null;
    });
  };
  const debouncedResize = () => {
    if (resizeTimeoutId) {
      clearTimeout(resizeTimeoutId);
    }
    resizeTimeoutId = window.setTimeout(() => {
      requestAnimationResize();
      resizeTimeoutId = null;
    }, RESIZE_DEBOUNCE_DELAY);
  };
  const multiDelayResize = (delays: readonly number[]) => {
    nextTick(requestAnimationResize);
    delays.forEach((delay) => {
      setTimeout(requestAnimationResize, delay);
    });
  };
  const setupMenuWatchers = () => {
    menuOpenStopHandle = watch(menuOpen, () => multiDelayResize(RESIZE_DELAYS));
    menuTypeStopHandle = watch(menuType, () => {
      nextTick(requestAnimationResize);
      setTimeout(() => multiDelayResize(MENU_RESIZE_DELAYS), 0);
    });
  };
  const cleanupMenuWatchers = () => {
    menuOpenStopHandle?.();
    menuTypeStopHandle?.();
    menuOpenStopHandle = null;
    menuTypeStopHandle = null;
  };
  return {
    clearTimers,
    debouncedResize,
    requestAnimationResize,
    setupMenuWatchers,
    cleanupMenuWatchers,
  };
}
