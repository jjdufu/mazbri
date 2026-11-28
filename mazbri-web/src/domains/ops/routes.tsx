import React, { type ReactNode } from "react";
import { Navigate, type RouteObject } from "react-router";
import { useAuthStore } from "@/stores/auth";
import type { AdminRole } from "@/types/api";
import {
  ROLES_ADMIN,
  ROLES_ALL,
  ROLES_BIZ,
  canAccessRoles,
} from "@/utils/ops/roles";
import { AlertsPage } from "./alerts/AlertsPage";
import { OverviewPage } from "./overview/OverviewPage";
import { InfraPage } from "./runtime/InfraPage";
import { NodesPage } from "./runtime/NodesPage";
import { PipelinePage } from "./runtime/PipelinePage";
import { ServiceDetailPage } from "./services/ServiceDetailPage";
import { TraceDetailPage } from "./traces/TraceDetailPage";
import { TrafficPage } from "./traffic/TrafficPage";

function RequireOpsRole({
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

export const opsRoutes: RouteObject[] = [
  {
    path: "/dashboard",
    element: (
      <RequireOpsRole allowedRoles={ROLES_ALL}>
        <OverviewPage />
      </RequireOpsRole>
    ),
  },
  {
    path: "/ops/traffic",
    element: (
      <RequireOpsRole allowedRoles={ROLES_ALL}>
        <TrafficPage />
      </RequireOpsRole>
    ),
  },
  {
    path: "/ops/traces/:traceId",
    element: (
      <RequireOpsRole allowedRoles={ROLES_ADMIN}>
        <TraceDetailPage />
      </RequireOpsRole>
    ),
  },
  {
    path: "/ops/alerts",
    element: (
      <RequireOpsRole allowedRoles={ROLES_ALL}>
        <AlertsPage />
      </RequireOpsRole>
    ),
  },
  {
    path: "/ops/nodes",
    element: (
      <RequireOpsRole allowedRoles={ROLES_ALL}>
        <NodesPage />
      </RequireOpsRole>
    ),
  },
  {
    path: "/ops/infra",
    element: (
      <RequireOpsRole allowedRoles={ROLES_ADMIN}>
        <InfraPage />
      </RequireOpsRole>
    ),
  },
  {
    path: "/ops/pipeline",
    element: (
      <RequireOpsRole allowedRoles={ROLES_BIZ}>
        <PipelinePage />
      </RequireOpsRole>
    ),
  },
  {
    path: "/ops/services/:service",
    element: (
      <RequireOpsRole allowedRoles={ROLES_ADMIN}>
        <ServiceDetailPage />
      </RequireOpsRole>
    ),
  },
];
