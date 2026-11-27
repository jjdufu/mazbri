import type { ReactNode } from "react";
import { Navigate, type RouteObject } from "react-router";
import { useAuthStore } from "@/stores/auth";
import type { AdminRole } from "@/types/api";
import { ROLES_ALL, canAccessRoles } from "@/utils/ops/roles";
import { CallbacksPage } from "./CallbacksPage";

function RequireCallbacksRole({
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

export const callbacksRoutes: RouteObject[] = [
  {
    path: "/operations/callbacks",
    element: (
      <RequireCallbacksRole allowedRoles={ROLES_ALL}>
        <CallbacksPage />
      </RequireCallbacksRole>
    ),
  },
];
