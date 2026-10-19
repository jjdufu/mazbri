import type {
  Router,
  RouteLocationNormalized,
  NavigationGuardNext,
} from "vue-router";
import { nextTick } from "vue";
import NProgress from "nprogress";
import { useSettingStore } from "@/store/modules/setting";
import { useUserStore } from "@/store/modules/user";
import { useMenuStore } from "@/store/modules/menu";
import { setWorktab } from "@/utils/navigation";
import { setPageTitle } from "@/utils/router";
import { RoutesAlias } from "../routesAlias";
import { staticRoutes } from "../routes/staticRoutes";
import { loadingService } from "@/utils/ui";
import { useCommon } from "@/hooks/core/useCommon";
import { useWorktabStore } from "@/store/modules/worktab";
import { fetchGetUserInfo, refreshAdminSession } from "@/api/auth";
import { ApiStatus } from "@/utils/http/status";
import { isHttpError } from "@/utils/http/error";
import { logger } from "@/utils/sys/logger";
import { ErrorCodes } from "@/constants/error-codes";
import {
  MenuProcessor,
  IframeRouteManager,
  RoutePermissionValidator,
} from "../core";
import { routeInitState } from "./routeInitState";
import { startupCoordinator } from "./startupCoordinator";
const menuProcessor = new MenuProcessor();
export function getPendingLoading(): boolean {
  return routeInitState.getPendingLoading();
}
export function resetPendingLoading(): void {
  routeInitState.resetPendingLoading();
}
export function getRouteInitFailed(): boolean {
  return routeInitState.getRouteInitFailed();
}
export function resetRouteInitState(): void {
  routeInitState.reset();
}
export function setupBeforeEachGuard(router: Router): void {
  routeInitState.initializeRouteRegistry(router);
  router.beforeEach(
    async (
      to: RouteLocationNormalized,
      from: RouteLocationNormalized,
      next: NavigationGuardNext,
    ) => {
      try {
        await handleRouteGuard(to, from, next, router);
      } catch (error) {
        logger.error("[RouteGuard] Route guard processing failed", {
          service: "mazbri-mgmt-web",
          event: "route_guard_failure",
          severity: "error",
          error_code: ErrorCodes.INTERNAL_ERROR,
          error,
        });
        closeLoading();
        next({ name: "Exception500" });
      }
    },
  );
}
function closeLoading(): void {
  if (routeInitState.getPendingLoading()) {
    nextTick(() => {
      loadingService.hideLoading();
      routeInitState.resetPendingLoading();
    });
  }
}
async function handleRouteGuard(
  to: RouteLocationNormalized,
  from: RouteLocationNormalized,
  next: NavigationGuardNext,
  router: Router,
): Promise<void> {
  const settingStore = useSettingStore();
  const userStore = useUserStore();
  useWorktabStore().bindNavigator(router);
  if (settingStore.showNprogress) {
    NProgress.start();
  }
  if (!handleLoginStatus(to, userStore, next)) {
    return;
  }
  if (handleRouteInitFailureState(to, next)) {
    return;
  }
  if (shouldInitializeDynamicRoutes(userStore)) {
    if (routeInitState.getRouteInitInProgress()) {
      startupCoordinator.deferNavigation(to);
      next(false);
      return;
    }
    await handleDynamicRoutes(to, next, router);
    return;
  }
  if (handleRootPathRedirect(to, next)) {
    return;
  }
  handleResolvedRoute(to, next);
}
function handleLoginStatus(
  to: RouteLocationNormalized,
  userStore: ReturnType<typeof useUserStore>,
  next: NavigationGuardNext,
): boolean {
  if (userStore.isLogin || to.path === RoutesAlias.Login) {
    return true;
  }
  if (isStaticRoute(to.path)) {
    return true;
  }

  refreshAndContinue(userStore, to, next);
  return false;
}
function redirectToLoginAfterSessionFailure(
  userStore: ReturnType<typeof useUserStore>,
  to: RouteLocationNormalized,
  next: NavigationGuardNext,
): void {
  userStore.clearUserState();
  resetRouterState(500);
  next({
    name: "Login",
    query: { redirect: to.fullPath },
  });
}
async function refreshAndContinue(
  userStore: ReturnType<typeof useUserStore>,
  to: RouteLocationNormalized,
  next: NavigationGuardNext,
): Promise<void> {
  try {
    const newToken = await refreshAdminSession();
    if (newToken) {
      next({ path: to.path, query: to.query, hash: to.hash, replace: true });
    } else {
      redirectToLoginAfterSessionFailure(userStore, to, next);
    }
  } catch {
    redirectToLoginAfterSessionFailure(userStore, to, next);
  }
}
function isStaticRoute(path: string): boolean {
  const checkRoute = (routes: any[], targetPath: string): boolean => {
    return routes.some((route) => {
      if (route.path === "/:pathMatch(.*)*") {
        return false;
      }
      const routePath = route.path;
      const pattern = routePath
        .replace(/:[^/]+/g, "[^/]+")
        .replace(/\*/g, ".*");
      const regex = new RegExp(`^${pattern}$`);
      if (regex.test(targetPath)) {
        return true;
      }
      if (route.children && route.children.length > 0) {
        return checkRoute(route.children, targetPath);
      }
      return false;
    });
  };
  return checkRoute(staticRoutes, path);
}
async function handleDynamicRoutes(
  to: RouteLocationNormalized,
  next: NavigationGuardNext,
  router: Router,
): Promise<void> {
  routeInitState.beginRouteInit();
  routeInitState.setPendingLoading(true);
  loadingService.showLoading();
  try {
    await fetchUserInfo();
    const menuList = await menuProcessor.getMenuList();
    if (!menuProcessor.validateMenuList(menuList)) {
      throw new Error("Failed to fetch menu list, please log in again");
    }
    routeInitState.getRouteRegistry()?.register(menuList);
    const menuStore = useMenuStore();
    menuStore.setMenuList(menuList);
    menuStore.addRemoveRouteFns(
      routeInitState.getRouteRegistry()?.getRemoveRouteFns() || [],
    );
    IframeRouteManager.getInstance().save();
    useWorktabStore().validateWorktabs(router);
    const { homePath } = useCommon();
    const { path: validatedPath, hasPermission } =
      RoutePermissionValidator.validatePath(
        to.path,
        menuList,
        homePath.value || "/",
      );
    routeInitState.finishRouteInit();
    if (startupCoordinator.resumeDeferredNavigation(to, next)) {
      closeLoading();
      return;
    }
    if (!hasPermission) {
      closeLoading();
      logger.warn("[RouteGuard] User lacks permission; redirected to home", {
        service: "mazbri-mgmt-web",
        event: "route_guard_permission_denied",
        severity: "warn",
        error_code: ErrorCodes.PERMISSION_DENIED,
        path: to.path,
      });
      next({
        path: validatedPath,
        replace: true,
      });
    } else {
      next({
        path: to.path,
        query: to.query,
        hash: to.hash,
        replace: true,
      });
    }
  } catch (error) {
    const errorCode =
      isHttpError(error) && error.code
        ? String(error.code)
        : ErrorCodes.INTERNAL_ERROR;
    logger.error("[RouteGuard] Dynamic route registration failed", {
      service: "mazbri-mgmt-web",
      event: "route_guard_failure",
      severity: "error",
      error_code: errorCode,
      error,
    });
    closeLoading();
    if (isUnauthorizedError(error)) {
      startupCoordinator.clear();
      routeInitState.finishRouteInit();
      next(false);
      return;
    }
    startupCoordinator.clear();
    routeInitState.markRouteInitFailed();
    routeInitState.finishRouteInit();
    if (isHttpError(error)) {
      logger.error("[RouteGuard] Error details", {
        service: "mazbri-mgmt-web",
        event: "route_guard_failure",
        severity: "error",
        error_code: error.code ? String(error.code) : ErrorCodes.INTERNAL_ERROR,
        message: error.message,
      });
    }
    next({ name: "Exception500", replace: true });
  }
}
async function fetchUserInfo(): Promise<void> {
  const userStore = useUserStore();
  const data = await fetchGetUserInfo();
  userStore.setUserInfo(data);
  userStore.checkAndClearWorktabs();
}
export function resetRouterState(delay: number): void {
  setTimeout(() => {
    routeInitState.getRouteRegistry()?.unregister();
    IframeRouteManager.getInstance().clear();
    const menuStore = useMenuStore();
    menuStore.removeAllDynamicRoutes();
    menuStore.setMenuList([]);
    routeInitState.reset();
  }, delay);
}
function handleRootPathRedirect(
  to: RouteLocationNormalized,
  next: NavigationGuardNext,
): boolean {
  if (to.path !== "/") {
    return false;
  }
  const { homePath } = useCommon();
  if (homePath.value && homePath.value !== "/") {
    next({ path: homePath.value, replace: true });
    return true;
  }
  return false;
}
function handleRouteInitFailureState(
  to: RouteLocationNormalized,
  next: NavigationGuardNext,
): boolean {
  if (!routeInitState.getRouteInitFailed()) {
    return false;
  }
  if (to.matched.length > 0) {
    next();
  } else {
    next({ name: "Exception500", replace: true });
  }
  return true;
}
function shouldInitializeDynamicRoutes(
  userStore: ReturnType<typeof useUserStore>,
): boolean {
  return (
    !routeInitState.getRouteRegistry()?.isRegistered() && userStore.isLogin
  );
}
function handleResolvedRoute(
  to: RouteLocationNormalized,
  next: NavigationGuardNext,
): void {
  if (to.matched.length > 0) {
    setWorktab(to);
    setPageTitle(to);
    next();
    return;
  }
  next({ name: "Exception404" });
}
function isUnauthorizedError(error: unknown): boolean {
  return isHttpError(error) && error.code === ApiStatus.unauthorized;
}
