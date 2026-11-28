import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";
import type { EChartsCoreOption } from "echarts";
import { RotateCcw } from "lucide-react";
import {
  ChartCard,
  EChartBox,
  OPS_CHART_GRID,
  formatChartAxisTime,
  formatChartTooltipTime,
  type ChartThemeTokens,
} from "@/components/charts";
import {
  ActionChip,
  Badge,
  Button,
  CopyableText,
  DataTablePage,
  type DataTableColumn,
} from "@/components/ui";
import { fetchTraceServices, searchTraces } from "@/api/traces";
import { useT } from "@/locales";
import type { TraceSearchQuery, TraceSummary } from "@/types/api/traces";
import type { ListFilter, ResourceFilterCatalog } from "@/types/list-query";
import { formatDateTime } from "@/utils/format";
import type { TimeRangeValue } from "@/utils/ops/time-range";
import { buildTraceRoute } from "@/utils/ops/trace-link";
import { showErrorToast } from "@/utils/ui/message-guard";
import { StructuredFilterBar } from "../shared/StructuredFilterBar";
import { toLocationHref } from "../shared/opsHelpers";

const MAX_SEARCH_SECONDS = 7 * 86400;

export interface TrafficTracesPanelProps {
  timeRange: TimeRangeValue;
  reloadTick: number;
}

