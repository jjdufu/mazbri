import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import type { EChartsCoreOption } from "echarts";
import {
  AlertTriangle,
  CircleDollarSign,
  HelpCircle,
  Percent,
  RefreshCw,
  TrendingUp,
  Wallet,
} from "lucide-react";
import {
  fetchFinanceBalances,
  fetchFinanceOverview,
  fetchFinanceRanks,
  type BalanceRow,
  type FinanceOverview,
  type FinanceRankRow,
  type FinanceRanks,
} from "@/api/billing-ops";
import {
  ChartCard,
  EChartBox,
  type ChartThemeTokens,
} from "@/components/charts";
import {
  Button,
  EmptyState,
  EntityDetailCard,
  Select,
  StatCard,
  StatusBadge,
  Tooltip,
  cn,
  type SelectOptionItem,
} from "@/components/ui";
import { LanguageEnum } from "@/enums/appEnum";
import { useT } from "@/locales";
import { getStatusLabel } from "@/utils/display-labels";
import { EMPTY_PLACEHOLDER, formatDays, formatMoney } from "@/utils/format";
import { showErrorToast } from "@/utils/ui/message-guard";
import { buildUniformCurrencyYAxisFormatter } from "../helpers/financeChartHelpers";

export interface FinanceOverviewPanelProps {
  active?: boolean;
}

