import { AppRouteRecord } from "@/types";
export function isIframe(url: string): boolean {
  return url.startsWith("/outside/iframe/");
}
const isValidMenuItem = (menuItem: AppRouteRecord): boolean => {
  return !!(menuItem.path && menuItem.path.trim() && !menuItem.meta?.isHide);
};
const normalizePath = (path: string): string => {
  return path.startsWith("/") ? path : `/${path}`;
};
export const getFirstMenuPath = (menuList: AppRouteRecord[]): string => {
  if (!Array.isArray(menuList) || menuList.length === 0) {
    return "";
  }
  for (const menuItem of menuList) {
    if (!isValidMenuItem(menuItem)) {
      continue;
    }
    if (menuItem.children?.length) {
      const childPath = getFirstMenuPath(menuItem.children);
      if (childPath) {
        return childPath;
      }
    }
    return normalizePath(menuItem.path!);
  }
  return "";
};