export function TrafficTracesPanel({
  timeRange,
  reloadTick,
}: TrafficTracesPanelProps) {
  const { t } = useT();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  const initialKeyword =
    searchParams.get("trace_id")?.trim() ||
    searchParams.get("request_id")?.trim() ||
    "";

  const initialFilters = useMemo<ListFilter[]>(() => {
    const out: ListFilter[] = [];
    const tid = searchParams.get("trace_id")?.trim();
    const rid = searchParams.get("request_id")?.trim();
    if (tid && rid) out.push({ field: "request_id", op: "eq", value: rid });
    const tenant = searchParams.get("tenant_code")?.trim();
    if (tenant) out.push({ field: "tenant_code", op: "eq", value: tenant });
    const product = searchParams.get("product_id")?.trim();
    if (product) out.push({ field: "product_id", op: "eq", value: product });
    const svc = searchParams.get("service")?.trim();
    if (svc) out.push({ field: "service", op: "eq", value: svc });
    const minDur = Number(searchParams.get("min_duration_ms"));
    if (searchParams.get("min_duration_ms") && Number.isFinite(minDur) && minDur >= 0) {
      out.push({ field: "min_duration_ms", op: "gte", value: minDur });
    }
    const maxDur = Number(searchParams.get("max_duration_ms"));
    if (searchParams.get("max_duration_ms") && Number.isFinite(maxDur) && maxDur >= 0) {
      out.push({ field: "max_duration_ms", op: "lte", value: maxDur });
    }
    return out;
  }, []);

  const [keyword, setKeyword] = useState(initialKeyword);
  const [listFilters, setListFilters] = useState<ListFilter[]>(initialFilters);
  const [errorsOnly, setErrorsOnly] = useState(
    () => searchParams.get("errors_only") === "true",
  );
  const [includeInternal, setIncludeInternal] = useState(
    () => searchParams.get("include_internal") === "true",
  );

  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  const [traces, setTraces] = useState<TraceSummary[]>([]);
  const [resolvedFromLogs, setResolvedFromLogs] = useState<number | null>(null);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);
  const [catalogServices, setCatalogServices] = useState<string[]>([]);

  const seqRef = useRef(0);
  const lastCatalogKeyRef = useRef("");

  const clampedRange = useMemo(() => {
    const start = Math.max(timeRange.start, timeRange.end - MAX_SEARCH_SECONDS);
    return { start, end: timeRange.end };
  }, [timeRange.start, timeRange.end]);

  const serviceOptions = useMemo(() => {
    const set = new Set<string>(catalogServices);
    for (const tr of traces) {
      if (tr.root_service) set.add(tr.root_service);
    }
    return [...set].filter(Boolean).sort();
  }, [catalogServices, traces]);

  const tracesCatalog = useMemo<ResourceFilterCatalog>(
    () => ({
      resource: "traces",
      max_filters: 10,
      fields: [
        {
          field: "trace_id",
          value_type: "string",
          ops: ["eq"],
          in_q: true,
          label_key: "ops.traces.columns.traceId",
        },
        {
          field: "request_id",
          value_type: "string",
          ops: ["eq"],
          in_q: true,
          label_key: "listFilters.fields.requestId",
        },
        {
          field: "tenant_code",
          value_type: "string",
          ops: ["eq"],
          in_q: true,
          label_key: "ops.traces.form.tenant",
        },
        {
          field: "product_id",
          value_type: "string",
          ops: ["eq"],
          in_q: true,
          label_key: "ops.traces.form.product",
        },
        {
          field: "service",
          value_type: serviceOptions.length > 0 ? "enum" : "string",
          ops: ["eq"],
          in_q: true,
          label_key: "ops.traces.form.service",
          enum_values: serviceOptions,
        },
        {
          field: "min_duration_ms",
          value_type: "int",
          ops: ["gte", "eq"],
          label_key: "ops.traces.form.minDuration",
        },
        {
          field: "max_duration_ms",
          value_type: "int",
          ops: ["lte", "eq"],
          label_key: "ops.traces.form.maxDuration",
        },
      ],
    }),
    [serviceOptions],
  );

  const runSearch = useCallback(
    async (opts: {
      reqPage: number;
      reqPageSize: number;
      reqKeyword: string;
      reqFilters: readonly ListFilter[];
      reqErrorsOnly: boolean;
      reqIncludeInternal: boolean;
    }) => {
      const seq = ++seqRef.current;
      setLoading(true);
      const query: TraceSearchQuery = {
        start: clampedRange.start,
        end: clampedRange.end,
        page: opts.reqPage,
        page_size: opts.reqPageSize,
      };
      const qTrim = opts.reqKeyword.trim();
      if (qTrim) {
        if (/^[0-9a-fA-F]{32}$/.test(qTrim)) {
          query.trace_id = qTrim.toLowerCase();
        } else {
          query.request_id = qTrim;
        }
      }
      for (const f of opts.reqFilters) {
        const val = String(f.value ?? "").trim();
        if (!val) continue;
        if (f.field === "trace_id") query.trace_id = val.toLowerCase();
        else if (f.field === "request_id") query.request_id = val;
        else if (f.field === "tenant_code") query.tenant_code = val;
        else if (f.field === "product_id") query.product_id = val;
        else if (f.field === "service") query.service = val;
        else if (f.field === "min_duration_ms") {
          const n = Number(val);
          if (Number.isFinite(n) && n >= 0) query.min_duration_ms = n;
        } else if (f.field === "max_duration_ms") {
          const n = Number(val);
          if (Number.isFinite(n) && n >= 0) query.max_duration_ms = n;
        }
      }
      if (opts.reqErrorsOnly) query.errors_only = true;
      if (opts.reqIncludeInternal) query.include_internal = true;

      try {
        const res = await searchTraces(query);
        if (seq !== seqRef.current) return;
        setTraces(res.traces ?? []);
        setTotal(res.total ?? 0);
        setPage(res.page ?? opts.reqPage);
        setPageSize(res.page_size ?? opts.reqPageSize);
        setResolvedFromLogs(res.resolved_from_logs ?? null);
        setSearched(true);

        setSearchParams(
          (prev) => {
            const next = new URLSearchParams(prev);
            const setOrDel = (k: string, v: string | number | boolean | undefined) => {
              if (v === undefined || v === "" || v === false) next.delete(k);
              else next.set(k, String(v));
            };
            setOrDel("trace_id", query.trace_id);
            setOrDel("request_id", query.request_id);
            setOrDel("tenant_code", query.tenant_code);
            setOrDel("product_id", query.product_id);
            setOrDel("service", query.service);
            setOrDel("min_duration_ms", query.min_duration_ms);
            setOrDel("max_duration_ms", query.max_duration_ms);
            setOrDel("errors_only", query.errors_only);
            setOrDel("include_internal", query.include_internal);
            return next;
          },
          { replace: true },
        );
      } catch {
        if (seq === seqRef.current) showErrorToast(t("ops.traces.loadFailed"));
      } finally {
        if (seq === seqRef.current) setLoading(false);
      }
    },
    [clampedRange.start, clampedRange.end, setSearchParams, t],
  );

  useEffect(() => {
    const key = `${clampedRange.start}:${clampedRange.end}`;
    if (key !== lastCatalogKeyRef.current) {
      lastCatalogKeyRef.current = key;
      fetchTraceServices(clampedRange)
        .then((rows) => {
          setCatalogServices(
            (rows ?? [])
              .map((r) => r.service_name)
              .filter((s) => Boolean(s) && !s.includes(":")),
          );
        })
        .catch(() => {
          lastCatalogKeyRef.current = "";
        });
    }
    setPage(1);
    void runSearch({
      reqPage: 1,
      reqPageSize: pageSize,
      reqKeyword: keyword,
      reqFilters: listFilters,
      reqErrorsOnly: errorsOnly,
      reqIncludeInternal: includeInternal,
    });
  }, [clampedRange.start, clampedRange.end, reloadTick]);

  const okTraces = useMemo(
    () => traces.filter((r) => r.error_count === 0),
    [traces],
  );
  const errorTraces = useMemo(
    () => traces.filter((r) => r.error_count > 0),
    [traces],
  );

  const openTraceRow = useCallback(
    (row: TraceSummary) => {
      void navigate(
        toLocationHref(
          buildTraceRoute(row.trace_id, {
            start: Math.floor(row.start_ms / 1000),
          }),
        ),
      );
    },
    [navigate],
  );

  const buildScatterOption = useCallback(
    (tokens: ChartThemeTokens): EChartsCoreOption => {
      const spanMs = Math.max(0, (clampedRange.end - clampedRange.start) * 1000);
      const toPoint = (r: TraceSummary) => [
        r.start_ms,
        Number(r.duration_ms.toFixed(2)),
      ];
      return {
        grid: { ...OPS_CHART_GRID, top: 36 },
        tooltip: {
          trigger: "item",
          formatter: (p: unknown) => {
            const item = Array.isArray(p) ? p[0] : p;
            const rec = item as { seriesIndex?: number; dataIndex?: number };
            const series = rec.seriesIndex === 1 ? errorTraces : okTraces;
            const row = series[rec.dataIndex ?? -1];
            if (!row) return "";
            return `${row.root_service} · ${row.root_span}<br/>${formatChartTooltipTime(row.start_ms)}<br/>${row.duration_ms.toFixed(1)} ms · ${row.span_count} ${t("ops.traces.columns.spans")}<br/>${row.trace_id}`;
          },
        },
        legend: {
          top: 0,
          right: 0,
          left: 84,
          icon: "circle",
          itemWidth: 8,
          itemHeight: 8,
          padding: [2, 0, 2, 0],
          textStyle: { color: tokens.textColor, fontSize: 12 },
        },
        xAxis: {
          type: "time",
          min: clampedRange.start * 1000,
          max: clampedRange.end * 1000,
          axisLine: { lineStyle: { color: tokens.axisLineColor } },
          axisTick: { lineStyle: { color: tokens.axisLineColor } },
          splitLine: { show: false },
          axisLabel: {
            color: tokens.textColor,
            hideOverlap: true,
            formatter: (val: number) => formatChartAxisTime(val, spanMs),
          },
        },
        yAxis: {
          type: "value",
          name: "ms",
          nameGap: 12,
          nameTextStyle: { color: tokens.textColor, align: "left" },
          scale: true,
          axisLine: { show: false },
          axisTick: { show: false },
          splitLine: { lineStyle: { color: tokens.splitLineColor } },
          axisLabel: { color: tokens.textColor, hideOverlap: true },
        },
        series: [
          {
            name: t("ops.traces.scatter.ok"),
            type: "scatter",
            symbol: "circle",
            symbolSize: 8,
            itemStyle: { color: tokens.primaryColor },
            data: okTraces.map(toPoint),
          },
          {
            name: t("ops.traces.scatter.error"),
            type: "scatter",
            symbol: "circle",
            symbolSize: 10,
            itemStyle: { color: tokens.dangerColor },
            data: errorTraces.map(toPoint),
          },
        ],
      };
    },
    [clampedRange.start, clampedRange.end, okTraces, errorTraces, t],
  );

  const handleScatterClick = useCallback(
    (params: unknown) => {
      const rec = params as { seriesIndex?: number; dataIndex?: number };
      const list = rec.seriesIndex === 1 ? errorTraces : okTraces;
      const row = list[rec.dataIndex ?? -1];
      if (row) openTraceRow(row);
    },
    [errorTraces, okTraces, openTraceRow],
  );

  const columns = useMemo<DataTableColumn<TraceSummary>[]>(
    () => [
      {
        id: "start_ms",
        header: t("ops.traces.columns.start"),
        minWidth: 164,
        mono: true,
        cell: (r) => formatDateTime(r.start_ms),
      },
      {
        id: "root_span",
        header: t("ops.traces.columns.root"),
        minWidth: 220,
        cell: (r) => (
          <div className="flex items-center gap-1.5 min-w-0">
            <span className="rounded-md bg-muted px-1.5 py-0.5 text-[11px] font-mono-tech text-foreground shrink-0">
              {r.root_service}
            </span>
            <span className="truncate">{r.root_span}</span>
          </div>
        ),
      },
      {
        id: "duration_ms",
        header: t("ops.traces.columns.duration"),
        width: 112,
        align: "right",
        mono: true,
        cell: (r) => `${r.duration_ms.toFixed(1)} ms`,
      },
      {
        id: "span_count",
        header: t("ops.traces.columns.spans"),
        width: 84,
        align: "right",
        mono: true,
        accessorKey: "span_count",
      },
      {
        id: "service_count",
        header: t("ops.traces.columns.services"),
        width: 88,
        align: "right",
        mono: true,
        accessorKey: "service_count",
      },
      {
        id: "error_count",
        header: t("ops.traces.columns.errors"),
        width: 84,
        align: "right",
        cell: (r) =>
          r.error_count > 0 ? (
            <Badge variant="danger">{r.error_count}</Badge>
          ) : (
            <span className="font-mono-tech tabular-nums text-muted-foreground">
              0
            </span>
          ),
      },
      {
        id: "trace_id",
        header: t("ops.traces.columns.traceId"),
        minWidth: 240,
        cell: (r) => (
          <div className="flex items-center gap-1.5 min-w-0">
            <Button
              type="button"
              variant="link"
              size="xs"
              className="px-1 font-mono-tech"
              onClick={() => openTraceRow(r)}
            >
              {r.trace_id}
            </Button>
            <CopyableText value={r.trace_id} displayText="" />
          </div>
        ),
      },
    ],
    [t, openTraceRow],
  );

  const emptyText = searched
    ? t("ops.traces.empty")
    : t("ops.traces.notSearched");

  const handleReset = () => {
    setKeyword("");
    setListFilters([]);
    setErrorsOnly(false);
    setIncludeInternal(false);
    setPage(1);
    void runSearch({
      reqPage: 1,
      reqPageSize: pageSize,
      reqKeyword: "",
      reqFilters: [],
      reqErrorsOnly: false,
      reqIncludeInternal: false,
    });
  };

  return (
    <div className="flex flex-col flex-1 min-h-0 gap-4 min-w-0">
      <ChartCard
        title={t("ops.traces.scatter.title")}
        loading={loading}
        empty={searched && !loading && traces.length === 0}
        emptyText={emptyText}
        height={220}
        extra={
          <span className="font-mono-tech">
            {t("ops.traces.scatter.summary", { count: total })}
            {resolvedFromLogs !== null
              ? ` · ${t("ops.traces.resolved", { count: resolvedFromLogs })}`
              : ""}
          </span>
        }
      >
        <EChartBox
          option={buildScatterOption}
          height={220}
          onClick={handleScatterClick}
        />
      </ChartCard>

      <DataTablePage
        columns={columns}
        data={traces}
        getRowId={(r) => r.trace_id}
        loading={loading}
        emptyText={emptyText}
        page={page}
        pageSize={pageSize}
        total={total}
        pageSizeOptions={[20, 50, 100]}
        onPageChange={(nextPage) => {
          setPage(nextPage);
          void runSearch({
            reqPage: nextPage,
            reqPageSize: pageSize,
            reqKeyword: keyword,
            reqFilters: listFilters,
            reqErrorsOnly: errorsOnly,
            reqIncludeInternal: includeInternal,
          });
        }}
        onPageSizeChange={(nextSize) => {
          setPageSize(nextSize);
          setPage(1);
          void runSearch({
            reqPage: 1,
            reqPageSize: nextSize,
            reqKeyword: keyword,
            reqFilters: listFilters,
            reqErrorsOnly: errorsOnly,
            reqIncludeInternal: includeInternal,
          });
        }}
        filters={
          <div className="flex flex-wrap items-center gap-2">
            <StructuredFilterBar
              q={keyword}
              onQChange={setKeyword}
              filters={listFilters}
              onFiltersChange={setListFilters}
              catalog={tracesCatalog}
              onSearch={() => {
                setPage(1);
                void runSearch({
                  reqPage: 1,
                  reqPageSize: pageSize,
                  reqKeyword: keyword,
                  reqFilters: listFilters,
                  reqErrorsOnly: errorsOnly,
                  reqIncludeInternal: includeInternal,
                });
              }}
            />

            <ActionChip
              size="sm"
              active={errorsOnly}
              onClick={() => {
                const next = !errorsOnly;
                setErrorsOnly(next);
                setPage(1);
                void runSearch({
                  reqPage: 1,
                  reqPageSize: pageSize,
                  reqKeyword: keyword,
                  reqFilters: listFilters,
                  reqErrorsOnly: next,
                  reqIncludeInternal: includeInternal,
                });
              }}
            >
              {t("ops.traces.form.errorsOnly")}
            </ActionChip>

            <label className="inline-flex h-8 items-center gap-1.5 rounded-md border border-control-border bg-control-bg hover:bg-control-hover hover:border-border-hover px-2.5 text-xs text-foreground cursor-pointer select-none transition-colors">
              <input
                type="checkbox"
                checked={includeInternal}
                onChange={(e) => {
                  const next = e.target.checked;
                  setIncludeInternal(next);
                  setPage(1);
                  void runSearch({
                    reqPage: 1,
                    reqPageSize: pageSize,
                    reqKeyword: keyword,
                    reqFilters: listFilters,
                    reqErrorsOnly: errorsOnly,
                    reqIncludeInternal: next,
                  });
                }}
              />
              <span>{t("ops.traces.form.includeInternal")}</span>
            </label>
          </div>
        }
        actions={
          <Button
            type="button"
            variant="outline"
            size="md"
            onClick={handleReset}
          >
            <RotateCcw className="h-3.5 w-3.5" />
            <span>{t("common.reset")}</span>
          </Button>
        }
      />
    </div>
  );
}
