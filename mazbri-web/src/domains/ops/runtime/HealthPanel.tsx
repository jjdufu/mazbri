import React, { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { ExternalLink, Loader2 } from "lucide-react";
import {
  Badge,
  Button,
  Card,
  DataTablePage,
  cn,
  type DataTableColumn,
} from "@/components/ui";
import { fetchServicesHealth } from "@/api/monitoring";
import { useT } from "@/locales";
import type { ServiceHealthRow } from "@/types/api/monitoring";
import { formatBytes, formatDateTime } from "@/utils/format";
import { formatShortVersion } from "@/utils/ops/components-health";
import { buildInfraRoute } from "@/utils/ops/infra-link";
import { buildPipelineRoute } from "@/utils/ops/pipeline-link";
import {
  filterRowsByGroup,
  formatCpuCoresDisplay,
  serviceHasPostgresPool,
  type HealthPanelMode,
} from "@/utils/ops/service-topology";
import { buildServiceDetailRoute } from "@/utils/ops/services-link";
import type { TimeRangeValue } from "@/utils/ops/time-range";
import { buildTraceSearchRoute } from "@/utils/ops/trace-link";
import { showErrorToast } from "@/utils/ui/message-guard";
import {
  ServiceHexagonGrid,
  type HexagonNode,
  type HexagonStatus,
} from "../shared/ServiceHexagonGrid";
import { toLocationHref, useOpsRole } from "../shared/opsHelpers";

export interface HealthPanelProps {
  mode?: HealthPanelMode;
  timeRange: TimeRangeValue;
  reloadTick: number;
}

function healthStatusOf(row: ServiceHealthRow): HexagonStatus {
  if (row.status) {
    switch (row.status) {
      case "green":
        return "healthy";
      case "yellow":
        return "degraded";
      case "red":
        return "down";
      default:
        return "unknown";
    }
  }
  if (!row.health_ok) return "down";
  if (row.ready === false) return "degraded";
  return "healthy";
}

function statusBadgeVariant(
  row: ServiceHealthRow,
): "success" | "warning" | "danger" | "neutral" {
  const s = healthStatusOf(row);
  if (s === "healthy") return "success";
  if (s === "degraded") return "warning";
  if (s === "down") return "danger";
  return "neutral";
}

export function HealthPanel({
  mode = "apps",
  timeRange,
  reloadTick,
}: HealthPanelProps) {
  const { t } = useT();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const { isAdmin } = useOpsRole();

  const [loading, setLoading] = useState(false);
  const [rows, setRows] = useState<ServiceHealthRow[]>([]);
  const [selectedService, setSelectedService] = useState<string | null>(
    () => searchParams.get("service")?.trim() || null,
  );

  const queryService = searchParams.get("service")?.trim() || null;
  useEffect(() => {
    if (queryService) setSelectedService(queryService);
  }, [queryService]);

  const filteredRows = useMemo(
    () => filterRowsByGroup(rows, mode),
    [rows, mode],
  );

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const list = await fetchServicesHealth();
      setRows(list ?? []);
    } catch (error) {
      showErrorToast(
        error instanceof Error ? error.message : t("ops.services.loadFailed"),
      );
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    void load();
  }, [load, reloadTick]);

  useEffect(() => {
    if (
      selectedService &&
      filteredRows.length > 0 &&
      !filteredRows.some((r) => r.service === selectedService)
    ) {
      setSelectedService(null);
    }
  }, [filteredRows, selectedService]);

  const statusText = useCallback(
    (row: ServiceHealthRow) => {
      const statusKey = row.status ?? (row.health_ok ? "green" : "red");
      return t(`ops.components.statusName.${statusKey}`);
    },
    [t],
  );

  const reasonsText = useCallback(
    (row: ServiceHealthRow) => {
      if (!row.reasons || row.reasons.length === 0) return "—";
      return row.reasons
        .map((r) => t(`ops.components.reason.${r}`))
        .join("、");
    },
    [t],
  );

  const resolveBadge = useCallback(
    (row: ServiceHealthRow) => {
      if (row.reasons && row.reasons.length > 0) {
        return t(`ops.components.reason.${row.reasons[0]}`);
      }
      const shortVer = formatShortVersion(row.version);
      if (shortVer) return shortVer;
      return t(`ops.components.group.${row.group ?? "app"}`);
    },
    [t],
  );

  const buildTooltipLines = useCallback(
    (row: ServiceHealthRow): string[] => {
      const lines: string[] = [
        `${row.name || row.service} (${row.service})`,
        `${t("ops.components.status")}：${statusText(row)}`,
      ];
      if (row.reasons && row.reasons.length > 0) {
        lines.push(reasonsText(row));
      }
      if (row.version) {
        lines.push(`${t("ops.components.version")}：v${row.version}`);
      }
      if (row.restarts_1h !== null && row.restarts_1h !== undefined) {
        lines.push(
          `${t("ops.components.restarts1h")}：${Math.round(row.restarts_1h)}`,
        );
      }
      if (row.cpu_cores !== null && row.cpu_cores !== undefined) {
        lines.push(
          `CPU：${formatCpuCoresDisplay(row.cpu_cores, t("ops.components.cores"))}`,
        );
      }
      if (row.memory_bytes !== null && row.memory_bytes !== undefined) {
        lines.push(
          `${t("ops.components.memory")}：${formatBytes(row.memory_bytes)}`,
        );
      }
      return lines;
    },
    [t, statusText, reasonsText],
  );

  const abnormalCount = useMemo(
    () =>
      filteredRows.filter((r) =>
        r.status ? r.status !== "green" : !r.health_ok || r.ready === false,
      ).length,
    [filteredRows],
  );

  const hexagonNodes = useMemo<HexagonNode[]>(
    () =>
      filteredRows.map((row) => ({
        id: row.service,
        label: row.name || row.service,
        status: healthStatusOf(row),
        badge: resolveBadge(row),
        tooltipLines: buildTooltipLines(row),
      })),
    [filteredRows, resolveBadge, buildTooltipLines],
  );

  const selectedRow = useMemo(
    () => filteredRows.find((r) => r.service === selectedService) ?? null,
    [filteredRows, selectedService],
  );

  const isSelectedApp = (selectedRow?.group ?? "app") === "app";

  const openDetail = useCallback(
    (row: ServiceHealthRow) => {
      if (!isAdmin) return;
      void navigate(toLocationHref(buildServiceDetailRoute(row.service)));
    },
    [isAdmin, navigate],
  );

  const openTraces = useCallback(
    (row: ServiceHealthRow) => {
      if (!isAdmin) return;
      void navigate(
        toLocationHref(
          buildTraceSearchRoute({
            start: timeRange.start,
            end: timeRange.end,
            filters: { service: row.service },
          }),
        ),
      );
    },
    [isAdmin, navigate, timeRange.start, timeRange.end],
  );

  const boolText = (val: boolean | null | undefined) => {
    if (val === null || val === undefined) return "—";
    return val ? t("ops.services.ok") : t("ops.services.down");
  };

  const containerText = (row: ServiceHealthRow) => {
    if (row.container_running === true) return t("ops.services.containerRunning");
    if (row.container_running === false) return t("ops.services.containerStopped");
    if (row.group === "middleware" && row.health_ok) {
      return t("ops.services.containerExternal");
    }
    return "—";
  };

  const overviewColumns = useMemo<DataTableColumn<ServiceHealthRow>[]>(
    () => [
      {
        id: "name",
        header: t("ops.services.columns.service"),
        minWidth: 200,
        cell: (row) => {
          const text = `${row.name || row.service} (${row.service})`;
          if (!isAdmin || (row.group ?? "app") !== "app") {
            return <span className="font-medium text-foreground">{text}</span>;
          }
          return (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                openDetail(row);
              }}
              className="inline-flex items-center gap-1 text-left font-medium text-primary hover:underline cursor-pointer"
            >
              <span>{text}</span>
              <ExternalLink className="h-3 w-3 shrink-0" />
            </button>
          );
        },
      },
      {
        id: "status",
        header: t("ops.services.columns.status"),
        width: 96,
        cell: (row) => (
          <Badge variant={statusBadgeVariant(row)}>{statusText(row)}</Badge>
        ),
      },
      {
        id: "version",
        header: t("ops.services.columns.version"),
        minWidth: 140,
        mono: true,
        cell: (row) => row.version || "—",
      },
      {
        id: "cpu_cores",
        header: t("ops.services.columns.cpu"),
        width: 116,
        align: "right",
        mono: true,
        cell: (row) =>
          formatCpuCoresDisplay(row.cpu_cores, t("ops.components.cores")),
      },
      {
        id: "memory_bytes",
        header: t("ops.services.columns.memory"),
        width: 116,
        align: "right",
        mono: true,
        cell: (row) =>
          row.memory_bytes !== null && row.memory_bytes !== undefined
            ? formatBytes(row.memory_bytes)
            : "—",
      },
      {
        id: "restarts_1h",
        header: t("ops.services.columns.restarts1h"),
        width: 116,
        align: "right",
        mono: true,
        cell: (row) =>
          row.restarts_1h !== null && row.restarts_1h !== undefined
            ? String(Math.round(row.restarts_1h))
            : "—",
      },
      {
        id: "reasons",
        header: t("ops.services.columns.reasons"),
        minWidth: 160,
        cell: (row) => (
          <span className="text-muted-foreground">{reasonsText(row)}</span>
        ),
      },
    ],
    [t, isAdmin, openDetail, statusText, reasonsText],
  );

  const cardTitle =
    mode === "apps"
      ? t("ops.services.appHexagonTitle")
      : t("ops.services.infraHexagonTitle");

  return (
    <div
      className={cn(
        "min-w-0",
        mode === "apps"
          ? "flex flex-col flex-1 min-h-0 gap-4"
          : "space-y-4 shrink-0",
      )}
    >
      <Card className="overflow-hidden shrink-0">
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border px-4 py-3">
          <h3 className="text-xs font-semibold text-foreground">{cardTitle}</h3>
          {filteredRows.length > 0 && (
            <span
              className={cn(
                "text-xs font-semibold font-mono-tech tabular-nums",
                abnormalCount > 0 ? "text-danger-fg" : "text-success-fg",
              )}
            >
              {abnormalCount > 0
                ? t("ops.components.abnormalOf", {
                    n: abnormalCount,
                    total: filteredRows.length,
                  })
                : t("ops.components.allHealthy", {
                    total: filteredRows.length,
                  })}
            </span>
          )}
        </div>
        <div className="relative p-4">
          {loading && filteredRows.length === 0 ? (
            <div className="flex items-center justify-center gap-2 py-8 text-xs text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin text-primary" />
              <span>{t("layout.loading")}</span>
            </div>
          ) : (
            <ServiceHexagonGrid
              nodes={hexagonNodes}
              selectedId={selectedService}
              onSelect={setSelectedService}
              emptyText={t("ops.services.empty")}
            />
          )}
        </div>
      </Card>

      {selectedRow && (
        <Card className="overflow-hidden">
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border px-4 py-3">
            <h3 className="text-xs font-semibold text-foreground">
              {t("ops.services.detailTitle", {
                name: selectedRow.name || selectedRow.service,
              })}
            </h3>
            <div className="flex flex-wrap items-center gap-2">
              {isSelectedApp && isAdmin && (
                <>
                  <Button
                    type="button"
                    variant="link"
                    size="xs"
                    onClick={() => openDetail(selectedRow)}
                  >
                    {t("ops.services.openDetail")}
                  </Button>
                  <Button
                    type="button"
                    variant="link"
                    size="xs"
                    onClick={() => openTraces(selectedRow)}
                  >
                    {t("ops.services.openTraces")}
                  </Button>
                </>
              )}
              {!isSelectedApp &&
                selectedRow.group === "observability" &&
                isAdmin && (
                  <Button
                    type="button"
                    variant="link"
                    size="xs"
                    onClick={() =>
                      void navigate(
                        toLocationHref(
                          selectedRow.service === "vector"
                            ? buildPipelineRoute({ tab: "storage" })
                            : buildInfraRoute({ tab: "hosts" }),
                        ),
                      )
                    }
                  >
                    {selectedRow.service === "vector"
                      ? t("ops.services.openPipeline")
                      : t("ops.services.openHosts")}
                  </Button>
                )}
              <Button
                type="button"
                variant="ghost"
                size="xs"
                onClick={() => setSelectedService(null)}
              >
                {t("ops.services.clearSelection")}
              </Button>
            </div>
          </div>

          <dl className="grid grid-cols-1 divide-y divide-border-subtle text-xs sm:grid-cols-2 lg:grid-cols-3">
            <DescItem label={t("ops.services.columns.service")}>
              {selectedRow.name || selectedRow.service}
            </DescItem>
            <DescItem label={t("ops.services.columns.key")}>
              <span className="font-mono-tech">{selectedRow.service}</span>
            </DescItem>
            <DescItem label={t("ops.services.columns.group")}>
              {t(`ops.components.group.${selectedRow.group ?? "app"}`)}
            </DescItem>
            <DescItem label={t("ops.services.columns.status")}>
              <Badge variant={statusBadgeVariant(selectedRow)}>
                {statusText(selectedRow)}
              </Badge>
            </DescItem>
            <DescItem label={t("ops.services.columns.health")}>
              <Badge variant={selectedRow.health_ok ? "success" : "danger"}>
                {boolText(selectedRow.health_ok)}
              </Badge>
            </DescItem>
            <DescItem label={t("ops.services.columns.ready")}>
              {boolText(selectedRow.ready)}
            </DescItem>
            <DescItem label={t("ops.services.columns.configCenter")}>
              {boolText(selectedRow.config_center)}
            </DescItem>
            <DescItem label={t("ops.services.columns.container")}>
              {containerText(selectedRow)}
            </DescItem>
            <DescItem label={t("ops.services.columns.dbPool")}>
              {(selectedRow.group ?? "app") !== "app" ||
              !serviceHasPostgresPool(selectedRow.service) ? (
                <span className="text-muted-foreground">
                  {t("ops.services.notApplicable")}
                </span>
              ) : selectedRow.db_pool_usage === null ||
                selectedRow.db_pool_usage === undefined ? (
                <Badge variant="warning">
                  {t("ops.services.unreported")} (—)
                </Badge>
              ) : (
                <span className="font-mono-tech tabular-nums">
                  {(selectedRow.db_pool_usage * 100).toFixed(1)}%
                </span>
              )}
            </DescItem>
            <DescItem label={t("ops.services.columns.cpu")}>
              <span className="font-mono-tech tabular-nums">
                {formatCpuCoresDisplay(
                  selectedRow.cpu_cores,
                  t("ops.components.cores"),
                )}
              </span>
            </DescItem>
            <DescItem label={t("ops.services.columns.memory")}>
              <span className="font-mono-tech tabular-nums">
                {selectedRow.memory_bytes !== null &&
                selectedRow.memory_bytes !== undefined
                  ? formatBytes(selectedRow.memory_bytes)
                  : "—"}
              </span>
            </DescItem>
            <DescItem label={t("ops.services.columns.fds")}>
              <span className="font-mono-tech tabular-nums">
                {selectedRow.open_fds !== null &&
                selectedRow.open_fds !== undefined
                  ? selectedRow.open_fds.toFixed(0)
                  : "—"}
              </span>
            </DescItem>
            <DescItem label={t("ops.services.columns.tokio")}>
              <span className="font-mono-tech tabular-nums">
                {selectedRow.tokio_busy !== null &&
                selectedRow.tokio_busy !== undefined
                  ? `${(selectedRow.tokio_busy * 100).toFixed(1)}%`
                  : "—"}
              </span>
            </DescItem>
            <DescItem label={t("ops.services.columns.panic")}>
              <span className="font-mono-tech tabular-nums">
                {selectedRow.panic_count !== null &&
                selectedRow.panic_count !== undefined
                  ? selectedRow.panic_count.toFixed(0)
                  : "—"}
              </span>
            </DescItem>
            <DescItem label={t("ops.services.columns.restarts1h")}>
              <span className="font-mono-tech tabular-nums">
                {selectedRow.restarts_1h !== null &&
                selectedRow.restarts_1h !== undefined
                  ? selectedRow.restarts_1h.toFixed(0)
                  : "—"}
              </span>
            </DescItem>
            <DescItem label={t("ops.services.columns.restart")}>
              <span className="font-mono-tech tabular-nums">
                {selectedRow.last_restart_ts
                  ? formatDateTime(selectedRow.last_restart_ts)
                  : "—"}
              </span>
            </DescItem>
            <DescItem label={t("ops.services.columns.version")}>
              <span className="font-mono-tech">{selectedRow.version || "—"}</span>
            </DescItem>
            <DescItem label={t("ops.services.columns.reasons")}>
              {reasonsText(selectedRow)}
            </DescItem>
          </dl>
        </Card>
      )}

      {mode === "apps" && filteredRows.length > 0 && (
        <Card className="flex flex-col flex-1 min-h-0 overflow-hidden">
          <div className="border-b border-border px-4 py-3 shrink-0">
            <h3 className="text-xs font-semibold text-foreground">
              {t("ops.services.overviewTableTitle")}
            </h3>
          </div>
          <DataTablePage
            columns={overviewColumns}
            data={filteredRows}
            getRowId={(r) => r.service}
            loading={loading}
            onRowClick={(row) =>
              setSelectedService((prev) =>
                prev === row.service ? null : row.service,
              )
            }
            className="[&>div]:border-0 [&>div]:rounded-none"
          />
        </Card>
      )}
    </div>
  );
}

function DescItem({
  label,
  children,
}: {
  label: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-2 px-4 py-2 border-r border-border-subtle last:border-r-0">
      <dt className="text-muted-foreground shrink-0">{label}</dt>
      <dd className="min-w-0 truncate text-right text-foreground">{children}</dd>
    </div>
  );
}
