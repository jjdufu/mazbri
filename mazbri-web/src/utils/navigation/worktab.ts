import { useWorktabStore } from "@/store/modules/worktab";
import { RouteLocationNormalized } from "vue-router";
import { isIframe } from "./route";
import { useSettingStore } from "@/store/modules/setting";
import { IframeRouteManager } from "@/router/core";
import { useCommon } from "@/hooks/core/useCommon";
import type { WorkTab } from "@/types";
import type { LocationQueryRaw } from "vue-router";
export const setWorktab = (to: RouteLocationNormalized): void => {
  const worktabStore = useWorktabStore();
  const { meta, path, name, params, query } = to;
  if (!meta.isHideTab) {
    if (isIframe(path)) {
      const iframeRoute = IframeRouteManager.getInstance().findByPath(to.path);
      if (iframeRoute?.meta) {
        worktabStore.openTab({
          title: iframeRoute.meta.title,
          icon: meta.icon as string,
          path,
          name: name as string,
          keepAlive: meta.keepAlive as boolean,
          params,
          query,
        });
      }
    } else if (
      useSettingStore().showWorkTab ||
      path === useCommon().homePath.value
    ) {
      worktabStore.openTab({
        title: meta.title as string,
        icon: meta.icon as string,
        path,
        name: name as string,
        keepAlive: meta.keepAlive as boolean,
        params,
        query,
        fixedTab: meta.fixedTab as boolean,
      });
    }
  }
};
export const resolveWorktabNavigationTarget = (
  tabs: WorkTab[],
  targetIndex: number,
  homePath: string,
): Partial<WorkTab> => {
  if (tabs.length === 0) {
    return { path: homePath };
  }
  const safeIndex = targetIndex >= tabs.length ? tabs.length - 1 : targetIndex;
  const tab = tabs[safeIndex];
  return {
    path: tab.path,
    query: tab.query as LocationQueryRaw,
  };
};
export const resolveWorktabFallbackTarget = (
  tabs: WorkTab[],
  homePath: string,
): Partial<WorkTab> => {
  const homeTab = tabs.find((tab) => tab.path === homePath);
  if (homeTab) {
    return {
      path: homeTab.path,
      query: homeTab.query as LocationQueryRaw,
    };
  }
  const fallbackTab = tabs[0];
  if (!fallbackTab) {
    return { path: homePath };
  }
  return {
    path: fallbackTab.path,
    query: fallbackTab.query as LocationQueryRaw,
  };
};
export const shouldKeepTabAfterRemoveAll = (
  tab: WorkTab,
  hasFixedTabs: boolean,
  homePath: string,
): boolean => {
  return tab.fixedTab || (!hasFixedTabs && tab.path === homePath);
};
