import { $t } from "@/locales";

export type TagType = "success" | "warning" | "danger" | "info" | "primary";

const STATUS_TYPE_MAP: Record<string, TagType> = {
  active: "success",
  enabled: "success",
  inactive: "info",
  disabled: "info",
  suspended: "warning",
  closed: "danger",
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
  firing: "danger",
  acked: "warning",
  resolved: "success",
};

const ROLE_TYPE_MAP: Record<string, TagType> = {
  R_ADMIN: "danger",
  R_FINANCE: "primary",
  R_VIEWER: "info",
};

const SEVERITY_TYPE_MAP: Record<string, TagType> = {
  critical: "danger",
  warning: "warning",
  info: "info",
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
    closed: $t("common.statusLabels.closed"),
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
    firing: $t("ops.alerts.status.firing"),
    acked: $t("ops.alerts.status.acked"),
    resolved: $t("ops.alerts.status.resolved"),
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

export function getSeverityType(severity: string): TagType {
  return SEVERITY_TYPE_MAP[severity.toLowerCase()] ?? "info";
}

/** 告警严重级别中文标签；未知回落原文。 */
export function getSeverityLabel(
  severity: string,
  labels: Record<string, string> = {},
): string {
  const key = severity.toLowerCase();
  const defaultLabels: Record<string, string> = {
    critical: $t("common.severityLabels.critical"),
    warning: $t("common.severityLabels.warning"),
    info: $t("common.severityLabels.info"),
  };
  return (
    labels[severity] ?? labels[key] ?? defaultLabels[key] ?? severity
  );
}
