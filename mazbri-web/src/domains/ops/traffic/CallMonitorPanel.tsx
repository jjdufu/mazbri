import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";
import type { EChartsCoreOption } from "echarts";
import {
  ChevronDown,
  ChevronRight,
  Download,
  Loader2,
  RotateCcw,
} from "lucide-react";
import {
  ChartCard,
  EChartBox,
  OPS_CHART_GRID,
  formatChartAxisTime,
  type ChartThemeTokens,
} from "@/components/charts";
import {
  ActionChip,
  Badge,
  Button,
  Card,
  CopyableText,
  Drawer,
  EmptyState,
  Pagination,
  Popover,
  Tooltip,
  cn,
} from "@/components/ui";
import {
  fetchCallHistogram,
  fetchCustomerCallLogs,
  fetchUpstreamCallLogs,
} from "@/api/call-monitor";
import {
  CALL_MONITOR_UNIVERSE_SIZE,
  callMonitorCatalogFor,
} from "@/list-catalog";
import { useT } from "@/locales";
import type {
  CallHistogramQuery,
  CallHistogramResponse,
  CallMonitorItem,
  CallMonitorMode,
  CallMonitorQuery,
  CallMonitorResponse,
} from "@/types/api/call-monitor";
import type { ListFilter, QuickFilterDef } from "@/types/list-query";
import { downloadBinary, packageTarArchive } from "@/utils/archive";
import {
  EMPTY_PLACEHOLDER,
  formatDateTime,
  formatIntervalHuman,
} from "@/utils/format";
import {
  parseRequestIdLookup,
  requestIdFromFilters,
} from "@/utils/list-query/request-id";
import { buildListQueryParams } from "@/utils/list-query/serialize";
import { resolveCallChannel } from "@/utils/ops/call-channel";
import { timestampFiltersToUnix } from "@/utils/ops/call-filters";
import { CALLS_FILTER_KEYS } from "@/utils/ops/calls-link";
import { sameRange, type TimeRangeValue } from "@/utils/ops/time-range";
import {
  buildTraceRoute,
  canDrillToTrace,
  isTraceExpired,
  unixSecondsOf,
} from "@/utils/ops/trace-link";
import { formatUrlDisplay, sanitizeSensitiveValue } from "@/utils/sensitive";
import {
  MAX_TABLE_BROWSE_TOTAL,
  clampPageToSafeRange,
  resolveSafePageCount,
} from "@/utils/table/paginationLimits";
import { showErrorToast, showWarningToast } from "@/utils/ui/message-guard";
import { StructuredFilterBar } from "../shared/StructuredFilterBar";
import { toLocationHref, useOpsRole } from "../shared/opsHelpers";
import { StageBreakdown } from "./StageBreakdown";

const NUMERIC_QUERY_KEYS = new Set(["status_code", "duration_ms_gte"]);
const TIME_ZONE = "Asia/Shanghai";

const DETAIL_COPYABLE_KEYS = new Set([
  "request_id",
  "trace_id",
  "span_id",
  "parent_span_id",
  "tenant_id",
  "tenant_code",
  "product_id",
  "supplier_id",
  "supplier_code",
  "channel_id",
  "channel_code",
  "client_ip",
  "xff",
]);

const DETAIL_EXPORT_EXCLUDED_KEYS = new Set([
  "service",
  "instance_id",
  "upstream_host",
  "upstream_status_code",
  "level",
  "cache_hit",
]);

const TIMESTAMP_PARTS_FORMATTER = new Intl.DateTimeFormat("sv-SE", {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false,
  timeZone: TIME_ZONE,
});

function formatTimestampWithMillis(value: string | number | null): string {
  if (!value) return EMPTY_PLACEHOLDER;
  const raw = String(value);
  const date =
    typeof value === "number"
      ? new Date(value > 1_000_000_000_000_000 ? value / 1_000_000 : value)
      : new Date(value);
  if (Number.isNaN(date.getTime())) return raw;
  const parts = TIMESTAMP_PARTS_FORMATTER.formatToParts(date);
  const lookup = Object.fromEntries(parts.map((p) => [p.type, p.value]));
  const milliDigits = String(date.getMilliseconds()).padStart(3, "0");
  return `${lookup.year}-${lookup.month}-${lookup.day} ${lookup.hour || "00"}:${lookup.minute || "00"}:${lookup.second || "00"}.${milliDigits}`;
}

function getCallResponseResult(row: CallMonitorItem | null | undefined): string {
  if (!row) return EMPTY_PLACEHOLDER;
  const detail = (row.detail || {}) as Record<string, unknown>;
  const fromDetail = (key: string) =>
    typeof detail[key] === "string" ? (detail[key] as string) : "";
  const val =
    row.response_result ||
    row.response_flags ||
    fromDetail("response_result") ||
    fromDetail("response_flags") ||
    "";
  return val.trim() || EMPTY_PLACEHOLDER;
}

function statusBadgeVariant(
  code: number,
): "success" | "danger" | "warning" | "neutral" {
  if (code >= 200 && code < 300) return "success";
  if (code >= 400) return "danger";
  if (code >= 300) return "warning";
  return "neutral";
}

function traceIdFromFilters(list: readonly ListFilter[]): string | null {
  const hit = list.find((f) => f.field === "trace_id" && f.op === "eq");
  if (!hit || typeof hit.value !== "string") return null;
  const v = hit.value.trim();
  return v || null;
}

function hasTimestampFilter(list: readonly ListFilter[]): boolean {
  return list.some((f) => f.field === "timestamp");
}

