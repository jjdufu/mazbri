import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  Gauge,
  Inbox,
  Info,
  RefreshCw,
  Timer,
} from "lucide-react";
import {
  ChartCard,
  MetricSeriesChart,
  isMetricSeriesEmpty,
} from "@/components/charts";
import {
  Card,
  DataTablePage,
  StatCard,
  Tooltip,
  type DataTableColumn,
} from "@/components/ui";
import { fetchBillingFailures, fetchBillingFunnel } from "@/api/billing-ops";
import { fetchMetricTimeseries } from "@/api/monitoring";
import { useT } from "@/locales";
import type {
  BillingFunnelPg,
  ChargeFailureRow,
  DeadLetterRow,
  FunnelStage,
} from "@/types/api/billing-ops";
import type { MetricSeries } from "@/types/api/monitoring";
import { getStatusLabel } from "@/utils/display-labels";
import { formatDateTime } from "@/utils/format";
import { aggregateLastMetricValue } from "@/utils/ops/metric-series";
import type { TimeRangeValue } from "@/utils/ops/time-range";
import { showErrorToast } from "@/utils/ui/message-guard";
import { resolveStep, toOpsRange } from "../shared/opsHelpers";
import {
  buildDeadLettersQueueQuery,
  buildFailuresQueueQuery,
  formatPipelineStageLabel,
} from "./billingPipelineHelpers";

const CHART_METRICS = [
  "billing_outbox_pending",
  "billing_oldest_pending_age",
  "billing_charge_failures",
  "billing_dead_letters",
  "billing_charge_lag_p50_ms",
  "billing_charge_lag_p99_ms",
  "billing_charge_items",
  "billing_charge_batches",
  "billing_deductions",
  "balance_precheck",
] as const;

type ChartMetric = (typeof CHART_METRICS)[number];

export interface BillingPipelinePanelProps {
  timeRange: TimeRangeValue;
  reloadTick: number;
}

