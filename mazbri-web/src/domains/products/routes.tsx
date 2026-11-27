import type { ReactNode } from "react";
import { Navigate, type RouteObject } from "react-router";
import { useAuthStore } from "@/stores/auth";
import type { AdminRole } from "@/types/api";
import { ROLES_ALL, canAccessRoles } from "@/utils/ops/roles";
import { ProductAuthorizationPage } from "./ProductAuthorizationPage";
import { ProductDetailPage } from "./ProductDetailPage";

function RequireProductsRole({
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

export const productsRoutes: RouteObject[] = [
  {
    path: "/operations/product-authorization",
    element: (
      <RequireProductsRole allowedRoles={ROLES_ALL}>
        <ProductAuthorizationPage />
      </RequireProductsRole>
    ),
  },
  {
    path: "/operations/products/:id",
    element: (
      <RequireProductsRole allowedRoles={ROLES_ALL}>
        <ProductDetailPage />
      </RequireProductsRole>
    ),
  },
];
