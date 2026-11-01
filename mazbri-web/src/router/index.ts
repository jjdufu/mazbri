import type { App } from "vue";
import { createRouter, createWebHashHistory } from "vue-router";
import { staticRoutes } from "./routes/staticRoutes";
import { configureNProgress } from "@/utils/router";
import { setupBeforeEachGuard } from "./guards/beforeEach";
import { setupAfterEachGuard } from "./guards/afterEach";
export const router = createRouter({
  history: createWebHashHistory(),
  routes: staticRoutes,
});
export function initRouter(app: App<Element>): void {
  configureNProgress();
  setupBeforeEachGuard(router);
  setupAfterEachGuard(router);
  app.use(router);
}
/** Role priority for the landing page: R_ADMIN > R_FINANCE > R_VIEWER. */
const HOME_ROLE_PATHS: Array<{ role: string; path: string }> = [
  { role: "R_ADMIN", path: "/dashboard" },
  { role: "R_FINANCE", path: "/finance/overview" },
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
  // Without a checker, only allow known role homes or ops paths that all ops roles share.
  const set = new Set(roleList);
  const allowedPrefix =
    set.has("R_ADMIN") || set.has("R_FINANCE") || set.has("R_VIEWER");
  if (allowedPrefix && (pref.startsWith("/ops/") || pref === "/dashboard" || pref.startsWith("/finance/"))) {
    if (set.has("R_FINANCE") && !set.has("R_ADMIN") && pref.startsWith("/ops/")) {
      // finance may open ops viewer pages in this console
    }
    return pref;
  }
  return roleHome;
}

