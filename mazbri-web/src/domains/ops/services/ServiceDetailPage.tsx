import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import { Database, GitCommit, HeartPulse, RotateCw } from "lucide-react";
import {
  ChartCard,
  MetricSeriesChart,
  isMetricSeriesEmpty,
  isMetricSeriesZeroOrEmpty,
} from "@/components/charts";
import {
  Card,
  EmptyState,
  EntityDetailLayout,
  Select,
  StatCard,
  cn,
  type SelectOptionItem,
} from "@/components/ui";
import {
  fetchMetricTimeseries,
  fetchMetricsCatalog,
  fetchServicesHealth,
} from "@/api/monitoring";
import { useT } from "@/locales";
import type {
  MetricCatalogItem,
  MetricSeries,
  ServiceHealthRow,
} from "@/types/api/monitoring";
import { formatDateTime } from "@/utils/format";
import { buildRuntimeRoute } from "@/utils/ops/runtime-link";
import {
  filterRowsByGroup,
  serviceHasPostgresPool,
} from "@/utils/ops/service-topology";
import { buildServiceDetailRoute } from "@/utils/ops/services-link";
import { showErrorToast } from "@/utils/ui/message-guard";
import { OpsTimeBar } from "../shared/OpsTimeBar";
import { toLocationHref, useOpsTimeRangeUrl } from "../shared/opsHelpers";

const USE_METRIC_KEYS = [
  "process_cpu_cores",
  "process_resident_memory_mb",
  "process_open_fds",
  "tokio_busy_ratio",
  "tokio_alive_tasks",
  "tokio_scheduler_lag_p99_ms",
] as const;

