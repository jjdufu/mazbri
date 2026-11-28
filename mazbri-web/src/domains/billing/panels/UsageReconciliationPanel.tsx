import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import {
  AlertCircle,
  CalendarCheck,
  ChevronDown,
  ChevronRight,
  Play,
  RefreshCw,
  Settings,
  Trash2,
} from "lucide-react";
import {
  deleteReconciliationDiscrepancy,
  fetchReconciliationDiscrepancies,
  fetchReconciliationSettings,
  runReconciliationAudit,
  updateReconciliationDiscrepancy,
  updateReconciliationSettings,
  type ReconciliationDiscrepancy,
  type ReconciliationSettings,
} from "@/api/billing";
import {
  fetchReconcileCalendar,
  type ReconcileCalendarDay,
} from "@/api/billing-ops";
import {
  ExportDropdown,
  type ExportDropdownItem,
} from "@/components/common/ExportDropdown";
import {
  Badge,
  Button,
  ConfirmDialog,
  DataTablePage,
  DateRangePicker,
  Drawer,
  EntityDetailCard,
  FormField,
  FormModal,
  Input,
  NumberInput,
  Select,
  Switch,
  Textarea,
  cn,
  type DataTableColumn,
  type SelectOptionItem,
} from "@/components/ui";
import { toLocationHref } from "@/domains/ops/shared/opsHelpers";
import { StructuredFilterBar } from "@/domains/ops/shared/StructuredFilterBar";
import { reconciliationCatalog } from "@/list-catalog/common-resources";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { ListFilter } from "@/types/list-query";
import { downloadCsv, serializeRawCsv } from "@/utils/csv";
import {
  EMPTY_PLACEHOLDER,
  formatDate,
  formatDateTime,
  formatDisplayDateYmd,
  formatMoney,
} from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { buildCallsRoute } from "@/utils/ops/opsRoutes";
import { isBizRole } from "@/utils/ops/roles";
import { logger } from "@/utils/sys/logger";
import {
  showErrorToast,
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";
import {
  hoursToSeconds,
  percentToRatio,
  ratioToPercent,
  secondsToHours,
} from "../helpers/reconcilePolicyHelpers";

export interface UsageReconciliationPanelProps {
  active?: boolean;
}

const DEFAULT_SETTINGS: ReconciliationSettings = {
  schedule: { enabled: false, interval_secs: 86400, use_previous_day: true },
  alert: { on_flagged: false, webhook_url: "" },
  sources: { include_amount_recon: true },
  loss_ratio_threshold: 0.05,
  default_scope_type: "system",
  default_scope_id: "",
  warn_on_open_discrepancies_before_bill: true,
  last_scheduled_run_at: null,
};

function asRecord(value: unknown): Record<string, unknown> | null {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }
  return null;
}

function asString(value: unknown): string | null {
  if (typeof value === "string" && value.trim() !== "") return value;
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  return null;
}

function formatRatio(value: string | number | null | undefined): string {
  const n = Number(value ?? 0);
  if (Number.isNaN(n)) return String(value ?? EMPTY_PLACEHOLDER);
  return `${(n * 100).toFixed(2)}%`;
}

function formatCount(value: string | number | null | undefined): string {
  const n = Number(value ?? 0);
  if (!Number.isFinite(n)) return String(value ?? EMPTY_PLACEHOLDER);
  return new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 0 }).format(n);
}

function statusBadgeVariant(
  status: string,
): "success" | "danger" | "warning" | "neutral" {
  switch (status) {
    case "ok":
      return "success";
    case "flagged":
    case "error":
      return "danger";
    case "acknowledged":
      return "warning";
    default:
      return "neutral";
  }
}

function calendarCellClass(status: string): string {
  if (status === "ok") {
    return "border-success-fg/30 bg-success-fg/10 text-foreground";
  }
  if (status === "open" || status === "flagged" || status === "acknowledged") {
    return "border-warning-border bg-warning-bg text-warning-fg";
  }
  if (status === "error") {
    return "border-danger-fg/35 bg-danger-fg/10 text-danger-fg";
  }
  return "border-border-subtle text-foreground";
}

