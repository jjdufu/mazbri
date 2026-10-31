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

/** Resolve the role-based home path. Falls back to `/dashboard`. */
export function resolveHomePath(roles: string[] | undefined | null): string {
  const set = new Set(roles ?? []);
  for (const entry of HOME_ROLE_PATHS) {
    if (set.has(entry.role)) {
      return entry.path;
    }
  }
  return "/dashboard";
}

