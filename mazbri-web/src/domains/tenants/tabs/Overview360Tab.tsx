import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { EChartsCoreOption } from "echarts";
import {
  Activity,
  CheckCircle2,
  CircleDollarSign,
  HeartPulse,
  Timer,
} from "lucide-react";
import { postOlapQuery } from "@/api/olap";
import { fetchTenantSpend, type TenantSpend } from "@/api/tenant";
import {
  EChartBox,
  type ChartThemeTokens,
} from "@/components/charts";
import { EmptyState, EntityDetailCard, StatCard, cn } from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { TenantErrorRow, TenantOverviewRow } from "@/types/api/olap";
import {
  EMPTY_PLACEHOLDER,
  formatDisplayDateYmd,
  formatMoney,
} from "@/utils/format";
import { isBizRole } from "@/utils/ops/roles";
import { PRESET_SECONDS } from "@/utils/ops/time-range";
import { showErrorToast } from "@/utils/ui/message-guard";

export interface Overview360TabProps {
  tenantCode: string;
}

function last30DayRangeUtc8(): { from: string; to: string } {
  const now = Date.now();
  return {
    from: formatDisplayDateYmd(now - 29 * 86_400_000),
    to: formatDisplayDateYmd(now),
  };
}

export function Overview360Tab({ tenantCode }: Overview360TabProps) {
  const { t } = useT();
  const role = useAuthStore((s) => s.info.role);
  const isBiz = isBizRole(role);

  const [loading, setLoading] = useState(false);
  const [overview, setOverview] = useState<TenantOverviewRow | null>(null);
  const [spend, setSpend] = useState<TenantSpend | null>(null);
  const [errors, setErrors] = useState<TenantErrorRow[]>([]);

  const abortRef = useRef<AbortController | null>(null);

  const reload = useCallback(async () => {
    if (!tenantCode) {
      setOverview(null);
      setSpend(null);
      setErrors([]);
      return;
    }
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setLoading(true);
    try {
      const range = last30DayRangeUtc8();
      const end = Math.floor(Date.now() / 1000);
      const start = end - PRESET_SECONDS["24h"];
      const [ov, er, sp] = await Promise.all([
        postOlapQuery<TenantOverviewRow>(
          {
            template: "tenant_overview",
            params: { tenant_code: tenantCode, start, end },
          },
          controller.signal,
        ),
        postOlapQuery<TenantErrorRow>(
          {
            template: "tenant_errors",
            params: { tenant_code: tenantCode, start, end, limit: 20 },
          },
          controller.signal,
        ),
        isBiz
          ? fetchTenantSpend(tenantCode, range, controller.signal)
          : Promise.resolve(null),
      ]);
      if (controller.signal.aborted) return;
      setOverview(ov.rows?.[0] ?? null);
      setErrors(er.rows ?? []);
      setSpend(sp);
    } catch {
      if (!controller.signal.aborted) {
        showErrorToast(t("tenant.overview360.loadFailed"));
      }
    } finally {
      if (!controller.signal.aborted) {
        setLoading(false);
      }
    }
  }, [tenantCode, isBiz, t]);

  useEffect(() => {
    void reload();
    return () => abortRef.current?.abort();
  }, [reload]);

  /** 成功率 50% + 时延 30% + 活跃度 20%；时延以 1000ms 为满分门槛。 */
  const healthScore = useMemo(() => {
    if (!overview) return null;
    const success = Math.min(Math.max(overview.success_rate, 0), 100) / 100;
    const latency = Math.max(0, 1 - overview.p95_ms / 1000);
    const activity = overview.calls > 0 ? 1 : 0;
    return Math.round((success * 0.5 + latency * 0.3 + activity * 0.2) * 100);
  }, [overview]);

  const spendDisplay = useMemo(() => {
    const raw = spend?.tenant_spend;
    if (raw == null || raw === "") return EMPTY_PLACEHOLDER;
    const formatted = formatMoney(raw);
    return formatted === EMPTY_PLACEHOLDER
      ? EMPTY_PLACEHOLDER
      : `${formatted} ${t("common.currencies.cny")}`;
  }, [spend, t]);

  const totalErrorCount = useMemo(
    () => errors.reduce((acc, row) => acc + (Number(row.count) || 0), 0),
    [errors],
  );

  const errorTableRows = useMemo(
    () =>
      errors.map((row) => {
        const count = Number(row.count) || 0;
        const pct = totalErrorCount > 0 ? (count / totalErrorCount) * 100 : 0;
        return {
          ...row,
          count,
          pct,
          share: `${pct.toFixed(1)}%`,
        };
      }),
    [errors, totalErrorCount],
  );

  const topErrors = useMemo(
    () => errorTableRows.slice(0, 10),
    [errorTableRows],
  );

  // A-19 Fix: compact height & barMaxWidth: 28 with percentage labels so 1~3 rows never render as a giant full-width block
  const errorChartHeight = useMemo(() => {
    const count = topErrors.length;
    return Math.min(280, Math.max(150, count * 42 + 64));
  }, [topErrors.length]);

  const buildErrorChartOption = useCallback(
    (tokens: ChartThemeTokens): EChartsCoreOption => {
      const reversed = [...topErrors].reverse();
      const categories = reversed.map((r) => r.error_code);
      const data = reversed.map((r) => ({
        value: r.count,
        share: r.share,
      }));
      const maxVal = Math.max(1, ...reversed.map((r) => r.count));

      return {
        grid: {
          left: 12,
          right: 78,
          top: 16,
          bottom: 24,
          containLabel: true,
        },
        tooltip: {
          trigger: "axis",
          axisPointer: { type: "shadow" },
          formatter: (params: unknown) => {
            const list = Array.isArray(params) ? params : [params];
            const first = list[0] as
              | { name?: string; value?: number; data?: { share?: string } }
              | undefined;
            if (!first) return "";
            const val = Number(first.value ?? 0).toLocaleString();
            const share = first.data?.share ?? "";
            return `<div style="font-weight:600;margin-bottom:4px;">${first.name ?? ""}</div><div>${t("tenant.overview360.count")}: <b>${val}</b> (${share})</div>`;
          },
        },
        xAxis: {
          type: "value",
          min: 0,
          max: Math.ceil(maxVal * 1.15),
          minInterval: 1,
          axisLabel: {
            color: tokens.textColor,
            fontSize: 11,
          },
          splitLine: {
            lineStyle: { color: tokens.splitLineColor },
          },
        },
        yAxis: {
          type: "category",
          data: categories,
          axisLine: { lineStyle: { color: tokens.axisLineColor } },
          axisLabel: {
            color: tokens.textColor,
            fontSize: 11,
            width: 132,
            overflow: "truncate",
          },
        },
        series: [
          {
            name: t("tenant.overview360.count"),
            type: "bar",
            barMaxWidth: 28,
            barCategoryGap: "42%",
            itemStyle: {
              color: tokens.primaryColor,
              borderRadius: [0, 4, 4, 0],
            },
            label: {
              show: true,
              position: "right",
              color: tokens.textColor,
              fontSize: 11,
              formatter: (p: unknown) => {
                const item = p as {
                  value?: number;
                  data?: { share?: string };
                };
                const cnt = Number(item.value ?? 0).toLocaleString();
                const share = item.data?.share ?? "";
                return share ? `${cnt} (${share})` : cnt;
              },
            },
            data,
          },
        ],
      };
    },
    [topErrors, t],
  );

  return (
    <div
      className={cn(
        "space-y-4 min-w-0",
        loading && "opacity-75 transition-opacity",
      )}
      data-testid="tenant-overview360-tab"
    >
      <div
        className={cn(
          "grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3",
          isBiz ? "xl:grid-cols-5" : "xl:grid-cols-4",
        )}
      >
        <StatCard
          label={t("tenant.overview360.calls")}
          value={
            overview?.calls != null
              ? Math.round(overview.calls).toLocaleString()
              : EMPTY_PLACEHOLDER
          }
          hint={t("ops.timeRange.presets.24h")}
          icon={
            <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-primary-soft text-primary">
              <Activity className="h-4 w-4" />
            </span>
          }
        />

        <StatCard
          label={t("tenant.overview360.successRate")}
          value={
            overview?.success_rate != null
              ? `${overview.success_rate.toFixed(1)}%`
              : EMPTY_PLACEHOLDER
          }
          hint={t("ops.timeRange.presets.24h")}
          icon={
            <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-success-fg/12 text-success-fg">
              <CheckCircle2 className="h-4 w-4" />
            </span>
          }
        />

        <StatCard
          label={t("tenant.overview360.p95")}
          value={
            overview?.p95_ms != null
              ? `${Math.round(overview.p95_ms)} ms`
              : EMPTY_PLACEHOLDER
          }
          hint={t("ops.timeRange.presets.24h")}
          icon={
            <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-warning-bg text-warning-fg">
              <Timer className="h-4 w-4" />
            </span>
          }
        />

        {isBiz && (
          <StatCard
            label={t("tenant.overview360.tenantSpend")}
            value={spendDisplay}
            hint={spend?.window?.label || t("ops.timeRange.presets.30d")}
            icon={
              <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-primary-soft text-primary">
                <CircleDollarSign className="h-4 w-4" />
              </span>
            }
          />
        )}

        <StatCard
          label={t("tenant.overview360.health")}
          value={healthScore != null ? String(healthScore) : EMPTY_PLACEHOLDER}
          hint={t("ops.timeRange.presets.24h")}
          icon={
            <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-danger-fg/12 text-danger-fg">
              <HeartPulse className="h-4 w-4" />
            </span>
          }
        />
      </div>

      <EntityDetailCard title={t("tenant.overview360.errors")}>
        {errors.length === 0 && !loading ? (
          <EmptyState title={t("tenant.overview360.noErrors24h")} />
        ) : (
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-2 items-start">
            <div className="rounded-lg border border-border-subtle bg-card p-3">
              <EChartBox
                option={buildErrorChartOption}
                height={errorChartHeight}
                testId="tenant-errors-chart"
              />
            </div>

            <div className="overflow-x-auto rounded-lg border border-border">
              <table className="w-full border-collapse text-xs text-foreground">
                <thead>
                  <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                    <th className="h-9 px-3 text-left font-medium whitespace-nowrap">
                      {t("tenant.overview360.errorCode")}
                    </th>
                    <th className="h-9 px-3 text-right font-medium whitespace-nowrap">
                      {t("tenant.overview360.count")}
                    </th>
                    <th className="h-9 px-3 text-right font-medium whitespace-nowrap">
                      {t("ops.common.share")}
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border-subtle">
                  {errorTableRows.map((row) => (
                    <tr
                      key={row.error_code}
                      className="transition-colors hover:bg-hover-bg/75"
                    >
                      <td className="px-3 py-2 font-mono-tech text-foreground">
                        {row.error_code}
                      </td>
                      <td className="px-3 py-2 text-right font-mono-tech tabular-nums">
                        {row.count.toLocaleString()}
                      </td>
                      <td className="px-3 py-2 text-right font-mono-tech tabular-nums text-muted-foreground">
                        {row.share}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </EntityDetailCard>
    </div>
  );
}