export function ServiceDetailPage() {
  const { t } = useT();
  const navigate = useNavigate();
  const params = useParams<{ service: string }>();
  const [searchParams] = useSearchParams();
  const { timeRange, setTimeRange } = useOpsTimeRangeUrl("1h");

  const serviceKey = (params.service ?? "").trim();
  const [refreshInterval, setRefreshInterval] = useState(30);
  const [loading, setLoading] = useState(false);

  const [allAppRows, setAllAppRows] = useState<ServiceHealthRow[]>([]);
  const [row, setRow] = useState<ServiceHealthRow | null>(null);
  const [catalogMetrics, setCatalogMetrics] = useState<MetricCatalogItem[]>([]);
  const [qpsSeries, setQpsSeries] = useState<MetricSeries | null>(null);
  const [errorSeries, setErrorSeries] = useState<MetricSeries | null>(null);
  const [latencySeries, setLatencySeries] = useState<MetricSeries | null>(null);
  const [useSeries, setUseSeries] = useState<Record<string, MetricSeries | null>>(
    {},
  );
  const [specificSeries, setSpecificSeries] = useState<
    Record<string, MetricSeries | null>
  >({});

  const seqRef = useRef(0);

  const chartWindow = useMemo(
    () => ({ start: timeRange.start, end: timeRange.end }),
    [timeRange.start, timeRange.end],
  );

  const hasPostgresPool = serviceHasPostgresPool(serviceKey);

  const specificPrefix = useMemo(() => {
    const key = serviceKey.replace(/^mazbri-/, "").replace(/-/g, "_");
    if (key === "gateway") return "gateway_";
    if (key === "auth") return "auth_";
    if (key === "cert") return "cert_";
    if (key === "billing") return "billing_";
    if (key === "cache") return "cache_";
    if (key === "bypass") return "bypass_";
    if (key.startsWith("cbpro")) return "cbpro_";
    return `${key}_`;
  }, [serviceKey]);

  const useMetrics = useMemo(
    () =>
      USE_METRIC_KEYS.map((key) =>
        catalogMetrics.find((m) => m.key === key),
      ).filter((m): m is MetricCatalogItem => m !== undefined),
    [catalogMetrics],
  );

  const specificMetrics = useMemo(
    () =>
      catalogMetrics
        .filter((m) => m.key.startsWith(specificPrefix))
        .slice(0, 9),
    [catalogMetrics, specificPrefix],
  );

  const reload = useCallback(async () => {
    if (!serviceKey) return;
    const mine = ++seqRef.current;
    setLoading(true);
    try {
      const [healthRows, catalog, qps, errors, latency] = await Promise.all([
        fetchServicesHealth(),
        fetchMetricsCatalog(),
        fetchMetricTimeseries({
          metric: "service_qps",
          start: timeRange.start,
          end: timeRange.end,
          service: serviceKey,
        }).catch(() => null),
        fetchMetricTimeseries({
          metric: "error_rate",
          start: timeRange.start,
          end: timeRange.end,
          service: serviceKey,
        }).catch(() => null),
        fetchMetricTimeseries({
          metric: "p95_latency_ms",
          start: timeRange.start,
          end: timeRange.end,
          service: serviceKey,
        }).catch(() => null),
      ]);
      if (mine !== seqRef.current) return;

      const metricsList = catalog.metrics ?? [];
      setAllAppRows(filterRowsByGroup(healthRows ?? [], "apps"));
      setRow(
        (healthRows ?? []).find((item) => item.service === serviceKey) ?? null,
      );
      setCatalogMetrics(metricsList);
      setQpsSeries(qps);
      setErrorSeries(errors);
      setLatencySeries(latency);

      const resolvedUse = USE_METRIC_KEYS.map((key) =>
        metricsList.find((m) => m.key === key),
      ).filter((m): m is MetricCatalogItem => m !== undefined);

      const resolvedSpecific = metricsList
        .filter((m) => m.key.startsWith(specificPrefix))
        .slice(0, 9);

      const [useEntries, specEntries] = await Promise.all([
        Promise.all(
          resolvedUse.map(async (item) => {
            const data = await fetchMetricTimeseries({
              metric: item.key,
              start: timeRange.start,
              end: timeRange.end,
              service: serviceKey,
            }).catch(() => null);
            return [item.key, data] as const;
          }),
        ),
        Promise.all(
          resolvedSpecific.map(async (item) => {
            const data = await fetchMetricTimeseries({
              metric: item.key,
              start: timeRange.start,
              end: timeRange.end,
              service: serviceKey,
            }).catch(() => null);
            return [item.key, data] as const;
          }),
        ),
      ]);

      if (mine !== seqRef.current) return;
      setUseSeries(Object.fromEntries(useEntries));
      setSpecificSeries(Object.fromEntries(specEntries));
    } catch (error) {
      if (mine !== seqRef.current) return;
      showErrorToast(
        error instanceof Error ? error.message : t("ops.services.loadFailed"),
      );
    } finally {
      if (mine === seqRef.current) setLoading(false);
    }
  }, [serviceKey, timeRange.start, timeRange.end, specificPrefix, t]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const serviceOptions = useMemo<SelectOptionItem[]>(() => {
    const opts: SelectOptionItem[] = allAppRows.map((item) => {
      const label = item.name ? `${item.name} (${item.service})` : item.service;
      return { value: item.service, label, text: label };
    });
    if (serviceKey && !opts.some((o) => o.value === serviceKey)) {
      opts.unshift({ value: serviceKey, label: serviceKey, text: serviceKey });
    }
    return opts;
  }, [allAppRows, serviceKey]);

  const handleSwitchService = (nextService: string) => {
    if (!nextService || nextService === serviceKey) return;
    const base = buildServiceDetailRoute(nextService);
    const preserved: Record<string, string> = {};
    for (const k of ["tr_preset", "tr_start", "tr_end"] as const) {
      const val = searchParams.get(k);
      if (val) preserved[k] = val;
    }
    if (typeof base === "object" && base !== null && Object.keys(preserved).length > 0) {
      void navigate(toLocationHref({ ...base, query: preserved }));
      return;
    }
    void navigate(toLocationHref(base));
  };

  const title = row?.name || serviceKey || t("menus.mazbri.serviceDetail");
  const statusCode = row
    ? row.status === "green" || (!row.status && row.health_ok)
      ? "HEALTHY"
      : row.status === "yellow" || row.ready === false
        ? "DEGRADED"
        : row.status === "red" || !row.health_ok
          ? "UNHEALTHY"
          : "UNKNOWN"
    : undefined;

  const healthLabel = !row
    ? "—"
    : row.health_ok
      ? t("ops.services.ok")
      : t("ops.services.down");
  const poolLabel =
    row?.db_pool_usage === null || row?.db_pool_usage === undefined
      ? "—"
      : `${(row.db_pool_usage * 100).toFixed(1)}%`;
  const restartLabel = row?.last_restart_ts
    ? formatDateTime(row.last_restart_ts)
    : "—";

  // A-10: unify RED row empty state when there is zero traffic across the window
  const redHasNoTraffic =
    isMetricSeriesZeroOrEmpty(qpsSeries) &&
    isMetricSeriesEmpty(errorSeries) &&
    isMetricSeriesEmpty(latencySeries);

  const redPanels = useMemo(
    () => [
      {
        key: "qps",
        title: t("ops.services.detail.redQps"),
        series: qpsSeries,
        empty: redHasNoTraffic || isMetricSeriesEmpty(qpsSeries),
      },
      {
        key: "errors",
        title: t("ops.services.detail.redErrors"),
        series: errorSeries,
        empty: redHasNoTraffic || isMetricSeriesEmpty(errorSeries),
      },
      {
        key: "latency",
        title: t("ops.services.detail.redLatencyP95"),
        series: latencySeries,
        empty: redHasNoTraffic || isMetricSeriesEmpty(latencySeries),
      },
    ],
    [t, qpsSeries, errorSeries, latencySeries, redHasNoTraffic],
  );

  return (
    <EntityDetailLayout
      title={title}
      code={serviceKey}
      status={statusCode}
      onBack={() =>
        void navigate(toLocationHref(buildRuntimeRoute({ tab: "apps" })))
      }
      actions={
        <div className="flex flex-wrap items-center gap-2">
          <Select
            value={serviceKey}
            options={serviceOptions}
            searchable
            placeholder={t("ops.services.switchService")}
            onChange={(e) => handleSwitchService(e.target.value)}
            className="w-56"
          />
          <OpsTimeBar
            value={timeRange}
            onChange={setTimeRange}
            refreshInterval={refreshInterval}
            onRefreshIntervalChange={setRefreshInterval}
            onRefresh={() => void reload()}
            loading={loading}
          />
        </div>
      }
    >
      {/* Top 4 KPI Overview Cards */}
      <div
        className={cn(
          "grid grid-cols-1 gap-3 sm:grid-cols-2",
          hasPostgresPool ? "lg:grid-cols-4" : "lg:grid-cols-3",
        )}
      >
        <StatCard
          label={t("ops.services.detail.health")}
          value={healthLabel}
          icon={<HeartPulse className="h-3.5 w-3.5 text-success-fg" />}
        />
        {hasPostgresPool && (
          <StatCard
            label={t("ops.services.detail.dbPool")}
            value={poolLabel}
            icon={<Database className="h-3.5 w-3.5 text-primary" />}
          />
        )}
        <StatCard
          label={t("ops.services.detail.version")}
          value={row?.version || "—"}
          icon={<GitCommit className="h-3.5 w-3.5 text-muted-foreground" />}
        />
        <StatCard
          label={t("ops.services.detail.lastRestart")}
          value={restartLabel}
          hint="UTC+8"
          icon={<RotateCw className="h-3.5 w-3.5 text-muted-foreground" />}
        />
      </div>

      {/* Golden Signals (RED) — uniform empty state across row (A-10) */}
      <section className="space-y-2.5">
        <h2 className="text-sm font-semibold text-foreground">
          {t("ops.services.detail.redTitle")}
        </h2>
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
          {redPanels.map((red) => (
            <ChartCard
              key={red.key}
              title={red.title}
              loading={loading}
              empty={!loading && red.empty}
              emptyText={t("ops.services.detail.noTraffic")}
              height={220}
            >
              <MetricSeriesChart
                series={red.series}
                window={chartWindow}
                height={220}
              />
            </ChartCard>
          ))}
        </div>
      </section>

      {/* Resource Saturation (USE) */}
      <section className="space-y-2.5">
        <h2 className="text-sm font-semibold text-foreground">
          {t("ops.services.detail.useTitle")}
        </h2>
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
          {useMetrics.map((item) => {
            const data = useSeries[item.key] ?? null;
            return (
              <ChartCard
                key={item.key}
                title={item.label}
                loading={loading}
                empty={!loading && isMetricSeriesEmpty(data)}
                emptyText={t("ops.charts.empty")}
                height={200}
              >
                <MetricSeriesChart
                  series={data}
                  window={chartWindow}
                  unitType={
                    item.key === "process_cpu_cores" ? "cores" : undefined
                  }
                  height={200}
                />
              </ChartCard>
            );
          })}
        </div>
      </section>

      {/* Service-Specific Metrics */}
      <section className="space-y-2.5">
        <h2 className="text-sm font-semibold text-foreground">
          {t("ops.services.detail.specific")}
        </h2>
        {specificMetrics.length === 0 ? (
          <Card className="p-4">
            <EmptyState title={t("ops.services.detail.noSpecific")} />
          </Card>
        ) : (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
            {specificMetrics.map((item) => {
              const data = specificSeries[item.key] ?? null;
              return (
                <ChartCard
                  key={item.key}
                  title={item.label}
                  loading={loading}
                  empty={!loading && isMetricSeriesEmpty(data)}
                  emptyText={t("ops.charts.empty")}
                  height={200}
                >
                  <MetricSeriesChart
                    series={data}
                    window={chartWindow}
                    height={200}
                  />
                </ChartCard>
              );
            })}
          </div>
        )}
      </section>
    </EntityDetailLayout>
  );
}
