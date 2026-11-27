import type { LocationQueryRaw, Router } from "vue-router";
import type { WorkTab } from "@/types";
import { logger } from "@/utils/sys";
type WorktabNavigator = Pick<Router, "push">;
export function pushWorktabNavigation(
  navigator: WorktabNavigator | null,
  tab: Partial<WorkTab>,
): void {
  if (!tab.path) {
    logger.warn("Attempted to navigate a worktab without a valid path");
    return;
  }
  if (!navigator) {
    logger.warn("Worktab navigator is not bound; skipping navigation");
    return;
  }
  try {
    navigator.push({
      path: tab.path,
      query: tab.query as LocationQueryRaw,
    });
  } catch (error) {
    logger.error("Failed to navigate worktab:", error);
  }
}
export function isWorktabRouteValid(
  routerInstance: Router,
  tab: Partial<WorkTab>,
): boolean {
  try {
    if (tab.name) {
      const routes = routerInstance.getRoutes();
      if (routes.some((route) => route.name === tab.name)) {
        return true;
      }
    }
    if (tab.path) {
      const resolved = routerInstance.resolve({
        path: tab.path,
        query: (tab.query as LocationQueryRaw) || undefined,
      });
      return resolved.matched.length > 0;
    }
    return false;
  } catch {
    return false;
  }
}
