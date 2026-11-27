import type { ReactNode } from "react";
import { Navigate, type RouteObject } from "react-router";
import { useAuthStore } from "@/stores/auth";
import type { AdminRole } from "@/types/api";
import { ROLES_ALL, canAccessRoles } from "@/utils/ops/roles";
import { TenantDetailPage } from "./TenantDetailPage";
import { TenantListPage } from "./TenantListPage";

function RequireTenantRole({
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

export const tenantsRoutes: RouteObject[] = [
  {
    path: "/operations/tenants",
    element: (
      <RequireTenantRole allowedRoles={ROLES_ALL}>
        <TenantListPage />
      </RequireTenantRole>
    ),
  },
  {
    path: "/operations/tenants/:code",
    element: (
      <RequireTenantRole allowedRoles={ROLES_ALL}>
        <TenantDetailPage />
      </RequireTenantRole>
    ),
  },
];
