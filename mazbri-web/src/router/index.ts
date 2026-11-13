import type { App } from "vue";
import { createRouter, createWebHashHistory } from "vue-router";
import { staticRoutes } from "./routes/staticRoutes";
import { configureNProgress } from "@/utils/router";
import { setupBeforeEachGuard } from "./guards/beforeEach";
import { setupAfterEachGuard } from "./guards/afterEach";
import { installChunkHardRefresh } from "@/utils/sys/chunk-hard-refresh";
export const router = createRouter({
  history: createWebHashHistory(),
  routes: staticRoutes,
});
export function initRouter(app: App<Element>): void {
  configureNProgress();
  setupBeforeEachGuard(router);
  setupAfterEachGuard(router);
  installChunkHardRefresh(router);
  app.use(router);
}
/** Role priority for the landing page: R_ADMIN > R_FINANCE > R_VIEWER. */
const HOME_ROLE_PATHS: Array<{ role: string; path: string }> = [
  { role: "R_ADMIN", path: "/dashboard" },
  { role: "R_FINANCE", path: "/operations/bills" },
  { role: "R_VIEWER", path: "/dashboard" },
];

/** Role-default home path. Falls back to `/dashboard`. */
export function resolveRoleHomePath(roles: string[] | undefined | null): string {
  const set = new Set(roles ?? []);
  for (const entry of HOME_ROLE_PATHS) {
    if (set.has(entry.role)) {
      return entry.path;
    }
  }
  return "/dashboard";
}

/**
 * Prefer user preference when the route is allowed for the caller's roles;
 * otherwise fall back to the role default (no redirect loop).
 */
export function resolveHomePath(
  roles: string[] | undefined | null,
  preferredPath?: string | null,
  canAccessPreferred?: (path: string, roles: string[]) => boolean,
): string {
  const roleHome = resolveRoleHomePath(roles);
  const pref = (preferredPath || "").trim();
  if (!pref) return roleHome;
  const roleList = roles ?? [];
  if (canAccessPreferred) {
    return canAccessPreferred(pref, roleList) ? pref : roleHome;
  }
  // 无菜单校验器时不做宽前缀放行，避免偏好落到无权限运维页。
  if (pref === roleHome) return pref;
  return roleHome;
}

