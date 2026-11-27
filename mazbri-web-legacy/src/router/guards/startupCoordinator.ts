import type { NavigationGuardNext, RouteLocationNormalized } from "vue-router";
import { routeInitState } from "./routeInitState";
function toDeferredTarget(to: RouteLocationNormalized) {
  return {
    path: to.path,
    query: to.query,
    hash: to.hash,
  };
}
function isSameTarget(
  current: RouteLocationNormalized,
  target: {
    path: string;
    query: unknown;
    hash: string;
  },
) {
  return (
    current.path === target.path &&
    JSON.stringify(current.query) === JSON.stringify(target.query) &&
    current.hash === target.hash
  );
}
export const startupCoordinator = {
  deferNavigation(to: RouteLocationNormalized) {
    routeInitState.setDeferredNavigation(toDeferredTarget(to));
  },
  resumeDeferredNavigation(
    to: RouteLocationNormalized,
    next: NavigationGuardNext,
  ): boolean {
    const deferredTarget = routeInitState.consumeDeferredNavigation();
    if (!deferredTarget) {
      return false;
    }
    if (isSameTarget(to, deferredTarget)) {
      return false;
    }
    next({
      ...deferredTarget,
      replace: true,
    });
    return true;
  },
  clear() {
    routeInitState.clearDeferredNavigation();
  },
};
