import { nextTick } from "vue";
import { useSettingStore } from "@/store/modules/setting";
import { Router } from "vue-router";
import NProgress from "nprogress";
import { useCommon } from "@/hooks/core/useCommon";
import { loadingService } from "@/utils/ui";
import { routeInitState } from "./routeInitState";
import { useMenuStore } from "@/store/modules/menu";
export function setupAfterEachGuard(router: Router) {
  const { scrollToTop } = useCommon();
  router.afterEach((to) => {
    scrollToTop();
    try {
      useMenuStore().rememberLastPath(to.path);
    } catch {
      /* pinia may be unavailable during early boot */
    }
    const settingStore = useSettingStore();
    if (settingStore.showNprogress) {
      NProgress.done();
      setTimeout(() => {
        NProgress.remove();
      }, 600);
    }
    if (routeInitState.getPendingLoading()) {
      nextTick(() => {
        loadingService.hideLoading();
        routeInitState.resetPendingLoading();
      });
    }
  });
}
