import { useEffect, useState } from "react";
import {
  Navigate,
  Outlet,
  createBrowserRouter,
  useLocation,
} from "react-router";
import { fetchGetUserInfo } from "@/api/auth";
import { AppShell, resolveDefaultHomePath } from "@/components/layout";
import { NotFoundPage } from "@/domains/auth/ExceptionPage";
import { authRoutes } from "@/domains/auth/routes";
import { billingRoutes } from "@/domains/billing/routes";
import { callbacksRoutes } from "@/domains/callbacks/routes";
import { controlPanelRoutes } from "@/domains/control-panel/routes";
import { opsRoutes } from "@/domains/ops/routes";
import { productsRoutes } from "@/domains/products/routes";
import { tenantsRoutes } from "@/domains/tenants/routes";
import { upstreamSupplyRoutes } from "@/domains/upstream-supply/routes";
import { workbenchRoutes } from "@/domains/workbench/routes";
import { useT } from "@/locales";
import { registerAuthLogoutRedirect, useAuthStore } from "@/stores/auth";

function RequireAuth() {
  const location = useLocation();
  const { t } = useT();
  const isLogin = useAuthStore((s) => s.isLogin);
  const userId = useAuthStore((s) => s.info.id);
  const userRole = useAuthStore((s) => s.info.role);
  const setUserInfo = useAuthStore((s) => s.setUserInfo);
  const logOut = useAuthStore((s) => s.logOut);
  const needsHydration = isLogin && (!userId || !userRole);
  const [hydrating, setHydrating] = useState(needsHydration);

  useEffect(() => {
    if (!needsHydration) {
      setHydrating(false);
      return;
    }
    let active = true;
    setHydrating(true);
    fetchGetUserInfo()
      .then((user) => {
        if (active) setUserInfo(user);
      })
      .catch(() => {
        if (active) void logOut({ skipRemote: true });
      })
      .finally(() => {
        if (active) setHydrating(false);
      });
    return () => {
      active = false;
    };
  }, [needsHydration, setUserInfo, logOut]);

  if (!isLogin) {
    const redirect = `${location.pathname}${location.search}`;
    const target =
      redirect && redirect !== "/" && !redirect.startsWith("/auth/login")
        ? `/auth/login?redirect=${encodeURIComponent(redirect)}`
        : "/auth/login";
    return <Navigate to={target} replace />;
  }

  if (needsHydration || hydrating) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-canvas-bg text-muted-foreground text-xs">
        {t("layout.loading")}
      </div>
    );
  }

  return <Outlet />;
}

function IndexRedirect() {
  const role = useAuthStore((s) => s.info.role);
  return <Navigate to={resolveDefaultHomePath(role)} replace />;
}

export const router = createBrowserRouter([
  ...authRoutes,
  {
    element: <RequireAuth />,
    children: [
      ...workbenchRoutes,
      {
        element: <AppShell />,
        children: [
          {
            index: true,
            element: <IndexRedirect />,
          },
          ...tenantsRoutes,
          ...productsRoutes,
          ...upstreamSupplyRoutes,
          ...callbacksRoutes,
          ...billingRoutes,
          ...opsRoutes,
          ...controlPanelRoutes,
        ],
      },
    ],
  },
  {
    path: "*",
    element: <NotFoundPage />,
  },
]);

registerAuthLogoutRedirect(() => {
  const { pathname, search } = router.state.location;
  if (pathname === "/auth/login") return;
  const redirect = `${pathname}${search}`;
  const target =
    redirect && redirect !== "/"
      ? `/auth/login?redirect=${encodeURIComponent(redirect)}`
      : "/auth/login";
  void router.navigate(target, { replace: true });
});
