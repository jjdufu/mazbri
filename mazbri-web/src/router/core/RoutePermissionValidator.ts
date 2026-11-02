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
    for (const menuPath of pathSet) {
      if (this.pathMatches(targetPath, menuPath)) {
        return true;
      }
    }
    return false;
  }
  static buildMenuPathSet(
    menuList: AppRouteRecord[],
    pathSet: Set<string> = new Set(),
  ): Set<string> {
    if (!Array.isArray(menuList) || menuList.length === 0) {
      return pathSet;
    }
    for (const menuItem of menuList) {
      // 隐藏页（总览/大屏/详情）仍需纳入直链权限集合；仅跳过无 path。
      if (!menuItem.path) {
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
  /** Exact match or vue-router-style :param / * pattern. No blanket prefix. */
  static pathMatches(targetPath: string, menuPath: string): boolean {
    if (targetPath === menuPath) {
      return true;
    }
    if (!menuPath.includes(":") && !menuPath.includes("*")) {
      return false;
    }
    const pattern = menuPath
      .replace(/:[^/]+/g, "[^/]+")
      .replace(/\*/g, ".*");
    return new RegExp(`^${pattern}$`).test(targetPath);
  }
  static checkPathPrefix(targetPath: string, pathSet: Set<string>): boolean {
    // Kept for callers; prefer pathMatches. Prefix only when menuPath itself is a pattern parent.
    for (const menuPath of pathSet) {
      if (this.pathMatches(targetPath, menuPath)) {
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
