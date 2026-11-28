import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { BellOff, X } from "lucide-react";
import {
  ActionChip,
  Badge,
  Button,
  CopyableText,
  DataTablePage,
  DateRangePicker,
  Drawer,
  EmptyState,
  FormField,
  FormModal,
  IconButton,
  Input,
  ListQueryBar,
  StatusBadge,
  Textarea,
  Tooltip,
  type DataTableColumn,
  type DateRangeValue,
} from "@/components/ui";
import { createAlertSilence, fetchAlertEvents } from "@/api/alerts";
import { fetchOpsChanges, type ChangeEvent } from "@/api/opsChanges";
import { LanguageEnum } from "@/enums/appEnum";
import { alertEventsCatalog } from "@/list-catalog/common-resources";
import { useT } from "@/locales";
import type {
  AlertEvent,
  AlertSeverity,
  AlertStatus,
} from "@/types/api/alerts";
import type { ListFilter } from "@/types/list-query";
import { getSeverityLabel, getStatusLabel } from "@/utils/display-labels";
import { formatDateTime, parseDisplayWallClockMs } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import {
  formatAlertLabels,
  formatAlertTime,
  formatAlertValue,
} from "@/utils/ops/alerts-format";
import {
  computeAlertDrillWindow,
  resolveAlertDrillRoute,
} from "@/utils/ops/alerts-link";
import {
  buildTraceRoute,
  canDrillToTrace,
  isTraceExpired,
} from "@/utils/ops/trace-link";
import { showErrorToast, showSuccessToast } from "@/utils/ui/message-guard";
import { AutoRefreshControl } from "../shared/OpsTimeBar";
import { toLocationHref, useOpsRole } from "../shared/opsHelpers";

function severityStatusKey(severity: string): string {
  if (severity === "critical") return "ERROR";
  if (severity === "warning") return "WARNING";
  return "DRAFT";
}

function eventStatusKey(status: string): string {
  if (status === "firing") return "ERROR";
  if (status === "resolved") return "ACTIVE";
  return "DRAFT";
}

