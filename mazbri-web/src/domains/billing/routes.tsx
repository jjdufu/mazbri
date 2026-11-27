import type { ReactNode } from "react";
import { Navigate, type RouteObject } from "react-router";
import { useAuthStore } from "@/stores/auth";
import type { AdminRole } from "@/types/api";
import { ROLES_BIZ, canAccessRoles } from "@/utils/ops/roles";
import { BillingHubPage } from "./BillingHubPage";

function RequireBillingRole({
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

export const billingRoutes: RouteObject[] = [
  {
    path: "/operations/bills",
    element: (
      <RequireBillingRole allowedRoles={ROLES_BIZ}>
        <BillingHubPage />
      </RequireBillingRole>
    ),
  },
];