function extractRawFields(
  detail: Record<string, unknown> | null | undefined,
): Record<string, unknown> {
  if (!detail || typeof detail !== "object" || Array.isArray(detail)) {
    return {};
  }
  const raw = detail.raw;
  if (typeof raw === "string" && raw.trim()) {
    try {
      const parsed = JSON.parse(raw) as Record<string, unknown>;
      const fields = parsed.fields;
      if (fields && typeof fields === "object" && !Array.isArray(fields)) {
        return fields as Record<string, unknown>;
      }
    } catch {
      return {};
    }
  }
  return {};
}

function buildDetailPayload(
  detail: Record<string, unknown> | null | undefined,
): Record<string, unknown> {
  const rawFields = extractRawFields(detail);
  const filtered = Object.fromEntries(
    Object.entries(rawFields).filter(
      ([key]) => !DETAIL_EXPORT_EXCLUDED_KEYS.has(key),
    ),
  );
  return sanitizeSensitiveValue(filtered) as Record<string, unknown>;
}

function formatDetailValue(value: unknown): string {
  if (value === null || value === undefined) return EMPTY_PLACEHOLDER;
  if (typeof value === "string") return formatUrlDisplay(value);
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function buildSafeFileName(value: string): string {
  return value.replace(/[^a-zA-Z0-9._-]+/g, "_");
}

function bucketLabel(
  ts: number,
  bucketSeconds: number,
  spanSecs: number,
): string {
  const ms = ts * 1000;
  if (bucketSeconds >= 86400) {
    return formatDateTime(ms).slice(5, 10);
  }
  if (bucketSeconds < 60) {
    return formatDateTime(ms).slice(11, 19);
  }
  return formatChartAxisTime(
    ms,
    Math.max(spanSecs, bucketSeconds >= 3600 ? 86401 : 0) * 1000,
  );
}

export interface CallMonitorPanelProps {
  mode: CallMonitorMode;
  timeRange: TimeRangeValue;
  onTimeRangeChange: (next: TimeRangeValue) => void;
  reloadTick: number;
}

export function CallMonitorPanel({
  mode,
  timeRange,
  onTimeRangeChange,
  reloadTick,
}: CallMonitorPanelProps) {
  const { t, locale } = useT();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const { isAdmin } = useOpsRole();

  const listCatalog = useMemo(() => callMonitorCatalogFor(mode), [mode]);
  const quickFilters = useMemo<QuickFilterDef[]>(
    () => listCatalog.quick_filters ?? [],
    [listCatalog],
  );

  const initialSeedFilters = useMemo<ListFilter[]>(() => {
    const out: ListFilter[] = [];
    for (const key of CALLS_FILTER_KEYS) {
      const value = searchParams.get(key)?.trim() ?? "";
      if (!value) continue;
      if (NUMERIC_QUERY_KEYS.has(key)) {
        const n = Number(value);
        if (!Number.isFinite(n)) continue;
        if (key === "duration_ms_gte") {
          out.push({ field: "duration_ms", op: "gte", value: n });
        } else {
          out.push({ field: key, op: "eq", value: n });
        }
        continue;
      }
      out.push({ field: key, op: "eq", value });
    }
    return out;
  }, []);

  const [q, setQ] = useState("");
  const [filters, setFilters] = useState<ListFilter[]>(initialSeedFilters);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);
  const [totalPages, setTotalPages] = useState(0);
  const [lastQueryMode, setLastQueryMode] = useState("latest");
  const [lastUniverseSize, setLastUniverseSize] = useState(
    CALL_MONITOR_UNIVERSE_SIZE,
  );

  const [loading, setLoading] = useState(false);
  const [tableData, setTableData] = useState<CallMonitorItem[]>([]);
  const [expandedKeys, setExpandedKeys] = useState<Set<string>>(
    () => new Set(),
  );
  const [currentDetail, setCurrentDetail] = useState<CallMonitorItem | null>(
    null,
  );
  const [exportOpen, setExportOpen] = useState(false);
  const [exporting, setExporting] = useState(false);

  const [histogram, setHistogram] = useState<CallHistogramResponse | null>(null);
  const [histogramLoading, setHistogramLoading] = useState(false);
  const [previousRange, setPreviousRange] = useState<TimeRangeValue | null>(
    null,
  );

  const lastBrushRef = useRef<TimeRangeValue | null>(null);
  const autoOpenedSeedRef = useRef(false);
  const logSeqRef = useRef(0);
  const histSeqRef = useRef(0);

  useEffect(() => {
    if (lastBrushRef.current && sameRange(timeRange, lastBrushRef.current)) {
      return;
    }
    lastBrushRef.current = null;
    setPreviousRange(null);
  }, [timeRange]);

  const fetchLogs = useCallback(
    (query: CallMonitorQuery, signal?: AbortSignal): Promise<CallMonitorResponse> =>
      mode === "upstream"
        ? fetchUpstreamCallLogs(query, signal)
        : fetchCustomerCallLogs(query, signal),
    [mode],
  );

  const buildQuery = useCallback(
    (
      reqPage: number,
      reqPageSize: number,
      reqQ: string,
      reqFilters: readonly ListFilter[],
    ): CallMonitorQuery => {
      const mapped = timestampFiltersToUnix([...reqFilters]);
      const selectiveSerial =
        parseRequestIdLookup(reqQ) ?? requestIdFromFilters([...reqFilters]);
      if (selectiveSerial) {
        const params = buildListQueryParams({
          q: reqQ,
          filters: mapped,
          encodeMode: "json",
          extra: {
            page: reqPage,
            page_size: reqPageSize,
            serial_no: selectiveSerial,
          },
        }) as CallMonitorQuery;
        delete params.q;
        delete params.start_time;
        delete params.end_time;
        return params;
      }

      const selectiveTrace = traceIdFromFilters(reqFilters);
      if (selectiveTrace) {
        const params = buildListQueryParams({
          q: reqQ,
          filters: mapped,
          encodeMode: "json",
          extra: {
            page: reqPage,
            page_size: reqPageSize,
            trace_id: selectiveTrace,
          },
        }) as CallMonitorQuery;
        delete params.start_time;
        delete params.end_time;
        return params;
      }

      const timeWindowParams = !hasTimestampFilter(reqFilters)
        ? { start_time: timeRange.start, end_time: timeRange.end }
        : {};

      return buildListQueryParams({
        q: reqQ,
        filters: mapped,
        encodeMode: "json",
        extra: {
          page: reqPage,
          page_size: reqPageSize,
          ...timeWindowParams,
        },
      }) as CallMonitorQuery;
    },
    [timeRange.start, timeRange.end],
  );

  const buildHistogramQuery = useCallback(
    (reqQ: string, reqFilters: readonly ListFilter[]): CallHistogramQuery => {
      const mapped = timestampFiltersToUnix([...reqFilters]);
      const params = buildListQueryParams({
        q: reqQ,
        filters: mapped,
        encodeMode: "json",
      }) as CallMonitorQuery;
      delete params.page;
      delete params.page_size;
      return {
        ...params,
        start_time: timeRange.start,
        end_time: timeRange.end,
      };
    },
    [timeRange.start, timeRange.end],
  );

  const loadAll = useCallback(
    async (
      reqPage: number,
      reqPageSize: number,
      reqQ: string,
      reqFilters: readonly ListFilter[],
    ) => {
      const logSeq = ++logSeqRef.current;
      const histSeq = ++histSeqRef.current;

      setLoading(true);
      setHistogramLoading(true);

      const histPromise = fetchCallHistogram(
        mode,
        buildHistogramQuery(reqQ, reqFilters),
      )
        .then((res) => {
          if (histSeq !== histSeqRef.current) return;
          setHistogram(res);
        })
        .catch(() => {
          if (histSeq !== histSeqRef.current) return;
          showErrorToast(t("ops.calls.histogram.loadFailed"));
        })
        .finally(() => {
          if (histSeq === histSeqRef.current) setHistogramLoading(false);
        });

      const logsPromise = fetchLogs(
        buildQuery(reqPage, reqPageSize, reqQ, reqFilters),
      )
        .then(async (response) => {
          if (logSeq !== logSeqRef.current) return;
          const rows = response.data || [];
          const respTotal = response.total || 0;
          const respTotalPages =
            response.total_pages || resolveSafePageCount(respTotal, reqPageSize);
          const safePage = clampPageToSafeRange(
            reqPage,
            respTotal,
            reqPageSize,
            respTotalPages,
          );

          if (rows.length === 0 && respTotal > 0 && safePage !== reqPage) {
            setPage(safePage);
            const retry = await fetchLogs(
              buildQuery(safePage, reqPageSize, reqQ, reqFilters),
            );
            if (logSeq !== logSeqRef.current) return;
            setTableData(retry.data || []);
            setTotal(retry.total || 0);
            setTotalPages(
              retry.total_pages ||
                resolveSafePageCount(retry.total || 0, reqPageSize),
            );
            setLastQueryMode(retry.query_mode || "latest");
            setLastUniverseSize(
              retry.universe_size || CALL_MONITOR_UNIVERSE_SIZE,
            );
            return;
          }

          setTableData(rows);
          setTotal(respTotal);
          setTotalPages(respTotalPages);
          setLastQueryMode(response.query_mode || "latest");
          setLastUniverseSize(
            response.universe_size || CALL_MONITOR_UNIVERSE_SIZE,
          );
          if (safePage !== reqPage) setPage(safePage);

          if (
            !autoOpenedSeedRef.current &&
            rows.length === 1 &&
            Boolean(requestIdFromFilters(initialSeedFilters))
          ) {
            autoOpenedSeedRef.current = true;
            setCurrentDetail(rows[0]!);
          }
        })
        .catch(() => {
          if (logSeq !== logSeqRef.current) return;
          showErrorToast(t("common.table.fetchFailed"));
        })
        .finally(() => {
          if (logSeq === logSeqRef.current) setLoading(false);
        });

      await Promise.all([histPromise, logsPromise]);
    },
    [mode, buildHistogramQuery, buildQuery, fetchLogs, initialSeedFilters, t],
  );

  useEffect(() => {
    setPage(1);
    setExpandedKeys(new Set());
    void loadAll(1, pageSize, q, filters);
  }, [mode, timeRange.start, timeRange.end, reloadTick]);

  const triggerSearch = (nextQ = q, nextFilters = filters) => {
    setPage(1);
    setExpandedKeys(new Set());
    void loadAll(1, pageSize, nextQ, nextFilters);
  };

  const isQuickFilterActive = (qf: QuickFilterDef): boolean =>
    qf.filters.every((target) =>
      filters.some(
        (f) =>
          f.field === target.field &&
          f.op === target.op &&
          String(f.value) === String(target.value),
      ),
    );

  const toggleQuickFilter = (qf: QuickFilterDef) => {
    const next = isQuickFilterActive(qf)
      ? filters.filter(
          (f) =>
            !qf.filters.some(
              (target) =>
                f.field === target.field &&
                f.op === target.op &&
                String(f.value) === String(target.value),
            ),
        )
      : [
          ...filters.filter(
            (f) => !qf.filters.some((target) => f.field === target.field),
          ),
          ...qf.filters,
        ];
    setFilters(next);
    triggerSearch(q, next);
  };

  const handleReset = () => {
    setQ("");
    setFilters([]);
    setLastQueryMode("latest");
    triggerSearch("", []);
  };

  const openTrace = (traceId: string, start?: number, requestId?: string) => {
    void navigate(toLocationHref(buildTraceRoute(traceId, { start, requestId })));
  };

  const channelText = (row: CallMonitorItem): string => {
    const view = resolveCallChannel(row);
    if (view.kind === "channel") return view.text;
    return view.productId
      ? t("operations.monitoring.channel.unroutedProduct", {
          product: view.productId,
        })
      : t("operations.monitoring.channel.unrouted");
  };

  const isUnrouted = (row: CallMonitorItem): boolean =>
    resolveCallChannel(row).kind === "unrouted";

  const channelTooltip = (row: CallMonitorItem): string => {
    const view = resolveCallChannel(row);
    if (view.kind === "unrouted") {
      return view.errorCode
        ? t("operations.monitoring.channel.unroutedTip", {
            code: view.errorCode,
          })
        : "";
    }
    if (mode !== "upstream") return view.text;
    const line = `${t("operations.monitoring.columns.channelName")}: ${view.text}`;
    const sup = String(row.supplier || row.supplier_name || "").trim();
    return sup
      ? `${line}\n${t("operations.monitoring.columns.supplier")}: ${sup}`
      : line;
  };

  const exportLogsArchive = async (
    rows: CallMonitorItem[],
    scope: "page" | "filtered",
  ) => {
    if (rows.length === 0) {
      showWarningToast(t("operations.monitoring.messages.noExport"));
      return;
    }
    setExporting(true);
    try {
      const encoder = new TextEncoder();
      const files = rows.map((row, index) => ({
        name: `${mode}-call-monitor-${buildSafeFileName(row.serial_no || `row-${index + 1}`)}.json`,
        content: encoder.encode(
          `${JSON.stringify(buildDetailPayload(row.detail), null, 2)}\n`,
        ),
      }));
      const packaged = await packageTarArchive(
        files,
        `${mode}-call-monitor-${scope}-${Date.now().toString(36)}.tar.gz`,
      );
      downloadBinary(packaged.filename, packaged.data, packaged.mimeType);
    } catch {
      showErrorToast(t("operations.monitoring.messages.exportFailed"));
    } finally {
      setExporting(false);
    }
  };

  const handleExportCommand = async (scope: "page" | "filtered") => {
    setExportOpen(false);
    if (exporting) return;
    if (scope === "page") {
      await exportLogsArchive(tableData, "page");
      return;
    }
    setExporting(true);
    try {
      const res = await fetchLogs(
        buildQuery(1, Math.min(MAX_TABLE_BROWSE_TOTAL, 20_000), q, filters),
      );
      await exportLogsArchive(res.data || [], "filtered");
    } catch {
      showErrorToast(t("operations.monitoring.messages.exportFailed"));
      setExporting(false);
    }
  };

  const downloadDetailJson = () => {
    if (!currentDetail) return;
    const json = JSON.stringify(
      buildDetailPayload(currentDetail.detail),
      null,
      2,
    );
    downloadBinary(
      `${mode}-call-detail-${buildSafeFileName(currentDetail.serial_no || "record")}.json`,
      `\uFEFF${json}`,
      "application/json;charset=utf-8;",
    );
  };

  const handleBrushEnd = useCallback(
    (params: unknown) => {
      if (!histogram || histogram.buckets.length === 0) return;
      const areas = (params as { areas?: Array<{ coordRange?: [number, number] }> })
        .areas ?? [];
      const range = areas[0]?.coordRange;
      if (!range) return;
      const lo = Math.max(0, Math.min(range[0], range[1]));
      const hi = Math.min(
        histogram.buckets.length - 1,
        Math.max(range[0], range[1]),
      );
      const first = histogram.buckets[lo];
      const last = histogram.buckets[hi];
      if (!first || !last) return;
      const start = first.ts;
      const end = Math.min(last.ts + histogram.bucket_seconds, histogram.end);
      if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) {
        return;
      }
      setPreviousRange({ ...timeRange });
      const next: TimeRangeValue = { preset: "custom", start, end };
      lastBrushRef.current = { ...next };
      onTimeRangeChange(next);
    },
    [histogram, timeRange, onTimeRangeChange],
  );

  const restorePreviousRange = () => {
    if (!previousRange) return;
    const prev = previousRange;
    setPreviousRange(null);
    lastBrushRef.current = null;
    onTimeRangeChange(prev);
  };

  const buildHistogramOption = useCallback(
    (tokens: ChartThemeTokens): EChartsCoreOption => {
      if (!histogram) return {};
      const spanSecs = Math.max(0, histogram.end - histogram.start);
      const labels = histogram.buckets.map((b) =>
        bucketLabel(b.ts, histogram.bucket_seconds, spanSecs),
      );
      return {
        tooltip: {
          trigger: "axis",
          axisPointer: { type: "shadow" },
        },
        legend: {
          data: [t("ops.calls.histogram.calls"), t("ops.calls.histogram.errors")],
          top: 0,
          right: 0,
          left: 84,
          icon: "roundRect",
          itemWidth: 10,
          itemHeight: 8,
          padding: [2, 0, 2, 0],
          textStyle: { color: tokens.textColor, fontSize: 12 },
        },
        grid: { ...OPS_CHART_GRID },
        toolbox: { show: false },
        brush: {
          toolbox: ["lineX"],
          xAxisIndex: 0,
          brushMode: "single",
          throttleType: "debounce",
        },
        xAxis: {
          type: "category",
          data: labels,
          axisLine: { lineStyle: { color: tokens.axisLineColor } },
          axisTick: { lineStyle: { color: tokens.axisLineColor } },
          axisLabel: { color: tokens.textColor, hideOverlap: true },
        },
        yAxis: {
          type: "value",
          minInterval: 1,
          axisLine: { show: false },
          axisTick: { show: false },
          splitLine: { lineStyle: { color: tokens.splitLineColor } },
          axisLabel: {
            color: tokens.textColor,
            hideOverlap: true,
            formatter: (v: number) => Math.round(v).toLocaleString(),
          },
        },
        series: [
          {
            name: t("ops.calls.histogram.calls"),
            type: "bar",
            stack: "calls",
            barMaxWidth: 32,
            data: histogram.buckets.map((b) => Math.max(0, b.calls - b.errors)),
            itemStyle: { color: tokens.primaryColor },
          },
          {
            name: t("ops.calls.histogram.errors"),
            type: "bar",
            stack: "calls",
            barMaxWidth: 32,
            data: histogram.buckets.map((b) => b.errors),
            itemStyle: { color: tokens.dangerColor },
          },
        ],
      };
    },
    [histogram, t],
  );

  const hasActiveFilter = Boolean(q.trim()) || filters.length > 0;
  const emptyDescription = useMemo(() => {
    if (!hasActiveFilter) return t("operations.monitoring.empty.noData");
    if (
      (lastQueryMode === "latest" || !lastQueryMode) &&
      !hasTimestampFilter(filters)
    ) {
      return t("operations.monitoring.empty.noMatchLatest", {
        size: lastUniverseSize || CALL_MONITOR_UNIVERSE_SIZE,
      });
    }
    return t("operations.monitoring.empty.noMatch");
  }, [hasActiveFilter, lastQueryMode, filters, lastUniverseSize, t]);

  const detailEntries = useMemo(() => {
    const payload = buildDetailPayload(currentDetail?.detail);
    return Object.entries(payload).map(([key, value]) => {
      const full = formatDetailValue(value);
      return {
        key,
        copyable:
          DETAIL_COPYABLE_KEYS.has(key) &&
          typeof value === "string" &&
          value !== "",
        full,
        display: full.length > 120 ? `${full.slice(0, 120)}...` : full,
        truncated: full.length > 120,
      };
    });
  }, [currentDetail]);

  const toggleExpand = (rowKey: string) => {
    setExpandedKeys((prev) => {
      const next = new Set(prev);
      if (next.has(rowKey)) next.delete(rowKey);
      else next.add(rowKey);
      return next;
    });
  };

  const queryPlaceholder =
    mode === "upstream"
      ? t("operations.monitoring.placeholders.upstream")
      : t("operations.monitoring.placeholders.customer");

  const histogramEmpty =
    histogram !== null && !histogramLoading && histogram.total === 0;

  return (
    <div className="flex flex-col flex-1 min-h-0 gap-4 min-w-0">
      <ChartCard
        title={t("ops.calls.histogram.title")}
        info={t("ops.calls.histogram.brushHint")}
        loading={histogramLoading}
        empty={histogramEmpty}
        emptyText={t("operations.monitoring.empty.noData")}
        height={190}
        extra={
          <div className="flex items-center gap-3">
            {previousRange && (
              <Button
                type="button"
                variant="link"
                size="xs"
                onClick={restorePreviousRange}
              >
                {t("ops.calls.histogram.resetZoom")}
              </Button>
            )}
            <span className="font-mono-tech tabular-nums">
              {t("ops.calls.histogram.summary", {
                total: histogram?.total ?? 0,
                errors: histogram?.errors ?? 0,
                bucket: formatIntervalHuman(
                  histogram?.bucket_seconds ?? 0,
                  locale,
                ),
              })}
            </span>
          </div>
        }
      >
        <EChartBox
          option={buildHistogramOption}
          height={190}
          autoBrushLineX
          onBrushEnd={handleBrushEnd}
        />
      </ChartCard>

      {/* Unified Single-Row Data Table Card Header */}
      <Card className="flex flex-col flex-1 min-h-0 overflow-hidden">
        <div className="flex flex-wrap items-center justify-between gap-2.5 border-b border-border-subtle px-4 py-3 shrink-0">
          <div className="flex flex-wrap items-center gap-2 min-w-0 flex-1">
            <StructuredFilterBar
              q={q}
              onQChange={setQ}
              filters={filters}
              onFiltersChange={setFilters}
              catalog={listCatalog}
              placeholder={queryPlaceholder}
              onSearch={() => triggerSearch()}
            />
            {quickFilters.map((qf) => (
              <ActionChip
                key={qf.id}
                size="sm"
                active={isQuickFilterActive(qf)}
                onClick={() => toggleQuickFilter(qf)}
              >
                {t(qf.label_key)}
              </ActionChip>
            ))}
          </div>

          <div className="flex flex-wrap items-center gap-2 shrink-0">
            <Popover
              open={exportOpen}
              onOpenChange={setExportOpen}
              align="right"
              contentClassName="w-44 p-1 space-y-0.5"
              trigger={
                <Button
                  type="button"
                  variant="outline"
                  size="md"
                  loading={exporting}
                  disabled={exporting || tableData.length === 0}
                  onClick={() => setExportOpen((v) => !v)}
                >
                  <Download className="h-3.5 w-3.5" />
                  <span>{t("operations.monitoring.actions.export")}</span>
                </Button>
              }
            >
              <button
                type="button"
                disabled={tableData.length === 0}
                onClick={() => void handleExportCommand("page")}
                className="w-full rounded-md px-2.5 py-1.5 text-left text-xs text-foreground hover:bg-hover-bg disabled:opacity-40 cursor-pointer"
              >
                {t("operations.monitoring.actions.exportPage")}
              </button>
              <button
                type="button"
                disabled={total === 0 && tableData.length === 0}
                onClick={() => void handleExportCommand("filtered")}
                className="w-full rounded-md px-2.5 py-1.5 text-left text-xs text-foreground hover:bg-hover-bg disabled:opacity-40 cursor-pointer"
              >
                {t("operations.monitoring.actions.exportFiltered")}
              </button>
            </Popover>

            <Button
              type="button"
              variant="outline"
              size="md"
              onClick={handleReset}
            >
              <RotateCcw className="h-3.5 w-3.5" />
              <span>{t("common.reset")}</span>
            </Button>
          </div>
        </div>

        <div className="relative flex-1 min-h-0 overflow-auto">
          {loading && tableData.length > 0 && (
            <div className="absolute inset-0 z-10 flex items-center justify-center bg-card/65 backdrop-blur-[1px]">
              <Loader2 className="h-5 w-5 animate-spin text-primary" />
            </div>
          )}

          {loading && tableData.length === 0 ? (
            <div className="flex items-center justify-center gap-2 py-14 text-xs text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin text-primary" />
              <span>{t("layout.loading")}</span>
            </div>
          ) : tableData.length === 0 ? (
            <div className="p-4">
              <EmptyState title={emptyDescription} />
            </div>
          ) : (
            <table className="w-full border-collapse text-xs text-foreground">
              <thead>
                <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                  <th className="h-9 w-9 px-2 text-center" />
                  <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                    {t("operations.monitoring.columns.responseTime")}
                  </th>
                  <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                    {t("operations.monitoring.columns.statusCode")}
                  </th>
                  <th className="h-9 px-3 pr-6 text-right font-medium whitespace-nowrap">
                    {mode === "customer"
                      ? t("operations.monitoring.columns.customerLatency")
                      : t("operations.monitoring.columns.upstreamLatency")}
                  </th>
                  <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                    {mode === "customer"
                      ? t("operations.monitoring.columns.tenantName")
                      : t("operations.monitoring.columns.supplier")}
                  </th>
                  <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                    {mode === "upstream"
                      ? t("operations.monitoring.columns.channelNameUpstream")
                      : t("operations.monitoring.columns.channelName")}
                  </th>
                  <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                    {t("operations.monitoring.columns.serialNo")}
                  </th>
                  {mode === "customer" && (
                    <>
                      <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                        {t("operations.monitoring.columns.requestIp")}
                      </th>
                      <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                        {t("operations.monitoring.columns.responseResult")}
                      </th>
                    </>
                  )}
                  <th className="h-9 w-20 px-3 text-right font-medium whitespace-nowrap">
                    {t("common.fields.operations")}
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border-subtle">
                {tableData.map((row, idx) => {
                  const rowKey = `${row.serial_no || idx}-${idx}`;
                  const expanded = expandedKeys.has(rowKey);
                  const rowTs = row.timestamp ?? row.response_time;
                  const tip = channelTooltip(row);
                  const colCount = mode === "customer" ? 10 : 8;
                  return (
                    <React.Fragment key={rowKey}>
                      <tr className="transition-colors hover:bg-hover-bg/75">
                        <td className="px-2 py-2.5 text-center align-middle">
                          <button
                            type="button"
                            onClick={() => toggleExpand(rowKey)}
                            aria-label={t("ops.calls.stages.breakdown")}
                            className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer"
                          >
                            {expanded ? (
                              <ChevronDown className="h-3.5 w-3.5" />
                            ) : (
                              <ChevronRight className="h-3.5 w-3.5" />
                            )}
                          </button>
                        </td>
                        <td className="px-3 py-2.5 align-middle font-mono-tech tabular-nums whitespace-nowrap">
                          {formatTimestampWithMillis(row.response_time)}
                        </td>
                        <td className="px-3 py-2.5 align-middle whitespace-nowrap">
                          <Badge variant={statusBadgeVariant(row.status_code)}>
                            {row.status_code || "—"}
                          </Badge>
                        </td>
                        <td className="px-3 py-2.5 pr-6 text-right align-middle font-mono-tech tabular-nums whitespace-nowrap">
                          {mode === "customer"
                            ? `${row.duration_ms} ms`
                            : `${row.upstream_service_time_ms} ms`}
                        </td>
                        <td className="max-w-[160px] truncate px-3 py-2.5 align-middle">
                          {mode === "customer"
                            ? row.tenant_name || row.tenant_id || "—"
                            : row.supplier || row.supplier_name || "—"}
                        </td>
                        <td className="max-w-[180px] px-3 py-2.5 align-middle">
                          <Tooltip content={tip} disabled={!tip}>
                            <span
                              className={cn(
                                "inline-block max-w-[160px] truncate",
                                isUnrouted(row) && "text-muted-foreground",
                              )}
                            >
                              {channelText(row)}
                            </span>
                          </Tooltip>
                        </td>
                        <td className="max-w-[190px] px-3 py-2.5 align-middle">
                          <CopyableText
                            value={row.serial_no}
                            mono
                            truncate
                            maxWidthClass="max-w-[140px]"
                          />
                        </td>
                        {mode === "customer" && (
                          <>
                            <td className="px-3 py-2.5 align-middle whitespace-nowrap">
                              <CopyableText
                                value={row.request_ip || row.client_ip || ""}
                                displayText={
                                  row.request_ip || row.client_ip || "—"
                                }
                                mono
                              />
                            </td>
                            <td className="max-w-[160px] truncate px-3 py-2.5 align-middle">
                              {getCallResponseResult(row)}
                            </td>
                          </>
                        )}
                        <td className="px-3 py-2.5 text-right align-middle whitespace-nowrap">
                          <Button
                            type="button"
                            variant="link"
                            size="xs"
                            onClick={() => setCurrentDetail(row)}
                          >
                            {t("common.actions.details")}
                          </Button>
                        </td>
                      </tr>
                      {expanded && (
                        <tr>
                          <td colSpan={colCount} className="px-4 py-2">
                            <StageBreakdown
                              requestId={row.serial_no}
                              traceId={row.trace_id}
                              timestamp={rowTs}
                              totalDurationMs={
                                mode === "customer" ? row.duration_ms : undefined
                              }
                              onOpenTrace={(tid) => {
                                if (!canDrillToTrace(rowTs, isAdmin)) return;
                                openTrace(
                                  tid,
                                  unixSecondsOf(rowTs),
                                  row.serial_no || undefined,
                                );
                              }}
                            />
                          </td>
                        </tr>
                      )}
                    </React.Fragment>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>

        <div className="mt-auto border-t border-border px-4 py-2.5 shrink-0">
          <Pagination
            page={page}
            pageSize={pageSize}
            total={total}
            pageSizeOptions={[20, 50, 100]}
            onPageChange={(nextPage) => {
              const clamped = clampPageToSafeRange(
                nextPage,
                total,
                pageSize,
                totalPages || undefined,
              );
              setPage(clamped);
              void loadAll(clamped, pageSize, q, filters);
            }}
            onPageSizeChange={(nextSize) => {
              setPageSize(nextSize);
              setPage(1);
              setTotalPages(0);
              void loadAll(1, nextSize, q, filters);
            }}
          />
        </div>
      </Card>

      {/* Three-part Call Detail Drawer */}
      <Drawer
        open={Boolean(currentDetail)}
        onClose={() => setCurrentDetail(null)}
        title={t("operations.monitoring.detail.title")}
        subtitle={currentDetail?.serial_no || "—"}
        size="lg"
        footer={
          currentDetail ? (
            <>
              <Button
                type="button"
                variant="outline"
                size="md"
                onClick={downloadDetailJson}
              >
                <Download className="h-3.5 w-3.5" />
                <span>{t("operations.monitoring.actions.downloadJson")}</span>
              </Button>
              <Button
                type="button"
                variant="primary"
                size="md"
                onClick={() => setCurrentDetail(null)}
              >
                {t("common.actions.close")}
              </Button>
            </>
          ) : undefined
        }
      >
        {currentDetail && (
          <div className="space-y-4">
            <Card className="overflow-hidden">
              <div className="border-b border-border px-3.5 py-2.5 text-xs font-semibold text-foreground">
                {t("operations.monitoring.detail.summaryTitle")}
              </div>
              <dl className="divide-y divide-border-subtle text-xs">
                <DetailRow
                  label={t("operations.monitoring.detail.labels.serialNo")}
                >
                  <CopyableText value={currentDetail.serial_no} mono />
                </DetailRow>
                <DetailRow label={t("ops.traces.columns.traceId")}>
                  {currentDetail.trace_id ? (
                    isAdmin &&
                    isTraceExpired(
                      currentDetail.timestamp ?? currentDetail.response_time,
                    ) ? (
                      <Tooltip content={t("ops.traces.expiredTooltip")}>
                        <span className="text-muted-foreground opacity-70">
                          <CopyableText value={currentDetail.trace_id} mono />
                        </span>
                      </Tooltip>
                    ) : canDrillToTrace(
                        currentDetail.timestamp ?? currentDetail.response_time,
                        isAdmin,
                      ) ? (
                      <div className="flex flex-wrap items-center gap-2">
                        <Button
                          type="button"
                          variant="link"
                          size="xs"
                          onClick={() => {
                            const ts =
                              currentDetail.timestamp ??
                              currentDetail.response_time;
                            openTrace(
                              currentDetail.trace_id,
                              unixSecondsOf(ts),
                              currentDetail.serial_no || undefined,
                            );
                          }}
                        >
                          {currentDetail.trace_id}
                        </Button>
                        <CopyableText
                          value={currentDetail.trace_id}
                          displayText=""
                        />
                      </div>
                    ) : (
                      <CopyableText value={currentDetail.trace_id} mono />
                    )
                  ) : (
                    EMPTY_PLACEHOLDER
                  )}
                </DetailRow>
                <DetailRow label={t("operations.monitoring.detail.labels.party")}>
                  {mode === "upstream"
                    ? currentDetail.supplier ||
                      currentDetail.supplier_name ||
                      EMPTY_PLACEHOLDER
                    : currentDetail.tenant_name ||
                      currentDetail.tenant_id ||
                      EMPTY_PLACEHOLDER}
                </DetailRow>
                <DetailRow
                  label={t("operations.monitoring.detail.labels.channelName")}
                >
                  <span
                    className={
                      isUnrouted(currentDetail) ? "text-muted-foreground" : ""
                    }
                  >
                    {channelText(currentDetail)}
                  </span>
                </DetailRow>
                <DetailRow
                  label={t("operations.monitoring.detail.labels.responseTime")}
                >
                  <span className="font-mono-tech tabular-nums">
                    {formatTimestampWithMillis(currentDetail.response_time)}
                  </span>
                </DetailRow>
                <DetailRow
                  label={
                    mode === "customer"
                      ? t("operations.monitoring.columns.customerLatency")
                      : t("operations.monitoring.columns.upstreamLatency")
                  }
                >
                  <span className="font-mono-tech tabular-nums">
                    {mode === "customer"
                      ? `${currentDetail.duration_ms} ms`
                      : `${currentDetail.upstream_service_time_ms} ms`}
                  </span>
                </DetailRow>
                <DetailRow
                  label={t("operations.monitoring.detail.labels.internalLatency")}
                >
                  <span className="font-mono-tech tabular-nums">
                    {currentDetail.internal_ms} ms
                  </span>
                </DetailRow>
                <DetailRow
                  label={t(
                    "operations.monitoring.detail.labels.postResponseLatency",
                  )}
                >
                  <span className="font-mono-tech tabular-nums">
                    {currentDetail.post_response_ms} ms
                  </span>
                </DetailRow>
                <DetailRow
                  label={t("operations.monitoring.detail.labels.statusCode")}
                >
                  <Badge variant={statusBadgeVariant(currentDetail.status_code)}>
                    {currentDetail.status_code}
                  </Badge>
                </DetailRow>
                {mode === "customer" && (
                  <>
                    <DetailRow
                      label={t("operations.monitoring.detail.labels.requestIp")}
                    >
                      <CopyableText
                        value={
                          currentDetail.request_ip ||
                          currentDetail.client_ip ||
                          ""
                        }
                        displayText={
                          currentDetail.request_ip ||
                          currentDetail.client_ip ||
                          EMPTY_PLACEHOLDER
                        }
                        mono
                      />
                    </DetailRow>
                    <DetailRow
                      label={t(
                        "operations.monitoring.detail.labels.responseResult",
                      )}
                    >
                      {getCallResponseResult(currentDetail)}
                    </DetailRow>
                  </>
                )}
                <DetailRow
                  label={t("operations.monitoring.detail.labels.errorSource")}
                >
                  {currentDetail.error_source || EMPTY_PLACEHOLDER}
                </DetailRow>
                <DetailRow
                  label={t("operations.monitoring.detail.labels.retryCount")}
                >
                  <span className="font-mono-tech tabular-nums">
                    {currentDetail.retry_count ?? 0}
                  </span>
                </DetailRow>
                {mode === "customer" ? (
                  <DetailRow
                    label={t("operations.monitoring.detail.labels.apiKeyId")}
                  >
                    {currentDetail.api_key_id ? (
                      <CopyableText value={currentDetail.api_key_id} mono />
                    ) : (
                      EMPTY_PLACEHOLDER
                    )}
                  </DetailRow>
                ) : (
                  <DetailRow
                    label={t(
                      "operations.monitoring.detail.labels.upstreamRequestId",
                    )}
                  >
                    {currentDetail.upstream_request_id ? (
                      <CopyableText
                        value={currentDetail.upstream_request_id}
                        mono
                      />
                    ) : (
                      EMPTY_PLACEHOLDER
                    )}
                  </DetailRow>
                )}
              </dl>
            </Card>

            <Card className="overflow-hidden">
              <div className="border-b border-border px-3.5 py-2.5 text-xs font-semibold text-foreground">
                {t("ops.calls.stages.breakdown")}
              </div>
              <StageBreakdown
                requestId={currentDetail.serial_no}
                traceId={currentDetail.trace_id}
                timestamp={
                  currentDetail.timestamp ?? currentDetail.response_time
                }
                totalDurationMs={
                  mode === "customer" ? currentDetail.duration_ms : undefined
                }
                onOpenTrace={(tid) => {
                  const ts =
                    currentDetail.timestamp ?? currentDetail.response_time;
                  if (!canDrillToTrace(ts, isAdmin)) return;
                  openTrace(
                    tid,
                    unixSecondsOf(ts),
                    currentDetail.serial_no || undefined,
                  );
                }}
              />
            </Card>

            <Card className="overflow-hidden">
              <div className="border-b border-border px-3.5 py-2.5 text-xs font-semibold text-foreground">
                {t("operations.monitoring.detail.rawTitle")}
              </div>
              <dl className="divide-y divide-border-subtle text-xs">
                {detailEntries.length === 0 ? (
                  <div className="px-3.5 py-3 text-muted-foreground">
                    {t("operations.monitoring.detail.empty")}
                  </div>
                ) : (
                  detailEntries.map((item) => (
                    <DetailRow key={item.key} label={item.key}>
                      {item.copyable ? (
                        <CopyableText value={item.full} mono />
                      ) : (
                        <Tooltip content={item.full} disabled={!item.truncated}>
                          <span className="break-words whitespace-pre-wrap font-mono-tech">
                            {item.display}
                          </span>
                        </Tooltip>
                      )}
                    </DetailRow>
                  ))
                )}
              </dl>
            </Card>
          </div>
        )}
      </Drawer>
    </div>
  );
}

function DetailRow({
  label,
  children,
}: {
  label: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-12 gap-2 px-3.5 py-2">
      <dt className="col-span-4 text-muted-foreground">{label}</dt>
      <dd className="col-span-8 min-w-0 text-foreground break-words">
        {children}
      </dd>
    </div>
  );
}
