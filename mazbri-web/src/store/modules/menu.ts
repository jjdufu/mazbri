import { defineStore } from "pinia";
import { ref } from "vue";
import { AppRouteRecord } from "@/types/router";
import { getFirstMenuPath } from "@/utils";
import { resolveHomePath } from "@/router";
import { RoutePermissionValidator } from "@/router/core";
import { RoutesAlias } from "@/router/routesAlias";
import { useUserStore } from "@/store/modules/user";
import { putPreferences } from "@/api/preferences";

export const useMenuStore = defineStore("menuStore", () => {
  const homePath = ref(resolveHomePath([]));
  const menuList = ref<AppRouteRecord[]>([]);
  const menuWidth = ref("");
  const removeRouteFns = ref<(() => void)[]>([]);

  const setMenuList = (list: AppRouteRecord[]) => {
    menuList.value = list;
    const roles = (useUserStore().info?.roles as string[] | undefined) ?? [];
    setHomePath(
      resolveHomePath(roles, undefined, (path) =>
        RoutePermissionValidator.hasPermission(path, list),
      ) || getFirstMenuPath(list),
    );
  };

  const rememberLastPath = (path: string) => {
    const userStore = useUserStore();
    if (!path || !userStore.isLogin) return;
    if (path.startsWith("/auth/") || path === RoutesAlias.Login) return;
    if (path.startsWith("/exception/")) return;
    // 个人 UI 偏好：不进安全审计；失败静默（API 已 showErrorMessage:false）。
    void putPreferences({ last_path: path }).catch(() => undefined);
  };

  const getHomePath = () => homePath.value;
  const setHomePath = (path: string) => {
    homePath.value = path;
  };
  const addRemoveRouteFns = (fns: (() => void)[]) => {
    removeRouteFns.value.push(...fns);
  };
  const removeAllDynamicRoutes = () => {
    removeRouteFns.value.forEach((fn) => fn());
    removeRouteFns.value = [];
  };
  const clearRemoveRouteFns = () => {
    removeRouteFns.value = [];
  };
  return {
    menuList,
    menuWidth,
    removeRouteFns,
    setMenuList,
    getHomePath,
    setHomePath,
    rememberLastPath,
    addRemoveRouteFns,
    removeAllDynamicRoutes,
    clearRemoveRouteFns,
  };
});
