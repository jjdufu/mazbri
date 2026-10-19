
import { $t } from "@/locales";
import type { RouteRule } from "@/api/route-management";

export interface TenantRouteOption {
  tenant_id: string;
  tenant_code: string;
  tenant_name: string;
  product_id: string;
  product_code: string;
}

export const DEFAULT_TIMEOUT_MS = 3000;
export const DEFAULT_MAX_RETRIES = 1;
export const DEFAULT_FAILURE_TYPES = ["timeout", "connection", "server_error"];

export const routePurposeOptions = [
  { label: $t("routes.routePurpose.standard"), value: "standard" },
  { label: $t("routes.routePurpose.emergency"), value: "emergency" },
  { label: $t("routes.routePurpose.costFirst"), value: "cost_first" },
  { label: $t("routes.routePurpose.backup"), value: "backup" },
];

export const failureTypeOptions = [
  { label: $t("routes.failureTypes.timeout"), value: "timeout" },
  { label: $t("routes.failureTypes.connection"), value: "connection" },
  { label: $t("routes.failureTypes.serverError"), value: "server_error" },
  { label: $t("routes.failureTypes.rateLimited"), value: "rate_limited" },
];

export function getRecord(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  return value as Record<string, unknown>;
}

export function getString(value: unknown, fallback = "") {
  return typeof value === "string" ? value : fallback;
}

export function getNumber(value: unknown, fallback: number) {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

export function getStringArray(value: unknown) {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : [];
}

export function hasMetadata(value: Record<string, unknown> | undefined) {
  return Boolean(value && Object.keys(value).length > 0);
}

export function routePurposeLabel(value?: string) {
  return routePurposeOptions.find((item) => item.value === value)?.label || "";
}

export function routeStatusLabel(value?: string) {
  if (value === "active") return $t("routes.status.active");
  if (value === "inactive") return $t("routes.status.inactive");
  if (value === "suspended") return $t("routes.status.suspended");
  return value || "-";
}

export function routeStatusType(value?: string) {
  if (value === "active") return "success";
  if (value === "suspended") return "warning";
  return "info";
}

export function channelStatusLabel(value?: string) {
  if (value === "active") return $t("routes.status.active");
  if (value === "inactive") return $t("routes.status.inactive");
  if (value === "suspended") return $t("routes.status.suspended");
  return value || "-";
}

export function failureTypeLabel(value: string) {
  return failureTypeOptions.find((item) => item.value === value)?.label || value;
}

export function switchConditionSummary(
  value: Record<string, unknown> | undefined,
) {
  const condition = getRecord(value);
  const timeoutMs = getNumber(condition.timeout_ms, DEFAULT_TIMEOUT_MS);
  const maxRetries = getNumber(condition.max_retries, DEFAULT_MAX_RETRIES);
  const failoverEnabled = condition.failover_enabled !== false;
  const failureTypes = getStringArray(condition.failure_types);
  const triggerText = failoverEnabled
    ? failureTypes.length
      ? failureTypes.map(failureTypeLabel).join("、")
      : $t("routes.labels.defaultFailureTypes")
    : $t("routes.labels.noSwitch");
  return $t("routes.labels.switchSummary", {
    timeoutMs,
    maxRetries,
    triggerText,
  });
}

export function isEmergencySwitchEnabled(
  value: Record<string, unknown> | undefined,
) {
  const emergencySwitch = value?.emergency_switch;
  return (
    Boolean(emergencySwitch) &&
    typeof emergencySwitch === "object" &&
    !Array.isArray(emergencySwitch) &&
    (emergencySwitch as Record<string, unknown>).enabled === true
  );
}

export function routePurposeLabelFromMetadata(
  value: Record<string, unknown> | undefined,
) {
  const routeConfig = getRecord(value?.route_config);
  return routePurposeLabel(getString(routeConfig.purpose));
}

export function isExampleRoute(row: RouteRule | null | undefined) {
  const routeConfig = getRecord(row?.metadata?.route_config);
  return routeConfig.example === true;
}

export function dedupeTenants(options: TenantRouteOption[]) {
  return Array.from(
    new Map(options.map((item) => [item.tenant_id, item])).values(),
  );
}
