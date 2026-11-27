import { type ReactNode, useEffect, useState } from "react";
import {
  Navigate,
  Outlet,
  createBrowserRouter,
  useLocation,
} from "react-router";
import { RefreshCw, ShieldCheck } from "lucide-react";
import { fetchGetUserInfo } from "@/api/auth";
import { AppShell, resolveDefaultHomePath } from "@/components/layout";
import {
  AuthorizationTooltipButton,
  Button,
  Card,
  Input,
  StatusBadge,
} from "@/components/ui";
import { NotFoundPage } from "@/domains/auth/ExceptionPage";
import { authRoutes } from "@/domains/auth/routes";
import { billingRoutes } from "@/domains/billing/routes";
import { callbacksRoutes } from "@/domains/callbacks/routes";
import { controlPanelRoutes } from "@/domains/control-panel/routes";
import { opsRoutes } from "@/domains/ops/routes";
import { productsRoutes } from "@/domains/products/routes";
import { tenantsRoutes } from "@/domains/tenants/routes";
import { upstreamSupplyRoutes } from "@/domains/upstream-supply/routes";
import { LanguageEnum } from "@/enums/appEnum";
import { useT } from "@/locales";
import { registerAuthLogoutRedirect, useAuthStore } from "@/stores/auth";
import { usePersonalSecurityStore } from "@/stores/personal-security";
import type { AdminRole } from "@/types/api";
import {
  ROLES_ALL,
  canAccessRoles,
  isBizRole,
} from "@/utils/ops/roles";
import { showSuccessToast } from "@/utils/ui/message-guard";

function RequireAuth() {
  const location = useLocation();
  const { t } = useT();
  const isLogin = useAuthStore((s) => s.isLogin);
  const userId = useAuthStore((s) => s.info.id);
  const setUserInfo = useAuthStore((s) => s.setUserInfo);
  const logOut = useAuthStore((s) => s.logOut);
  const [hydrating, setHydrating] = useState(false);

  useEffect(() => {
    if (!isLogin || userId) return;
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
  }, [isLogin, userId, setUserInfo, logOut]);

  if (!isLogin) {
    const redirect = `${location.pathname}${location.search}`;
    const target =
      redirect && redirect !== "/" && !redirect.startsWith("/auth/login")
        ? `/auth/login?redirect=${encodeURIComponent(redirect)}`
        : "/auth/login";
    return <Navigate to={target} replace />;
  }

  if (hydrating && !userId) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-canvas-bg text-muted-foreground text-xs">
        {t("layout.loading")}
      </div>
    );
  }

  return <Outlet />;
}

function RequireRole({
  allowedRoles,
  children,
}: {
  allowedRoles: readonly AdminRole[];
  children: ReactNode;
}) {
  const role = useAuthStore((s) => s.info.role);
  if (role && !canAccessRoles(role, allowedRoles)) {
    return <Navigate to="/403" replace />;
  }
  return <>{children}</>;
}

function IndexRedirect() {
  const role = useAuthStore((s) => s.info.role);
  return <Navigate to={resolveDefaultHomePath(role)} replace />;
}

function DomainPlaceholder({ titleKey }: { titleKey: string }) {
  const { t, locale } = useT();
  const role = useAuthStore((s) => s.info.role);
  const openSecurityDrawer = usePersonalSecurityStore((s) => s.openDrawer);
  const [keyword, setKeyword] = useState("");
  const canWrite = isBizRole(role);

  return (
    <Card className="overflow-hidden" data-testid="domain-placeholder-card">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-3">
        <div className="flex items-center gap-2.5 min-w-0">
          <Input
            id="placeholder-search-input"
            name="search"
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            placeholder={t("common.search")}
            aria-label={t("common.search")}
            data-testid="placeholder-search-input"
            className="w-60"
          />
          <StatusBadge
            status="READY"
            label={locale === LanguageEnum.ZH ? "就绪" : "Ready"}
          />
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <Button
            type="button"
            variant="outline"
            size="md"
            onClick={() => showSuccessToast(t("common.actions.refresh"))}
            data-testid="placeholder-refresh-btn"
          >
            <RefreshCw className="h-3.5 w-3.5" />
            {t("common.actions.refresh")}
          </Button>
          <AuthorizationTooltipButton
            type="button"
            variant="primary"
            size="md"
            disabled={!canWrite}
            disabledReason={t("layout.forbidden.description")}
            onClick={() => openSecurityDrawer("mfa")}
            data-testid="placeholder-security-btn"
          >
            <ShieldCheck className="h-3.5 w-3.5" />
            {t("accountSecurity.drawer.title")}
          </AuthorizationTooltipButton>
        </div>
      </div>
      <div className="p-6 flex items-center justify-between gap-4 text-xs text-muted-foreground">
        <span>{t(titleKey)}</span>
        <code className="font-mono-tech text-[11px]">v{__APP_VERSION__}</code>
      </div>
    </Card>
  );
}

export const router = createBrowserRouter([
  ...authRoutes,
  {
    element: <RequireAuth />,
    children: [
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
          {
            path: "/operations/upstream-supply/workbench",
            element: (
              <RequireRole allowedRoles={ROLES_ALL}>
                <DomainPlaceholder titleKey="upstreamConnector.workbenchTitle" />
              </RequireRole>
            ),
          },
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
