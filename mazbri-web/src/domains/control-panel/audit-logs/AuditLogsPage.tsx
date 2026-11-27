import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronUp,
  Download,
  RefreshCw,
  RotateCcw,
  Search,
} from "lucide-react";
import {
  exportAuditLogs,
  fetchAuditLogById,
  fetchAuditLogList,
} from "@/api/audit";
import {
  Badge,
  Button,
  CopyableText,
  DataTablePage,
  DateRangePicker,
  Input,
  Popover,
  Select,
  StatusBadge,
  Tooltip,
  type DataTableColumn,
  type DateRangeValue,
} from "@/components/ui";
import { useT } from "@/locales";
import type {
  AuditLog,
  AuditLogDetail,
  AuditLogListQuery,
  AuditService,
  AuditStatus,
} from "@/types/api/audit";
import { downloadCsv, generateCsv } from "@/utils/csv";
import {
  EMPTY_PLACEHOLDER,
  formatDateTime,
  parseDisplayWallClockMs,
} from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import {
  AuditDetailDrawer,
  formatAuditAction,
  formatAuditActorName,
  formatAuditDetailValue,
  formatAuditFailureReason,
  formatAuditResourceType,
  formatAuditService,
  mapAuditStatusBadgeCode,
} from "./AuditDetailDrawer";
import {
  ACTION_LABELS,
  AUDIT_SERVICES,
  RESOURCE_TYPE_LABELS,
  formatChainFlowCode,
  groupAuditLogsByRequestId,
  guardCsvFormula,
  statusLabelKey,
  type GroupedAuditRow,
} from "./labels";

