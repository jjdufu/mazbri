import { useMemo, useState } from "react";
import {
  AlertCircle,
  ChevronDown,
  ChevronRight,
  Copy,
  Loader2,
} from "lucide-react";
import {
  Button,
  CopyableText,
  Drawer,
  StatusBadge,
  cn,
} from "@/components/ui";
import { useT } from "@/locales";
import type { AuditLog, AuditLogDetail } from "@/types/api/audit";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { stripUrlScheme } from "@/utils/sensitive";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import {
  ACTION_LABELS,
  DETAIL_LABELS,
  FAILURE_REASON_LABELS,
  MFA_CATEGORY_LABELS,
  MFA_METHOD_LABELS,
  RESOURCE_TYPE_LABELS,
  SERVICE_LABELS,
  diffChanges,
  isAuditDurationSecondsKey,
  isAuditTimestampKey,
  statusLabelKey,
} from "./labels";

const SENSITIVE_DETAIL_KEYS = new Set(["endpoint_url", "upstream_url"]);
const ISO_DATETIME_RE = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/;

const ACTION_VERB_LABELS: Record<string, string> = {
  login: "auditLogs.verbs.login",
  logout: "auditLogs.verbs.logout",
  create: "auditLogs.verbs.create",
  update: "auditLogs.verbs.update",
  delete: "auditLogs.verbs.delete",
  revoke: "auditLogs.verbs.revoke",
  grant: "auditLogs.verbs.grant",
  renew: "auditLogs.verbs.renew",
  issue: "auditLogs.verbs.issue",
  suspend: "auditLogs.verbs.suspend",
  activate: "auditLogs.verbs.activate",
  recharge: "auditLogs.verbs.recharge",
  refund: "auditLogs.verbs.refund",
  rollback: "auditLogs.verbs.rollback",
  export: "auditLogs.verbs.export",
  import: "auditLogs.verbs.import",
  change: "auditLogs.verbs.change",
  reset: "auditLogs.verbs.reset",
};

type TranslateFn = (key: string, named?: Record<string, unknown>) => string;

function getRecord(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  return value as Record<string, unknown>;
}

export function formatAuditService(
  service: string | null | undefined,
  t: TranslateFn,
): string {
  if (!service) return EMPTY_PLACEHOLDER;
  return SERVICE_LABELS[service] ? t(SERVICE_LABELS[service]) : service;
}

export function formatAuditResourceType(
  type: string,
  t: TranslateFn,
): string {
  if (!type) return EMPTY_PLACEHOLDER;
  return RESOURCE_TYPE_LABELS[type]
    ? t(RESOURCE_TYPE_LABELS[type])
    : type.replaceAll("_", " ");
}

export function formatAuditAction(action: string, t: TranslateFn): string {
  if (!action) return EMPTY_PLACEHOLDER;
  if (ACTION_LABELS[action]) return t(ACTION_LABELS[action]);
  const [resourceType, verb] = action.split(".");
  const resourceLabel = formatAuditResourceType(resourceType, t);
  const verbLabel = ACTION_VERB_LABELS[verb] ? t(ACTION_VERB_LABELS[verb]) : "";
  if (resourceType && verbLabel) {
    return `${verbLabel}${resourceLabel}`;
  }
  return action.replaceAll(".", " / ").replaceAll("_", " ");
}

export function formatAuditFailureReason(
  reason: string | null | undefined,
  t: TranslateFn,
): string {
  if (!reason) return EMPTY_PLACEHOLDER;
  return FAILURE_REASON_LABELS[reason]
    ? t(FAILURE_REASON_LABELS[reason])
    : reason.replaceAll("_", " ");
}

export function formatAuditActorName(log: AuditLog, t: TranslateFn): string {
  if (log.actor_name) return log.actor_name;
  return log.actor_type === "anonymous"
    ? t("auditLogs.values.anonymous")
    : EMPTY_PLACEHOLDER;
}

export function mapAuditStatusBadgeCode(status?: string): string {
  if (status === "success") return "SUCCESS";
  if (status === "denied") return "DENIED";
  return "FAILED";
}

function formatDetailLabel(key: string, t: TranslateFn): string {
  return DETAIL_LABELS[key] ? t(DETAIL_LABELS[key]) : key.replaceAll("_", " ");
}

