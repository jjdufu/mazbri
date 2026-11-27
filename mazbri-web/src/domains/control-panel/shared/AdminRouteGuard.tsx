import type { ReactNode } from "react";
import { Navigate } from "react-router";
import { useAuthStore } from "@/stores/auth";
import { ROLES_ADMIN, canAccessRoles } from "@/utils/ops/roles";

export function AdminRouteGuard({ children }: { children: ReactNode }) {
  const role = useAuthStore((s) => s.info.role);
  if (!canAccessRoles(role, ROLES_ADMIN)) {
    return <Navigate to="/403" replace />;
  }
  return <>{children}</>;
}
