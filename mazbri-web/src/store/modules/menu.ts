import { defineStore } from "pinia";
import { ref } from "vue";
import { AppRouteRecord } from "@/types/router";
import { getFirstMenuPath } from "@/utils";
import { resolveHomePath } from "@/router";
import { useUserStore } from "@/store/modules/user";
import { getPreferences, putPreferences } from "@/api/preferences";

export const useMenuStore = defineStore("menuStore", () => {
  const homePath = ref(resolveHomePath([]));
  const menuList = ref<AppRouteRecord[]>([]);
  const menuWidth = ref("");
  const removeRouteFns = ref<(() => void)[]>([]);

  const pathAllowedForRoles = (path: string, roles: string[]) => {
    const set = new Set(roles);
    if (!set.has("R_ADMIN") && !set.has("R_FINANCE") && !set.has("R_VIEWER")) {
      return false;
    }
    // Preference route must exist in registered menu paths (role-filtered already).
    const flat: string[] = [];
    const walk = (items: AppRouteRecord[]) => {
      for (const it of items) {
        if (it.path) flat.push(it.path);
        if (it.children?.length) walk(it.children as AppRouteRecord[]);
      }
    };
    walk(menuList.value);
    if (flat.some((p) => p === path || path.startsWith(p + "/"))) return true;
    // Also allow hash-console absolute paths used as home.
    return (
      path === "/dashboard" ||
      path.startsWith("/ops/") ||
      path.startsWith("/finance/")
    );
  };

  const setMenuList = (list: AppRouteRecord[]) => {
    menuList.value = list;
    const roles = (useUserStore().info?.roles as string[] | undefined) ?? [];
    // Sync path first from roles, then overlay preferences asynchronously.
    setHomePath(resolveHomePath(roles) || getFirstMenuPath(list));
    void (async () => {
      try {
        const pref = (await getPreferences()) as {
          home_path?: string;
          last_path?: string;
        };
        const preferred = (pref.home_path || "").trim();
        const next = resolveHomePath(roles, preferred, pathAllowedForRoles);
        setHomePath(next || getFirstMenuPath(list));
      } catch {
        // ignore preference fetch errors — keep role home
      }
    })();
  };

  const rememberLastPath = (path: string) => {
    if (!path || path.startsWith("/login")) return;
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