export function UsageReconciliationPanel({
  active: _active = true,
}: UsageReconciliationPanelProps) {
  const { t } = useT();
  const navigate = useNavigate();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const [loading, setLoading] = useState(false);
  const [items, setItems] = useState<ReconciliationDiscrepancy[]>([]);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);

  const [qInput, setQInput] = useState("");
  const [appliedQ, setAppliedQ] = useState("");
  const [listFilters, setListFilters] = useState<ListFilter[]>([]);
  const [filterFrom, setFilterFrom] = useState("");
  const [filterTo, setFilterTo] = useState("");

  // Calendar
  const [calendarDays, setCalendarDays] = useState(30);
  const [calendar, setCalendar] = useState<ReconcileCalendarDay[]>([]);

  // Settings & Policy Modal (P0 A-13)
  const [settings, setSettings] =
    useState<ReconciliationSettings>(DEFAULT_SETTINGS);
  const [settingsLoading, setSettingsLoading] = useState(false);
  const [policyOpen, setPolicyOpen] = useState(false);
  const [policySaving, setPolicySaving] = useState(false);
  const [policyEnabled, setPolicyEnabled] = useState(false);
  const [policyIntervalHours, setPolicyIntervalHours] = useState(24);
  const [policyThresholdPercent, setPolicyThresholdPercent] = useState(5);
  const [policyUsePreviousDay, setPolicyUsePreviousDay] = useState(true);
  const [policyIncludeAmount, setPolicyIncludeAmount] = useState(true);
  const [policyWarnBeforeBill, setPolicyWarnBeforeBill] = useState(true);
  const [policyAlertOnFlagged, setPolicyAlertOnFlagged] = useState(false);
  const [policyWebhookUrl, setPolicyWebhookUrl] = useState("");
  const [policyErrors, setPolicyErrors] = useState<{
    intervalHours?: string;
    thresholdPercent?: string;
    webhookUrl?: string;
  }>({});

  // Run Audit Modal
  const [runOpen, setRunOpen] = useState(false);
  const [runDate, setRunDate] = useState(() => formatDisplayDateYmd());
  const [runLoading, setRunLoading] = useState(false);

  // Detail Drawer
  const [detailRow, setDetailRow] = useState<ReconciliationDiscrepancy | null>(
    null,
  );
  const [methodologyOpen, setMethodologyOpen] = useState(false);
  const [rawJsonOpen, setRawJsonOpen] = useState(false);

  // Resolve / Acknowledge Modal
  const [resolveState, setResolveState] = useState<{
    row: ReconciliationDiscrepancy;
    target: "acknowledged" | "resolved";
  } | null>(null);
  const [resolveNote, setResolveNote] = useState("");
  const [resolveLoading, setResolveLoading] = useState(false);

  // Delete Confirms
  const [deleteTarget, setDeleteTarget] =
    useState<ReconciliationDiscrepancy | null>(null);
  const [batchDeleteIds, setBatchDeleteIds] = useState<string[] | null>(null);
  const [deleting, setDeleting] = useState(false);

  const statusLabel = useCallback(
    (status: string) => {
      const key = `billing.reconciliation.statuses.${status}`;
      const label = t(key);
      return label === key ? status : label;
    },
    [t],
  );

  const sourceLabel = useCallback(
    (code: string | null | undefined) => {
      if (!code) return t("billing.reconciliation.detail.sources.unknown");
      const key = `billing.reconciliation.detail.sources.${code}`;
      const label = t(key);
      return label === key ? code : label;
    },
    [t],
  );

  const loadCalendar = useCallback(async () => {
    if (!canWrite) return;
    try {
      const res = await fetchReconcileCalendar(calendarDays);
      setCalendar(res ?? []);
    } catch (error) {
      logger.error("Failed to load reconcile calendar:", error);
      showErrorToast(t("ops.reconcile.loadFailed"));
    }
  }, [calendarDays, canWrite, t]);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetchReconciliationDiscrepancies({
        filters:
          listFilters.length > 0 ? JSON.stringify(listFilters) : undefined,
        from: filterFrom || undefined,
        to: filterTo || undefined,
        q: appliedQ || undefined,
        page,
        page_size: pageSize,
      });
      setItems(res.items ?? []);
      setTotal(res.total ?? 0);
      setSelectedIds([]);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to load reconciliation discrepancies:", error);
      }
    } finally {
      setLoading(false);
    }
  }, [listFilters, filterFrom, filterTo, appliedQ, page, pageSize]);

  const loadSettings = useCallback(async () => {
    setSettingsLoading(true);
    try {
      const res = await fetchReconciliationSettings();
      setSettings(res);
    } catch (error) {
      logger.error("Failed to load reconciliation settings:", error);
    } finally {
      setSettingsLoading(false);
    }
  }, []);

  const reloadAll = useCallback(async () => {
    await Promise.all([loadData(), loadCalendar()]);
  }, [loadData, loadCalendar]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  useEffect(() => {
    void loadCalendar();
  }, [loadCalendar]);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  const openPolicyDialog = () => {
    setPolicyEnabled(settings.schedule.enabled);
    setPolicyIntervalHours(secondsToHours(settings.schedule.interval_secs));
    setPolicyThresholdPercent(ratioToPercent(settings.loss_ratio_threshold));
    setPolicyUsePreviousDay(settings.schedule.use_previous_day);
    setPolicyIncludeAmount(settings.sources.include_amount_recon);
    setPolicyWarnBeforeBill(settings.warn_on_open_discrepancies_before_bill);
    setPolicyAlertOnFlagged(settings.alert.on_flagged);
    setPolicyWebhookUrl(settings.alert.webhook_url ?? "");
    setPolicyErrors({});
    setPolicyOpen(true);
  };

  const handleSavePolicy = async () => {
    if (policySaving) return;
    const nextErrors: {
      intervalHours?: string;
      thresholdPercent?: string;
      webhookUrl?: string;
    } = {};
    if (!Number.isFinite(policyIntervalHours) || policyIntervalHours < 0.5) {
      nextErrors.intervalHours = t("billing.reconciliation.intervalHoursInvalid");
    }
    if (
      !Number.isFinite(policyThresholdPercent) ||
      policyThresholdPercent < 0 ||
      policyThresholdPercent > 100
    ) {
      nextErrors.thresholdPercent = t("billing.reconciliation.thresholdInvalid");
    }
    if (policyAlertOnFlagged) {
      const trimmed = policyWebhookUrl.trim();
      if (!trimmed || !/^https?:\/\/\S+$/i.test(trimmed)) {
        nextErrors.webhookUrl = t("billing.reconciliation.alertWebhookRequired");
      }
    }
    setPolicyErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;

    setPolicySaving(true);
    try {
      const payload: ReconciliationSettings = {
        ...settings,
        schedule: {
          ...settings.schedule,
          enabled: policyEnabled,
          interval_secs: hoursToSeconds(policyIntervalHours),
          use_previous_day: policyUsePreviousDay,
        },
        alert: {
          on_flagged: policyAlertOnFlagged,
          webhook_url: policyWebhookUrl.trim(),
        },
        sources: {
          ...settings.sources,
          include_amount_recon: policyIncludeAmount,
        },
        loss_ratio_threshold: percentToRatio(policyThresholdPercent),
        warn_on_open_discrepancies_before_bill: policyWarnBeforeBill,
      };
      const saved = await updateReconciliationSettings(payload);
      setSettings(saved);
      setPolicyOpen(false);
      showSuccessToast(t("billing.reconciliation.settingsSaved"));
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to save reconciliation settings:", error);
      }
    } finally {
      setPolicySaving(false);
    }
  };

  const handleRunAudit = async () => {
    if (runLoading) return;
    if (!runDate) {
      showWarningToast(t("billing.reconciliation.runDateRequired"));
      return;
    }
    setRunLoading(true);
    try {
      const res = await runReconciliationAudit({
        reconciliation_date: runDate,
        scope_type: "system",
      });
      showSuccessToast(
        t("billing.reconciliation.runSuccess", {
          status: statusLabel(res.status),
        }),
      );
      setRunOpen(false);
      await reloadAll();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to run reconciliation audit:", error);
      }
    } finally {
      setRunLoading(false);
    }
  };

  const handleSubmitResolve = async () => {
    if (!resolveState || resolveLoading) return;
    setResolveLoading(true);
    try {
      await updateReconciliationDiscrepancy(resolveState.row.id, {
        status: resolveState.target,
        resolution_note: resolveNote.trim() || undefined,
      });
      setResolveState(null);
      await reloadAll();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to update discrepancy:", error);
      }
    } finally {
      setResolveLoading(false);
    }
  };

  const handleConfirmSingleDelete = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    try {
      await deleteReconciliationDiscrepancy(deleteTarget.id);
      setDeleteTarget(null);
      await reloadAll();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete discrepancy:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const handleConfirmBatchDelete = async () => {
    if (!batchDeleteIds || batchDeleteIds.length === 0 || deleting) return;
    setDeleting(true);
    let deletedCount = 0;
    try {
      for (const id of batchDeleteIds) {
        try {
          await deleteReconciliationDiscrepancy(id);
          deletedCount += 1;
        } catch (error) {
          if (isMfaStepUpCancelled(error)) break;
          logger.error(`Failed to delete discrepancy ${id}:`, error);
        }
      }
      if (deletedCount > 0) {
        setSelectedIds([]);
        await reloadAll();
      }
      setBatchDeleteIds(null);
    } finally {
      setDeleting(false);
    }
  };

  const selectedRows = useMemo(
    () => items.filter((row) => selectedIds.includes(row.id)),
    [items, selectedIds],
  );

  const exportItems = useMemo<ExportDropdownItem[]>(
    () => [
      {
        command: "page",
        label: t("billing.actions.exportPage"),
        disabled: items.length === 0,
      },
      {
        command: "selected",
        label: t("billing.actions.exportSelected", {
          count: selectedRows.length,
        }),
        disabled: selectedRows.length === 0,
      },
    ],
    [items.length, selectedRows.length, t],
  );

  const handleExportCommand = (command: string) => {
    const targetRows = command === "selected" ? selectedRows : items;
    if (targetRows.length === 0) {
      showWarningToast(t("billing.reconciliation.exportEmpty"));
      return;
    }
    const header = [
      t("billing.reconciliation.columns.date"),
      t("billing.reconciliation.columns.oltp"),
      t("billing.reconciliation.columns.gateway"),
      t("billing.reconciliation.columns.olap"),
      t("billing.reconciliation.columns.ratio"),
      t("billing.reconciliation.columns.status"),
      t("billing.reconciliation.note"),
      t("common.fields.createdAt"),
    ];
    const body = targetRows.map((r) => [
      r.reconciliation_date,
      r.billable_call_count,
      r.gateway_call_count,
      r.upstream_preagg_call_count,
      formatRatio(r.discrepancy_ratio),
      statusLabel(r.status),
      r.resolution_note || "",
      formatDateTime(r.created_at),
    ]);
    downloadCsv(
      `\uFEFF${serializeRawCsv([header, ...body])}\n`,
      `usage-reconciliation-${command}-${Date.now()}.csv`,
    );
    showSuccessToast(t("billing.reconciliation.exportSuccess"));
  };

  const openCallMonitor = useCallback(
    (row: ReconciliationDiscrepancy, mode: "customer" | "upstream") => {
      const day = row.reconciliation_date;
      if (!day) return;
      const start = new Date(`${day}T00:00:00+08:00`).getTime();
      const end = new Date(`${day}T23:59:59.999+08:00`).getTime();
      const loc = buildCallsRoute({
        tab: mode,
        start: Math.floor(start / 1000),
        end: Math.floor(end / 1000),
      });
      navigate(toLocationHref(loc));
    },
    [navigate],
  );

  const settingsSummaryLine = useMemo(() => {
    const scheduled = settings.schedule.enabled
      ? t("billing.reconciliation.summaryScheduledOn")
      : t("billing.reconciliation.summaryScheduledOff");
    const intervalText = `${secondsToHours(settings.schedule.interval_secs)} ${t("ops.units.hours")}`;
    const threshold = t("billing.reconciliation.summaryThreshold", {
      value: formatRatio(settings.loss_ratio_threshold),
    });
    const last = settings.last_scheduled_run_at
      ? formatDate(settings.last_scheduled_run_at)
      : t("billing.reconciliation.neverRun");
    return `${scheduled} (${intervalText}) · ${threshold} · ${last}`;
  }, [settings, t]);

  const detailView = useMemo(() => {
    if (!detailRow) return null;
    const details = asRecord(detailRow.details) ?? {};
    const oltpCalls = detailRow.billable_call_count;
    const gatewayCalls = detailRow.gateway_call_count;
    const upstreamCalls = detailRow.upstream_preagg_call_count;
    const comparisonLabel = sourceLabel("request_logs_gateway");
    const errorText =
      detailRow.status === "error"
        ? (asString(details.error) ?? EMPTY_PLACEHOLDER)
        : null;
    const thresholdRaw = asString(details.loss_ratio_threshold);
    const threshold =
      thresholdRaw != null && Number.isFinite(Number(thresholdRaw))
        ? Number(thresholdRaw)
        : null;
    const ratio = Number(detailRow.discrepancy_ratio ?? 0);
    const overThreshold =
      threshold != null && Number.isFinite(ratio)
        ? ratio > threshold
        : detailRow.status === "flagged";

    const callDiffAbs = Math.abs(oltpCalls - gatewayCalls);
    let callDiffText: string;
    if (callDiffAbs === 0) {
      callDiffText = t("billing.reconciliation.detail.callDiffMatch");
    } else if (oltpCalls < gatewayCalls) {
      callDiffText = t("billing.reconciliation.detail.callDiffBillingLess", {
        count: formatCount(callDiffAbs),
        peerLabel: comparisonLabel,
      });
    } else {
      callDiffText = t("billing.reconciliation.detail.callDiffBillingMore", {
        count: formatCount(callDiffAbs),
        peerLabel: comparisonLabel,
      });
    }

    const callRows = [
      {
        label: t("billing.reconciliation.detail.sourceBilling"),
        countText: formatCount(oltpCalls),
        role: t("billing.reconciliation.detail.roleBilling"),
        primary: false,
      },
      {
        label: comparisonLabel,
        countText: formatCount(gatewayCalls),
        role: t("billing.reconciliation.detail.rolePrimary"),
        primary: true,
      },
      {
        label: t("billing.reconciliation.detail.sourceUpstream"),
        countText: formatCount(upstreamCalls),
        role: t("billing.reconciliation.detail.roleReference"),
        primary: false,
      },
    ];

    const amountObj = asRecord(details.amount_recon);
    const amount = amountObj
      ? (() => {
          const usageBillable = asString(amountObj.usage_billable_amount) ?? "0";
          const billItemsTotal = asString(amountObj.bill_items_total) ?? "0";
          const diff = asString(amountObj.amount_diff) ?? "0";
          const diffN = Math.abs(Number(diff));
          return {
            usageBillable,
            billItemsTotal,
            diff,
            diffIsZero: !Number.isFinite(diffN) || diffN < 1e-9,
          };
        })()
      : null;

    const methodologyLines = [
      t("billing.reconciliation.detail.methodBilling"),
      t("billing.reconciliation.detail.methodGateway"),
      t("billing.reconciliation.detail.methodUpstream"),
      t("billing.reconciliation.detail.methodPrimary", {
        peerLabel: comparisonLabel,
      }),
    ];

    return {
      oltpCalls,
      comparisonCalls: gatewayCalls,
      comparisonLabel,
      threshold,
      overThreshold,
      callDiffAbs,
      callDiffText,
      callRows,
      amount,
      methodologyLines,
      errorText,
    };
  }, [detailRow, sourceLabel, t]);

  const calendarDaysOptions = useMemo<SelectOptionItem[]>(
    () => [
      {
        value: "7",
        label: t("ops.timeRange.presets.7d"),
        text: t("ops.timeRange.presets.7d"),
      },
      {
        value: "30",
        label: t("ops.timeRange.presets.30d"),
        text: t("ops.timeRange.presets.30d"),
      },
      {
        value: "90",
        label: `90 ${t("ops.units.days")}`,
        text: `90 ${t("ops.units.days")}`,
      },
    ],
    [t],
  );

  const columns = useMemo<DataTableColumn<ReconciliationDiscrepancy>[]>(
    () => [
      {
        id: "reconciliation_date",
        header: t("billing.reconciliation.columns.date"),
        width: 112,
        mono: true,
        cell: (row) => row.reconciliation_date,
      },
      {
        id: "billable_call_count",
        header: t("billing.reconciliation.columns.oltp"),
        width: 108,
        align: "right",
        mono: true,
        cell: (row) => formatCount(row.billable_call_count),
      },
      {
        id: "gateway_call_count",
        header: t("billing.reconciliation.columns.gateway"),
        width: 108,
        align: "right",
        mono: true,
        cell: (row) => formatCount(row.gateway_call_count),
      },
      {
        id: "upstream_preagg_call_count",
        header: t("billing.reconciliation.columns.olap"),
        width: 108,
        align: "right",
        mono: true,
        cell: (row) => formatCount(row.upstream_preagg_call_count),
      },
      {
        id: "discrepancy_ratio",
        header: t("billing.reconciliation.columns.ratio"),
        width: 96,
        align: "right",
        mono: true,
        cell: (row) => formatRatio(row.discrepancy_ratio),
      },
      {
        id: "status",
        header: t("billing.reconciliation.columns.status"),
        width: 96,
        cell: (row) => (
          <Badge variant={statusBadgeVariant(row.status)}>
            {statusLabel(row.status)}
          </Badge>
        ),
      },
      {
        id: "created_at",
        header: t("common.fields.createdAt"),
        width: 120,
        mono: true,
        cell: (row) => formatDate(row.created_at),
      },
      {
        id: "operations",
        header: t("common.fields.operations"),
        width: 310,
        align: "right",
        cell: (row) => {
          const canAck =
            row.status === "ok" ||
            row.status === "flagged" ||
            row.status === "error";
          const canRes = canAck || row.status === "acknowledged";
          return (
            <div
              className="inline-flex flex-wrap items-center justify-end gap-1"
              onClick={(e) => e.stopPropagation()}
            >
              <Button
                type="button"
                variant="link"
                size="xs"
                onClick={() => {
                  setDetailRow(row);
                  setMethodologyOpen(false);
                  setRawJsonOpen(false);
                }}
              >
                {t("common.actions.details")}
              </Button>
              <Button
                type="button"
                variant="link"
                size="xs"
                onClick={() => openCallMonitor(row, "customer")}
              >
                {t("billing.reconciliation.viewCustomerCalls")}
              </Button>
              <Button
                type="button"
                variant="link"
                size="xs"
                onClick={() => openCallMonitor(row, "upstream")}
              >
                {t("billing.reconciliation.viewUpstreamCalls")}
              </Button>
              {canWrite && canAck && (
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  className="text-warning-fg hover:text-warning-fg"
                  onClick={() => {
                    setResolveNote(row.resolution_note ?? "");
                    setResolveState({ row, target: "acknowledged" });
                  }}
                >
                  {t("billing.reconciliation.acknowledge")}
                </Button>
              )}
              {canWrite && canRes && (
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  className="text-success-fg hover:text-success-fg"
                  onClick={() => {
                    setResolveNote(row.resolution_note ?? "");
                    setResolveState({ row, target: "resolved" });
                  }}
                >
                  {t("billing.reconciliation.resolve")}
                </Button>
              )}
              {canWrite && (
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  className="text-danger-fg hover:text-danger-fg"
                  onClick={() => setDeleteTarget(row)}
                >
                  {t("common.actions.delete")}
                </Button>
              )}
            </div>
          );
        },
      },
    ],
    [canWrite, openCallMonitor, statusLabel, t],
  );

  const hasActiveSearch =
    Boolean(appliedQ) ||
    listFilters.length > 0 ||
    Boolean(filterFrom) ||
    Boolean(filterTo);

  return (
    <div
      className="flex flex-col flex-1 min-h-0 gap-4 min-w-0"
      data-testid="billing-reconciliation-panel"
    >
      {/* Calendar Heatmap Card (only rendered when records exist to avoid sparse empty banner) */}
      {calendar.length > 0 && (
        <EntityDetailCard
          title={t("ops.reconcile.calendar")}
          icon={<CalendarCheck className="h-4 w-4" />}
          actions={
            <div className="flex flex-wrap items-center gap-2.5">
              <span className="text-xs text-muted-foreground">
                {settingsSummaryLine}
              </span>
              <Select
                value={String(calendarDays)}
                options={calendarDaysOptions}
                onChange={(e) => setCalendarDays(Number(e.target.value) || 30)}
                className="w-28"
              />
            </div>
          }
        >
          {calendar.length > 0 ? (
            <div className="grid grid-cols-3 gap-2 sm:grid-cols-6 md:grid-cols-10">
              {calendar.map((d) => (
                <div
                  key={d.day}
                  title={`${d.day}: ${statusLabel(d.status)}`}
                  className={cn(
                    "rounded-md border px-2 py-2 text-center text-xs",
                    calendarCellClass(d.status),
                  )}
                >
                  <div className="font-mono-tech font-semibold">
                    {d.day.slice(5)}
                  </div>
                  <div className="mt-0.5 text-[11px] opacity-85">
                    {statusLabel(d.status)}
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="text-xs text-muted-foreground">
              {t("ops.reconcile.noCalendarRecords")}
            </div>
          )}
        </EntityDetailCard>
      )}

      {/* Discrepancy Table */}
      <DataTablePage<ReconciliationDiscrepancy>
        columns={columns}
        data={items}
        getRowId={(row) => row.id}
        loading={loading}
        emptyText={
          hasActiveSearch
            ? t("billing.reconciliation.emptyNoMatch")
            : t("billing.reconciliation.empty")
        }
        selectable={canWrite}
        selectedKeys={selectedIds}
        onSelectionChange={setSelectedIds}
        page={page}
        pageSize={pageSize}
        total={total}
        pageSizeOptions={[10, 20, 50]}
        onPageChange={setPage}
        onPageSizeChange={(next) => {
          setPageSize(next);
          setPage(1);
        }}
        filters={
          <div className="flex flex-wrap items-center gap-2">
            <StructuredFilterBar
              q={qInput}
              onQChange={setQInput}
              filters={listFilters}
              onFiltersChange={(next) => {
                setListFilters(next);
                setPage(1);
              }}
              catalog={reconciliationCatalog}
              onSearch={() => {
                setAppliedQ(qInput.trim());
                setPage(1);
              }}
            />
            <DateRangePicker
              value={{
                start: filterFrom ? `${filterFrom} 00:00:00` : "",
                end: filterTo ? `${filterTo} 23:59:59` : "",
              }}
              onChange={(next) => {
                setFilterFrom(next.start ? next.start.slice(0, 10) : "");
                setFilterTo(next.end ? next.end.slice(0, 10) : "");
                setPage(1);
              }}
              showPresets={false}
              clearable={Boolean(filterFrom || filterTo)}
              placeholder={t("billing.reconciliation.columns.date")}
            />
            {calendar.length === 0 && (
              <span className="hidden xl:inline-flex items-center gap-1.5 rounded-md bg-muted/65 px-2.5 py-1 text-[11px] text-muted-foreground">
                <CalendarCheck className="h-3.5 w-3.5 text-xblue shrink-0" />
                <span className="truncate">{settingsSummaryLine}</span>
              </span>
            )}
          </div>
        }
        batchActions={
          canWrite
            ? (ids) => (
                <Button
                  type="button"
                  variant="danger"
                  size="sm"
                  onClick={() => setBatchDeleteIds(ids)}
                  data-testid="reconcile-batch-delete-btn"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  {t("common.actions.delete")}
                </Button>
              )
            : undefined
        }
        actions={
          <>
            <Button
              type="button"
              variant="ghost"
              size="md"
              onClick={() => void reloadAll()}
              disabled={loading}
            >
              <RefreshCw
                className={loading ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"}
              />
              {t("common.actions.refresh")}
            </Button>
            <ExportDropdown
              items={exportItems}
              label={t("billing.actions.export")}
              disabled={items.length === 0}
              onCommand={handleExportCommand}
            />
            {canWrite && (
              <>
                <Button
                  type="button"
                  variant="outline"
                  size="md"
                  loading={settingsLoading}
                  onClick={openPolicyDialog}
                  data-testid="reconcile-policy-btn"
                >
                  <Settings className="h-3.5 w-3.5" />
                  {t("billing.reconciliation.policySettings")}
                </Button>
                <Button
                  type="button"
                  variant="primary"
                  size="md"
                  onClick={() => {
                    if (!runDate) setRunDate(formatDisplayDateYmd());
                    setRunOpen(true);
                  }}
                  data-testid="reconcile-run-btn"
                >
                  <Play className="h-3.5 w-3.5" />
                  {t("billing.reconciliation.runAudit")}
                </Button>
              </>
            )}
          </>
        }
      />

      {/* P0 A-13 Fix: Reconciliation Policy Settings Modal (size="md" 600px, label-top FormFields, non-wrapping units & switch rows) */}
      <FormModal
        open={policyOpen}
        onClose={() => setPolicyOpen(false)}
        onSubmit={handleSavePolicy}
        title={t("billing.reconciliation.policySettings")}
        size="md"
        submitLabel={t("common.save")}
        submitting={policySaving}
        testId="reconcile-policy-modal"
      >
        <div className="space-y-3">
          {/* Switch row 1 */}
          <div className="flex items-center justify-between gap-4 rounded-md border border-border-subtle px-3.5 py-2.5">
            <span className="text-xs font-medium text-foreground whitespace-nowrap">
              {t("billing.reconciliation.scheduledEnabled")}
            </span>
            <Switch
              checked={policyEnabled}
              onCheckedChange={setPolicyEnabled}
              aria-label={t("billing.reconciliation.scheduledEnabled")}
              data-testid="policy-scheduled-switch"
            />
          </div>

          {/* Numeric inputs in label-top FormFields with shrink-0 whitespace-nowrap units */}
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <FormField
              label={t("billing.reconciliation.intervalHours")}
              required
              error={policyErrors.intervalHours}
            >
              <div className="flex items-center gap-2">
                <NumberInput
                  value={policyIntervalHours}
                  min={0.5}
                  max={720}
                  step={1}
                  onChange={(next) => {
                    setPolicyIntervalHours(next);
                    if (policyErrors.intervalHours) {
                      setPolicyErrors((prev) => ({
                        ...prev,
                        intervalHours: undefined,
                      }));
                    }
                  }}
                  className="w-40"
                  aria-label={t("billing.reconciliation.intervalHours")}
                />
                <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
                  {t("ops.units.hours")}
                </span>
              </div>
            </FormField>

            <FormField
              label={t("billing.reconciliation.lossRatioThreshold")}
              required
              error={policyErrors.thresholdPercent}
            >
              <div className="flex items-center gap-2">
                <NumberInput
                  value={policyThresholdPercent}
                  min={0}
                  max={100}
                  step={0.5}
                  onChange={(next) => {
                    setPolicyThresholdPercent(next);
                    if (policyErrors.thresholdPercent) {
                      setPolicyErrors((prev) => ({
                        ...prev,
                        thresholdPercent: undefined,
                      }));
                    }
                  }}
                  className="w-40"
                  aria-label={t("billing.reconciliation.lossRatioThreshold")}
                />
                <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
                  %
                </span>
              </div>
            </FormField>
          </div>

          {/* Boolean policy toggles — clean full-row flex items-center justify-between */}
          <div className="space-y-2">
            <div className="flex items-center justify-between gap-4 rounded-md border border-border-subtle px-3.5 py-2.5">
              <span className="text-xs font-medium text-foreground whitespace-nowrap">
                {t("billing.reconciliation.usePreviousDay")}
              </span>
              <Switch
                checked={policyUsePreviousDay}
                onCheckedChange={setPolicyUsePreviousDay}
                aria-label={t("billing.reconciliation.usePreviousDay")}
              />
            </div>

            <div className="flex items-center justify-between gap-4 rounded-md border border-border-subtle px-3.5 py-2.5">
              <span className="text-xs font-medium text-foreground whitespace-nowrap">
                {t("billing.reconciliation.includeAmount")}
              </span>
              <Switch
                checked={policyIncludeAmount}
                onCheckedChange={setPolicyIncludeAmount}
                aria-label={t("billing.reconciliation.includeAmount")}
              />
            </div>

            <div className="flex items-center justify-between gap-4 rounded-md border border-border-subtle px-3.5 py-2.5">
              <span className="text-xs font-medium text-foreground whitespace-nowrap">
                {t("billing.reconciliation.warnBeforeBill")}
              </span>
              <Switch
                checked={policyWarnBeforeBill}
                onCheckedChange={setPolicyWarnBeforeBill}
                aria-label={t("billing.reconciliation.warnBeforeBill")}
              />
            </div>

            <div className="flex items-center justify-between gap-4 rounded-md border border-border-subtle px-3.5 py-2.5">
              <span className="text-xs font-medium text-foreground whitespace-nowrap">
                {t("billing.reconciliation.alertOnFlagged")}
              </span>
              <Switch
                checked={policyAlertOnFlagged}
                onCheckedChange={(checked) => {
                  setPolicyAlertOnFlagged(checked);
                  if (!checked && policyErrors.webhookUrl) {
                    setPolicyErrors((prev) => ({
                      ...prev,
                      webhookUrl: undefined,
                    }));
                  }
                }}
                aria-label={t("billing.reconciliation.alertOnFlagged")}
              />
            </div>
          </div>

          <FormField
            label={t("billing.reconciliation.alertWebhook")}
            htmlFor="policy-webhook-input"
            required={policyAlertOnFlagged}
            error={policyErrors.webhookUrl}
          >
            <Input
              id="policy-webhook-input"
              value={policyWebhookUrl}
              disabled={!policyAlertOnFlagged}
              onChange={(e) => {
                setPolicyWebhookUrl(e.target.value);
                if (policyErrors.webhookUrl) {
                  setPolicyErrors((prev) => ({
                    ...prev,
                    webhookUrl: undefined,
                  }));
                }
              }}
              placeholder={t("billing.reconciliation.alertWebhookPlaceholder")}
            />
          </FormField>
        </div>
      </FormModal>

      {/* Run Audit Modal */}
      <FormModal
        open={runOpen}
        onClose={() => setRunOpen(false)}
        onSubmit={handleRunAudit}
        title={t("billing.reconciliation.runAudit")}
        size="sm"
        submitLabel={t("billing.reconciliation.runAudit")}
        submitting={runLoading}
        testId="reconcile-run-modal"
      >
        <FormField
          label={t("billing.reconciliation.runDatePlaceholder")}
          htmlFor="reconcile-run-date"
          required
        >
          <Input
            id="reconcile-run-date"
            type="date"
            value={runDate}
            onChange={(e) => setRunDate(e.target.value)}
            className="font-mono-tech"
          />
        </FormField>
      </FormModal>

      {/* Resolve / Acknowledge Modal */}
      <FormModal
        open={Boolean(resolveState)}
        onClose={() => setResolveState(null)}
        onSubmit={handleSubmitResolve}
        title={
          resolveState?.target === "resolved"
            ? t("billing.reconciliation.resolveTitle")
            : t("billing.reconciliation.acknowledgeTitle")
        }
        size="sm"
        submitLabel={t("common.confirm")}
        submitting={resolveLoading}
      >
        <FormField
          label={t("billing.reconciliation.note")}
          htmlFor="reconcile-resolve-note"
        >
          <Textarea
            id="reconcile-resolve-note"
            rows={3}
            maxLength={500}
            value={resolveNote}
            onChange={(e) => setResolveNote(e.target.value)}
            placeholder={t("billing.reconciliation.notePlaceholder")}
          />
        </FormField>
      </FormModal>

      {/* Discrepancy Detail Drawer */}
      <Drawer
        open={Boolean(detailRow)}
        onClose={() => setDetailRow(null)}
        title={t("billing.reconciliation.detailTitle")}
        size="lg"
        testId="reconcile-detail-drawer"
      >
        {detailRow && detailView && (
          <div className="space-y-4">
            <div className="divide-y divide-border-subtle rounded-md border border-border">
              <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <span className="text-muted-foreground">
                  {t("billing.reconciliation.columns.date")}
                </span>
                <span className="font-mono-tech font-medium">
                  {detailRow.reconciliation_date}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <span className="text-muted-foreground">
                  {t("billing.reconciliation.columns.scope")}
                </span>
                <span className="font-mono-tech font-medium">
                  {detailRow.scope_type}
                  {detailRow.scope_id ? ` / ${detailRow.scope_id}` : ""}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <span className="text-muted-foreground">
                  {t("billing.reconciliation.columns.status")}
                </span>
                <Badge variant={statusBadgeVariant(detailRow.status)}>
                  {statusLabel(detailRow.status)}
                </Badge>
              </div>
              <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <span className="text-muted-foreground">
                  {t("billing.reconciliation.note")}
                </span>
                <span className="text-foreground">
                  {detailRow.resolution_note || EMPTY_PLACEHOLDER}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <span className="text-muted-foreground">
                  {t("common.fields.createdAt")}
                </span>
                <span className="font-mono-tech">
                  {formatDate(detailRow.created_at)}
                </span>
              </div>
              {detailRow.resolved_at && (
                <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                  <span className="text-muted-foreground">
                    {t("billing.reconciliation.resolvedAt")}
                  </span>
                  <span className="font-mono-tech">
                    {formatDate(detailRow.resolved_at)}
                  </span>
                </div>
              )}
            </div>

            {detailView.errorText && (
              <div className="flex items-start gap-2 rounded-md border border-danger-fg/35 bg-danger-fg/10 px-3 py-2.5 text-xs text-danger-fg">
                <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
                <div className="space-y-0.5">
                  <div className="font-semibold">
                    {t("billing.reconciliation.detail.errorTitle")}
                  </div>
                  <div>{detailView.errorText}</div>
                </div>
              </div>
            )}

            {/* Summary Box */}
            <EntityDetailCard
              title={t("billing.reconciliation.detail.summaryTitle")}
            >
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <div className="space-y-0.5">
                  <div className="text-[11px] text-muted-foreground">
                    {t("billing.reconciliation.columns.ratio")}
                  </div>
                  <div className="text-xs font-semibold font-mono-tech">
                    {formatRatio(detailRow.discrepancy_ratio)}
                    {detailView.threshold != null && (
                      <span className="ml-1.5 font-normal text-muted-foreground">
                        (
                        {t("billing.reconciliation.detail.threshold", {
                          value: formatRatio(detailView.threshold),
                        })}
                        )
                      </span>
                    )}
                  </div>
                </div>
                <div className="space-y-0.5">
                  <div className="text-[11px] text-muted-foreground">
                    {t("billing.reconciliation.detail.vsThreshold")}
                  </div>
                  <div>
                    <Badge
                      variant={detailView.overThreshold ? "danger" : "success"}
                    >
                      {detailView.overThreshold
                        ? t("billing.reconciliation.detail.overThreshold")
                        : t("billing.reconciliation.detail.withinThreshold")}
                    </Badge>
                  </div>
                </div>
                <div className="space-y-0.5 sm:col-span-2">
                  <div className="text-[11px] text-muted-foreground">
                    {t("billing.reconciliation.detail.primaryCompare")}
                  </div>
                  <div className="text-xs text-foreground">
                    {t("billing.reconciliation.detail.primaryCompareLine", {
                      billing: formatCount(detailView.oltpCalls),
                      peer: formatCount(detailView.comparisonCalls),
                      peerLabel: detailView.comparisonLabel,
                    })}
                  </div>
                </div>
                <div className="space-y-0.5 sm:col-span-2">
                  <div className="text-[11px] text-muted-foreground">
                    {t("billing.reconciliation.detail.callDiff")}
                  </div>
                  <div
                    className={cn(
                      "text-xs font-medium",
                      detailView.callDiffAbs > 0
                        ? "text-warning-fg"
                        : "text-foreground",
                    )}
                  >
                    {detailView.callDiffText}
                  </div>
                </div>
              </div>
            </EntityDetailCard>

            {/* Call Breakdown */}
            <EntityDetailCard
              title={t("billing.reconciliation.detail.callsTitle")}
              bodyClassName="p-0"
            >
              <table className="w-full border-collapse text-xs text-foreground">
                <thead>
                  <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                    <th className="h-8 px-3 text-left font-medium">
                      {t("billing.reconciliation.detail.sourceCol")}
                    </th>
                    <th className="h-8 px-3 text-right font-medium">
                      {t("billing.reconciliation.detail.countCol")}
                    </th>
                    <th className="h-8 px-3 text-left font-medium">
                      {t("billing.reconciliation.detail.roleCol")}
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border-subtle">
                  {detailView.callRows.map((row) => (
                    <tr key={row.label}>
                      <td className="px-3 py-2">{row.label}</td>
                      <td className="px-3 py-2 text-right font-mono-tech tabular-nums">
                        {row.countText}
                      </td>
                      <td className="px-3 py-2">
                        {row.primary ? (
                          <Badge variant="xblue">{row.role}</Badge>
                        ) : (
                          <span className="text-muted-foreground">
                            {row.role}
                          </span>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </EntityDetailCard>

            {/* Amount Recon */}
            {detailView.amount && (
              <EntityDetailCard
                title={t("billing.reconciliation.detail.amountTitle")}
              >
                <div className="divide-y divide-border-subtle rounded-md border border-border">
                  <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                    <span className="text-muted-foreground">
                      {t("billing.reconciliation.detail.usageAmount")}
                    </span>
                    <span className="font-mono-tech tabular-nums">
                      {formatMoney(detailView.amount.usageBillable)}
                    </span>
                  </div>
                  <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                    <span className="text-muted-foreground">
                      {t("billing.reconciliation.detail.billItemsTotal")}
                    </span>
                    <span className="font-mono-tech tabular-nums">
                      {formatMoney(detailView.amount.billItemsTotal)}
                    </span>
                  </div>
                  <div className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                    <span className="text-muted-foreground">
                      {t("billing.reconciliation.detail.amountDiff")}
                    </span>
                    <span
                      className={cn(
                        "font-mono-tech font-medium tabular-nums",
                        detailView.amount.diffIsZero
                          ? "text-success-fg"
                          : "text-warning-fg",
                      )}
                    >
                      {formatMoney(detailView.amount.diff)}
                      {detailView.amount.diffIsZero &&
                        ` · ${t("billing.reconciliation.detail.amountMatch")}`}
                    </span>
                  </div>
                </div>
              </EntityDetailCard>
            )}

            {/* Collapsible Methodology & Raw JSON */}
            <div className="space-y-2">
              <div className="overflow-hidden rounded-md border border-border">
                <button
                  type="button"
                  onClick={() => setMethodologyOpen((v) => !v)}
                  className="flex w-full items-center gap-1.5 bg-muted/30 px-3 py-2 text-left text-xs font-medium text-foreground hover:bg-hover-bg cursor-pointer"
                >
                  {methodologyOpen ? (
                    <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
                  ) : (
                    <ChevronRight className="h-3.5 w-3.5 text-muted-foreground" />
                  )}
                  <span>
                    {t("billing.reconciliation.detail.methodologyTitle")}
                  </span>
                </button>
                {methodologyOpen && (
                  <ul className="list-disc space-y-1 border-t border-border py-2.5 pl-7 pr-3 text-xs leading-relaxed text-muted-foreground">
                    {detailView.methodologyLines.map((line, idx) => (
                      <li key={idx}>{line}</li>
                    ))}
                  </ul>
                )}
              </div>

              {detailRow.details && (
                <div className="overflow-hidden rounded-md border border-border">
                  <button
                    type="button"
                    onClick={() => setRawJsonOpen((v) => !v)}
                    className="flex w-full items-center gap-1.5 bg-muted/30 px-3 py-2 text-left text-xs font-medium text-foreground hover:bg-hover-bg cursor-pointer"
                  >
                    {rawJsonOpen ? (
                      <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
                    ) : (
                      <ChevronRight className="h-3.5 w-3.5 text-muted-foreground" />
                    )}
                    <span>
                      {t("billing.reconciliation.detail.rawSnapshotTitle")}
                    </span>
                  </button>
                  {rawJsonOpen && (
                    <pre className="max-h-72 overflow-auto border-t border-border bg-muted/25 p-3 font-mono-tech text-[11px] leading-relaxed text-foreground">
                      {JSON.stringify(detailRow.details, null, 2)}
                    </pre>
                  )}
                </div>
              )}
            </div>
          </div>
        )}
      </Drawer>

      {/* Delete Confirms */}
      <ConfirmDialog
        open={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        onConfirm={() => void handleConfirmSingleDelete()}
        loading={deleting}
        danger
        title={t("billing.reconciliation.deleteConfirm.title")}
        confirmLabel={t("common.actions.delete")}
        description={
          deleteTarget
            ? t("billing.reconciliation.deleteConfirm.one", {
                name: `${deleteTarget.reconciliation_date} · ${statusLabel(deleteTarget.status)}`,
              })
            : ""
        }
      />

      <ConfirmDialog
        open={Boolean(batchDeleteIds && batchDeleteIds.length > 0)}
        onClose={() => setBatchDeleteIds(null)}
        onConfirm={() => void handleConfirmBatchDelete()}
        loading={deleting}
        danger
        title={t("billing.reconciliation.deleteConfirm.title")}
        confirmLabel={t("common.actions.delete")}
        description={
          batchDeleteIds
            ? t("billing.reconciliation.deleteConfirm.selected", {
                count: batchDeleteIds.length,
              })
            : ""
        }
      />
    </div>
  );
}
