import { defineStore } from "pinia";
import { ref } from "vue";
import { AppRouteRecord } from "@/types/router";
import { getFirstMenuPath } from "@/utils";
import { resolveHomePath } from "@/router";
import { RoutePermissionValidator } from "@/router/core";
import { useUserStore } from "@/store/modules/user";

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
    addRemoveRouteFns,
    removeAllDynamicRoutes,
    clearRemoveRouteFns,
  };
});
