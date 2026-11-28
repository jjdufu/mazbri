import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import {
  Activity,
  AlertTriangle,
  Building2,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Clock,
  Layers,
  Link2,
  OctagonAlert,
  PowerOff,
  StopCircle,
} from "lucide-react";
import {
  ChartCard,
  MetricSeriesChart,
  isMetricSeriesEmpty,
  isMetricSeriesZeroOrEmpty,
  type MetricPointClick,
} from "@/components/charts";
import { PageHeaderActions } from "@/components/layout";
import { Button, Card, cn } from "@/components/ui";
import { fetchAlertSummary } from "@/api/alerts";
import { fetchPlatformInventory } from "@/api/analytics";
import { fetchMetricTimeseries, fetchMetricsOverview } from "@/api/monitoring";
import { postOlapQuery } from "@/api/olap";
import { LanguageEnum } from "@/enums/appEnum";
import { useT } from "@/locales";
import type { AlertSummary } from "@/types/api/alerts";
import type { PlatformInventory } from "@/types/api/analytics";
import type {
  MetricOverview,
  MetricSeries,
  TrafficKpis,
} from "@/types/api/monitoring";
import type { ExemplarRow } from "@/types/api/olap";
import { formatWallClockHm, formatWallClockMdHm } from "@/utils/format";
import {
  TRAFFIC_PATH,
  buildAlertsRoute,
  buildCallsRoute,
  buildTraceSearchRoute,
} from "@/utils/ops/opsRoutes";
import {
  canOverviewDrill,
  resolveOverviewDrill,
  type OverviewDrillId,
} from "@/utils/ops/overview-drill";
import { resolveRange } from "@/utils/ops/time-range";
import { showErrorToast, showInfoToast } from "@/utils/ui/message-guard";
import { OpsTimeBar } from "../shared/OpsTimeBar";
import {
  resolveStep,
  toLocationHref,
  useOpsRole,
  useOpsTimeRangeUrl,
} from "../shared/opsHelpers";
import { CertExpiryBadge } from "./CertExpiryBadge";
import { ProductTopN } from "./ProductTopN";
import { RecentAnomalies } from "./RecentAnomalies";
import { SecurityGovSummary } from "./SecurityGovSummary";
import { SloBudgetPanel } from "./SloBudgetPanel";

const WINDOWED_DRILLS: ReadonlySet<OverviewDrillId> = new Set([
  "2.1",
  "2.2",
  "2.4",
  "4.1",
]);

interface KpiCardProps {
  label: string;
  value: number | null | undefined;
  unit?: string;
  hint?: string;
  digits?: number;
  icon: React.ReactNode;
  iconTone?: "primary" | "success" | "warning" | "danger" | "neutral";
  interactive?: boolean;
  mutedValue?: boolean;
  loading?: boolean;
  locale: string;
  onClick?: () => void;
}

const ICON_TONE_CLASS = {
  primary: "bg-primary-soft text-primary",
  success: "bg-success-fg/12 text-success-fg",
  warning: "bg-warning-bg text-warning-fg",
  danger: "bg-danger-fg/12 text-danger-fg",
  neutral: "bg-muted text-muted-foreground",
} as const;

