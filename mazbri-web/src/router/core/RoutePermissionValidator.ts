import type { AppRouteRecord } from "@/types/router";
export class RoutePermissionValidator {
  static hasPermission(
    targetPath: string,
    menuList: AppRouteRecord[],
    homePath?: string,
  ): boolean {
    if (targetPath === "/") {
      return true;
    }
    if (homePath && targetPath === homePath) {
      return true;
    }
    const pathSet = this.buildMenuPathSet(menuList);
    return pathSet.has(targetPath) || this.checkPathPrefix(targetPath, pathSet);
  }
  static buildMenuPathSet(
    menuList: AppRouteRecord[],
    pathSet: Set<string> = new Set(),
  ): Set<string> {
    if (!Array.isArray(menuList) || menuList.length === 0) {
      return pathSet;
    }
    for (const menuItem of menuList) {
      if (menuItem.meta?.isHide || !menuItem.path) {
        continue;
      }
      const menuPath = menuItem.path.startsWith("/")
        ? menuItem.path
        : `/${menuItem.path}`;
      pathSet.add(menuPath);
      if (menuItem.children?.length) {
        this.buildMenuPathSet(menuItem.children, pathSet);
      }
    }
    return pathSet;
  }
  static checkPathPrefix(targetPath: string, pathSet: Set<string>): boolean {
    for (const menuPath of pathSet) {
      if (targetPath.startsWith(`${menuPath}/`)) {
        return true;
      }
    }
    return false;
  }
  static validatePath(
    targetPath: string,
    menuList: AppRouteRecord[],
    homePath: string = "/",
  ): {
    path: string;
    hasPermission: boolean;
  } {
    const hasPermission = this.hasPermission(targetPath, menuList, homePath);
    if (hasPermission) {
      return { path: targetPath, hasPermission: true };
    }
    return { path: homePath, hasPermission: false };
  }
}