export function AuditLogsPage() {
  const { t } = useT();

  const [rawLogs, setRawLogs] = useState<AuditLog[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);
  const [loading, setLoading] = useState(false);

  const [keywordInput, setKeywordInput] = useState("");
  const [appliedKeyword, setAppliedKeyword] = useState("");
  const [actionFilter, setActionFilter] = useState("");
  const [resourceFilter, setResourceFilter] = useState("");
  const [statusFilter, setStatusFilter] = useState<"" | AuditStatus>("");
  const [serviceFilter, setServiceFilter] = useState<"" | AuditService>("");
  const [dateRange, setDateRange] = useState<DateRangeValue>({
    start: "",
    end: "",
  });
  const [includeSelfAudit, setIncludeSelfAudit] = useState(false);

  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [expandedRequestIds, setExpandedRequestIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [exportMenuOpen, setExportMenuOpen] = useState(false);
  const [exporting, setExporting] = useState(false);

  const [detailOpen, setDetailOpen] = useState(false);
  const [detailLoading, setDetailLoading] = useState(false);
  const [currentLog, setCurrentLog] = useState<AuditLogDetail | null>(null);

  const buildQueryParams = useCallback(
    (includePagination = true): AuditLogListQuery => {
      const params: AuditLogListQuery = {};
      if (includePagination) {
        params.page = page;
        params.limit = pageSize;
      }
      if (appliedKeyword) params.q = appliedKeyword;
      if (actionFilter) params.action = actionFilter;
      if (resourceFilter) params.resource_type = resourceFilter;
      if (statusFilter) params.status = statusFilter;
      if (serviceFilter) params.service = serviceFilter;

      const startMs = parseDisplayWallClockMs(dateRange.start);
      const endMs = parseDisplayWallClockMs(dateRange.end);
      if (Number.isFinite(startMs)) {
        params.start_time = Math.floor(startMs / 1000);
      }
      if (Number.isFinite(endMs)) {
        params.end_time = Math.floor(endMs / 1000);
      }
      if (includeSelfAudit) {
        params.include_self_audit = true;
      }
      return params;
    },
    [
      page,
      pageSize,
      appliedKeyword,
      actionFilter,
      resourceFilter,
      statusFilter,
      serviceFilter,
      dateRange,
      includeSelfAudit,
    ],
  );

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetchAuditLogList(buildQueryParams(true));
      setRawLogs(res.logs ?? []);
      setTotal(res.total ?? 0);
      setSelectedIds([]);
      setExpandedRequestIds(new Set());
    } catch (error) {
      logger.error("Failed to load audit logs:", error);
    } finally {
      setLoading(false);
    }
  }, [buildQueryParams]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const displayData = useMemo<GroupedAuditRow<AuditLog>[]>(
    () => groupAuditLogsByRequestId(rawLogs, expandedRequestIds),
    [rawLogs, expandedRequestIds],
  );

  const toggleChainExpand = useCallback((requestId?: string | null) => {
    const key = requestId?.trim();
    if (!key) return;
    setExpandedRequestIds((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const handleOpenDetail = useCallback(async (row: AuditLog) => {
    setDetailOpen(true);
    setDetailLoading(true);
    try {
      const detail = await fetchAuditLogById(row.id);
      setCurrentLog(detail);
    } catch (error) {
      logger.error("Failed to load audit log detail:", error);
    } finally {
      setDetailLoading(false);
    }
  }, []);

  const relatedChainRecords = useMemo<AuditLog[]>(() => {
    const reqId = currentLog?.request_id?.trim();
    if (!reqId) return [];
    const matches = rawLogs.filter((item) => item.request_id?.trim() === reqId);
    const uniqueServices = new Set(matches.map((item) => item.service));
    return matches.length >= 2 && uniqueServices.size >= 2 ? matches : [];
  }, [currentLog, rawLogs]);

  const handleSearchSubmit = () => {
    setPage(1);
    setAppliedKeyword(keywordInput.trim());
  };

  const handleResetFilters = () => {
    setKeywordInput("");
    setAppliedKeyword("");
    setActionFilter("");
    setResourceFilter("");
    setStatusFilter("");
    setServiceFilter("");
    setDateRange({ start: "", end: "" });
    setIncludeSelfAudit(false);
    setPage(1);
  };

  const handleExport = async (mode: "all" | "selected") => {
    setExportMenuOpen(false);
    setExporting(true);
    try {
      if (mode === "selected") {
        const selectedRows = displayData.filter((r) =>
          selectedIds.includes(r.id),
        );
        if (selectedRows.length === 0) return;
        const csvRows = selectedRows.map((row) => {
          const rec: Record<string, string> = {
            timestamp: formatDateTime(row.timestamp),
            actor_name: formatAuditActorName(row, t),
            actor_role: row.actor_role || EMPTY_PLACEHOLDER,
            action: formatAuditAction(row.action, t),
            resource_type: formatAuditResourceType(row.resource_type, t),
            status: t(statusLabelKey(row.status)),
            failure_reason: formatAuditFailureReason(row.failure_reason, t),
            ip_address: row.ip_address?.trim()
              ? row.ip_address
              : t("auditLogs.detail.notRecorded"),
            request_id: row.request_id || EMPTY_PLACEHOLDER,
            route:
              [row.method, row.path].filter(Boolean).join(" ") ||
              EMPTY_PLACEHOLDER,
            status_code:
              row.status_code == null
                ? EMPTY_PLACEHOLDER
                : String(row.status_code),
            duration_ms:
              row.duration_ms == null
                ? EMPTY_PLACEHOLDER
                : String(row.duration_ms),
            service: formatAuditService(row.service, t),
            details: formatAuditDetailValue(row.details, t),
          };
          return Object.fromEntries(
            Object.entries(rec).map(([k, v]) => [k, guardCsvFormula(v)]),
          );
        });
        const csv = generateCsv(csvRows, [
          { key: "timestamp", label: t("auditLogs.columns.timestamp") },
          { key: "actor_name", label: t("auditLogs.columns.actorName") },
          { key: "actor_role", label: t("auditLogs.columns.actorRole") },
          { key: "action", label: t("auditLogs.columns.action") },
          { key: "resource_type", label: t("auditLogs.columns.resourceType") },
          { key: "status", label: t("auditLogs.columns.status") },
          {
            key: "failure_reason",
            label: t("auditLogs.columns.failureReason"),
          },
          { key: "ip_address", label: t("auditLogs.columns.ipAddress") },
          { key: "request_id", label: t("auditLogs.columns.requestId") },
          { key: "route", label: t("auditLogs.columns.route") },
          { key: "status_code", label: t("auditLogs.columns.statusCode") },
          { key: "duration_ms", label: t("auditLogs.columns.durationMs") },
          { key: "service", label: t("auditLogs.columns.service") },
          { key: "details", label: t("auditLogs.columns.details") },
        ]);
        const stamp = formatDateTime(Date.now())
          .replace(" ", "_")
          .replaceAll(":", "-");
        downloadCsv(csv, `audit_logs_selected_${stamp}.csv`);
        return;
      }

      const csv = await exportAuditLogs(buildQueryParams(false));
      const stamp = formatDateTime(Date.now())
        .replace(" ", "_")
        .replaceAll(":", "-");
      downloadCsv(
        csv.startsWith("\uFEFF") ? csv : `\uFEFF${csv}`,
        `audit_logs_all_${stamp}.csv`,
      );
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to export audit logs:", error);
      }
    } finally {
      setExporting(false);
    }
  };

  const actionOptions = useMemo(
    () => [
      {
        value: "",
        label: `${t("auditLogs.columns.action")}: ${t("common.all")}`,
        text: t("common.all"),
      },
      ...Object.entries(ACTION_LABELS).map(([code, key]) => {
        const label = t(key);
        return { value: code, label, text: `${label} ${code}` };
      }),
    ],
    [t],
  );

  const resourceOptions = useMemo(
    () => [
      {
        value: "",
        label: `${t("auditLogs.columns.resourceType")}: ${t("common.all")}`,
        text: t("common.all"),
      },
      ...Object.entries(RESOURCE_TYPE_LABELS).map(([code, key]) => {
        const label = t(key);
        return { value: code, label, text: `${label} ${code}` };
      }),
    ],
    [t],
  );

  const serviceOptions = useMemo(
    () => [
      {
        value: "",
        label: `${t("auditLogs.columns.service")}: ${t("common.all")}`,
        text: t("common.all"),
      },
      ...AUDIT_SERVICES.map((svc) => {
        const label = formatAuditService(svc, t);
        return { value: svc, label, text: label };
      }),
    ],
    [t],
  );

  const columns = useMemo<DataTableColumn<GroupedAuditRow<AuditLog>>[]>(
    () => [
      {
        id: "timestamp",
        header: t("auditLogs.columns.timestamp"),
        minWidth: 164,
        mono: true,
        cell: (row) => formatDateTime(row.timestamp),
      },
      {
        id: "actor_name",
        header: t("auditLogs.columns.actorName"),
        minWidth: 96,
        cell: (row) => formatAuditActorName(row, t),
      },
      {
        id: "action",
        header: t("auditLogs.columns.action"),
        minWidth: 132,
        cell: (row) => (
          <Badge variant="xblue">{formatAuditAction(row.action, t)}</Badge>
        ),
      },
      {
        id: "resource_type",
        header: t("auditLogs.columns.resourceType"),
        minWidth: 96,
        cell: (row) => formatAuditResourceType(row.resource_type, t),
      },
      {
        id: "status",
        header: t("auditLogs.columns.status"),
        width: 96,
        cell: (row) => (
          <StatusBadge
            status={mapAuditStatusBadgeCode(row.status)}
            label={t(statusLabelKey(row.status))}
          />
        ),
      },
      {
        id: "service",
        header: t("auditLogs.columns.service"),
        minWidth: 132,
        cell: (row) => {
          if (row.chainServices.length >= 2 && row.request_id) {
            const flow = row.chainServices
              .map((svc) => formatAuditService(svc, t))
              .join(" ➔ ");
            const tip = row.chainExpanded
              ? t("auditLogs.chain.collapseTooltip", { flow })
              : t("auditLogs.chain.expandTooltip", {
                  flow,
                  count: row.mirrorChildren.length + 1,
                });
            return (
              <Tooltip content={tip}>
                <button
                  type="button"
                  aria-expanded={row.chainExpanded}
                  onClick={(e) => {
                    e.stopPropagation();
                    toggleChainExpand(row.request_id);
                  }}
                  className="inline-flex items-center gap-1 rounded-md border border-primary/35 bg-primary-soft px-2 py-0.5 text-[11px] font-mono-tech text-primary hover:bg-primary-soft-hover transition-colors cursor-pointer"
                >
                  <span>{formatChainFlowCode(row.chainServices)}</span>
                  {row.chainExpanded ? (
                    <ChevronUp className="h-3 w-3" />
                  ) : (
                    <ChevronDown className="h-3 w-3" />
                  )}
                </button>
              </Tooltip>
            );
          }
          if (row.isMirrorChild) {
            return (
              <span className="inline-flex items-center gap-1.5 whitespace-nowrap">
                <span className="text-[11px] text-muted-foreground">
                  {t("auditLogs.chain.mirrorPrefix")}
                </span>
                <Badge variant="neutral">
                  {formatAuditService(row.service, t)}
                </Badge>
              </span>
            );
          }
          return (
            <Badge variant="neutral">
              {formatAuditService(row.service, t)}
            </Badge>
          );
        },
      },
      {
        id: "failure_reason",
        header: t("auditLogs.columns.failureReason"),
        minWidth: 108,
        cell: (row) => (
          <span className="text-muted-foreground">
            {formatAuditFailureReason(row.failure_reason, t)}
          </span>
        ),
      },
      {
        id: "ip_address",
        header: t("auditLogs.columns.ipAddress"),
        minWidth: 132,
        cell: (row) =>
          row.ip_address?.trim() ? (
            <CopyableText
              value={row.ip_address}
              copyTooltip={t("auditLogs.detail.fields.ipAddress")}
            />
          ) : (
            <span className="text-muted-foreground">
              {t("auditLogs.detail.notRecorded")}
            </span>
          ),
      },
      {
        id: "operations",
        header: t("common.fields.operations"),
        width: 84,
        align: "right",
        cell: (row) => (
          <div
            className="inline-flex items-center justify-end"
            onClick={(e) => e.stopPropagation()}
          >
            <Button
              type="button"
              variant="link"
              size="xs"
              onClick={() => void handleOpenDetail(row)}
            >
              {t("common.actions.details")}
            </Button>
          </div>
        ),
      },
    ],
    [t, toggleChainExpand, handleOpenDetail],
  );

  const hasFilters = Boolean(
    appliedKeyword ||
      actionFilter ||
      resourceFilter ||
      statusFilter ||
      serviceFilter ||
      dateRange.start ||
      dateRange.end ||
      includeSelfAudit,
  );

  return (
    <>
      <DataTablePage<GroupedAuditRow<AuditLog>>
        testId="audit-logs-page"
        columns={columns}
        data={displayData}
        getRowId={(row) => row.id}
        loading={loading}
        selectable
        selectedKeys={selectedIds}
        onSelectionChange={setSelectedIds}
        emptyText={
          hasFilters
            ? t("auditLogs.empty.filtered")
            : t("auditLogs.empty.default")
        }
        page={page}
        pageSize={pageSize}
        total={total}
        pageSizeOptions={[10, 20, 50]}
        onPageChange={setPage}
        onPageSizeChange={(nextSize) => {
          setPageSize(nextSize);
          setPage(1);
        }}
        onRowClick={(row) => void handleOpenDetail(row)}
        filters={
          <>
            <div className="relative w-48">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
              <Input
                value={keywordInput}
                onChange={(e) => setKeywordInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleSearchSubmit();
                }}
                placeholder={t("common.search")}
                aria-label={t("common.search")}
                className="pl-8"
                data-testid="audit-search-input"
              />
            </div>

            <Select
              value={actionFilter}
              onChange={(e) => {
                setActionFilter(e.target.value);
                setPage(1);
              }}
              searchable
              aria-label={t("auditLogs.columns.action")}
              className="w-36"
              options={actionOptions}
            />

            <Select
              value={resourceFilter}
              onChange={(e) => {
                setResourceFilter(e.target.value);
                setPage(1);
              }}
              searchable
              aria-label={t("auditLogs.columns.resourceType")}
              className="w-32"
              options={resourceOptions}
            />

            <Select
              value={statusFilter}
              onChange={(e) => {
                setStatusFilter(e.target.value as "" | AuditStatus);
                setPage(1);
              }}
              aria-label={t("auditLogs.columns.status")}
              className="w-28"
              options={[
                {
                  value: "",
                  label: `${t("auditLogs.columns.status")}: ${t("common.all")}`,
                  text: t("common.all"),
                },
                {
                  value: "success",
                  label: t("auditLogs.status.success"),
                  text: t("auditLogs.status.success"),
                },
                {
                  value: "failure",
                  label: t("auditLogs.status.failure"),
                  text: t("auditLogs.status.failure"),
                },
                {
                  value: "denied",
                  label: t("auditLogs.status.denied"),
                  text: t("auditLogs.status.denied"),
                },
              ]}
            />

            <Select
              value={serviceFilter}
              onChange={(e) => {
                setServiceFilter(e.target.value as "" | AuditService);
                setPage(1);
              }}
              aria-label={t("auditLogs.columns.service")}
              className="w-32"
              options={serviceOptions}
            />

            <DateRangePicker
              value={dateRange}
              onChange={(next) => {
                setDateRange(next);
                setPage(1);
              }}
              placeholder={t("common.timeRange")}
              testId="audit-date-range-picker"
            />

            <label className="inline-flex h-8 items-center gap-1.5 rounded-md border border-input bg-card px-2.5 text-xs text-foreground hover:bg-hover-bg transition-colors cursor-pointer select-none">
              <input
                type="checkbox"
                checked={includeSelfAudit}
                onChange={(e) => {
                  setIncludeSelfAudit(e.target.checked);
                  setPage(1);
                }}
              />
              <span className="whitespace-nowrap">
                {t("auditLogs.filters.includeSelfAudit")}
              </span>
            </label>

            <Button
              type="button"
              variant="outline"
              size="md"
              onClick={handleSearchSubmit}
            >
              {t("common.search")}
            </Button>

            {hasFilters && (
              <Button
                type="button"
                variant="ghost"
                size="md"
                onClick={handleResetFilters}
              >
                <RotateCcw className="h-3.5 w-3.5" />
                {t("common.reset")}
              </Button>
            )}
          </>
        }
        actions={
          <>
            <Popover
              open={exportMenuOpen}
              onOpenChange={setExportMenuOpen}
              align="right"
              contentClassName="w-44 p-1 space-y-0.5"
              trigger={
                <Button
                  type="button"
                  variant="outline"
                  size="md"
                  loading={exporting}
                  disabled={displayData.length === 0}
                  onClick={() => setExportMenuOpen((v) => !v)}
                >
                  <Download className="h-3.5 w-3.5" />
                  {t("common.actions.export")}
                </Button>
              }
            >
              <button
                type="button"
                disabled={displayData.length === 0}
                onClick={() => void handleExport("all")}
                className="w-full rounded-md px-2.5 py-1.5 text-left text-xs text-foreground hover:bg-hover-bg disabled:opacity-40 transition-colors cursor-pointer"
              >
                {t("common.actions.exportAll")}
              </button>
              <button
                type="button"
                disabled={selectedIds.length === 0}
                onClick={() => void handleExport("selected")}
                className="w-full rounded-md px-2.5 py-1.5 text-left text-xs text-foreground hover:bg-hover-bg disabled:opacity-40 transition-colors cursor-pointer"
              >
                {t("common.actions.exportSelected", {
                  count: selectedIds.length,
                })}
              </button>
            </Popover>

            <Button
              type="button"
              variant="outline"
              size="md"
              onClick={() => void loadData()}
            >
              <RefreshCw className="h-3.5 w-3.5" />
              {t("common.actions.refresh")}
            </Button>
          </>
        }
      />

      <AuditDetailDrawer
        open={detailOpen}
        loading={detailLoading}
        log={currentLog}
        relatedChainRecords={relatedChainRecords}
        onClose={() => setDetailOpen(false)}
        onSelectRelated={(item) => void handleOpenDetail(item)}
      />
    </>
  );
}
