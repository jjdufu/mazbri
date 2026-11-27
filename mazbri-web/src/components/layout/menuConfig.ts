import {
  Activity,
  Award,
  BellRing,
  Building2,
  FileClock,
  GitFork,
  LayoutDashboard,
  Network,
  ReceiptText,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
  Users,
  Webhook,
  type LucideIcon,
} from "lucide-react";
import type { AdminRole } from "@/types/api";
import {
  ROLES_ADMIN,
  ROLES_ALL,
  ROLES_BIZ,
  canAccessRoles,
  type RoleLike,
} from "@/utils/ops/roles";

export interface MenuItem {
  readonly key: string;
  readonly path: string;
  readonly titleKey: string;
  readonly icon: LucideIcon;
  readonly roles: readonly AdminRole[];
}

export interface MenuGroup {
  readonly key: string;
  readonly titleKey: string;
  readonly roles: readonly AdminRole[];
  readonly items: readonly MenuItem[];
}

interface DetailRouteRule {
  readonly pattern: RegExp;
  readonly groupKey: string;
  readonly activePath: string;
}

export const MENU_GROUPS: readonly MenuGroup[] = [
  {
    key: "operations",
    titleKey: "menus.mazbri.opsManagement",
    roles: ROLES_ALL,
    items: [
      {
        key: "tenants",
        path: "/operations/tenants",
        titleKey: "menus.mazbri.tenantList",
        icon: Building2,
        roles: ROLES_ALL,
      },
      {
        key: "product-authorization",
        path: "/operations/product-authorization",
        titleKey: "menus.mazbri.productAuthorization",
        icon: ShieldCheck,
        roles: ROLES_ALL,
      },
      {
        key: "upstream-supply",
        path: "/operations/upstream-supply",
        titleKey: "menus.mazbri.upstreamSupply",
        icon: Network,
        roles: ROLES_ALL,
      },
      {
        key: "callbacks",
        path: "/operations/callbacks",
        titleKey: "menus.mazbri.callbackList",
        icon: Webhook,
        roles: ROLES_ALL,
      },
      {
        key: "bills",
        path: "/operations/bills",
        titleKey: "menus.mazbri.billsAndReconciliation",
        icon: ReceiptText,
        roles: ROLES_BIZ,
      },
    ],
  },
  {
    key: "ops",
    titleKey: "menus.mazbri.opsMonitoring",
    roles: ROLES_ALL,
    items: [
      {
        key: "ops-overview",
        path: "/dashboard",
        titleKey: "menus.mazbri.opsOverview",
        icon: LayoutDashboard,
        roles: ROLES_ALL,
      },
      {
        key: "ops-traffic",
        path: "/ops/traffic",
        titleKey: "menus.mazbri.opsTraffic",
        icon: GitFork,
        roles: ROLES_ALL,
      },
      {
        key: "ops-alerts",
        path: "/ops/alerts",
        titleKey: "menus.mazbri.opsAlerts",
        icon: BellRing,
        roles: ROLES_ALL,
      },
      {
        key: "ops-runtime",
        path: "/ops/runtime",
        titleKey: "menus.mazbri.opsRuntime",
        icon: Activity,
        roles: ROLES_ALL,
      },
    ],
  },
  {
    key: "control-panel",
    titleKey: "menus.mazbri.controlPanel",
    roles: ROLES_ADMIN,
    items: [
      {
        key: "users",
        path: "/control-panel/users",
        titleKey: "menus.mazbri.userManagement",
        icon: Users,
        roles: ROLES_ADMIN,
      },
      {
        key: "certificates",
        path: "/control-panel/certificates",
        titleKey: "menus.mazbri.certificates",
        icon: Award,
        roles: ROLES_ADMIN,
      },
      {
        key: "system",
        path: "/control-panel/system",
        titleKey: "menus.mazbri.platformSettings",
        icon: Settings,
        roles: ROLES_ADMIN,
      },
      {
        key: "audit-logs",
        path: "/control-panel/audit-logs",
        titleKey: "menus.mazbri.auditLogs",
        icon: FileClock,
        roles: ROLES_ADMIN,
      },
      {
        key: "config-center",
        path: "/control-panel/config-center",
        titleKey: "menus.mazbri.configCenter",
        icon: SlidersHorizontal,
        roles: ROLES_ADMIN,
      },
    ],
  },
];

const DETAIL_ROUTE_RULES: readonly DetailRouteRule[] = [
  {
    pattern: /^\/operations\/tenants\/[^/]+$/,
    groupKey: "operations",
    activePath: "/operations/tenants",
  },
  {
    pattern: /^\/operations\/products\/[^/]+$/,
    groupKey: "operations",
    activePath: "/operations/product-authorization",
  },
  {
    pattern: /^\/operations\/upstream-supply\/suppliers\/[^/]+$/,
    groupKey: "operations",
    activePath: "/operations/upstream-supply",
  },
  {
    pattern: /^\/operations\/upstream-supply\/workbench(?:\/.*)?$/,
    groupKey: "operations",
    activePath: "/operations/upstream-supply",
  },
  {
    pattern: /^\/ops\/traces\/[^/]+$/,
    groupKey: "ops",
    activePath: "/ops/traffic",
  },
  {
    pattern: /^\/ops\/services\/[^/]+$/,
    groupKey: "ops",
    activePath: "/ops/runtime",
  },
];

function normalizePathname(pathname: string): string {
  const cleaned = pathname.split("?")[0]?.split("#")[0] || "/";
  if (cleaned.length > 1 && cleaned.endsWith("/")) {
    return cleaned.slice(0, -1);
  }
  return cleaned;
}

export function getFilteredMenuGroups(role: RoleLike): MenuGroup[] {
  const groups: MenuGroup[] = [];
  for (const group of MENU_GROUPS) {
    if (!canAccessRoles(role, group.roles)) continue;
    const items = group.items.filter((item) => canAccessRoles(role, item.roles));
    if (items.length > 0) {
      groups.push({ ...group, items });
    }
  }
  return groups;
}

export function resolveActiveMenuPath(pathname: string): string {
  const normalized = normalizePathname(pathname);
  for (const rule of DETAIL_ROUTE_RULES) {
    if (rule.pattern.test(normalized)) {
      return rule.activePath;
    }
  }
  for (const group of MENU_GROUPS) {
    for (const item of group.items) {
      if (normalized === item.path || normalized.startsWith(`${item.path}/`)) {
        return item.path;
      }
    }
  }
  return normalized;
}

export function resolveMenuPageHeader(pathname: string): MenuItem | null {
  const normalized = normalizePathname(pathname);
  for (const rule of DETAIL_ROUTE_RULES) {
    if (rule.pattern.test(normalized)) {
      return null;
    }
  }
  for (const group of MENU_GROUPS) {
    for (const item of group.items) {
      if (normalized === item.path) {
        return item;
      }
    }
  }
  return null;
}

export function resolveDefaultHomePath(role: RoleLike): string {
  const groups = getFilteredMenuGroups(role);
  return groups[0]?.items[0]?.path ?? "/operations/tenants";
}
