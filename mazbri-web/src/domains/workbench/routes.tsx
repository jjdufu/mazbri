import type { ReactNode } from "react";
import { Navigate, type RouteObject } from "react-router";
import { useAuthStore } from "@/stores/auth";
import type { AdminRole } from "@/types/api";
import { ROLES_ALL, canAccessRoles } from "@/utils/ops/roles";
import { WorkbenchPage } from "./WorkbenchPage";

function RequireWorkbenchRole({
  allowedRoles,
  children,
}: {
  allowedRoles: readonly AdminRole[];
  children: ReactNode;
}) {
  const role = useAuthStore((s) => s.info.role);
  if (!canAccessRoles(role, allowedRoles)) {
    return <Navigate to="/403" replace />;
  }
  return <>{children}</>;
}

export const workbenchRoutes: RouteObject[] = [
  {
    path: "/operations/upstream-supply/workbench",
    element: (
      <RequireWorkbenchRole allowedRoles={ROLES_ALL}>
        <WorkbenchPage />
      </RequireWorkbenchRole>
    ),
  },
];