export function BillingPipelinePanel({
  timeRange,
  reloadTick,
}: BillingPipelinePanelProps) {
  const { t, locale } = useT();
  const [loading, setLoading] = useState(false);
  const [failuresLoading, setFailuresLoading] = useState(false);
  const [deadLoading, setDeadLoading] = useState(false);

  const [funnel, setFunnel] = useState<BillingFunnelPg | null>(null);
  const [failures, setFailures] = useState<ChargeFailureRow[]>([]);
  const [deadLetters, setDeadLetters] = useState<DeadLetterRow[]>([]);
  const [failuresTotal, setFailuresTotal] = useState(0);
  const [deadLettersTotal, setDeadLettersTotal] = useState(0);
  const [failurePage, setFailurePage] = useState(1);
  const [failurePageSize, setFailurePageSize] = useState(20);
  const [deadPage, setDeadPage] = useState(1);
  const [deadPageSize, setDeadPageSize] = useState(20);

  const [series, setSeries] = useState<Record<ChartMetric, MetricSeries | null>>(
    () =>
      Object.fromEntries(CHART_METRICS.map((m) => [m, null])) as Record<
        ChartMetric,
        MetricSeries | null
      >,
  );

  const seqRef = useRef(0);
  const chartWindow = useMemo(
    () => ({ start: timeRange.start, end: timeRange.end }),
    [timeRange.start, timeRange.end],
  );

  useEffect(() => {
    const mine = ++seqRef.current;
    const ctrl = new AbortController();
    const start = timeRange.start;
    const end = timeRange.end;
    const step = resolveStep(end - start);
    const opsRange = toOpsRange(timeRange);

    setFailurePage(1);
    setDeadPage(1);
    setLoading(true);

    Promise.all([
      fetchBillingFunnel(opsRange, ctrl.signal),
      fetchBillingFailures(
        {
          range: opsRange,
          limit: Math.max(failurePageSize, deadPageSize),
          failed_offset: 0,
          dlq_offset: 0,
        },
        50,
        0,
        ctrl.signal,
      ),
      ...CHART_METRICS.map((metric) =>
        fetchMetricTimeseries({ metric, start, end, step }, ctrl.signal).catch(
          () => null,
        ),
      ),
    ])
      .then(([pg, fail, ...charts]) => {
        if (mine !== seqRef.current || ctrl.signal.aborted) return;
        setFunnel(pg);
        setFailures((fail.failures ?? []).slice(0, failurePageSize));
        setDeadLetters((fail.dead_letters ?? []).slice(0, deadPageSize));
        setFailuresTotal(fail.failures_total ?? 0);
        setDeadLettersTotal(fail.dead_letters_total ?? 0);
        const next: Partial<Record<ChartMetric, MetricSeries | null>> = {};
        CHART_METRICS.forEach((m, i) => {
          next[m] = (charts[i] as MetricSeries | null) ?? null;
        });
        setSeries(next as Record<ChartMetric, MetricSeries | null>);
      })
      .catch(() => {
        if (mine === seqRef.current && !ctrl.signal.aborted) {
          showErrorToast(t("ops.billing.loadFailed"));
        }
      })
      .finally(() => {
        if (mine === seqRef.current) setLoading(false);
      });

    return () => ctrl.abort();
  }, [timeRange.start, timeRange.end, timeRange.preset, reloadTick]);

  const loadFailuresPage = useCallback(
    async (nextPage: number, nextSize: number) => {
      setFailuresLoading(true);
      try {
        const res = await fetchBillingFailures(
          buildFailuresQueueQuery(toOpsRange(timeRange), nextPage, nextSize),
        );
        setFailures(res.failures ?? []);
        setFailuresTotal(res.failures_total ?? 0);
      } catch {
        showErrorToast(t("ops.billing.loadFailed"));
      } finally {
        setFailuresLoading(false);
      }
    },
    [timeRange, t],
  );

  const loadDeadLettersPage = useCallback(
    async (nextPage: number, nextSize: number) => {
      setDeadLoading(true);
      try {
        const res = await fetchBillingFailures(
          buildDeadLettersQueueQuery(toOpsRange(timeRange), nextPage, nextSize),
        );
        setDeadLetters(res.dead_letters ?? []);
        setDeadLettersTotal(res.dead_letters_total ?? 0);
      } catch {
        showErrorToast(t("ops.billing.loadFailed"));
      } finally {
        setDeadLoading(false);
      }
    },
    [timeRange, t],
  );

  const fmtVal = (val: number | null, digits: number, suffix = "") => {
    if (val === null || !Number.isFinite(val)) return "—";
    const text = new Intl.NumberFormat(locale, {
      maximumFractionDigits: digits,
    }).format(val);
    return suffix ? `${text} ${suffix}` : text;
  };

  const lagSeries = useMemo<MetricSeries | null>(() => {
    const p50 = series.billing_charge_lag_p50_ms;
    const p99 = series.billing_charge_lag_p99_ms;
    if (!p50 && !p99) return null;
    const tag = (data: MetricSeries | null, quantile: string) =>
      (data?.series ?? []).map((s) => ({
        ...s,
        labels: { ...s.labels, quantile },
      }));
    return {
      metric: "billing_charge_lag_ms",
      label: t("ops.billing.lagTrend"),
      unit: "ms",
      series: [...tag(p50, "p50"), ...tag(p99, "p99")],
    };
  }, [series.billing_charge_lag_p50_ms, series.billing_charge_lag_p99_ms, t]);

  const batchThroughputSeries = useMemo<MetricSeries | null>(() => {
    const items = series.billing_charge_items;
    const batches = series.billing_charge_batches;
    if (!items && !batches) return null;
    const tag = (data: MetricSeries | null, kind: string) =>
      (data?.series ?? []).map((s) => ({
        ...s,
        labels: { ...s.labels, kind },
      }));
    return {
      metric: "billing_charge_throughput",
      label: t("ops.billing.batchThroughput"),
      unit: items?.unit ?? batches?.unit ?? "",
      series: [...tag(items, "items"), ...tag(batches, "batches")],
    };
  }, [series.billing_charge_items, series.billing_charge_batches, t]);

  const trendCharts = useMemo(
    () => [
      {
        key: "failures",
        title: t("ops.billing.failureTrend"),
        data: series.billing_charge_failures,
      },
      {
        key: "dead",
        title: t("ops.billing.deadLetterTrend"),
        data: series.billing_dead_letters,
      },
      { key: "lag", title: t("ops.billing.lagTrend"), data: lagSeries },
      {
        key: "throughput",
        title: t("ops.billing.batchThroughput"),
        data: batchThroughputSeries,
      },
      {
        key: "deductions",
        title: t("ops.billing.deductions"),
        data: series.billing_deductions,
      },
      {
        key: "precheck",
        title: t("ops.billing.precheck"),
        data: series.balance_precheck,
      },
    ],
    [series, lagSeries, batchThroughputSeries, t],
  );

  const funnelColumns = useMemo<DataTableColumn<FunnelStage>[]>(
    () => [
      {
        id: "name",
        header: t("ops.billing.stage"),
        minWidth: 160,
        cell: (row) => formatPipelineStageLabel(row.name || row.key, t),
      },
      {
        id: "count",
        header: t("ops.billing.count"),
        width: 100,
        align: "right",
        mono: true,
        accessorKey: "count",
      },
      {
        id: "ratio_from_prev",
        header: t("ops.billing.ratioFromPrev"),
        width: 110,
        align: "right",
        mono: true,
        cell: (row) =>
          row.ratio_from_prev === null ||
          row.ratio_from_prev === undefined ||
          Number.isNaN(row.ratio_from_prev)
            ? "—"
            : `${(row.ratio_from_prev * 100).toFixed(1)}%`,
      },
      {
        id: "failed",
        header: t("ops.billing.stageFailed"),
        width: 90,
        align: "right",
        mono: true,
        accessorKey: "failed",
      },
    ],
    [t],
  );

  const failureColumns = useMemo<DataTableColumn<ChargeFailureRow>[]>(
    () => [
      {
        id: "created_at",
        header: t("ops.billing.columns.time"),
        minWidth: 156,
        mono: true,
        cell: (r) => formatDateTime(r.created_at),
      },
      {
        id: "tenant_id",
        header: t("ops.billing.columns.tenant"),
        minWidth: 120,
        mono: true,
        accessorKey: "tenant_id",
      },
      {
        id: "queue_id",
        header: t("ops.billing.columns.queueId"),
        minWidth: 120,
        mono: true,
        accessorKey: "queue_id",
      },
      {
        id: "attempt_number",
        header: t("ops.billing.columns.attempt"),
        width: 84,
        align: "right",
        mono: true,
        accessorKey: "attempt_number",
      },
      {
        id: "queue_status",
        header: t("ops.billing.columns.status"),
        width: 96,
        cell: (r) => (r.queue_status ? getStatusLabel(r.queue_status) : "—"),
      },
      {
        id: "error_message",
        header: t("ops.billing.columns.error"),
        minWidth: 160,
        accessorKey: "error_message",
      },
    ],
    [t],
  );

  const deadColumns = useMemo<DataTableColumn<DeadLetterRow>[]>(
    () => [
      {
        id: "created_at",
        header: t("ops.billing.columns.time"),
        minWidth: 156,
        mono: true,
        cell: (r) => formatDateTime(r.created_at),
      },
      {
        id: "source",
        header: t("ops.billing.columns.source"),
        minWidth: 140,
        cell: (r) => formatPipelineStageLabel(r.source, t),
      },
      {
        id: "id",
        header: t("ops.billing.columns.id"),
        minWidth: 120,
        mono: true,
        accessorKey: "id",
      },
      {
        id: "tenant_id",
        header: t("ops.billing.columns.tenant"),
        minWidth: 120,
        mono: true,
        accessorKey: "tenant_id",
      },
      {
        id: "retry_count",
        header: t("ops.billing.columns.attempt"),
        width: 84,
        align: "right",
        mono: true,
        accessorKey: "retry_count",
      },
      {
        id: "error_message",
        header: t("ops.billing.columns.error"),
        minWidth: 160,
        accessorKey: "error_message",
      },
    ],
    [t],
  );

  const funnelTitle =
    funnel?.mode === "sync_pass_through"
      ? t("ops.billing.pipelineHealth")
      : t("ops.billing.funnel");
  const funnelHint =
    funnel?.mode === "sync_pass_through"
      ? t("ops.billing.noBacklog")
      : funnel?.mode === "async_funnel"
        ? t("ops.billing.asyncFunnelHint")
        : undefined;

  return (
    <div className="space-y-4 min-w-0">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-5">
        <StatCard
          label={t("ops.billing.backlog")}
          value={fmtVal(
            aggregateLastMetricValue(series.billing_outbox_pending, "sum"),
            0,
          )}
          icon={<Inbox className="h-3.5 w-3.5 text-primary" />}
        />
        <StatCard
          label={t("ops.billing.throughputNow")}
          value={fmtVal(
            aggregateLastMetricValue(series.billing_charge_items, "sum"),
            2,
            t("ops.units.perSec"),
          )}
          icon={<Gauge className="h-3.5 w-3.5 text-success-fg" />}
        />
        <StatCard
          label={t("ops.billing.failureCount")}
          value={fmtVal(funnel?.charge_failures ?? null, 0)}
          icon={<RefreshCw className="h-3.5 w-3.5 text-warning-fg" />}
        />
        <StatCard
          label={t("ops.billing.deadLettersNow")}
          value={fmtVal(
            aggregateLastMetricValue(series.billing_dead_letters, "sum"),
            0,
          )}
          icon={<AlertTriangle className="h-3.5 w-3.5 text-danger-fg" />}
        />
        <StatCard
          label={t("ops.billing.lagP99")}
          value={fmtVal(
            aggregateLastMetricValue(series.billing_charge_lag_p99_ms, "max"),
            1,
            "ms",
          )}
          icon={<Timer className="h-3.5 w-3.5 text-muted-foreground" />}
        />
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card className="overflow-hidden">
          <div className="flex items-center gap-1.5 border-b border-border px-4 py-3">
            <h3 className="text-xs font-semibold text-foreground">
              {funnelTitle}
            </h3>
            {funnelHint && (
              <Tooltip content={funnelHint}>
                <span className="inline-flex text-muted-foreground hover:text-foreground cursor-help">
                  <Info className="h-3.5 w-3.5" />
                </span>
              </Tooltip>
            )}
          </div>
          <DataTablePage
            columns={funnelColumns}
            data={funnel?.stages ?? []}
            getRowId={(r) => r.key}
            loading={loading}
            className="[&>div]:border-0 [&>div]:rounded-none"
          />
        </Card>

        <div className="flex flex-col gap-4">
          <ChartCard
            title={t("ops.billing.queueTrend")}
            loading={loading}
            empty={!loading && isMetricSeriesEmpty(series.billing_outbox_pending)}
            height={180}
          >
            <MetricSeriesChart
              series={series.billing_outbox_pending}
              window={chartWindow}
              height={180}
            />
          </ChartCard>
          <ChartCard
            title={t("ops.billing.oldestAge")}
            loading={loading}
            empty={
              !loading && isMetricSeriesEmpty(series.billing_oldest_pending_age)
            }
            height={180}
          >
            <MetricSeriesChart
              series={series.billing_oldest_pending_age}
              window={chartWindow}
              height={180}
            />
          </ChartCard>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
        {trendCharts.map((c) => (
          <ChartCard
            key={c.key}
            title={c.title}
            loading={loading}
            empty={!loading && isMetricSeriesEmpty(c.data)}
            height={210}
          >
            <MetricSeriesChart
              series={c.data}
              window={chartWindow}
              height={210}
            />
          </ChartCard>
        ))}
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card className="overflow-hidden">
          <div className="border-b border-border px-4 py-3">
            <h3 className="text-xs font-semibold text-foreground">
              {t("ops.billing.failures")}
            </h3>
          </div>
          <DataTablePage
            columns={failureColumns}
            data={failures}
            getRowId={(r) => String(r.id)}
            loading={loading || failuresLoading}
            emptyText={t("ops.charts.empty")}
            page={failurePage}
            pageSize={failurePageSize}
            total={failuresTotal}
            pageSizeOptions={[10, 20, 50]}
            onPageChange={(nextPage) => {
              setFailurePage(nextPage);
              void loadFailuresPage(nextPage, failurePageSize);
            }}
            onPageSizeChange={(nextSize) => {
              setFailurePageSize(nextSize);
              setFailurePage(1);
              void loadFailuresPage(1, nextSize);
            }}
            className="[&>div]:border-0 [&>div]:rounded-none"
          />
        </Card>

        <Card className="overflow-hidden">
          <div className="border-b border-border px-4 py-3">
            <h3 className="text-xs font-semibold text-foreground">
              {t("ops.billing.deadLetters")}
            </h3>
          </div>
          <DataTablePage
            columns={deadColumns}
            data={deadLetters}
            getRowId={(r) => r.id}
            loading={loading || deadLoading}
            emptyText={t("ops.charts.empty")}
            page={deadPage}
            pageSize={deadPageSize}
            total={deadLettersTotal}
            pageSizeOptions={[10, 20, 50]}
            onPageChange={(nextPage) => {
              setDeadPage(nextPage);
              void loadDeadLettersPage(nextPage, deadPageSize);
            }}
            onPageSizeChange={(nextSize) => {
              setDeadPageSize(nextSize);
              setDeadPage(1);
              void loadDeadLettersPage(1, nextSize);
            }}
            className="[&>div]:border-0 [&>div]:rounded-none"
          />
        </Card>
      </div>
    </div>
  );
}
