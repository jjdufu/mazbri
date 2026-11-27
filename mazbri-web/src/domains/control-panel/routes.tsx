import type { RouteObject } from "react-router";
import { AuditLogsPage } from "./audit-logs/AuditLogsPage";
import { CertificatesPage } from "./certificates/CertificatesPage";
import { ConfigCenterPage } from "./config-center/ConfigCenterPage";
import { AdminRouteGuard } from "./shared/AdminRouteGuard";
import { SystemSettingsPage } from "./system/SystemSettingsPage";
import { UsersPage } from "./users/UsersPage";

export const controlPanelRoutes: RouteObject[] = [
  {
    path: "/control-panel/users",
    element: (
      <AdminRouteGuard>
        <UsersPage />
      </AdminRouteGuard>
    ),
  },
  {
    path: "/control-panel/certificates",
    element: (
      <AdminRouteGuard>
        <CertificatesPage />
      </AdminRouteGuard>
    ),
  },
  {
    path: "/control-panel/system",
    element: (
      <AdminRouteGuard>
        <SystemSettingsPage />
      </AdminRouteGuard>
    ),
  },
  {
    path: "/control-panel/audit-logs",
    element: (
      <AdminRouteGuard>
        <AuditLogsPage />
      </AdminRouteGuard>
    ),
  },
  {
    path: "/control-panel/config-center",
    element: (
      <AdminRouteGuard>
        <ConfigCenterPage />
      </AdminRouteGuard>
    ),
  },
];
