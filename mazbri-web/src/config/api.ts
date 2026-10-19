export const API_BASE_PATH = "/api/v1/mgmt" as const;
export const API_PATHS = {
  ADMINS: "/admins",
  AUTHORIZATIONS: "/authorizations",
  AUDIT_LOGS: "/audit-logs",
  AUTH: "/auth",
  BILLING: "/billing",
  CERTIFICATES: "/certificates",
  CONFIG_CENTER: "/config-center",
  PRODUCTS: "/products",
  SERVICE_NAMES: "/service-names",
  SETTINGS: "/settings",
  STATS: "/stats",
  TENANTS: "/tenants",
} as const;
export const buildApiPath = (resource: string): string =>
  `${API_BASE_PATH}${resource}`;