export function FinanceOverviewPanel({
  active = true,
}: FinanceOverviewPanelProps) {
  const { t, locale } = useT();
  const isZh = locale === LanguageEnum.ZH;
  const navigate = useNavigate();

  const [days, setDays] = useState<number>(30);
  const [refreshSeconds, setRefreshSeconds] = useState<number>(60);
  const [loading, setLoading] = useState(false);

  const [overview, setOverview] = useState<FinanceOverview | null>(null);
  const [ranks, setRanks] = useState<FinanceRanks | null>(null);
  const [balances, setBalances] = useState<BalanceRow[]>([]);

  const abortRef = useRef<AbortController | null>(null);

  const reload = useCallback(async () => {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setLoading(true);
    try {
      const [ov, rk, bal] = await Promise.all([
        fetchFinanceOverview(days, controller.signal),
        fetchFinanceRanks(days, 10, controller.signal),
        fetchFinanceBalances(50, controller.signal),
      ]);
      if (controller.signal.aborted) return;
      setOverview(ov);
      setRanks(rk);
      setBalances(bal ?? []);
    } catch {
      if (!controller.signal.aborted) {
        showErrorToast(t("ops.finance.loadFailed"));
      }
    } finally {
      if (!controller.signal.aborted) {
        setLoading(false);
      }
    }
  }, [days, t]);

  useEffect(() => {
    void reload();
    return () => abortRef.current?.abort();
  }, [reload]);

  useEffect(() => {
    if (!active || refreshSeconds <= 0) return;
    const timer = window.setInterval(() => {
      void reload();
    }, refreshSeconds * 1000);
    return () => window.clearInterval(timer);
  }, [active, refreshSeconds, reload]);

  const costUnavailable =
    overview != null && overview.cost_status === "unavailable";
  const costPartial = overview?.cost_status === "partial";

  const windowHint =
    overview?.window?.label ?? t("ops.finance.windowFallback", { days });
  const costHint = costPartial
    ? `${windowHint} · ${t("ops.finance.partialBadge")}`
    : windowHint;

  const daysOptions = useMemo<SelectOptionItem[]>(
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

  const refreshOptions = useMemo<SelectOptionItem[]>(
    () => [
      {
        value: "0",
        label: isZh ? "自动刷新：关闭" : "Auto-refresh: Off",
        text: isZh ? "自动刷新：关闭" : "Auto-refresh: Off",
      },
      {
        value: "30",
        label: isZh ? "每 30 秒" : "Every 30s",
        text: isZh ? "每 30 秒" : "Every 30s",
      },
      {
        value: "60",
        label: isZh ? "每 60 秒" : "Every 60s",
        text: isZh ? "每 60 秒" : "Every 60s",
      },
      {
        value: "300",
        label: isZh ? "每 5 分钟" : "Every 5m",
        text: isZh ? "每 5 分钟" : "Every 5m",
      },
    ],
    [isZh],
  );

  const dayList = useMemo(() => overview?.days ?? [], [overview?.days]);

  const buildTrendOption = useCallback(
    (tokens: ChartThemeTokens): EChartsCoreOption => {
      const categories = dayList.map((d) => d.day);
      const revValues = dayList.map((d) => Number(d.revenue) || 0);
      const costValues = dayList.map((d) => Number(d.cost) || 0);
      const marginValues = dayList.map((d) =>
        d.margin == null || d.margin === "" ? 0 : Number(d.margin) || 0,
      );

      const allValues = costUnavailable
        ? revValues
        : [...revValues, ...costValues, ...marginValues];
      const yFormatter = buildUniformCurrencyYAxisFormatter(allValues, "¥");
      const fewDays = categories.length <= 3;

      const series = [
        {
          name: t("ops.finance.revenue"),
          type: "bar",
          barMaxWidth: fewDays ? 36 : 24,
          barGap: fewDays ? "18%" : "12%",
          barCategoryGap: fewDays ? "45%" : "30%",
          itemStyle: {
            color: tokens.primaryColor,
            borderRadius: [3, 3, 0, 0],
          },
          data: revValues,
        },
      ];

      if (!costUnavailable) {
        series.push(
          {
            name: t("ops.finance.cost"),
            type: "bar",
            barMaxWidth: fewDays ? 36 : 24,
            barGap: fewDays ? "18%" : "12%",
            barCategoryGap: fewDays ? "45%" : "30%",
            itemStyle: {
              color: tokens.warningColor,
              borderRadius: [3, 3, 0, 0],
            },
            data: costValues,
          },
          {
            name: t("ops.finance.margin"),
            type: "bar",
            barMaxWidth: fewDays ? 36 : 24,
            barGap: fewDays ? "18%" : "12%",
            barCategoryGap: fewDays ? "45%" : "30%",
            itemStyle: {
              color: tokens.successColor,
              borderRadius: [3, 3, 0, 0],
            },
            data: marginValues,
          },
        );
      }

      return {
        grid: {
          left: fewDays ? 24 : 14,
          right: fewDays ? 24 : 16,
          top: 42,
          bottom: 26,
          containLabel: true,
        },
        legend: {
          top: 6,
          right: 12,
          itemWidth: 10,
          itemHeight: 10,
          textStyle: { color: tokens.textColor, fontSize: 11 },
        },
        tooltip: {
          trigger: "axis",
          axisPointer: { type: "shadow" },
          formatter: (params: unknown) => {
            const list = Array.isArray(params) ? params : [params];
            const first = list[0] as { axisValueLabel?: string } | undefined;
            const title = first?.axisValueLabel ?? "";
            const rows = list
              .map((entry) => {
                const item = entry as {
                  marker?: string;
                  seriesName?: string;
                  value?: number;
                };
                return `<div style="display:flex;justify-content:space-between;gap:14px;"><span>${item.marker ?? ""}${item.seriesName ?? ""}</span><b style="font-variant-numeric:tabular-nums;">¥${formatMoney(item.value ?? 0)}</b></div>`;
              })
              .join("");
            return `<div style="font-weight:600;margin-bottom:4px;">${title}</div>${rows}`;
          },
        },
        xAxis: {
          type: "category",
          data: categories,
          axisLine: { lineStyle: { color: tokens.axisLineColor } },
          axisLabel: { color: tokens.textColor, fontSize: 11 },
        },
        yAxis: {
          type: "value",
          axisLabel: {
            color: tokens.textColor,
            fontSize: 11,
            formatter: (val: number) => yFormatter(val),
          },
          splitLine: { lineStyle: { color: tokens.splitLineColor } },
        },
        series,
      };
    },
    [dayList, costUnavailable, t],
  );

  const rankName = (row: FinanceRankRow) =>
    row.display_name?.trim() || t("ops.finance.unnamed");

  const moneyOrDash = (value: string | null | undefined, status: string) =>
    status === "unavailable" ? EMPTY_PLACEHOLDER : formatMoney(value);

  const formatDaysCell = (val: number | null | undefined) => {
    const formatted = formatDays(val);
    return formatted === EMPTY_PLACEHOLDER
      ? formatted
      : `${formatted} ${t("ops.finance.daysUnit")}`;
  };

  return (
    <div
      className="space-y-4 min-w-0"
      data-testid="billing-finance-overview-panel"
    >
      {/* Toolbar */}
      <div className="flex flex-wrap items-center justify-end gap-2">
        <Select
          value={String(days)}
          options={daysOptions}
          onChange={(e) => setDays(Number(e.target.value) || 30)}
          className="w-32"
          aria-label={t("ops.timeRange.presets.30d")}
        />
        <Select
          value={String(refreshSeconds)}
          options={refreshOptions}
          onChange={(e) => setRefreshSeconds(Number(e.target.value) || 0)}
          className="w-36"
        />
        <Button
          type="button"
          variant="outline"
          size="md"
          onClick={() => void reload()}
          disabled={loading}
        >
          <RefreshCw
            className={loading ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"}
          />
          {t("common.actions.refresh")}
        </Button>
      </div>

      {/* Cost Unavailable Alert */}
      {costUnavailable && (
        <div className="flex items-start gap-2.5 rounded-lg border border-warning-border bg-warning-bg px-4 py-3 text-xs text-warning-fg">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <div className="space-y-0.5">
            <div className="font-semibold">
              {t("ops.finance.costUnavailableTitle")}
            </div>
            <div>{t("ops.finance.costUnavailableDesc")}</div>
          </div>
        </div>
      )}

      {/* 4 KPI Cards (A-02 Fix: every card has a crisp Lucide icon) */}
      <div
        className={cn(
          "grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4",
          loading && "opacity-75 transition-opacity",
        )}
      >
        <StatCard
          label={t("ops.finance.revenue")}
          value={
            overview?.revenue_total != null
              ? `${formatMoney(overview.revenue_total)} ${t("common.currencies.cny")}`
              : EMPTY_PLACEHOLDER
          }
          hint={windowHint}
          icon={
            <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-primary-soft text-primary">
              <CircleDollarSign className="h-4 w-4" />
            </span>
          }
        />

        <StatCard
          label={t("ops.finance.cost")}
          value={
            costUnavailable || overview?.cost_total == null
              ? EMPTY_PLACEHOLDER
              : `${formatMoney(overview.cost_total)} ${t("common.currencies.cny")}`
          }
          hint={costHint}
          icon={
            <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-warning-bg text-warning-fg">
              <Wallet className="h-4 w-4" />
            </span>
          }
        />

        <StatCard
          label={t("ops.finance.margin")}
          value={
            costUnavailable || overview?.margin_total == null
              ? EMPTY_PLACEHOLDER
              : `${formatMoney(overview.margin_total)} ${t("common.currencies.cny")}`
          }
          hint={costHint}
          icon={
            <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-success-fg/12 text-success-fg">
              <TrendingUp className="h-4 w-4" />
            </span>
          }
        />

        <StatCard
          label={t("ops.finance.marginRate")}
          value={
            costUnavailable || overview?.margin_rate == null
              ? EMPTY_PLACEHOLDER
              : `${overview.margin_rate.toFixed(1)}%`
          }
          hint={costHint}
          icon={
            <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-primary-soft text-primary">
              <Percent className="h-4 w-4" />
            </span>
          }
        />
      </div>

      {/* Inverted Days Alert */}
      {(overview?.inverted_days?.length ?? 0) > 0 && (
        <div className="flex items-start gap-2.5 rounded-lg border border-warning-border bg-warning-bg px-4 py-3 text-xs text-warning-fg">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <div className="space-y-0.5">
            <div className="font-semibold">{t("ops.finance.invertedWarn")}</div>
            <div className="font-mono-tech">
              {overview?.inverted_days.join(", ")}
            </div>
          </div>
        </div>
      )}

      {/* Trend Chart (A-02 Fix: uniform Y-axis + balanced bar width) */}
      <ChartCard
        title={t("ops.finance.trend")}
        loading={loading}
        empty={dayList.length === 0}
        height={280}
        testId="finance-trend-chart-card"
      >
        <EChartBox
          option={buildTrendOption}
          height={280}
          testId="finance-trend-chart"
        />
      </ChartCard>

      {/* 3 Rankings Grid */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        {/* Tenant Rank */}
        <EntityDetailCard
          title={t("ops.finance.rankTenant")}
          bodyClassName="p-0"
        >
          {(ranks?.by_tenant ?? []).length === 0 ? (
            <div className="p-4">
              <EmptyState title={t("common.feedback.noData")} />
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-xs text-foreground">
                <thead>
                  <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                    <th className="h-8 px-3 text-left font-medium">
                      {t("ops.finance.tenantName")}
                    </th>
                    <th className="h-8 px-2.5 text-left font-medium">
                      {t("ops.finance.tenantCode")}
                    </th>
                    <th className="h-8 px-3 text-right font-medium">
                      {t("ops.finance.revenue")}
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border-subtle">
                  {(ranks?.by_tenant ?? []).map((row) => (
                    <tr
                      key={row.key}
                      onClick={
                        row.tenant_code
                          ? () =>
                              navigate(
                                `/operations/tenants/${row.tenant_code}?tab=account`,
                              )
                          : undefined
                      }
                      className={cn(
                        "transition-colors hover:bg-hover-bg/75",
                        row.tenant_code && "cursor-pointer",
                      )}
                    >
                      <td className="px-3 py-2 font-medium truncate max-w-[140px]">
                        {rankName(row)}
                      </td>
                      <td className="px-2.5 py-2 font-mono-tech text-muted-foreground">
                        {row.tenant_code?.trim() || EMPTY_PLACEHOLDER}
                      </td>
                      <td className="px-3 py-2 text-right font-mono-tech tabular-nums">
                        {formatMoney(row.revenue)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </EntityDetailCard>

        {/* Product Rank */}
        <EntityDetailCard
          title={t("ops.finance.rankProduct")}
          bodyClassName="p-0"
        >
          {(ranks?.by_product ?? []).length === 0 ? (
            <div className="p-4">
              <EmptyState title={t("common.feedback.noData")} />
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-xs text-foreground">
                <thead>
                  <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                    <th className="h-8 px-3 text-left font-medium">
                      {t("ops.finance.productName")}
                    </th>
                    <th className="h-8 px-3 text-right font-medium">
                      {t("ops.finance.revenue")}
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border-subtle">
                  {(ranks?.by_product ?? []).map((row) => (
                    <tr
                      key={row.key}
                      className="transition-colors hover:bg-hover-bg/75"
                    >
                      <td className="px-3 py-2 font-medium truncate max-w-[200px]">
                        {rankName(row)}
                      </td>
                      <td className="px-3 py-2 text-right font-mono-tech tabular-nums">
                        {formatMoney(row.revenue)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </EntityDetailCard>

        {/* Supplier Rank */}
        <EntityDetailCard
          title={t("ops.finance.rankSupplier")}
          bodyClassName="p-0"
        >
          {(ranks?.by_supplier ?? []).length === 0 ? (
            <div className="p-4">
              <EmptyState title={t("common.feedback.noData")} />
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-xs text-foreground">
                <thead>
                  <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                    <th className="h-8 px-3 text-left font-medium">
                      {t("ops.finance.supplierName")}
                    </th>
                    <th className="h-8 px-3 text-right font-medium">
                      {t("ops.finance.cost")}
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border-subtle">
                  {(ranks?.by_supplier ?? []).map((row) => (
                    <tr
                      key={row.key}
                      className="transition-colors hover:bg-hover-bg/75"
                    >
                      <td className="px-3 py-2 font-medium truncate max-w-[200px]">
                        {rankName(row)}
                      </td>
                      <td className="px-3 py-2 text-right font-mono-tech tabular-nums">
                        {moneyOrDash(row.cost, row.cost_status)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </EntityDetailCard>
      </div>

      {/* Balances & Days Remaining Table */}
      <EntityDetailCard title={t("ops.finance.balances")} bodyClassName="p-0">
        {balances.length === 0 ? (
          <div className="p-4">
            <EmptyState title={t("common.feedback.noData")} />
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-xs text-foreground">
              <thead>
                <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                  <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                    {t("ops.finance.tenantName")}
                  </th>
                  <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                    {t("ops.finance.tenantCode")}
                  </th>
                  <th className="h-9 px-3 text-right font-medium whitespace-nowrap">
                    {t("ops.finance.cashBalance")}
                  </th>
                  <th className="h-9 px-3 text-right font-medium whitespace-nowrap">
                    {t("ops.finance.creditLimit")}
                  </th>
                  <th className="h-9 px-3 text-right font-medium whitespace-nowrap">
                    <span className="inline-flex items-center justify-end gap-1">
                      <span>{t("ops.finance.available")}</span>
                      <Tooltip content={t("ops.finance.availableTooltip")}>
                        <span className="inline-flex text-muted-foreground hover:text-foreground cursor-help">
                          <HelpCircle className="h-3.5 w-3.5" />
                        </span>
                      </Tooltip>
                    </span>
                  </th>
                  <th className="h-9 px-3 text-right font-medium whitespace-nowrap">
                    {t("ops.finance.recharge")}
                  </th>
                  <th className="h-9 px-3 text-right font-medium whitespace-nowrap">
                    {t("ops.finance.avgSpend")}
                  </th>
                  <th className="h-9 px-3 text-right font-medium whitespace-nowrap">
                    {t("ops.finance.daysLeft")}
                  </th>
                  <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                    {t("ops.finance.status")}
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border-subtle">
                {balances.map((row) => {
                  const displayName =
                    row.tenant_name?.trim() ||
                    row.tenant_code?.trim() ||
                    t("ops.finance.unnamed");
                  const lowDays =
                    row.days_remaining != null &&
                    Number.isFinite(row.days_remaining) &&
                    row.days_remaining <= 7;
                  return (
                    <tr
                      key={row.tenant_id}
                      onClick={
                        row.tenant_code
                          ? () =>
                              navigate(
                                `/operations/tenants/${row.tenant_code}?tab=account`,
                              )
                          : undefined
                      }
                      className={cn(
                        "transition-colors hover:bg-hover-bg/75",
                        row.tenant_code && "cursor-pointer",
                      )}
                    >
                      <td className="px-3 py-2.5 font-medium text-primary">
                        {displayName}
                      </td>
                      <td className="px-3 py-2.5 font-mono-tech text-muted-foreground">
                        {row.tenant_code?.trim() || EMPTY_PLACEHOLDER}
                      </td>
                      <td className="px-3 py-2.5 text-right font-mono-tech tabular-nums">
                        {formatMoney(row.balance)}
                      </td>
                      <td className="px-3 py-2.5 text-right font-mono-tech tabular-nums">
                        {formatMoney(row.credit_limit)}
                      </td>
                      <td className="px-3 py-2.5 text-right font-mono-tech tabular-nums font-medium">
                        {formatMoney(row.available_balance)}
                      </td>
                      <td className="px-3 py-2.5 text-right font-mono-tech tabular-nums">
                        {formatMoney(row.recent_recharge)}
                      </td>
                      <td className="px-3 py-2.5 text-right font-mono-tech tabular-nums">
                        {formatMoney(row.avg_daily_spend)}
                      </td>
                      <td
                        className={cn(
                          "px-3 py-2.5 text-right font-mono-tech tabular-nums",
                          lowDays && "font-semibold text-warning-fg",
                        )}
                      >
                        {formatDaysCell(row.days_remaining)}
                      </td>
                      <td className="px-3 py-2.5">
                        <StatusBadge
                          status={row.status}
                          label={getStatusLabel(row.status)}
                        />
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </EntityDetailCard>
    </div>
  );
}
