import { defineStore } from "pinia";
import { ref } from "vue";
import { AppRouteRecord } from "@/types/router";
import { getFirstMenuPath } from "@/utils";
import { HOME_PAGE_PATH } from "@/router";
export const useMenuStore = defineStore("menuStore", () => {
  const homePath = ref(HOME_PAGE_PATH);
  const menuList = ref<AppRouteRecord[]>([]);
  const menuWidth = ref("");
  const removeRouteFns = ref<(() => void)[]>([]);
  const setMenuList = (list: AppRouteRecord[]) => {
    menuList.value = list;
    setHomePath(HOME_PAGE_PATH || getFirstMenuPath(list));
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