function OverviewKpiCard({
  label,
  value,
  unit,
  hint,
  digits = 2,
  icon,
  iconTone = "primary",
  interactive = false,
  mutedValue = false,
  loading = false,
  locale,
  onClick,
}: KpiCardProps) {
  const formatted = useMemo(() => {
    if (value === null || value === undefined || Number.isNaN(value)) {
      return "—";
    }
    return new Intl.NumberFormat(locale, {
      maximumFractionDigits: digits,
    }).format(value);
  }, [value, locale, digits]);

  const inner = (
    <>
      <div className="flex items-center justify-between gap-2">
        <span className="truncate text-xs text-muted-foreground">{label}</span>
        <span
          className={cn(
            "inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md",
            ICON_TONE_CLASS[iconTone],
          )}
        >
          {icon}
        </span>
      </div>
      <div className="flex items-baseline gap-1 min-w-0">
        <span
          className={cn(
            "text-xl font-semibold font-mono-tech tabular-nums tracking-tight truncate",
            mutedValue ? "font-medium text-muted-foreground" : "text-foreground",
            loading && "opacity-60",
          )}
        >
          {formatted}
        </span>
        {unit && formatted !== "—" && (
          <span className="text-[11px] text-muted-foreground shrink-0">
            {unit}
          </span>
        )}
      </div>
      {hint && (
        <div className="truncate text-[11px] text-muted-foreground font-mono-tech tabular-nums">
          {hint}
        </div>
      )}
    </>
  );

  if (interactive && onClick) {
    return (
      <button
        type="button"
        onClick={onClick}
        className="w-full rounded-lg border border-border bg-card p-3.5 text-left space-y-1.5 transition-colors hover:border-primary/50 hover:bg-hover-bg/40 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
      >
        {inner}
      </button>
    );
  }

  return <Card className="p-3.5 space-y-1.5">{inner}</Card>;
}

