import type { ReactNode } from "react";
import { Navigate, type RouteObject } from "react-router";
import { useAuthStore } from "@/stores/auth";
import type { AdminRole } from "@/types/api";
import { ROLES_ALL, canAccessRoles } from "@/utils/ops/roles";
import { SupplierDetailPage } from "./SupplierDetailPage";
import { UpstreamSupplyPage } from "./UpstreamSupplyPage";

function RequireUpstreamSupplyRole({
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

export const upstreamSupplyRoutes: RouteObject[] = [
  {
    path: "/operations/upstream-supply",
    element: (
      <RequireUpstreamSupplyRole allowedRoles={ROLES_ALL}>
        <UpstreamSupplyPage />
      </RequireUpstreamSupplyRole>
    ),
  },
  {
    path: "/operations/upstream-supply/suppliers/:id",
    element: (
      <RequireUpstreamSupplyRole allowedRoles={ROLES_ALL}>
        <SupplierDetailPage />
      </RequireUpstreamSupplyRole>
    ),
  },
];