export function formatAuditDetailValue(
  value: unknown,
  t: TranslateFn,
  key?: string,
): string {
  if (value === null || value === undefined || value === "") {
    return EMPTY_PLACEHOLDER;
  }
  if (typeof value === "boolean") {
    return value ? t("auditLogs.values.yes") : t("auditLogs.values.no");
  }
  if (typeof value === "number") {
    if (isAuditTimestampKey(key)) return formatDateTime(value);
    if (key === "content_length") return `${value} B`;
    if (isAuditDurationSecondsKey(key)) return `${value}s`;
    return String(value);
  }
  if (typeof value === "string") {
    if ((key === "category" || key === "mfa") && MFA_CATEGORY_LABELS[value]) {
      return t(MFA_CATEGORY_LABELS[value]);
    }
    if (
      (key === "step_up_method" || key === "method") &&
      MFA_METHOD_LABELS[value]
    ) {
      return t(MFA_METHOD_LABELS[value]);
    }
    if (isAuditTimestampKey(key)) {
      const asNum = Number(value);
      if (Number.isFinite(asNum) && value.trim() !== "") {
        return formatDateTime(asNum);
      }
      return formatDateTime(value);
    }
    if (ISO_DATETIME_RE.test(value)) {
      return formatDateTime(value);
    }
    return stripUrlScheme(value);
  }
  if (Array.isArray(value)) {
    if (!value.length) return t("auditLogs.values.none");
    return value.map((item) => formatAuditDetailValue(item, t, key)).join("、");
  }
  const record = getRecord(value);
  const summary = Object.entries(record)
    .filter(([, item]) => item !== null && item !== undefined && item !== "")
    .slice(0, 8)
    .map(([subKey, item]) => {
      const formatted =
        SENSITIVE_DETAIL_KEYS.has(subKey) && typeof item === "string"
          ? stripUrlScheme(item)
          : formatAuditDetailValue(item, t, subKey);
      return `${formatDetailLabel(subKey, t)}：${formatted}`;
    });
  return summary.length ? summary.join("；") : EMPTY_PLACEHOLDER;
}

export interface AuditDetailDrawerProps {
  open: boolean;
  loading: boolean;
  log: AuditLogDetail | null;
  relatedChainRecords: readonly AuditLog[];
  onClose: () => void;
  onSelectRelated: (item: AuditLog) => void;
}