export function OverviewPage() {
  const { t, locale } = useT();
  const navigate = useNavigate();
  const { isAdmin, isBiz, isViewer } = useOpsRole();
  const { timeRange, setTimeRange } = useOpsTimeRangeUrl("6h");

  const [refreshInterval, setRefreshInterval] = useState(30);
  const [reloadTick, setReloadTick] = useState(0);
  const [loading, setLoading] = useState(false);
  const [exemplarLoading, setExemplarLoading] = useState(false);
  const [alertsExpanded, setAlertsExpanded] = useState(false);

  const [overview, setOverview] = useState<MetricOverview | null>(null);
  const [inventory, setInventory] = useState<PlatformInventory | null>(null);
  const [qpsSeries, setQpsSeries] = useState<MetricSeries | null>(null);
  const [p99Series, setP99Series] = useState<MetricSeries | null>(null);
  const [alertSummary, setAlertSummary] = useState<AlertSummary | null>(null);
  const [chartWindow, setChartWindow] = useState<{
    start: number;
    end: number;
  } | null>(null);

  const seqRef = useRef(0);
  const drillRole = useMemo(
    () => ({ isAdmin, isBiz, isViewer }),
    [isAdmin, isBiz, isViewer],
  );

  const fmt = useCallback(
    (n: number | null | undefined) => {
      if (n === null || n === undefined || Number.isNaN(n)) return "—";
      return new Intl.NumberFormat(locale, { maximumFractionDigits: 2 }).format(
        n,
      );
    },
    [locale],
  );

  const reload = useCallback(async () => {
    const mine = ++seqRef.current;
    const resolved = resolveRange(timeRange);
    const win = { start: resolved.start, end: resolved.end };
    const step = resolveStep(win.end - win.start);
    setLoading(true);
    setReloadTick((v) => v + 1);

    try {
      const [ov, inv, qps, p99, alerts] = await Promise.all([
        fetchMetricsOverview(win),
        fetchPlatformInventory().catch(() => null),
        fetchMetricTimeseries({ metric: "qps", ...win, step }).catch(() => null),
        fetchMetricTimeseries({ metric: "p99_latency_ms", ...win, step }).catch(
          () => null,
        ),
        fetchAlertSummary().catch(() => null),
      ]);
      if (mine !== seqRef.current) return;
      setChartWindow(win);
      setOverview(ov ?? null);
      setInventory(inv);
      setQpsSeries(qps);
      setP99Series(p99);
      setAlertSummary(alerts);
    } catch (error) {
      if (mine !== seqRef.current) return;
      showErrorToast(
        error instanceof Error ? error.message : t("ops.overview.loadFailed"),
      );
    } finally {
      if (mine === seqRef.current) setLoading(false);
    }
  }, [timeRange, t]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const traffic = overview?.traffic ?? null;
  const realtime = overview?.realtime ?? null;

  const windowSemantics = useMemo(() => {
    const w = overview?.window ?? chartWindow;
    if (!w) return t("ops.overview.followsWindow");
    return t("ops.overview.windowSemantics", {
      start: formatWallClockMdHm(w.start * 1000),
      end: formatWallClockMdHm(w.end * 1000),
      label: t(`ops.timeRange.presets.${timeRange.preset}`),
    });
  }, [overview?.window, chartWindow, timeRange.preset, t]);

  const realtimeSemantics = useMemo(() => {
    if (!realtime) return t("ops.overview.realtimeNow");
    return t("ops.overview.realtimeSemantics", {
      secs: realtime.lookback_secs,
      at: formatWallClockHm(realtime.eval_at * 1000),
    });
  }, [realtime, t]);

  const latencyHint = useCallback(
    (k: TrafficKpis | null) => {
      if (!k || k.p99_latency_ms == null) return "";
      return t("ops.overview.latencySecondary", {
        p50: fmt(k.p50_latency_ms),
        p95: fmt(k.p95_latency_ms),
      });
    },
    [t, fmt],
  );

  const inventoryHint = useMemo(() => {
    if (!inventory) return "";
    return t("ops.overview.inventoryHint", {
      total: inventory.tenants.total,
      products: inventory.products.enabled,
    });
  }, [inventory, t]);

  const billingBacklogHint = useMemo(() => {
    const age = realtime?.billing_pipeline?.oldest_pending_age_seconds;
    if (age === undefined || age === null) return "";
    return t("ops.overview.billingOldestAgeHint", { age: fmt(age) });
  }, [realtime?.billing_pipeline?.oldest_pending_age_seconds, t, fmt]);

  const canDrill = (id: OverviewDrillId) => canOverviewDrill(id, drillRole);

  const drill = (id: OverviewDrillId) => {
    const target = resolveOverviewDrill(id, drillRole);
    if (!target) return;
    const query: Record<string, string> = { ...target.query };
    if (WINDOWED_DRILLS.has(id) && chartWindow) {
      query.tr_preset = timeRange.preset;
      query.tr_start = String(chartWindow.start);
      query.tr_end = String(chartWindow.end);
    }
    if (target.path === TRAFFIC_PATH && !query.tab) {
      query.tab = "customer";
    }
    void navigate(toLocationHref({ path: target.path, query }));
  };

  const openCallsWindow = () => {
    const target = chartWindow
      ? buildCallsRoute({
          tab: "customer",
          start: chartWindow.start,
          end: chartWindow.end,
        })
      : buildCallsRoute({ tab: "customer" });
    void navigate(toLocationHref(target));
  };

  const onQpsClick = useCallback(
    (point: MetricPointClick) => {
      if (isViewer) return;
      const span = chartWindow ? chartWindow.end - chartWindow.start : 3600;
      const windowSecs = Math.min(3600, Math.max(60, Math.ceil(span / 120)));
      void navigate(
        toLocationHref(
          buildCallsRoute({
            tab: "customer",
            start: point.timestamp - windowSecs,
            end: point.timestamp + windowSecs,
          }),
        ),
      );
    },
    [isViewer, chartWindow, navigate],
  );

  const onP99Click = useCallback(
    async (point: MetricPointClick) => {
      if (isViewer) return;
      const span = chartWindow ? chartWindow.end - chartWindow.start : 3600;
      const windowSecs = Math.min(3600, Math.max(60, Math.ceil(span / 120)));
      const start = point.timestamp - windowSecs;
      const end = point.timestamp + windowSecs;

      if (!isAdmin) {
        void navigate(
          toLocationHref(
            buildCallsRoute({
              tab: "customer",
              start,
              end,
              filters: { duration_ms_gte: Math.max(0, Math.floor(point.value)) },
            }),
          ),
        );
        return;
      }

      setExemplarLoading(true);
      try {
        const res = await postOlapQuery<ExemplarRow>({
          template: "exemplar_traces",
          params: { ts: point.timestamp, window_secs: windowSecs, limit: 5 },
        });
        const rows = res.rows.filter((r) => r.trace_id);
        if (!rows.length) {
          showInfoToast(t("ops.traces.exemplarEmpty"));
          return;
        }
        const threshold = Math.floor(
          Math.min(...rows.map((r) => r.p99_ms || r.value_ms)),
        );
        void navigate(
          toLocationHref(
            buildTraceSearchRoute({
              start,
              end,
              filters: { min_duration_ms: threshold },
            }),
          ),
        );
      } catch {
        showErrorToast(t("ops.traces.loadFailed"));
      } finally {
        setExemplarLoading(false);
      }
    },
    [isViewer, isAdmin, chartWindow, navigate, t],
  );

  const firingCount = alertSummary?.firing ?? 0;
  const noTrafficInWindow =
    isMetricSeriesZeroOrEmpty(qpsSeries) && isMetricSeriesEmpty(p99Series);
  const qpsEmpty = noTrafficInWindow || isMetricSeriesEmpty(qpsSeries);
  const p99Empty = noTrafficInWindow || isMetricSeriesEmpty(p99Series);

  return (
    <div className="space-y-5 min-w-0">
      {isViewer && (
        <div className="rounded-lg border border-primary/25 bg-primary-soft px-3.5 py-2 text-xs text-primary">
          {t("ops.overview.viewerSlim")}
        </div>
      )}

      <PageHeaderActions>
        <OpsTimeBar
          className="xl:flex-nowrap"
          value={timeRange}
          onChange={setTimeRange}
          refreshInterval={refreshInterval}
          onRefreshIntervalChange={setRefreshInterval}
          onRefresh={() => void reload()}
          loading={loading}
          extra={
            isBiz ? (
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() =>
                  void navigate("/operations/bills?tab=overview")
                }
              >
                <span>{t("ops.overview.financeShortcut")}</span>
              </Button>
            ) : undefined
          }
        />
      </PageHeaderActions>

      {/* Window KPI section */}
      <section className="space-y-2.5">
        <div className="flex items-baseline gap-2.5 min-w-0">
          <h2 className="text-sm font-semibold text-foreground shrink-0 whitespace-nowrap">
            {t("ops.overview.sections.kpi")}
          </h2>
          <span className="text-xs text-muted-foreground font-mono-tech truncate">
            {windowSemantics}
          </span>
        </div>

        <div className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-6">
          <OverviewKpiCard
            label={t("ops.overview.stats.qps")}
            value={traffic?.qps}
            unit={t("ops.units.perSec")}
            icon={<Activity className="h-3.5 w-3.5" />}
            iconTone="primary"
            loading={loading}
            locale={locale}
            interactive={canDrill("2.1")}
            onClick={() => drill("2.1")}
          />
          <OverviewKpiCard
            label={t("ops.overview.stats.successRate")}
            value={traffic?.success_rate}
            unit={t("ops.units.percent")}
            icon={<CheckCircle2 className="h-3.5 w-3.5" />}
            iconTone="success"
            loading={loading}
            locale={locale}
            interactive={canDrill("2.4")}
            onClick={() => drill("2.4")}
          />
          <OverviewKpiCard
            label={t("ops.overview.stats.latency")}
            value={traffic?.p99_latency_ms}
            unit={t("ops.units.ms")}
            hint={latencyHint(traffic)}
            icon={<Clock className="h-3.5 w-3.5" />}
            iconTone="warning"
            loading={loading}
            locale={locale}
            interactive={canDrill("2.2")}
            onClick={() => drill("2.2")}
          />
          <OverviewKpiCard
            label={t("ops.overview.stats.error4xx")}
            value={traffic?.error_rate_4xx}
            unit={t("ops.units.percent")}
            icon={<AlertTriangle className="h-3.5 w-3.5" />}
            iconTone="warning"
            loading={loading}
            locale={locale}
            interactive={canDrill("2.4")}
            onClick={() => drill("2.4")}
          />
          <OverviewKpiCard
            label={t("ops.overview.stats.error5xx")}
            value={traffic?.error_rate_5xx}
            unit={t("ops.units.percent")}
            icon={<OctagonAlert className="h-3.5 w-3.5" />}
            iconTone="danger"
            loading={loading}
            locale={locale}
            interactive={canDrill("2.4")}
            onClick={() => drill("2.4")}
          />
          <OverviewKpiCard
            label={t("ops.overview.stats.ratelimit")}
            value={traffic?.rate_limit_rejections_per_sec}
            unit={t("ops.units.perSec")}
            icon={<StopCircle className="h-3.5 w-3.5" />}
            iconTone="neutral"
            loading={loading}
            locale={locale}
            interactive={canDrill("4.1")}
            onClick={() => drill("4.1")}
          />
        </div>
      </section>

      {/* Realtime section */}
      <section className="space-y-2.5">
        <div className="flex flex-wrap items-baseline gap-2.5">
          <h2 className="text-sm font-semibold text-foreground">
            {t("ops.overview.sections.realtime")}
          </h2>
          <span className="text-xs text-muted-foreground font-mono-tech">
            {realtimeSemantics}
          </span>
        </div>

        <div
          className={cn(
            "grid grid-cols-1 gap-3 sm:grid-cols-2",
            isBiz ? "lg:grid-cols-4" : "lg:grid-cols-3",
          )}
        >
          <OverviewKpiCard
            label={t("ops.overview.stats.proxyConnections")}
            value={realtime?.proxy_active_connections}
            digits={0}
            icon={<Link2 className="h-3.5 w-3.5" />}
            iconTone="primary"
            loading={loading}
            locale={locale}
          />
          <OverviewKpiCard
            label={t("ops.overview.stats.circuitOpen")}
            value={realtime?.circuit_breaker_open_count}
            digits={0}
            icon={<PowerOff className="h-3.5 w-3.5" />}
            iconTone={
              (realtime?.circuit_breaker_open_count ?? 0) > 0
                ? "danger"
                : "neutral"
            }
            loading={loading}
            locale={locale}
            interactive={canDrill("3.2")}
            onClick={() => drill("3.2")}
          />
          <OverviewKpiCard
            label={t("ops.overview.stats.enabledTenants")}
            value={inventory?.tenants.enabled}
            hint={inventoryHint}
            digits={0}
            icon={<Building2 className="h-3.5 w-3.5" />}
            iconTone="neutral"
            mutedValue={isViewer}
            loading={loading}
            locale={locale}
          />
          {isBiz && (
            <OverviewKpiCard
              label={t("ops.overview.stats.billingBacklog")}
              value={realtime?.billing_pipeline?.backlog}
              hint={billingBacklogHint}
              digits={0}
              icon={<Layers className="h-3.5 w-3.5" />}
              iconTone={
                (realtime?.billing_pipeline?.backlog ?? 0) > 0
                  ? "warning"
                  : "primary"
              }
              loading={loading}
              locale={locale}
              interactive={canDrill("6.1")}
              onClick={() => drill("6.1")}
            />
          )}
        </div>
      </section>

      {/* Active alerts compact strip */}
      <div
        className={cn(
          "rounded-lg border px-4",
          firingCount > 0
            ? "border-danger-fg/40 bg-danger-fg/8"
            : "border-border bg-card",
        )}
      >
        <div className="flex min-h-[44px] flex-wrap items-center justify-between gap-3 py-1.5">
          <div className="flex items-center gap-2.5 min-w-0">
            <span
              className={cn(
                "h-2 w-2 rounded-full shrink-0",
                firingCount > 0 ? "bg-danger-fg" : "bg-success-fg",
              )}
            />
            <span className="text-xs font-medium text-foreground">
              {firingCount > 0
                ? t("ops.overview.alertsFiring", { n: firingCount })
                : t("ops.overview.alertsNone")}
            </span>
            {Boolean(alertSummary?.latest.length) && (
              <Button
                type="button"
                variant="ghost"
                size="xs"
                onClick={() => setAlertsExpanded((v) => !v)}
                className="text-danger-fg hover:text-danger-fg"
              >
                {alertsExpanded ? (
                  <>
                    <span>{locale === LanguageEnum.ZH ? "收起" : "Collapse"}</span>
                    <ChevronUp className="h-3.5 w-3.5" />
                  </>
                ) : (
                  <>
                    <span>{locale === LanguageEnum.ZH ? "展开" : "Expand"}</span>
                    <ChevronDown className="h-3.5 w-3.5" />
                  </>
                )}
              </Button>
            )}
          </div>

          <div className="flex items-center gap-3 shrink-0">
            {isAdmin && (
              <CertExpiryBadge days={realtime?.cert_min_expiry_days} />
            )}
            <Button
              type="button"
              variant="link"
              size="xs"
              onClick={() => void navigate(toLocationHref(buildAlertsRoute()))}
            >
              {t("ops.overview.alertsOpen")}
            </Button>
          </div>
        </div>

        {alertsExpanded && alertSummary?.latest.length ? (
          <div className="border-t border-border-subtle py-2 space-y-1">
            {alertSummary.latest.map((item) => {
              const labelText = Object.entries(item.labels ?? {})
                .filter(([k]) => k !== "category" && k !== "unit")
                .map(([k, v]) => `${k}=${v}`)
                .join(", ");
              return (
                <button
                  key={item.id}
                  type="button"
                  onClick={() =>
                    void navigate(
                      toLocationHref(buildAlertsRoute({ eventId: item.id })),
                    )
                  }
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-xs hover:bg-hover-bg cursor-pointer"
                >
                  <span
                    className={cn(
                      "h-2 w-2 rounded-full shrink-0",
                      item.severity === "critical"
                        ? "bg-danger-fg"
                        : "bg-warning-fg",
                    )}
                  />
                  <span className="font-medium text-foreground shrink-0">
                    {item.rule_name}
                  </span>
                  {labelText && (
                    <span className="truncate font-mono-tech text-muted-foreground">
                      {labelText}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        ) : null}
      </div>

      {/* Traffic quality charts */}
      <section className="space-y-2.5">
        <div className="flex items-center justify-between gap-2">
          <h2 className="text-sm font-semibold text-foreground">
            {t("ops.overview.sections.trafficQuality")}
          </h2>
          <Button
            type="button"
            variant="link"
            size="xs"
            onClick={openCallsWindow}
          >
            {t("ops.overview.trafficQualityOpenCalls")}
          </Button>
        </div>

        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          <ChartCard
            title={t("ops.overview.charts.qps")}
            loading={loading}
            empty={!loading && qpsEmpty}
            emptyText={t("ops.charts.empty")}
            height={240}
          >
            <MetricSeriesChart
              series={qpsSeries}
              window={chartWindow}
              height={240}
              onPointClick={isViewer ? undefined : onQpsClick}
            />
          </ChartCard>

          <ChartCard
            title={t("ops.overview.charts.p99")}
            loading={loading || exemplarLoading}
            empty={!loading && p99Empty}
            emptyText={t("ops.charts.empty")}
            height={240}
          >
            <MetricSeriesChart
              series={p99Series}
              window={chartWindow}
              height={240}
              onPointClick={isViewer ? undefined : (p) => void onP99Click(p)}
            />
          </ChartCard>
        </div>
      </section>

      {/* Security & Governance summary (Admin only) */}
      {isAdmin && chartWindow && (
        <section className="space-y-2.5">
          <h2 className="text-sm font-semibold text-foreground">
            {t("ops.overview.sections.securityGov")}
          </h2>
          <SecurityGovSummary
            start={chartWindow.start}
            end={chartWindow.end}
            reloadTick={reloadTick}
          />
        </section>
      )}

      {/* Insights: ProductTopN + RecentAnomalies + SloBudgetPanel */}
      <section className="space-y-3">
        <h2 className="text-sm font-semibold text-foreground">
          {t("ops.overview.sections.insight")}
        </h2>
        <ProductTopN window={chartWindow} reloadTick={reloadTick} />
        {isBiz && (
          <RecentAnomalies window={chartWindow} reloadTick={reloadTick} />
        )}
        <SloBudgetPanel
          connected={overview?.error_budget_connected}
          remaining={overview?.error_budget_remaining}
          sloName={overview?.error_budget_slo}
          reloadTick={reloadTick}
        />
      </section>
    </div>
  );
}
