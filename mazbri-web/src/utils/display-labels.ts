import { $t } from "@/locales";
export type TagType = "success" | "warning" | "danger" | "info" | "primary";
const STATUS_TYPE_MAP: Record<string, TagType> = {
  active: "success",
  enabled: "success",
  inactive: "info",
  disabled: "info",
  suspended: "warning",
  pending: "info",
  error: "danger",
  failed: "danger",
  success: "success",
  valid: "success",
  expiring_soon: "warning",
  expired: "danger",
  revoked: "info",
  imported: "info",
  pending_import: "info",
  deleted: "danger",
};
const ROLE_TYPE_MAP: Record<string, TagType> = {
  R_ADMIN: "danger",
  R_OPERATOR: "primary",
  R_FINANCE: "warning",
  R_VIEWER: "info",
};
export function getStatusType(status: string): TagType {
  return STATUS_TYPE_MAP[status.toLowerCase()] ?? "info";
}
export function getStatusLabel(
  status: string,
  labels: Record<string, string> = {},
): string {
  const defaultLabels: Record<string, string> = {
    active: $t("common.statusLabels.active"),
    enabled: $t("common.statusLabels.enabled"),
    inactive: $t("common.statusLabels.inactive"),
    disabled: $t("common.statusLabels.disabled"),
    suspended: $t("common.statusLabels.suspended"),
    pending: $t("common.statusLabels.pending"),
    error: $t("common.statusLabels.error"),
    failed: $t("common.statusLabels.failed"),
    success: $t("common.statusLabels.success"),
    valid: $t("common.statusLabels.valid"),
    expiring_soon: $t("common.statusLabels.expiringSoon"),
    expired: $t("common.statusLabels.expired"),
    revoked: $t("common.statusLabels.revoked"),
    imported: $t("common.statusLabels.imported"),
    pending_import: $t("common.statusLabels.pendingImport"),
    deleted: $t("common.statusLabels.deleted"),
  };
  return (
    labels[status] ??
    labels[status.toLowerCase()] ??
    defaultLabels[status.toLowerCase()] ??
    status
  );
}
export function getRoleType(role: string): TagType {
  return ROLE_TYPE_MAP[role] ?? ROLE_TYPE_MAP[role.toUpperCase()] ?? "info";
}
export function getRoleLabel(
  role: string,
  labels: Record<string, string> = {},
): string {
  const defaultLabels: Record<string, string> = {
    R_ADMIN: $t("common.roleLabels.admin"),
    R_OPERATOR: $t("common.roleLabels.operator"),
    R_FINANCE: $t("common.roleLabels.finance"),
    R_VIEWER: $t("common.roleLabels.viewer"),
  };
  return (
    labels[role] ??
    labels[role.toUpperCase()] ??
    defaultLabels[role.toUpperCase()] ??
    role
  );
}