export function AlertEventsPanel() {
  const { t, locale } = useT();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { isAdmin, isBiz, isViewer } = useOpsRole();

  const [keyword, setKeyword] = useState("");
  const [listFilters, setListFilters] = useState<ListFilter[]>([]);
  const [refreshInterval, setRefreshInterval] = useState(30);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [rows, setRows] = useState<AlertEvent[]>([]);

  const [selected, setSelected] = useState<AlertEvent | null>(null);
  const [relatedChanges, setRelatedChanges] = useState<ChangeEvent[]>([]);
  const [changesLoading, setChangesLoading] = useState(false);

  const [silenceTarget, setSilenceTarget] = useState<AlertEvent | null>(null);
  const [silenceMatchers, setSilenceMatchers] = useState("");
  const [silenceWindow, setSilenceWindow] = useState<DateRangeValue>({
    start: "",
    end: "",
  });
  const [silenceComment, setSilenceComment] = useState("");
  const [silenceSaving, setSilenceSaving] = useState(false);

  const eventIdFilter = searchParams.get("event_id")?.trim() ?? "";
  const seqRef = useRef(0);

  const statusFilter = useMemo<AlertStatus | "">(() => {
    const f = listFilters.find((item) => item.field === "status" && item.op === "eq");
    return f && (f.value === "firing" || f.value === "resolved") ? f.value : "";
  }, [listFilters]);

  const severityFilter = useMemo<AlertSeverity | "">(() => {
    const f = listFilters.find((item) => item.field === "severity" && item.op === "eq");
    return f && (f.value === "critical" || f.value === "warning" || f.value === "info")
      ? f.value
      : "";
  }, [listFilters]);

  const ruleNameFilters = useMemo(
    () => listFilters.filter((item) => item.field === "rule_name"),
    [listFilters],
  );

  const toggleEnumFilter = (field: "status" | "severity", val: string) => {
    setListFilters((prev) => {
      const exists = prev.some((f) => f.field === field && f.value === val);
      const rest = prev.filter((f) => f.field !== field);
      return exists ? rest : [...rest, { field, op: "eq", value: val }];
    });
    setPage(1);
  };

  const load = useCallback(async () => {
    const mine = ++seqRef.current;
    setLoading(true);
    try {
      const res = await fetchAlertEvents({
        status: statusFilter || undefined,
        severity: severityFilter || undefined,
        event_id: eventIdFilter || undefined,
        page,
        page_size: pageSize,
      });
      if (mine !== seqRef.current) return;
      setRows(res.data ?? []);
      setTotal(res.total ?? 0);
    } catch {
      // http layer handles errors
    } finally {
      if (mine === seqRef.current) setLoading(false);
    }
  }, [statusFilter, severityFilter, eventIdFilter, page, pageSize]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!refreshInterval || refreshInterval <= 0) return;
    const timer = window.setInterval(() => {
      if (typeof document !== "undefined" && document.hidden) return;
      void load();
    }, refreshInterval * 1000);
    return () => window.clearInterval(timer);
  }, [refreshInterval, load]);

  const filteredRows = useMemo(() => {
    const q = keyword.trim().toLowerCase();
    return rows.filter((row) => {
      for (const rf of ruleNameFilters) {
        const needle = String(rf.value ?? "").toLowerCase();
        if (rf.op === "eq" && row.rule_name.toLowerCase() !== needle) return false;
        if (!row.rule_name.toLowerCase().includes(needle)) return false;
      }
      if (!q) return true;
      const labelsText = formatAlertLabels(row.labels, "").toLowerCase();
      return (
        row.rule_name.toLowerCase().includes(q) ||
        labelsText.includes(q) ||
        row.fingerprint.toLowerCase().includes(q)
      );
    });
  }, [rows, keyword, ruleNameFilters]);

  const openDetailDrawer = (row: AlertEvent) => {
    setSelected(row);
    setRelatedChanges([]);
    if (!isBiz) return;
    const started = Date.parse(row.started_at);
    if (!Number.isFinite(started)) return;
    setChangesLoading(true);
    fetchOpsChanges({
      start: Math.floor(started / 1000) - 300,
      end: Math.floor(started / 1000) + 300,
      limit: 20,
    })
      .then((res) => setRelatedChanges(res.events ?? []))
      .catch(() => setRelatedChanges([]))
      .finally(() => setChangesLoading(false));
  };

  const openQuickSilence = (row: AlertEvent) => {
    const now = Date.now();
    const end = now + 30 * 60 * 1000;
    const matchers = Object.fromEntries(
      Object.entries(row.labels ?? {}).filter(
        ([k]) => k !== "unit" && k !== "category",
      ),
    );
    setSilenceMatchers(
      JSON.stringify(
        Object.keys(matchers).length > 0 ? matchers : row.labels ?? {},
      ),
    );
    setSilenceWindow({
      start: formatDateTime(now),
      end: formatDateTime(end),
    });
    setSilenceComment(row.rule_name);
    setSilenceTarget(row);
  };

  const submitQuickSilence = async () => {
    setSilenceSaving(true);
    try {
      const parsed = JSON.parse(silenceMatchers || "{}") as unknown;
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new Error(t("ops.alerts.rules.form.labelsInvalid"));
      }
      const startMs = parseDisplayWallClockMs(silenceWindow.start);
      const endMs = parseDisplayWallClockMs(silenceWindow.end);
      if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || endMs <= startMs) {
        throw new Error(t("ops.alerts.silences.window"));
      }
      await createAlertSilence({
        matchers: parsed as Record<string, string>,
        starts_at: new Date(startMs).toISOString(),
        ends_at: new Date(endMs).toISOString(),
        comment: silenceComment.trim(),
      });
      setSilenceTarget(null);
      showSuccessToast(t("common.success"));
      void load();
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
      showErrorToast(
        error instanceof Error ? error.message : t("common.failed"),
      );
    } finally {
      setSilenceSaving(false);
    }
  };

  const drillRouteOf = useCallback(
    (row: AlertEvent) =>
      resolveAlertDrillRoute(
        {
          labels: row.labels,
          startedAt: row.started_at,
          resolvedAt: row.resolved_at,
          forSeconds: row.for_seconds,
        },
        { isAdmin, isBiz, isViewer },
      ),
    [isAdmin, isBiz, isViewer],
  );

  const statusText = useCallback(
    (status: string) => {
      if (status === "firing") return t("ops.alerts.status.firing");
      if (status === "resolved") return t("ops.alerts.status.resolved");
      return getStatusLabel(status);
    },
    [t],
  );

  const severityText = useCallback(
    (sev: string) => {
      if (sev === "critical") return t("ops.alerts.severity.critical");
      if (sev === "warning") return t("ops.alerts.severity.warning");
      if (sev === "info") return t("ops.alerts.severity.info");
      return getSeverityLabel(sev);
    },
    [t],
  );

  const columns = useMemo<DataTableColumn<AlertEvent>[]>(
    () => [
      {
        id: "severity",
        header: t("ops.alerts.columns.severity"),
        width: 84,
        cell: (row) => (
          <StatusBadge
            status={severityStatusKey(row.severity)}
            label={severityText(row.severity)}
          />
        ),
      },
      {
        id: "status",
        header: t("ops.alerts.columns.status"),
        minWidth: 100,
        cell: (row) => (
          <div className="flex flex-wrap items-center gap-1">
            <StatusBadge
              status={eventStatusKey(row.status)}
              label={statusText(row.status)}
            />
            {row.silenced && (
              <Badge variant="neutral">{t("ops.alerts.silenced")}</Badge>
            )}
            {row.inhibited && (
              <Badge variant="warning">{t("ops.alerts.inhibited")}</Badge>
            )}
          </div>
        ),
      },
      {
        id: "rule_name",
        header: t("ops.alerts.columns.rule"),
        minWidth: 128,
        cell: (row) => (
          <span className="font-medium text-foreground">{row.rule_name}</span>
        ),
      },
      {
        id: "labels",
        header: t("ops.alerts.columns.labels"),
        minWidth: 150,
        cell: (row) => (
          <span
            className="block max-w-[180px] truncate font-mono-tech text-muted-foreground"
            title={formatAlertLabels(row.labels)}
          >
            {formatAlertLabels(row.labels)}
          </span>
        ),
      },
      {
        id: "trace_ids",
        header: t("ops.traces.traceIds"),
        minWidth: 128,
        cell: (row) => {
          if (!row.trace_ids?.length) return "—";
          return (
            <div className="flex flex-col gap-1">
              {row.trace_ids.map((tid) => {
                const expired = isTraceExpired(row.started_at);
                const canDrill = canDrillToTrace(row.started_at, isAdmin);
                if (isAdmin && expired) {
                  return (
                    <Tooltip key={tid} content={t("ops.traces.expiredTooltip")}>
                      <span className="text-muted-foreground opacity-70">
                        <CopyableText
                          value={tid}
                          mono
                          truncate
                          maxWidthClass="max-w-[100px]"
                        />
                      </span>
                    </Tooltip>
                  );
                }
                if (canDrill) {
                  const win = computeAlertDrillWindow(
                    row.started_at,
                    row.resolved_at,
                    row.for_seconds,
                  );
                  return (
                    <div key={tid} className="flex items-center gap-1">
                      <Button
                        type="button"
                        variant="link"
                        size="xs"
                        className="px-1 font-mono-tech"
                        onClick={(e) => {
                          e.stopPropagation();
                          void navigate(
                            toLocationHref(
                              buildTraceRoute(tid, {
                                start: win?.start,
                                end: win?.end,
                              }),
                            ),
                          );
                        }}
                      >
                        {tid.slice(0, 12)}
                      </Button>
                      <CopyableText value={tid} displayText="" />
                    </div>
                  );
                }
                return (
                  <CopyableText
                    key={tid}
                    value={tid}
                    mono
                    truncate
                    maxWidthClass="max-w-[100px]"
                  />
                );
              })}
            </div>
          );
        },
      },
      {
        id: "value",
        header: t("ops.alerts.columns.value"),
        minWidth: 96,
        align: "right",
        mono: true,
        className: "whitespace-nowrap",
        cell: (row) =>
          `${formatAlertValue(row.value)} ${row.op} ${formatAlertValue(row.threshold)}`,
      },
      {
        id: "started_at",
        header: t("ops.alerts.columns.startedAt"),
        width: 152,
        mono: true,
        className: "whitespace-nowrap",
        cell: (row) => formatAlertTime(row.started_at),
      },
      {
        id: "resolved_at",
        header: t("ops.alerts.columns.resolvedAt"),
        width: 152,
        mono: true,
        className: "whitespace-nowrap",
        cell: (row) => (row.resolved_at ? formatAlertTime(row.resolved_at) : "—"),
      },
      {
        id: "actions",
        header: t("ops.alerts.columns.actions"),
        width: 140,
        align: "right",
        className: "whitespace-nowrap",
        cell: (row) => {
          const drillTarget = drillRouteOf(row);
          return (
            <div
              className="flex items-center justify-end gap-1"
              onClick={(e) => e.stopPropagation()}
            >
              {drillTarget && (
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  onClick={() => void navigate(toLocationHref(drillTarget))}
                >
                  {t("ops.alerts.drillDown")}
                </Button>
              )}
              {isAdmin && (
                <Tooltip content={t("ops.alerts.silences.create")}>
                  <IconButton
                    size="sm"
                    onClick={() => openQuickSilence(row)}
                    aria-label={t("ops.alerts.silences.create")}
                  >
                    <BellOff className="h-3.5 w-3.5" />
                  </IconButton>
                </Tooltip>
              )}
              <Button
                type="button"
                variant="link"
                size="xs"
                onClick={() => openDetailDrawer(row)}
              >
                {t("common.actions.details")}
              </Button>
            </div>
          );
        },
      },
    ],
    [t, isAdmin, severityText, statusText, drillRouteOf, navigate],
  );

  const clearEventIdFilter = () => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.delete("event_id");
        return next;
      },
      { replace: true },
    );
    setPage(1);
  };

  return (
    <div className="flex flex-col flex-1 min-h-0 gap-4 min-w-0">
      <DataTablePage
        columns={columns}
        data={filteredRows}
        getRowId={(r) => r.id}
        loading={loading}
        emptyText={t("ops.alerts.empty")}
        onRowClick={openDetailDrawer}
        page={page}
        pageSize={pageSize}
        total={total}
        pageSizeOptions={[20, 50, 100]}
        onPageChange={(nextPage) => setPage(nextPage)}
        onPageSizeChange={(nextSize) => {
          setPageSize(nextSize);
          setPage(1);
        }}
        filters={
          <div className="flex flex-wrap items-center gap-2">
            <ListQueryBar
              q={keyword}
              onQChange={setKeyword}
              filters={listFilters}
              onFiltersChange={(next) => {
                setListFilters(next);
                setPage(1);
              }}
              catalog={alertEventsCatalog}
              onSearch={() => setPage(1)}
            />

            <ActionChip
              size="sm"
              active={statusFilter === "firing"}
              onClick={() => toggleEnumFilter("status", "firing")}
            >
              {t("ops.alerts.status.firing")}
            </ActionChip>

            <ActionChip
              size="sm"
              active={severityFilter === "critical"}
              onClick={() => toggleEnumFilter("severity", "critical")}
            >
              {t("ops.alerts.severity.critical")}
            </ActionChip>

            {eventIdFilter && (
              <span className="inline-flex h-8 items-center gap-1.5 rounded-md border border-primary/35 bg-primary-soft px-2.5 text-xs text-primary font-mono-tech">
                <span>
                  {t("ops.alerts.filters.event")} {eventIdFilter.slice(0, 8)}
                </span>
                <button
                  type="button"
                  onClick={clearEventIdFilter}
                  aria-label={t("common.reset")}
                  className="inline-flex h-4 w-4 items-center justify-center rounded-md hover:bg-primary-soft-hover cursor-pointer"
                >
                  <X className="h-3 w-3" />
                </button>
              </span>
            )}
          </div>
        }
        actions={
          <AutoRefreshControl
            refreshInterval={refreshInterval}
            onRefreshIntervalChange={setRefreshInterval}
            onRefresh={() => void load()}
            loading={loading}
          />
        }
      />

      {/* Event detail drawer */}
      <Drawer
        open={Boolean(selected)}
        onClose={() => setSelected(null)}
        title={selected?.rule_name ?? ""}
        subtitle={selected ? formatAlertTime(selected.started_at) : undefined}
        size="md"
        footer={
          selected ? (
            <>
              {isAdmin && (
                <Button
                  type="button"
                  variant="outline"
                  size="md"
                  onClick={() => {
                    const target = selected;
                    setSelected(null);
                    openQuickSilence(target);
                  }}
                >
                  <BellOff className="h-3.5 w-3.5" />
                  <span>{t("ops.alerts.silences.create")}</span>
                </Button>
              )}
              {drillRouteOf(selected) && (
                <Button
                  type="button"
                  variant="primary"
                  size="md"
                  onClick={() => {
                    const target = drillRouteOf(selected);
                    if (target) void navigate(toLocationHref(target));
                  }}
                >
                  {t("ops.alerts.drillDown")}
                </Button>
              )}
            </>
          ) : undefined
        }
      >
        {selected && (
          <div className="space-y-4">
            <dl className="divide-y divide-border-subtle rounded-lg border border-border overflow-hidden text-xs">
              <div className="grid grid-cols-12 gap-2 px-3 py-2">
                <dt className="col-span-4 text-muted-foreground">
                  {t("ops.alerts.columns.severity")}
                </dt>
                <dd className="col-span-8">
                  <StatusBadge
                    status={severityStatusKey(selected.severity)}
                    label={severityText(selected.severity)}
                  />
                </dd>
              </div>
              <div className="grid grid-cols-12 gap-2 px-3 py-2">
                <dt className="col-span-4 text-muted-foreground">
                  {t("ops.alerts.columns.status")}
                </dt>
                <dd className="col-span-8 flex flex-wrap gap-1">
                  <StatusBadge
                    status={eventStatusKey(selected.status)}
                    label={statusText(selected.status)}
                  />
                  {selected.silenced && (
                    <Badge variant="neutral">{t("ops.alerts.silenced")}</Badge>
                  )}
                  {selected.inhibited && (
                    <Badge variant="warning">{t("ops.alerts.inhibited")}</Badge>
                  )}
                </dd>
              </div>
              <div className="grid grid-cols-12 gap-2 px-3 py-2">
                <dt className="col-span-4 text-muted-foreground">
                  {t("ops.alerts.columns.value")}
                </dt>
                <dd className="col-span-8 font-mono-tech tabular-nums">
                  {formatAlertValue(selected.value)} {selected.op}{" "}
                  {formatAlertValue(selected.threshold)}
                </dd>
              </div>
              <div className="grid grid-cols-12 gap-2 px-3 py-2">
                <dt className="col-span-4 text-muted-foreground">
                  {t("ops.alerts.columns.startedAt")}
                </dt>
                <dd className="col-span-8 font-mono-tech tabular-nums">
                  {formatAlertTime(selected.started_at)}
                </dd>
              </div>
              <div className="grid grid-cols-12 gap-2 px-3 py-2">
                <dt className="col-span-4 text-muted-foreground">
                  {t("ops.alerts.columns.resolvedAt")}
                </dt>
                <dd className="col-span-8 font-mono-tech tabular-nums">
                  {selected.resolved_at
                    ? formatAlertTime(selected.resolved_at)
                    : "—"}
                </dd>
              </div>
              <div className="grid grid-cols-12 gap-2 px-3 py-2">
                <dt className="col-span-4 text-muted-foreground">
                  {t("ops.alerts.columns.labels")}
                </dt>
                <dd className="col-span-8 font-mono-tech break-all">
                  {formatAlertLabels(selected.labels)}
                </dd>
              </div>
            </dl>

            {isBiz && (
              <div className="space-y-2">
                <h4 className="text-xs font-semibold text-foreground">
                  {t("ops.alerts.relatedChanges")}
                </h4>
                {changesLoading ? (
                  <div className="text-xs text-muted-foreground">
                    {t("layout.loading")}
                  </div>
                ) : relatedChanges.length === 0 ? (
                  <EmptyState title={t("ops.alerts.noRelatedChanges")} />
                ) : (
                  <ol className="space-y-2 border-l border-border pl-3">
                    {relatedChanges.map((c, idx) => (
                      <li key={idx} className="space-y-0.5 text-xs">
                        <div className="font-mono-tech text-[11px] text-muted-foreground">
                          {formatAlertTime(c.ts)}
                        </div>
                        <div className="text-foreground">
                          <span className="font-medium">{c.kind}</span> ·{" "}
                          {c.title}
                          {c.actor ? (
                            <span className="ml-1.5 text-muted-foreground">
                              ({c.actor})
                            </span>
                          ) : null}
                        </div>
                      </li>
                    ))}
                  </ol>
                )}
              </div>
            )}
          </div>
        )}
      </Drawer>

      {/* Quick Silence Modal */}
      <FormModal
        open={Boolean(silenceTarget)}
        onClose={() => setSilenceTarget(null)}
        onSubmit={submitQuickSilence}
        title={t("ops.alerts.silences.create")}
        size="md"
        submitting={silenceSaving}
      >
        <FormField label={t("ops.alerts.silences.matchers")} required>
          <Textarea
            rows={3}
            value={silenceMatchers}
            onChange={(e) => setSilenceMatchers(e.target.value)}
            className="font-mono-tech"
            placeholder='{"product_id":"A"}'
          />
        </FormField>
        <FormField label={t("ops.alerts.silences.window")} required>
          <DateRangePicker
            value={silenceWindow}
            onChange={setSilenceWindow}
            showPresets={false}
            className="w-full justify-between"
          />
        </FormField>
        <FormField label={t("ops.alerts.silences.comment")}>
          <Input
            value={silenceComment}
            onChange={(e) => setSilenceComment(e.target.value)}
            placeholder={
              locale === LanguageEnum.ZH ? "临时静默原因" : "Silence note"
            }
          />
        </FormField>
      </FormModal>
    </div>
  );
}