export function AuditDetailDrawer({
  open,
  loading,
  log,
  relatedChainRecords,
  onClose,
  onSelectRelated,
}: AuditDetailDrawerProps) {
  const { t } = useT();
  const [rawExpanded, setRawExpanded] = useState(false);

  const changeRows = useMemo(() => {
    if (!log) return [];
    const changes = log.changes;
    const details = log.details || {};
    const hasExplicit = Boolean(
      changes && (changes.before !== undefined || changes.after !== undefined),
    );
    const beforeSource = hasExplicit ? changes?.before : details.before;
    const afterSource = hasExplicit ? changes?.after : details.after;
    if (beforeSource === undefined && afterSource === undefined) return [];
    return diffChanges(beforeSource, afterSource).map((row) => ({
      key: row.key,
      label: formatDetailLabel(row.key, t),
      before: formatAuditDetailValue(row.before, t, row.key),
      after: formatAuditDetailValue(row.after, t, row.key),
      changed: row.changed,
    }));
  }, [log, t]);

  const detailRows = useMemo(() => {
    if (!log) return [];
    const details = log.details || {};
    const hasExplicitChanges = Boolean(log.changes);
    return Object.entries(details)
      .filter(([key, value]) => {
        if (value === null || value === undefined || value === "") return false;
        if (key === "mfa" && details.category === value) return false;
        if (!hasExplicitChanges && (key === "before" || key === "after")) {
          return false;
        }
        return true;
      })
      .map(([key, value]) => ({
        key,
        label: formatDetailLabel(key, t),
        value: formatAuditDetailValue(value, t, key),
      }));
  }, [log, t]);

  const rawJsonText = useMemo(() => {
    if (!log) return "";
    const payload: Record<string, unknown> = {};
    if (log.details && Object.keys(log.details).length > 0) {
      payload.details = log.details;
    }
    if (log.changes) payload.changes = log.changes;
    if (log.request_summary) payload.request_summary = log.request_summary;
    return Object.keys(payload).length > 0
      ? JSON.stringify(payload, null, 2)
      : "";
  }, [log]);

  const handleClose = () => {
    setRawExpanded(false);
    onClose();
  };

  return (
    <Drawer
      open={open}
      onClose={handleClose}
      size="lg"
      testId="audit-detail-drawer"
      title={
        <span className="inline-flex items-center gap-2 min-w-0">
          <span className="truncate">
            {log
              ? formatAuditAction(log.action, t)
              : t("auditLogs.detail.title")}
          </span>
          {log && (
            <StatusBadge
              status={mapAuditStatusBadgeCode(log.status)}
              label={t(statusLabelKey(log.status))}
            />
          )}
        </span>
      }
      subtitle={log?.request_id || undefined}
      footer={
        <div className="flex items-center justify-end gap-2 w-full">
          {rawJsonText && (
            <Button
              type="button"
              variant="outline"
              onClick={() => void copyWithFeedback(rawJsonText)}
            >
              <Copy className="h-3.5 w-3.5" />
              {t("auditLogs.detail.copyRawJson")}
            </Button>
          )}
          <Button type="button" variant="outline" onClick={handleClose}>
            {t("common.actions.close")}
          </Button>
        </div>
      }
    >
      {loading ? (
        <div className="flex items-center justify-center py-14 text-xs text-muted-foreground gap-2">
          <Loader2 className="h-4 w-4 animate-spin text-primary" />
          <span>{t("layout.loading")}</span>
        </div>
      ) : log ? (
        <div className="space-y-5">
          {relatedChainRecords.length > 1 && (
            <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2">
              <span className="text-xs font-medium text-muted-foreground">
                {t("auditLogs.chain.detailTitle")}
              </span>
              <div className="flex flex-wrap items-center gap-1.5">
                {relatedChainRecords.map((item) => (
                  <Button
                    key={item.id}
                    type="button"
                    size="xs"
                    variant={item.id === log.id ? "primary" : "outline"}
                    onClick={() => onSelectRelated(item)}
                  >
                    {formatAuditService(item.service, t)} ·{" "}
                    {formatAuditAction(item.action, t)}
                  </Button>
                ))}
              </div>
            </div>
          )}

          <dl className="grid grid-cols-1 sm:grid-cols-2 border border-border rounded-lg overflow-hidden text-xs">
            <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.timestamp")}
              </dt>
              <dd className="font-mono-tech tabular-nums text-foreground">
                {formatDateTime(log.timestamp)}
              </dd>
            </div>

            <div className="flex flex-col gap-1 p-3 border-b border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.status")}
              </dt>
              <dd>
                <StatusBadge
                  status={mapAuditStatusBadgeCode(log.status)}
                  label={t(statusLabelKey(log.status))}
                />
              </dd>
            </div>

            <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.actorName")}
              </dt>
              <dd className="text-foreground">
                {formatAuditActorName(log, t)}
              </dd>
            </div>

            <div className="flex flex-col gap-1 p-3 border-b border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.actorRole")}
              </dt>
              <dd className="text-foreground">
                {log.actor_role || EMPTY_PLACEHOLDER}
              </dd>
            </div>

            <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.action")}
              </dt>
              <dd className="text-foreground font-medium">
                {formatAuditAction(log.action, t)}
              </dd>
            </div>

            <div className="flex flex-col gap-1 p-3 border-b border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.service")}
              </dt>
              <dd className="text-foreground">
                {formatAuditService(log.service, t)}
              </dd>
            </div>

            <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.resourceType")}
              </dt>
              <dd className="text-foreground">
                {formatAuditResourceType(log.resource_type, t)}
              </dd>
            </div>

            <div className="flex flex-col gap-1 p-3 border-b border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.resourceId")}
              </dt>
              <dd className="text-foreground">
                {log.resource_id ? (
                  <CopyableText value={log.resource_id} truncate />
                ) : (
                  <span className="text-muted-foreground">
                    {EMPTY_PLACEHOLDER}
                  </span>
                )}
              </dd>
            </div>

            <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.ipAddress")}
              </dt>
              <dd className="text-foreground">
                {log.ip_address?.trim() ? (
                  <CopyableText value={log.ip_address} />
                ) : (
                  <span className="text-muted-foreground">
                    {t("auditLogs.detail.notRecorded")}
                  </span>
                )}
              </dd>
            </div>

            <div className="flex flex-col gap-1 p-3 border-b border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.requestId")}
              </dt>
              <dd className="text-foreground">
                {log.request_id ? (
                  <CopyableText value={log.request_id} truncate />
                ) : (
                  <span className="text-muted-foreground">
                    {EMPTY_PLACEHOLDER}
                  </span>
                )}
              </dd>
            </div>

            <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.method")}
              </dt>
              <dd className="font-mono-tech text-foreground">
                {log.method || EMPTY_PLACEHOLDER}
              </dd>
            </div>

            <div className="flex flex-col gap-1 p-3 border-b border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.statusCode")}
              </dt>
              <dd className="font-mono-tech tabular-nums text-foreground">
                {log.status_code ?? EMPTY_PLACEHOLDER}
              </dd>
            </div>

            <div className="flex flex-col gap-1 p-3 border-b sm:border-r border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.durationMs")}
              </dt>
              <dd className="font-mono-tech tabular-nums text-foreground">
                {log.duration_ms == null
                  ? EMPTY_PLACEHOLDER
                  : `${log.duration_ms} ms`}
              </dd>
            </div>

            <div className="flex flex-col gap-1 p-3 border-b border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.failureReason")}
              </dt>
              <dd className="text-foreground">
                {formatAuditFailureReason(log.failure_reason, t)}
              </dd>
            </div>

            <div className="sm:col-span-2 flex flex-col gap-1 p-3 border-b border-border">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.path")}
              </dt>
              <dd className="font-mono-tech text-foreground break-all">
                {log.path || EMPTY_PLACEHOLDER}
              </dd>
            </div>

            <div className="sm:col-span-2 flex flex-col gap-1 p-3">
              <dt className="text-[11px] text-muted-foreground">
                {t("auditLogs.detail.fields.userAgent")}
              </dt>
              <dd className="text-foreground break-all">
                {log.user_agent || EMPTY_PLACEHOLDER}
              </dd>
            </div>

            {log.request_summary && (
              <div className="sm:col-span-2 flex flex-col gap-1 p-3 border-t border-border">
                <dt className="text-[11px] text-muted-foreground">
                  {t("auditLogs.detail.fields.requestSummary")}
                </dt>
                <dd className="text-foreground break-words">
                  {formatAuditDetailValue(log.request_summary, t)}
                </dd>
              </div>
            )}

            {log.error_message && (
              <div className="sm:col-span-2 flex flex-col gap-1.5 p-3 border-t border-border">
                <dt className="text-[11px] text-muted-foreground">
                  {t("auditLogs.detail.fields.errorMessage")}
                </dt>
                <dd className="flex items-start gap-2 rounded-md border border-danger-fg/35 bg-danger-fg/10 px-3 py-2 text-xs text-danger-fg">
                  <AlertCircle className="h-4 w-4 shrink-0 mt-0.5" />
                  <span className="break-all">{log.error_message}</span>
                </dd>
              </div>
            )}
          </dl>

          {detailRows.length > 0 && (
            <section className="space-y-2">
              <h4 className="text-xs font-semibold text-foreground">
                {t("auditLogs.detail.moreTitle")}
              </h4>
              <dl className="border border-border rounded-lg overflow-hidden divide-y divide-border text-xs">
                {detailRows.map((item) => (
                  <div
                    key={item.key}
                    className="grid grid-cols-1 sm:grid-cols-[168px_1fr] gap-1 sm:gap-3 px-3 py-2"
                  >
                    <dt className="text-muted-foreground font-medium">
                      {item.label}
                    </dt>
                    <dd className="text-foreground break-words">
                      {item.value}
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          )}

          {changeRows.length > 0 && (
            <section className="space-y-2">
              <h4 className="text-xs font-semibold text-foreground">
                {t("auditLogs.detail.changesTitle")}
              </h4>
              <div className="overflow-x-auto rounded-lg border border-border">
                <table className="w-full border-collapse text-xs">
                  <thead>
                    <tr className="border-b border-border bg-transparent text-muted-foreground">
                      <th className="px-3 py-2 text-left font-medium">
                        {t("auditLogs.detail.changesField")}
                      </th>
                      <th className="px-3 py-2 text-left font-medium">
                        {t("auditLogs.detail.changesBefore")}
                      </th>
                      <th className="px-3 py-2 text-left font-medium">
                        {t("auditLogs.detail.changesAfter")}
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border">
                    {changeRows.map((row) => (
                      <tr key={row.key}>
                        <td className="px-3 py-2 font-medium text-foreground">
                          {row.label}
                        </td>
                        <td
                          className={cn(
                            "px-3 py-2 break-all",
                            row.changed
                              ? "font-semibold text-warning-fg"
                              : "text-muted-foreground",
                          )}
                        >
                          {row.before}
                        </td>
                        <td
                          className={cn(
                            "px-3 py-2 break-all",
                            row.changed
                              ? "font-semibold text-warning-fg"
                              : "text-muted-foreground",
                          )}
                        >
                          {row.after}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </section>
          )}

          {rawJsonText && (
            <section className="rounded-lg border border-border overflow-hidden">
              <button
                type="button"
                onClick={() => setRawExpanded((v) => !v)}
                className="flex w-full items-center justify-between gap-2 bg-card px-3 py-2 text-xs font-medium text-foreground hover:bg-hover-bg transition-colors cursor-pointer"
              >
                <span className="inline-flex items-center gap-1.5">
                  {rawExpanded ? (
                    <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
                  ) : (
                    <ChevronRight className="h-3.5 w-3.5 text-muted-foreground" />
                  )}
                  <span>{t("auditLogs.detail.rawJsonTitle")}</span>
                </span>
              </button>
              {rawExpanded && (
                <pre className="max-h-72 overflow-auto border-t border-border bg-muted/30 p-3 font-mono-tech text-[11px] leading-relaxed text-foreground whitespace-pre-wrap break-all">
                  {rawJsonText}
                </pre>
              )}
            </section>
          )}
        </div>
      ) : null}
    </Drawer>
  );
}
