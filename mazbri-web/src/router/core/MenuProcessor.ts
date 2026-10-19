import type { AppRouteRecord } from "@/types/router";
import { useUserStore } from "@/store/modules/user";
import { useAppMode } from "@/hooks/core/useAppMode";
import { fetchGetMenuList } from "@/api/system-manage";
import { asyncRoutes } from "../routes/asyncRoutes";
import { RoutesAlias } from "../routesAlias";
import { formatMenuTitle } from "@/utils";
import { logger } from "@/utils/sys/logger";
import { ErrorCodes } from "@/constants/error-codes";
export class MenuProcessor {
  async getMenuList(): Promise<AppRouteRecord[]> {
    const { isFrontendMode } = useAppMode();
    let menuList: AppRouteRecord[];
    if (isFrontendMode.value) {
      menuList = await this.processFrontendMenu();
    } else {
      menuList = await this.processBackendMenu();
    }
    this.validateMenuPaths(menuList);
    return this.normalizeMenuPaths(menuList);
  }
  private async processFrontendMenu(): Promise<AppRouteRecord[]> {
    const userStore = useUserStore();
    const roles = userStore.info?.roles;
    let menuList = [...asyncRoutes];
    if (roles && roles.length > 0) {
      menuList = this.filterMenuByRoles(menuList, roles);
    }
    return this.filterEmptyMenus(menuList);
  }
  private async processBackendMenu(): Promise<AppRouteRecord[]> {
    const list = await fetchGetMenuList();
    return this.filterEmptyMenus(list);
  }
  private filterMenuByRoles(
    menu: AppRouteRecord[],
    roles: string[],
  ): AppRouteRecord[] {
    return menu.reduce((acc: AppRouteRecord[], item) => {
      const itemRoles = item.meta?.roles;
      const hasPermission =
        !itemRoles || itemRoles.some((role) => roles?.includes(role));
      if (hasPermission) {
        const filteredItem = { ...item };
        if (filteredItem.children?.length) {
          filteredItem.children = this.filterMenuByRoles(
            filteredItem.children,
            roles,
          );
        }
        acc.push(filteredItem);
      }
      return acc;
    }, []);
  }
  private filterEmptyMenus(menuList: AppRouteRecord[]): AppRouteRecord[] {
    return menuList
      .map((item) => {
        if (item.children && item.children.length > 0) {
          const filteredChildren = this.filterEmptyMenus(item.children);
          return {
            ...item,
            children: filteredChildren,
          };
        }
        return item;
      })
      .filter((item) => {
        if ("children" in item) {
          return true;
        }
        if (item.meta?.isIframe === true || item.meta?.link) {
          return true;
        }
        if (
          item.component &&
          item.component !== "" &&
          item.component !== RoutesAlias.Layout
        ) {
          return true;
        }
        return false;
      });
  }
  validateMenuList(menuList: AppRouteRecord[]): boolean {
    return Array.isArray(menuList) && menuList.length > 0;
  }
  private normalizeMenuPaths(
    menuList: AppRouteRecord[],
    parentPath = "",
  ): AppRouteRecord[] {
    return menuList.map((item) => {
      const fullPath = this.buildFullPath(item.path || "", parentPath);
      const children = item.children?.length
        ? this.normalizeMenuPaths(item.children, fullPath)
        : item.children;
      return {
        ...item,
        path: fullPath,
        children,
      };
    });
  }
  private validateMenuPaths(menuList: AppRouteRecord[], level = 1): void {
    menuList.forEach((route) => {
      if (!route.children?.length) return;
      const parentName = String(route.name || route.path || "Unknown route");
      route.children.forEach((child) => {
        const childPath = child.path || "";
        if (this.isValidAbsolutePath(childPath)) return;
        if (childPath.startsWith("/")) {
          this.logPathError(child, childPath, parentName, level);
        }
      });
      this.validateMenuPaths(route.children, level + 1);
    });
  }
  private isValidAbsolutePath(path: string): boolean {
    return (
      path.startsWith("http://") ||
      path.startsWith("https://") ||
      path.startsWith("/outside/iframe/")
    );
  }
  private logPathError(
    route: AppRouteRecord,
    path: string,
    parentName: string,
    level: number,
  ): void {
    const routeName = String(route.name || path || "Unknown route");
    const menuTitle = route.meta?.title || routeName;
    const suggestedPath = path.split("/").pop() || path.slice(1);
    logger.error(
      `[MenuProcessor] Invalid path configuration: menu "${formatMenuTitle(menuTitle)}" (name: ${routeName}, path: ${path})`,
      {
        service: "mazbri-mgmt-web",
        event: "menu_processor_path_error",
        severity: "error",
        error_code: ErrorCodes.VALIDATION_ERROR,
        parent: parentName,
        suggested_path: suggestedPath,
      },
    );
  }
  private buildFullPath(path: string, parentPath: string): string {
    if (!path) {
      return parentPath || "";
    }
    if (path.startsWith("http://") || path.startsWith("https://")) {
      return path;
    }
    if (path.startsWith("/")) {
      return path;
    }
    if (parentPath) {
      const cleanParent = parentPath.replace(/\/$/, "");
      const cleanChild = path.replace(/^\//, "");
      return `${cleanParent}/${cleanChild}`;
    }
    return `/${path}`;
  }
}
